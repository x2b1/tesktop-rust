//! Message visibility ports: what you hide, and what stays after a delete.

use crate::{Fallback, Meta, Setting, SettingKind, Values, flag_or, number_or, text_or};
use model::Id;
use std::str::FromStr;

/// Message ids a session can hide, bounded so a long session cannot grow without limit.
pub const MAX_HIDDEN: usize = 2048;

const HIDE_SETTINGS: &[Setting] = &[Setting {
	key: "showInMenu",
	label: "Show the hide entry in the message menu",
	kind: SettingKind::Toggle,
	default: Fallback::Flag(true),
}];

/// HideMessages: hide a message you regret, and keep it hidden until the app restarts.
#[derive(Default)]
pub struct HideMessages {
	hidden: std::cell::RefCell<std::collections::BTreeSet<Id>>,
	show_in_menu: Option<bool>,
}

impl HideMessages {
	/// A hidden message stays hidden while the app runs, which is what TestCord does too.
	pub fn hide(&self, id: Id) {
		let mut hidden = self.hidden.borrow_mut();
		while hidden.len() >= MAX_HIDDEN {
			if let Some(oldest) = hidden.iter().next().copied() {
				hidden.remove(&oldest);
			} else {
				break;
			}
		}
		hidden.insert(id);
	}

	pub fn is_hidden(&self, id: Id) -> bool {
		self.hidden.borrow().contains(&id)
	}
}

impl crate::Plugin for HideMessages {
	fn meta(&self) -> Meta {
		Meta {
			id: "HideMessages",
			name: "HideMessages",
			description: "Hides a message from the conversation until you restart.",
			authors: "Vencord",
			tags: &["Utility", "Appearance"],
			aliases: &["hideMessages"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		HIDE_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.show_in_menu = Some(flag_or(values, HIDE_SETTINGS, "showInMenu"));
	}

	fn reset(&mut self) {
		self.hidden.borrow_mut().clear();
	}

	fn ignore(&self, message: &model::Message) -> bool {
		self.is_hidden(message.id)
	}

	fn message_actions(&self) -> &'static [crate::MessageAction] {
		&[crate::MessageAction {
			id: "hide",
			label: "Hide this message",
		}]
	}

	fn run_action(&self, action: &str, message: &model::Message) -> Option<crate::ActionResult> {
		if action != "hide" || !self.show_in_menu.unwrap_or(true) {
			return None;
		}
		self.hide(message.id);
		Some(crate::ActionResult::Notice(
			"Message hidden until you restart",
		))
	}

	fn summary(&self) -> Option<String> {
		let hidden = self.hidden.borrow().len();
		Some(format!("{hidden} hidden"))
	}
}

const ANTI_DELETE_SETTINGS: &[Setting] = &[
	Setting {
		key: "dmProtection",
		label: "Also protect direct messages",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
	Setting {
		key: "serverBlacklist",
		label: "Servers where deletes are not protected",
		kind: SettingKind::Text { multiline: true },
		default: Fallback::Text(""),
	},
	Setting {
		key: "maxCacheSize",
		label: "Messages kept for recovery",
		kind: SettingKind::Number {
			min: 50,
			max: 2_000,
		},
		default: Fallback::Number(500),
	},
];

/// AntiDeleteMessage: keep the body of a deleted message so it can still be read.
#[derive(Default)]
pub struct AntiDeleteMessage {
	protected: Option<bool>,
	dm_protection: bool,
	blacklist: Vec<Id>,
	cache: u32,
}

impl crate::Plugin for AntiDeleteMessage {
	fn meta(&self) -> Meta {
		Meta {
			id: "AntiDeleteMessage",
			name: "AntiDeleteMessage",
			description: "Keeps the body of a deleted message so you can still read it.",
			authors: "Vencord",
			tags: &["Utility", "Appearance"],
			aliases: &["antiDeleteMessage"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		ANTI_DELETE_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.protected = Some(true);
		self.dm_protection = flag_or(values, ANTI_DELETE_SETTINGS, "dmProtection");
		self.blacklist = text_or(values, ANTI_DELETE_SETTINGS, "serverBlacklist")
			.split([' ', ',', '\n', '\t', '\r'])
			.filter_map(|part| Id::from_str(part.trim()).ok())
			.take(256)
			.collect();
		self.cache =
			(number_or(values, ANTI_DELETE_SETTINGS, "maxCacheSize").clamp(50, 2_000)) as u32;
	}

	fn display(&self) -> crate::display::DisplayPatch {
		crate::display::DisplayPatch {
			preserve_deleted: self
				.protected
				.map(|protected| protected && !self.dm_protection && self.blacklist.is_empty()),
			..crate::display::DisplayPatch::default()
		}
	}

	fn summary(&self) -> Option<String> {
		let mut summary = format!("{} kept for recovery", self.cache);
		if self.dm_protection {
			summary.push_str(", direct messages included");
		}
		if !self.blacklist.is_empty() {
			summary.push_str(&format!(", {} servers exempt", self.blacklist.len()));
		}
		Some(summary)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::Plugin;

	fn message(id: u64) -> model::Message {
		test_support::message(id, Id(7))
	}

	#[test]
	fn a_hidden_message_stays_hidden_and_can_be_released() {
		let hide = HideMessages::default();
		assert!(!hide.ignore(&message(1)));
		assert_eq!(
			hide.run_action("hide", &message(1)),
			Some(crate::ActionResult::Notice(
				"Message hidden until you restart"
			))
		);
		assert!(hide.ignore(&message(1)));
		assert!(!hide.ignore(&message(2)));
	}

	#[test]
	fn the_hidden_set_is_bounded() {
		let hide = HideMessages::default();
		for id in 0..(MAX_HIDDEN as u64 + 100) {
			hide.hide(Id(id));
		}
		assert_eq!(
			hide.summary().as_deref(),
			Some(format!("{MAX_HIDDEN} hidden").as_str())
		);
	}

	#[test]
	fn the_hide_entry_can_be_turned_off() {
		let mut hide = HideMessages::default();
		hide.configure(&Values(
			[("showInMenu".to_string(), serde_json::json!(false))]
				.into_iter()
				.collect(),
		));
		assert!(hide.run_action("hide", &message(1)).is_none());
		assert!(!hide.ignore(&message(1)));
	}

	#[test]
	fn disabled_again_the_hidden_set_is_forgotten() {
		let hide = HideMessages::default();
		hide.run_action("hide", &message(1));
		assert!(hide.ignore(&message(1)));
		let mut registry = crate::Registry::new();
		registry.set_enabled("HideMessages", true);
		registry.set_enabled("HideMessages", false);
		assert!(!registry.any_enabled());
	}

	#[test]
	fn deleted_bodies_are_kept_when_asked() {
		let mut registry = crate::Registry::new();
		assert!(!registry.display().preserve_deleted);
		registry.set_enabled("AntiDeleteMessage", true);
		assert!(registry.display().preserve_deleted);
	}

	#[test]
	fn a_dm_or_blacklist_exemption_turns_protection_off() {
		let mut registry = crate::Registry::new();
		registry.set_enabled("AntiDeleteMessage", true);
		registry.set_value("AntiDeleteMessage", "dmProtection", true.into());
		assert!(
			!registry.display().preserve_deleted,
			"the app protects direct messages on its own, so the port stands aside"
		);
		registry.set_value("AntiDeleteMessage", "dmProtection", false.into());
		registry.set_value("AntiDeleteMessage", "serverBlacklist", "10".into());
		assert!(!registry.display().preserve_deleted);
	}

	#[test]
	fn the_recovery_cache_never_exceeds_the_client_ceiling() {
		let mut registry = crate::Registry::new();
		registry.set_enabled("AntiDeleteMessage", true);
		registry.set_value("AntiDeleteMessage", "maxCacheSize", 99_999.into());
		assert_eq!(
			registry.summary("AntiDeleteMessage").as_deref(),
			Some("2000 kept for recovery")
		);
	}
}
