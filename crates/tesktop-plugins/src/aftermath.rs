//! Ports that act on a conversation rather than on its text: what to tidy up, what
//! arrived after a ping, and what the service said about a send.

use crate::{
	Delivery, Fallback, Inbound, Intent, IntentContext, Meta, PendingReply, Setting, SettingKind,
	Values, flag_or, number_or, text_or,
};
use model::Id;
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

/// How many pings are remembered for the delete that follows them.
pub const MAX_PINGS: usize = 512;
/// How many conversations a port remembers a name for.
pub const MAX_PEERS: usize = 512;
/// Ids remembered as already recorded, so a message that arrives twice is handled once.
pub const MAX_SEEN: usize = 4096;

/// Whether a message pings the owner, by mention or by everyone.
fn pings_owner(message: &model::Message, me: Id) -> bool {
	message.mention_everyone
		|| message.mentions.iter().any(|user| user.id == me)
		|| message.content.contains(&format!("<@{me}>"))
		|| message.content.contains(&format!("<@!{me}>"))
}

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
pub struct GhostPingAlert {
	everyone: bool,
	preview: usize,
	pings: BTreeMap<Id, Ping>,
	pending: Option<String>,
}

impl Default for GhostPingAlert {
	fn default() -> Self {
		Self {
			everyone: true,
			preview: 80,
			pings: BTreeMap::new(),
			pending: None,
		}
	}
}

impl GhostPingAlert {
	/// How many pings are waiting for a delete that may never come.
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
		self.preview = number_or(values, GHOST_SETTINGS, "preview").clamp(20, 200) as usize;
	}

	fn reset(&mut self) {
		self.pings.clear();
		self.pending = None;
	}

	fn on_created(
		&mut self,
		inbound: &Inbound,
		message: &model::Message,
		_replies: &mut Vec<PendingReply>,
	) {
		let pinged =
			pings_owner(message, inbound.me) && (self.everyone || !message.mention_everyone);
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
				author: message.author.name.chars().take(64).collect(),
				content: message.content.chars().take(MAX_BODY).collect(),
			},
		);
	}

	fn on_deleted(
		&mut self,
		_inbound: &Inbound,
		_channel: Id,
		id: Id,
		_last: Option<&model::Message>,
	) {
		let Some(ping) = self.pings.remove(&id) else {
			return;
		};
		let mut quoted: String = ping.content.chars().take(self.preview).collect();
		if ping.content.chars().count() > self.preview {
			quoted.push('…');
		}
		if quoted.trim().is_empty() {
			quoted = "(no text)".to_string();
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

/// A message body kept for a failure that will be described, which is a service ceiling.
const MAX_BODY: usize = 2000;

const BLOCK_SETTINGS: &[Setting] = &[Setting {
	key: "alert",
	label: "Say when a person may have blocked you",
	kind: SettingKind::Toggle,
	default: Fallback::Flag(true),
}];

/// DetectBlock: a direct message that will not go out is often a block.
///
/// The port only reports a refusal. It never decides that someone blocked you, and it never
/// reports anything for a network failure or a rate limit, which say nothing about the other
/// person.
pub struct DetectBlock {
	alert: bool,
	pending: Option<String>,
	/// The name of the person each conversation is with, as the app already knows it.
	peers: BTreeMap<Id, String>,
	seen: BTreeSet<Id>,
}

impl Default for DetectBlock {
	fn default() -> Self {
		Self {
			alert: true,
			pending: None,
			peers: BTreeMap::new(),
			seen: BTreeSet::new(),
		}
	}
}

impl DetectBlock {
	/// Note who a conversation is with, so a refusal can be about a person.
	pub fn remember(&mut self, channel: Id, name: String) {
		while self.peers.len() >= MAX_PEERS {
			if let Some(oldest) = self.peers.keys().next().copied() {
				self.peers.remove(&oldest);
			} else {
				break;
			}
		}
		self.peers.insert(channel, name.chars().take(64).collect());
	}
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
		self.seen.clear();
	}

	fn delivered(&mut self, event: &Delivery<'_>) {
		if !self.alert {
			return;
		}
		let Delivery::Failed {
			channel,
			content,
			failure,
			..
		} = event
		else {
			return;
		};
		if !failure.contains("Permission denied") {
			return;
		}
		// The same refusal twice is one thing that happened once.
		if !self.seen.insert(*channel) {
			return;
		}
		while self.seen.len() > MAX_SEEN {
			if let Some(oldest) = self.seen.iter().next().copied() {
				self.seen.remove(&oldest);
			} else {
				break;
			}
		}
		let who = self
			.peers
			.get(channel)
			.cloned()
			.unwrap_or_else(|| "Someone you were writing to".to_string());
		let preview: String = content.chars().take(40).collect();
		self.pending = Some(format!(
			"{who} would not take that message ({failure}): \"{preview}\""
		));
	}

	fn take_toast(&mut self) -> Option<String> {
		self.pending.take()
	}
}

const DELETE_SETTINGS: &[Setting] = &[Setting {
	key: "armed",
	label: "Delete your last message in this conversation",
	kind: SettingKind::Toggle,
	default: Fallback::Flag(true),
}];

/// QuickDelete: a tidy-up without reaching for the menu.
pub struct QuickDelete {
	armed: bool,
	pending: bool,
}

impl Default for QuickDelete {
	fn default() -> Self {
		Self {
			armed: true,
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
		self.armed = flag_or(values, DELETE_SETTINGS, "armed");
	}

	fn on_created(
		&mut self,
		inbound: &Inbound,
		message: &model::Message,
		_replies: &mut Vec<PendingReply>,
	) {
		// Only a message of your own arms the delete, and only the newest one: a delete
		// that guessed at a message would be a delete nobody could undo.
		self.pending = message.author.id == inbound.me;
	}

	fn take_intent(&mut self, context: &IntentContext<'_>) -> Option<Intent> {
		if !self.armed || !self.pending {
			return None;
		}
		self.pending = false;
		let previous = context.previous?;
		(previous.author == context.me).then_some(Intent::Delete {
			channel: context.channel,
			message: previous.id,
		})
	}

	fn composer_button(&self) -> Option<crate::ComposerButton> {
		Some(crate::ComposerButton {
			id: "quick-delete",
			// TestCord binds this to a chord rather than painting a button, and the app's
			// own trash glyph stands in until the port gets a real chord.
			icon: Some("trash"),
			label: "Delete",
			tooltip: "Delete your last message in this conversation.",
			active: Some(self.armed),
		})
	}

	fn press_composer(&mut self, id: &str) {
		if id == "quick-delete" {
			self.armed = !self.armed;
		}
	}
}

const REACT_SETTINGS: &[Setting] = &[Setting {
	key: "rules",
	label: "Rules: channel id, then one emoji per line",
	kind: SettingKind::Text { multiline: true },
	default: Fallback::Text(""),
}];

/// One rule from the setting: a conversation and the reactions to put on its messages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReactRule {
	pub channel: Id,
	pub reactions: Vec<String>,
}

/// Parse the rules, in either of the two forms they arrive in.
///
/// The original keeps them as a JSON string, so a settings file imported from it hands over
/// `[{"channelId":"7","reactions":[{"name":"👀"}]}]`, and reading that as lines would find
/// nothing at all. Both are therefore read: the original's JSON, and the flat `id emoji
/// emoji` lines this port writes, which is the same information without a parser in front of
/// it.
pub fn parse_rules(text: &str) -> Vec<ReactRule> {
	let trimmed = text.trim_start();
	if trimmed.starts_with('[') {
		return parse_json_rules(trimmed);
	}
	parse_line_rules(text)
}

/// The original's own form: a JSON array of rules, each with a conversation and its
/// reactions. Anything that does not fit is skipped rather than guessed at, and the list is
/// bounded before it is walked so a large file cannot cost an allocation.
fn parse_json_rules(text: &str) -> Vec<ReactRule> {
	let mut rules: Vec<ReactRule> = Vec::new();
	let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
		return rules;
	};
	let Some(entries) = value.as_array() else {
		return rules;
	};
	for entry in entries.iter().take(MAX_RULES) {
		let Some(channel) = entry
			.get("channelId")
			.and_then(serde_json::Value::as_str)
			.and_then(|id| Id::from_str(id).ok())
		else {
			continue;
		};
		let Some(reactions) = entry.get("reactions").and_then(serde_json::Value::as_array) else {
			continue;
		};
		let cleaned: Vec<String> = reactions
			.iter()
			.take(MAX_REACTIONS)
			.filter_map(|reaction| match reaction {
				// A custom emoji carries its id; a unicode one only has a name.
				serde_json::Value::String(name) => clean_reaction(name),
				serde_json::Value::Object(_) => reaction
					.get("id")
					.or_else(|| reaction.get("name"))
					.and_then(serde_json::Value::as_str)
					.and_then(clean_reaction),
				_ => None,
			})
			.collect();
		if !cleaned.is_empty() {
			rules.push(ReactRule {
				channel,
				reactions: cleaned,
			});
		}
	}
	rules
}

/// The form this port writes: one conversation per line, then its reactions.
fn parse_line_rules(text: &str) -> Vec<ReactRule> {
	let mut rules: Vec<ReactRule> = Vec::new();
	for line in text.lines() {
		let line = line.trim();
		if line.is_empty() || line.starts_with('#') {
			continue;
		}
		let mut parts = line.split_whitespace();
		let Some(channel) = parts.next().and_then(|id| Id::from_str(id).ok()) else {
			continue;
		};
		let reactions: Vec<String> = parts
			.filter_map(clean_reaction)
			.take(MAX_REACTIONS)
			.collect();
		if reactions.is_empty() {
			continue;
		}
		rules.push(ReactRule { channel, reactions });
	}
	rules.truncate(MAX_RULES);
	rules
}

/// One emoji, with nothing in it that the service would not accept in a reaction.
fn clean_reaction(emoji: &str) -> Option<String> {
	let emoji: String = emoji
		.chars()
		.filter(|character| !character.is_whitespace() && !character.is_control())
		.take(16)
		.collect();
	(!emoji.is_empty()).then_some(emoji)
}

/// Rules kept, so a long list cannot cost a scan per message.
pub const MAX_RULES: usize = 64;
/// Reactions in one rule.
pub const MAX_REACTIONS: usize = 8;
/// Messages one rule reacts to, so a busy channel cannot turn this into a flood.
pub const MAX_PER_RULE: usize = 16;

/// AutoChannelReact: the reactions a conversation always gets, from your own list.
#[derive(Default)]
pub struct AutoChannelReact {
	rules: Vec<ReactRule>,
	used: BTreeMap<Id, usize>,
	pending: Vec<Intent>,
}

impl AutoChannelReact {
	/// The reactions this rule says for a message, taking what it has already spent.
	fn take(&mut self, channel: Id) -> Vec<String> {
		let Some(rule) = self.rules.iter().find(|rule| rule.channel == channel) else {
			return Vec::new();
		};
		let spent = self.used.entry(channel).or_insert(0);
		if *spent >= MAX_PER_RULE {
			return Vec::new();
		}
		*spent += 1;
		rule.reactions.clone()
	}
}

impl crate::Plugin for AutoChannelReact {
	fn meta(&self) -> Meta {
		Meta {
			id: "AutoChannelReact",
			name: "AutoChannelReact",
			description: "Reacts to messages in the conversations your rules name.",
			authors: "Vencord",
			tags: &["Chat", "Fun"],
			aliases: &["autoChannelReact"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		REACT_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.rules = parse_rules(&text_or(values, REACT_SETTINGS, "rules"));
		self.used.clear();
		self.pending.clear();
	}

	fn reset(&mut self) {
		self.used.clear();
		self.pending.clear();
	}

	fn on_created(
		&mut self,
		inbound: &Inbound,
		message: &model::Message,
		_replies: &mut Vec<PendingReply>,
	) {
		// Your own message already has your reaction on it from the send path, and a
		// reaction you cannot see is not one to add.
		if message.author.id == inbound.me {
			return;
		}
		for emoji in self.take(inbound.channel) {
			self.pending.push(Intent::React {
				channel: inbound.channel,
				message: message.id,
				emoji,
				add: true,
			});
		}
	}

	fn take_intent(&mut self, _context: &IntentContext<'_>) -> Option<Intent> {
		// One at a time, so the app's queue and its permission check see each of them.
		if self.pending.is_empty() {
			return None;
		}
		Some(self.pending.remove(0))
	}

	fn summary(&self) -> Option<String> {
		(!self.rules.is_empty()).then(|| {
			let total: usize = self.rules.iter().map(|rule| rule.reactions.len()).sum();
			format!(
				"{} reactions over {} conversations",
				total,
				self.rules.len()
			)
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{Plugin, Registry};

	fn message(id: u64, author: u64, content: &str) -> model::Message {
		let mut message = test_support::message(id, model::Id(7));
		message.author.id = model::Id(author);
		message.author.name = "someone".to_string();
		message.content = content.to_string();
		message
	}

	fn inbound(me: u64) -> Inbound {
		Inbound::new(model::Id(7), None, model::Id(me), 0)
	}

	/// A message that really mentions the owner, which is what a ping is.
	fn ping(id: u64, me: u64, content: &str) -> model::Message {
		let mut message = message(id, 2, content);
		message.content = format!("<@{me}> {content}");
		message
			.mentions
			.push(test_support::message(1, model::Id(me)).author);
		message
	}

	#[test]
	fn a_ping_is_remembered_and_named_when_it_is_taken_back() {
		let mut plugin = GhostPingAlert::default();
		let mut replies = Vec::new();
		plugin.on_created(&inbound(1), &ping(2, 1, "hey you"), &mut replies);
		assert_eq!(plugin.remembered(), 1);
		plugin.on_deleted(&inbound(1), model::Id(7), model::Id(2), None);
		assert_eq!(
			plugin.take_toast().as_deref(),
			Some("👻 Ghost ping from someone: \"<@1> hey you\"")
		);
		assert!(plugin.take_toast().is_none(), "the line is shown once");
	}

	#[test]
	fn a_message_that_did_not_ping_you_is_not_remembered() {
		let mut plugin = GhostPingAlert::default();
		let mut replies = Vec::new();
		plugin.on_created(&inbound(1), &message(2, 2, "hey you"), &mut replies);
		assert_eq!(
			plugin.remembered(),
			0,
			"a message that does not ping is not a ping"
		);
		plugin.on_deleted(&inbound(1), model::Id(7), model::Id(2), None);
		assert!(plugin.take_toast().is_none());
	}

	#[test]
	fn a_plain_mention_pings_and_everyone_is_separate() {
		let mut plugin = GhostPingAlert::default();
		let mut replies = Vec::new();
		plugin.on_created(&inbound(1), &ping(1, 1, "hey"), &mut replies);
		assert_eq!(plugin.remembered(), 1, "a mention of you is a ping");

		let mut everyone = message(2, 2, "listen");
		everyone.mention_everyone = true;
		plugin.on_created(&inbound(1), &everyone, &mut replies);
		assert_eq!(plugin.remembered(), 2, "so is @everyone, by default");
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
		let mut everyone = message(2, 2, "listen");
		everyone.mention_everyone = true;
		plugin.on_created(&inbound(1), &everyone, &mut replies);
		assert_eq!(plugin.remembered(), 0);
	}

	#[test]
	fn a_long_ping_is_quoted_to_the_length_you_ask_for() {
		let mut plugin = GhostPingAlert::default();
		let long = "x".repeat(400);
		let mut replies = Vec::new();
		plugin.on_created(&inbound(1), &ping(2, 1, &long), &mut replies);
		plugin.on_deleted(&inbound(1), model::Id(7), model::Id(2), None);
		let line = plugin.take_toast().expect("a line");
		assert!(line.contains('…'), "the quote is cut: {line}");
		assert!(line.chars().count() < 140, "and it stays short: {line}");
	}

	#[test]
	fn an_empty_ping_is_still_named() {
		let mut plugin = GhostPingAlert::default();
		let mut replies = Vec::new();
		// A ping can arrive with no text of its own, which is still a ping.
		let mut empty = message(2, 2, "");
		empty.mention_everyone = true;
		plugin.on_created(&inbound(1), &empty, &mut replies);
		plugin.on_deleted(&inbound(1), model::Id(7), model::Id(2), None);
		let line = plugin.take_toast().expect("a line");
		assert!(line.contains("(no text)"), "{line}");
	}

	#[test]
	fn remembered_pings_stay_bounded() {
		let mut plugin = GhostPingAlert::default();
		let mut replies = Vec::new();
		for id in 0..(MAX_PINGS as u64 + 50) {
			plugin.on_created(&inbound(1), &ping(id, 1, "hey"), &mut replies);
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
	fn the_same_refusal_is_reported_once() {
		let mut plugin = DetectBlock::default();
		let event = Delivery::Failed {
			channel: model::Id(7),
			me: model::Id(1),
			content: "hello",
			failure: "Permission denied",
		};
		plugin.delivered(&event);
		plugin.delivered(&event);
		assert!(plugin.take_toast().is_some());
		assert!(plugin.take_toast().is_none());
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
		let sent = message(1, 1, "hi");
		plugin.delivered(&Delivery::Sent {
			channel: model::Id(7),
			message: &sent,
			me: model::Id(1),
		});
		assert!(plugin.take_toast().is_none());
	}

	#[test]
	fn a_name_is_only_kept_within_the_bound() {
		let mut plugin = DetectBlock::default();
		for id in 0..(MAX_PEERS as u64 + 10) {
			plugin.remember(model::Id(id), format!("peer {id}"));
		}
		assert_eq!(plugin.peers.len(), MAX_PEERS);
	}

	#[test]
	fn the_delete_names_only_your_own_last_message() {
		let mut plugin = QuickDelete::default();
		let mut replies = Vec::new();
		plugin.on_created(&inbound(1), &message(5, 1, "mine"), &mut replies);
		let previous = crate::Previous {
			id: model::Id(5),
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
		assert_eq!(
			plugin.take_intent(&context),
			Some(Intent::Delete {
				channel: model::Id(7),
				message: model::Id(5)
			})
		);
		assert!(
			plugin.take_intent(&context).is_none(),
			"one press, one delete"
		);
	}

	#[test]
	fn someones_else_message_never_arms_the_delete() {
		let mut plugin = QuickDelete::default();
		let mut replies = Vec::new();
		plugin.on_created(&inbound(1), &message(5, 2, "theirs"), &mut replies);
		let context = IntentContext {
			channel: model::Id(7),
			me: model::Id(1),
			previous: None,
		};
		assert!(plugin.take_intent(&context).is_none());
	}

	#[test]
	fn the_delete_button_arms_and_disarms_it() {
		let mut plugin = QuickDelete::default();
		assert_eq!(plugin.composer_button().unwrap().active, Some(true));
		plugin.press_composer("quick-delete");
		assert_eq!(plugin.composer_button().unwrap().active, Some(false));
	}

	#[test]
	fn the_rules_are_read_a_line_at_a_time() {
		let rules = parse_rules("# a comment\n7 👀 ✅\n\nnot-an-id 👀\n8 🎉\n");
		assert_eq!(rules.len(), 2);
		assert_eq!(rules[0].channel, model::Id(7));
		assert_eq!(rules[0].reactions, vec!["👀", "✅"]);
		assert_eq!(rules[1].reactions, vec!["🎉"]);
	}

	#[test]
	fn a_rule_reacts_once_per_message() {
		let mut plugin = AutoChannelReact::default();
		plugin.configure(&Values(
			[("rules".to_string(), serde_json::json!("7 👀"))]
				.into_iter()
				.collect(),
		));
		let context = IntentContext {
			channel: model::Id(7),
			me: model::Id(1),
			previous: None,
		};
		let mut replies = Vec::new();
		plugin.on_created(&inbound(1), &message(5, 2, "hello"), &mut replies);
		assert_eq!(
			plugin.take_intent(&context),
			Some(Intent::React {
				channel: model::Id(7),
				message: model::Id(5),
				emoji: "👀".to_string(),
				add: true,
			})
		);
		assert!(plugin.take_intent(&context).is_none());
	}

	#[test]
	fn a_conversation_with_no_rule_reacts_to_nothing() {
		let mut plugin = AutoChannelReact::default();
		plugin.configure(&Values(
			[("rules".to_string(), serde_json::json!("9 👀"))]
				.into_iter()
				.collect(),
		));
		let mut replies = Vec::new();
		plugin.on_created(&inbound(1), &message(5, 2, "hello"), &mut replies);
		assert!(
			plugin
				.take_intent(&IntentContext {
					channel: model::Id(7),
					me: model::Id(1),
					previous: None,
				})
				.is_none()
		);
	}

	#[test]
	fn a_busy_conversation_is_capped() {
		let mut plugin = AutoChannelReact::default();
		plugin.configure(&Values(
			[("rules".to_string(), serde_json::json!("7 👀"))]
				.into_iter()
				.collect(),
		));
		let mut replies = Vec::new();
		for id in 0..(MAX_PER_RULE as u64 + 5) {
			plugin.on_created(&inbound(1), &message(id, 2, "hello"), &mut replies);
		}
		let context = IntentContext {
			channel: model::Id(7),
			me: model::Id(1),
			previous: None,
		};
		let mut count = 0;
		while plugin.take_intent(&context).is_some() {
			count += 1;
		}
		assert_eq!(count, MAX_PER_RULE);
	}

	#[test]
	fn your_own_message_is_not_reacted_to() {
		let mut plugin = AutoChannelReact::default();
		plugin.configure(&Values(
			[("rules".to_string(), serde_json::json!("7 👀"))]
				.into_iter()
				.collect(),
		));
		let mut replies = Vec::new();
		plugin.on_created(&inbound(1), &message(5, 1, "mine"), &mut replies);
		assert!(
			plugin
				.take_intent(&IntentContext {
					channel: model::Id(7),
					me: model::Id(1),
					previous: None,
				})
				.is_none()
		);
	}

	#[test]
	fn a_reaction_may_not_bring_whitespace() {
		let rules = parse_rules("7 a\u{2003}b");
		assert_eq!(rules.len(), 1);
		assert!(
			rules[0]
				.reactions
				.iter()
				.all(|emoji| { !emoji.chars().any(char::is_whitespace) }),
			"{:?}",
			rules[0].reactions
		);
	}

	#[test]
	fn the_registry_hands_out_one_intent_at_a_time() {
		let mut registry = Registry::new();
		registry.set_enabled("QuickDelete", true);
		assert!(
			registry
				.take_intent(&IntentContext {
					channel: model::Id(7),
					me: model::Id(1),
					previous: None,
				})
				.is_none()
		);

		let mut replies: Vec<PendingReply> = Vec::new();
		let _ = registry.observe(
			&inbound(1),
			crate::InboundEvent::Created(&message(5, 1, "mine")),
		);
		let _ = &mut replies;
		let previous = crate::Previous {
			id: model::Id(5),
			author: model::Id(1),
			content: "mine".into(),
			attachments: 0,
			age_ms: 100,
			is_group: false,
			replying: false,
		};
		assert_eq!(
			registry.take_intent(&IntentContext {
				channel: model::Id(7),
				me: model::Id(1),
				previous: Some(&previous),
			}),
			Some(Intent::Delete {
				channel: model::Id(7),
				message: model::Id(5)
			})
		);
	}
}
