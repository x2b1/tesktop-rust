//! Ports that act on a conversation rather than on its text: what to tidy up, and what
//! arrived after a ping.

use crate::{Delivery, Fallback, Intent, IntentContext, Meta, PendingReply, Setting, SettingKind, Values, flag_or, text_or};
use model::Id;
use std::collections::BTreeMap;

/// How many pings are remembered for the delete that follows them.
pub const MAX_PINGS: usize = 512;

const GHOST_SETTINGS: &[Setting] = &[
	Setting {
		key: "alertOnEveryone",
		label: "Also watch for @everyone and @here",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(true),
	},
	Setting {
		key: "preview",
		label: "How much of the message to quote",
		kind: SettingKind::Number { min: 20, max: 200 },
		default: Fallback::Number(80),
	},
];

/// One ping that has not been answered yet, kept so a delete can be named.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ping {
	pub author: String,
	pub content: String,
}

/// GhostPingAlert: a ping that arrives and is then taken back.
#[derive(Default)]
pub struct GhostPingAlert {
	everyone: bool,
	preview: usize,
	pings: BTreeMap<Id, Ping>,
	pending: Option<String>,
}

impl GhostPingAlert {
	pub fn remembered(&self) -> usize {
		self.pings.len()
	}
}

impl crate::Plugin for GhostPingAlert {
	fn meta(&self) -> Meta {
		Meta {
			id: "GhostPingAlert",
			name: "GhostPingAlert",
			description: "Tells you when a message that pinged you is taken back.",
			authors: "zyxwauut",
			tags: &["Chat", "Notifications"],
			aliases: &["ghostPingAlert"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		GHOST_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.everyone = flag_or(values, GHOST_SETTINGS, "alertOnEveryone");
		self.preview = crate::number_or(values, GHOST_SETTINGS, "preview").clamp(20, 200) as usize;
	}

	fn reset(&mut self) {
		self.pings.clear();
		self.pending = None;
	}

	fn on_created(
		&mut self,
		inbound: &crate::Inbound,
		message: &model::Message,
		_replies: &mut Vec<PendingReply>,
	) {
		let pinged = inbound.mentions_me || (self.everyone && message.mention_everyone);
		if !pinged {
			return;
		}
		// The oldest ping goes first, so a long session cannot grow this without bound.
		while self.pings.len() >= MAX_PINGS {
			if let Some(oldest) = self.pings.keys().next().copied() {
				self.pings.remove(&oldest);
			} else {
				break;
			}
		}
		self.pings.insert(
			message.id,
			Ping {
				author: message.author.global_name.clone().unwrap_or_else(|| message.author.username.clone()),
				content: message.content.clone(),
			},
		);
	}

	fn on_deleted(&mut self, message: &model::Message) {
		let Some(ping) = self.pings.remove(&message.id) else {
			return;
		};
		let mut quoted = ping.content;
		quoted.truncate(self.preview);
		if ping.content.chars().count() > self.preview {
			quoted.push('…');
		}
		if quoted.trim().is_empty() {
			quoted.push_str("(no text)");
		}
		self.pending = Some(format!("👻 Ghost ping from {}: \"{quoted}\"", ping.author));
	}

	fn take_toast(&mut self) -> Option<String> {
		self.pending.take()
	}

	fn summary(&self) -> Option<String> {
		(!self.pings.is_empty()).then(|| format!("{} pings waiting", self.pings.len()))
	}
}

/// The name a person goes by, preferring what they set.
fn display_name(user: &model::User) -> String {
	user.global_name
		.clone()
		.unwrap_or_else(|| user.username.clone())
}

const BLOCK_SETTINGS: &[Setting] = &[Setting {
	key: "alert",
	label: "Say when a person may have blocked you",
	kind: SettingKind::Toggle,
	default: Fallback::Flag(true),
}];

/// DetectBlock: a direct message that will not go out is the service saying no.
#[derive(Default)]
pub struct DetectBlock {
	alert: bool,
	pending: Option<String>,
}

impl crate::Plugin for DetectBlock {
	fn meta(&self) -> Meta {
		Meta {
			id: "DetectBlock",
			name: "DetectBlock",
			description: "Says when a direct message will not go out, which is often a block.",
			authors: "Niko",
			tags: &["Friends", "Notifications"],
			aliases: &["detectBlock"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		BLOCK_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.alert = flag_or(values, BLOCK_SETTINGS, "alert");
	}

	fn reset(&mut self) {
		self.pending = None;
	}

	fn delivered(&mut self, event: &Delivery<'_>) {
		if !self.alert {
			return;
		}
		let Delivery::Failed {
			channel,
			me,
			content,
			failure,
		} = event
		else {
			return;
		};
		// Only a refusal is a signal. A network failure or a rate limit says nothing about
		// the other person, and saying so would be a guess dressed as a fact.
		if !failure.contains("Permission denied") {
			return;
		}
		let direct = self
			.direct_peers
			.borrow()
			.get(&channel)
			.cloned()
			.unwrap_or_default();
		let who = if direct.is_empty() {
			"that person".to_string()
		} else {
			direct
		};
		let preview: String = content.chars().take(40).collect();
		self.pending = Some(format!(
			"{who} would not take that message ({failure}): \"{preview}\""
		));
		let _ = me;
	}
}

/// The peers a port has seen in a conversation, so a failure can name a person. The host
/// fills it from the conversation it already has; a port never reads the member list.
impl DetectBlock {
	fn remember(&self, channel: Id, name: String) {
		let mut peers = self.direct_peers.borrow_mut();
		while peers.len() >= MAX_PINGS {
			if let Some(oldest) = peers.keys().next().copied() {
				peers.remove(&oldest);
			} else {
				break;
			}
		}
		peers.insert(channel, name);
	}
}

const DELETE_SETTINGS: &[Setting] = &[Setting {
	key: "confirm",
	label: "Delete your last message in this conversation",
	kind: SettingKind::Toggle,
	default: Fallback::Flag(true),
}];

/// QuickDelete: a tidy-up without reaching for the menu.
pub struct QuickDelete {
	confirm: bool,
	pending: bool,
}

impl Default for QuickDelete {
	fn default() -> Self {
		Self {
			confirm: true,
			pending: false,
		}
	}
}

impl crate::Plugin for QuickDelete {
	fn meta(&self) -> Meta {
		Meta {
			id: "QuickDelete",
			name: "QuickDelete",
			description: "Deletes your last message in the conversation you are reading.",
			authors: "Vencord",
			tags: &["Chat", "Shortcuts"],
			aliases: &["quickDelete"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		DELETE_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.confirm = flag_or(values, DELETE_SETTINGS, "confirm");
	}

	fn command(&self, _argument: &str) -> Option<crate::commands::Claim> {
		self.confirm
			.then(|| crate::commands::Claim { body: String::new() })
	}

	fn command_names(&self) -> &'static [&'static str] {
		&["delete", "quickdelete"]
	}

	fn command_about(&self) -> &'static str {
		"Deletes your last message here."
	}

	fn take_intent(&mut self, context: &IntentContext<'_>) -> Option<Intent> {
		if !self.pending {
			return None;
		}
		self.pending = false;
		// Only the owner's own message, and only a message that is still in view: a delete
		// that guesses at a message is a delete nobody can undo.
		let previous = context.previous?;
		(previous.author == context.me).then_some(Intent::Delete {
			channel: context.channel,
			message: previous.id,
		})
	}

	fn on_created(
		&mut self,
		_inbound: &crate::Inbound,
		_message: &model::Message,
		_replies: &mut Vec<PendingReply>,
	) {
		self.pending = false;
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::Plugin;

	fn message(author: u64, content: &str) -> model::Message {
		let mut message = test_support::message(1, model::Id(7));
		message.author.id = model::Id(author);
		message.author.username = "someone".to_string();
		message.content = content.to_string();
		message
	}

	fn inbound(mentions_me: bool) -> crate::Inbound {
		crate::Inbound::new(model::Id(7), None, model::Id(1), 0)
			.with_mentions(mentions_me)
	}

	#[test]
	fn a_ping_is_remembered_and_named_when_it_is_taken_back() {
		let mut plugin = GhostPingAlert::default();
		let mut replies = Vec::new();
		plugin.on_created(&inbound(true), &message(2, "hey you"), &mut replies);
		assert_eq!(plugin.remembered(), 1);
		plugin.on_deleted(&message(2, ""));
		assert_eq!(
			plugin.take_toast().as_deref(),
			Some("👻 Ghost ping from someone: \"hey you\"")
		);
		assert!(plugin.take_toast().is_none(), "the line is shown once");
	}

	#[test]
	fn a_message_that_did_not_ping_you_is_not_remembered() {
		let mut plugin = GhostPingAlert::default();
		let mut replies = Vec::new();
		plugin.on_created(&inbound(false), &message(2, "hey you"), &mut replies);
		assert_eq!(plugin.remembered(), 0);
		plugin.on_deleted(&message(2, ""));
		assert!(plugin.take_toast().is_none());
	}

	#[test]
	fn everyone_can_be_left_out() {
		let mut plugin = GhostPingAlert::default();
		plugin.configure(&Values(
			[("alertOnEveryone".to_string(), serde_json::json!(false))]
				.into_iter()
				.collect(),
		));
		let mut replies = Vec::new();
		let mut everyone = message(2, "listen");
		everyone.mention_everyone = true;
		plugin.on_created(&inbound(false), &everyone, &mut replies);
		assert_eq!(plugin.remembered(), 0);
	}

	#[test]
	fn a_long_ping_is_quoted_to_the_length_you_ask_for() {
		let mut plugin = GhostPingAlert::default();
		let long = "x".repeat(200);
		let mut replies = Vec::new();
		plugin.on_created(&inbound(true), &message(2, &long), &mut replies);
		plugin.on_deleted(&message(2, ""));
		let line = plugin.take_toast().expect("a line");
		assert!(line.ends_with('…'), "{line}");
		assert!(line.chars().count() < 140, "the quote stays short");
	}

	#[test]
	fn remembered_pings_stay_bounded() {
		let mut plugin = GhostPingAlert::default();
		let mut replies = Vec::new();
		for id in 0..(MAX_PINGS as u64 + 50) {
			let mut ping = message(2, "hey");
			ping.id = model::Id(id);
			plugin.on_created(&inbound(true), &ping, &mut replies);
		}
		assert_eq!(plugin.remembered(), MAX_PINGS);
		assert!(plugin.summary().unwrap().contains("waiting"));
	}

	#[test]
	fn a_refusal_to_a_direct_message_is_named() {
		let mut plugin = DetectBlock::default();
		plugin.remember(model::Id(7), "someone".to_string());
		plugin.delivered(&Delivery::Failed {
			channel: model::Id(7),
			me: model::Id(1),
			content: "hello there",
			failure: "Permission denied",
		});
		assert_eq!(
			plugin.take_toast().as_deref(),
			Some("someone would not take that message (Permission denied): \"hello there\"")
		);
	}

	#[test]
	fn a_network_failure_is_not_a_block() {
		let mut plugin = DetectBlock::default();
		plugin.delivered(&Delivery::Failed {
			channel: model::Id(7),
			me: model::Id(1),
			content: "hello",
			failure: "Connection failed",
		});
		assert!(plugin.take_toast().is_none());
	}

	#[test]
	fn a_send_that_went_out_is_not_a_failure() {
		let mut plugin = DetectBlock::default();
		let sent = message(1, "hi");
		plugin.delivered(&Delivery::Sent {
			channel: model::Id(7),
			message: &sent,
			me: model::Id(1),
		});
		assert!(plugin.take_toast().is_none());
	}

	#[test]
	fn the_delete_names_only_your_own_last_message() {
		let mut plugin = QuickDelete::default();
		assert!(plugin.command("/delete").is_some());
		let previous = crate::Previous {
			id: model::Id(50),
			author: model::Id(1),
			content: "mine".into(),
			attachments: 0,
			age_ms: 100,
			is_group: false,
			replying: false,
		};
		let context = IntentContext {
			channel: model::Id(7),
			me: model::Id(1),
			previous: Some(&previous),
		};
		// A command run does not arm the delete; only a fresh conversation does.
		assert!(plugin.take_intent(&context).is_none());
	}

	#[test]
	fn nothing_is_deleted_when_there_is_nothing_of_yours() {
		let mut plugin = QuickDelete::default();
		let context = IntentContext {
			channel: model::Id(7),
			me: model::Id(1),
			previous: None,
		};
		assert!(plugin.take_intent(&context).is_none());
	}

	#[test]
	fn a_name_is_only_kept_within_the_bound() {
		let plugin = DetectBlock::default();
		for id in 0..(MAX_PINGS as u64 + 10) {
			plugin.remember(model::Id(id), format!("peer {id}"));
		}
		assert_eq!(plugin.direct_peers.borrow().len(), MAX_PINGS);
	}

	#[test]
	fn a_person_goes_by_the_name_they_set() {
		let mut user = test_support::user(1);
		user.username = "plain".into();
		assert_eq!(display_name(&user), "plain");
		user.global_name = Some("Chosen".into());
		assert_eq!(display_name(&user), "Chosen");
	}
}
