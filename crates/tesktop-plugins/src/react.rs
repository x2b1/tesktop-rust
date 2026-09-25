//! Ports that react to what a message says and answer with something local.

use crate::{Fallback, Meta, PendingReply, Setting, SettingKind, Values, flag_or, text_or};
use model::Id;
use regex::Regex;

const HOP_ON_SETTINGS: &[Setting] = &[
	Setting {
		key: "regex",
		label: "Pattern that triggers it",
		kind: SettingKind::Text { multiline: false },
		default: Fallback::Text("hop on (?:fortnite|fn)"),
	},
	Setting {
		key: "url",
		label: "Address to open",
		kind: SettingKind::Text { multiline: false },
		default: Fallback::Text(
			"com.epicgames.launcher://apps/fn%3A4fe75bbc5a674f4f9b356b5c90567da5%3AFortnite?action=launch&silent=true",
		),
	},
];

/// HopOn: a message that matches your pattern opens an address you choose.
///
/// The app owns opening addresses, so the port reports the address rather than opening it
/// itself; a launcher scheme is not a web link and never belongs in a browser call.
#[derive(Default)]
pub struct HopOn {
	pattern: Option<String>,
	url: Option<String>,
	regex: Option<Regex>,
	/// Whether a hop has already been offered, so one run is one hop.
	hopped: bool,
	/// The address waiting for the host to take it.
	pending: Option<String>,
}

impl crate::Plugin for HopOn {
	fn meta(&self) -> Meta {
		Meta {
			id: "HopOn",
			name: "HopOn",
			description: "Opens a configurable address when a message matches your pattern.",
			authors: "Vencord",
			tags: &["Chat", "Utility"],
			aliases: &["hopOn"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		HOP_ON_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.pattern = Some(text_or(values, HOP_ON_SETTINGS, "regex"));
		self.url = Some(text_or(values, HOP_ON_SETTINGS, "url"));
		self.regex = Regex::new(&text_or(values, HOP_ON_SETTINGS, "regex")).ok();
		self.hopped = false;
		self.pending = None;
	}

	fn reset(&mut self) {
		self.hopped = false;
		self.pending = None;
	}

	fn on_created(
		&mut self,
		_inbound: &crate::Inbound,
		message: &model::Message,
		_replies: &mut Vec<PendingReply>,
	) {
		if self.hopped {
			// One hop per plugin run, the way a launcher should be.
			return;
		}
		let Some(url) = self.url.clone() else {
			return;
		};
		if url.is_empty() || url.len() > 2048 {
			return;
		}
		if self
			.regex
			.as_ref()
			.is_some_and(|regex| regex.is_match(&message.content))
		{
			self.hopped = true;
			self.pending = Some(url);
		}
	}

	fn take_url(&mut self) -> Option<String> {
		self.pending.take()
	}

	fn summary(&self) -> Option<String> {
		match self.regex.as_ref() {
			Some(_) => self.url.clone(),
			// An unusable pattern is reported rather than silently never matching.
			None if self
				.pattern
				.as_ref()
				.is_some_and(|pattern| !pattern.is_empty()) =>
			{
				Some("Pattern does not compile".to_string())
			}
			_ => Some("No pattern set".to_string()),
		}
	}
}

/// AskMeToMute: nothing here mutes you, because the app owns the mute state. The port keeps
/// the reminder TestCord's plugin exists for, without pretending to toggle the service.
pub struct AskMeToMute;

impl crate::Plugin for AskMeToMute {
	fn meta(&self) -> Meta {
		Meta {
			id: "AskMeToMute",
			name: "AskMeToMute",
			description: "Reminds you when a moderator mutes you in a server.",
			authors: "Vencord",
			tags: &["Servers", "Notifications"],
			aliases: &["askMeToMute"],
			default_enabled: false,
		}
	}

	fn message_actions(&self) -> Vec<crate::MessageAction> {
		vec![crate::MessageAction {
			id: "mute-reminder",
			label: "Remind me when a moderator mutes me",
		}]
	}

	fn run_action(&self, action: &str, _message: &model::Message) -> Option<crate::ActionResult> {
		(action == "mute-reminder").then_some(crate::ActionResult::Notice(
			"Mute reminders are handled by the app itself".to_string(),
		))
	}
}

const REMEMBER_SETTINGS: &[Setting] = &[
	Setting {
		key: "rememberServers",
		label: "Also remember people from servers",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(true),
	},
	Setting {
		key: "maxEntries",
		label: "People remembered",
		kind: SettingKind::Number {
			min: 50,
			max: 20_000,
		},
		default: Fallback::Number(5_000),
	},
];

/// IRememberYou: keeps who you have talked to, so a lost account list is recoverable.
#[derive(Default)]
pub struct IRememberYou {
	seen: std::cell::RefCell<std::collections::BTreeMap<Id, u64>>,
	remember_servers: bool,
	max: usize,
}

impl IRememberYou {
	pub fn known(&self) -> usize {
		self.seen.borrow().len()
	}

	/// Everything remembered, oldest first, for the app to show or export.
	pub fn export(&self) -> Vec<(Id, u64)> {
		let seen = self.seen.borrow();
		let mut entries: Vec<(Id, u64)> = seen.iter().map(|(id, at)| (*id, *at)).collect();
		entries.sort_by_key(|(_, at)| *at);
		entries
	}
}

impl crate::Plugin for IRememberYou {
	fn meta(&self) -> Meta {
		Meta {
			id: "IRememberYou",
			name: "IRememberYou",
			description: "Remembers everyone you have talked to, in case you lose the list.",
			authors: "Equicord",
			tags: &["Friends", "Utility"],
			aliases: &["iRememberYou"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		REMEMBER_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.remember_servers = flag_or(values, REMEMBER_SETTINGS, "rememberServers");
		self.max =
			crate::number_or(values, REMEMBER_SETTINGS, "maxEntries").clamp(50, 20_000) as usize;
	}

	fn reset(&mut self) {
		self.seen.borrow_mut().clear();
	}

	fn on_created(
		&mut self,
		inbound: &crate::Inbound,
		message: &model::Message,
		_replies: &mut Vec<PendingReply>,
	) {
		if !self.remember_servers && inbound.guild.is_some() {
			return;
		}
		if message.author.id == inbound.me {
			return;
		}
		let mut seen = self.seen.borrow_mut();
		seen.insert(message.author.id, inbound.now);
		while seen.len() > self.max {
			if let Some(oldest) = seen.iter().min_by_key(|(_, at)| **at).map(|(id, _)| *id) {
				seen.remove(&oldest);
			} else {
				break;
			}
		}
	}

	fn summary(&self) -> Option<String> {
		Some(format!("{} people remembered", self.known()))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::Plugin;

	fn message(content: &str) -> model::Message {
		let mut message = test_support::message(1, model::Id(7));
		message.content = content.to_string();
		message
	}

	fn inbound(now: u64) -> crate::Inbound {
		crate::Inbound::new(model::Id(7), Some(model::Id(10)), model::Id(1), now)
	}

	#[test]
	fn a_matching_message_hops_once() {
		let mut plugin = HopOn::default();
		plugin.configure(&Values(
			[(
				"regex".to_string(),
				serde_json::json!("hop on (?:fortnite|fn)"),
			)]
			.into_iter()
			.collect(),
		));
		let mut replies = Vec::new();
		plugin.on_created(&inbound(0), &message("hop on fn"), &mut replies);
		assert_eq!(
			plugin.take_url().as_deref(),
			Some(
				"com.epicgames.launcher://apps/fn%3A4fe75bbc5a674f4f9b356b5c90567da5%3AFortnite?action=launch&silent=true"
			)
		);
		assert!(plugin.summary().unwrap().contains("epicgames"));
		// A second match does not reopen the launcher.
		plugin.on_created(&inbound(1), &message("hop on fn"), &mut replies);
		assert!(plugin.take_url().is_none());
	}

	#[test]
	fn a_message_that_does_not_match_leaves_it_shut() {
		let mut plugin = HopOn::default();
		let mut replies = Vec::new();
		plugin.on_created(&inbound(0), &message("nothing here"), &mut replies);
		assert!(!plugin.hopped);
		assert!(plugin.take_url().is_none());
	}

	#[test]
	fn a_pattern_that_does_not_compile_is_reported() {
		let mut plugin = HopOn::default();
		plugin.configure(&Values(
			[("regex".to_string(), serde_json::json!("("))]
				.into_iter()
				.collect(),
		));
		assert_eq!(
			plugin.summary().as_deref(),
			Some("Pattern does not compile")
		);
	}

	#[test]
	fn remembered_people_are_bounded_and_oldest_goes() {
		let mut plugin = IRememberYou::default();
		plugin.configure(&Values(
			[("maxEntries".to_string(), serde_json::json!(50))]
				.into_iter()
				.collect(),
		));
		for id in 1..200u64 {
			let mut message = message("hi");
			message.author.id = model::Id(id);
			let mut replies = Vec::new();
			plugin.on_created(&inbound(id), &message, &mut replies);
		}
		assert_eq!(plugin.known(), 50);
		let exported = plugin.export();
		assert_eq!(exported.len(), 50);
		assert!(
			exported.windows(2).all(|pair| pair[0].1 <= pair[1].1),
			"the export is oldest first"
		);
	}

	#[test]
	fn a_server_can_be_left_out_and_yourself_never_remembered() {
		let mut plugin = IRememberYou::default();
		plugin.configure(&Values(
			[("rememberServers".to_string(), serde_json::json!(false))]
				.into_iter()
				.collect(),
		));
		let mut replies = Vec::new();
		let mut other = message("hi");
		other.author.id = model::Id(5);
		plugin.on_created(&inbound(1), &other, &mut replies);
		assert_eq!(plugin.known(), 0);
		let direct = crate::Inbound::new(model::Id(7), None, model::Id(1), 2);
		plugin.on_created(&direct, &other, &mut replies);
		assert_eq!(plugin.known(), 1);
		let mut mine = message("hi");
		mine.author.id = model::Id(1);
		plugin.on_created(&direct, &mine, &mut replies);
		assert_eq!(plugin.known(), 1);
	}

	#[test]
	fn the_mute_reminder_is_offered_and_explains_itself() {
		let plugin = AskMeToMute;
		assert_eq!(plugin.message_actions().len(), 1);
		assert_eq!(
			plugin.run_action("mute-reminder", &message("hi")),
			Some(crate::ActionResult::Notice(
				"Mute reminders are handled by the app itself".to_string()
			))
		);
		assert!(plugin.run_action("other", &message("hi")).is_none());
	}
}
