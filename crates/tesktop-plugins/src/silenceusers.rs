//! SilenceUsers: takes the pings out of messages by people you muted.
//!
//! TestCord also drops the notification for those messages. Notifications here are raised by the
//! state owner from the message it accepted, so that part is not ported yet; the pings are.

use crate::{Fallback, Meta, Setting, SettingKind, Values, text_or};
use model::{Id, Message};

const SETTINGS: &[Setting] = &[Setting {
	key: "mutedUserIds",
	label: "Muted user ids",
	kind: SettingKind::Text { multiline: true },
	default: Fallback::Text(""),
}];

/// Ids a single message may carry, matching the model's own ceiling.
const MAX_TRACKED: usize = 256;

#[derive(Default)]
pub struct SilenceUsers {
	muted: Vec<Id>,
}

impl SilenceUsers {
	fn is_muted(&self, author: Id) -> bool {
		self.muted.contains(&author)
	}
}

impl crate::Plugin for SilenceUsers {
	fn meta(&self) -> Meta {
		Meta {
			id: "SilenceUsers",
			name: "SilenceUsers",
			description: "Takes the @mentions out of messages by the users you list.",
			authors: "dka",
			tags: &["Notifications", "Chat"],
			aliases: &["silenceUsers"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.muted = ids(&text_or(values, SETTINGS, "mutedUserIds"));
	}

	fn mutate_incoming(&mut self, message: &mut Message) {
		if !self.is_muted(message.author.id) {
			return;
		}
		message.mention_everyone = false;
		message.mention_roles.clear();
		message.mentions.clear();
	}

	fn summary(&self) -> Option<String> {
		Some(format!(
			"{} user{} silenced",
			self.muted.len(),
			if self.muted.len() == 1 { "" } else { "s" }
		))
	}
}

fn ids(input: &str) -> Vec<Id> {
	use std::str::FromStr;
	let mut ids: Vec<Id> = input
		.split([' ', ',', '\n', '\t', '\r'])
		.filter_map(|part| Id::from_str(part.trim()).ok())
		.collect();
	ids.sort_unstable();
	ids.dedup();
	ids.truncate(MAX_TRACKED);
	ids
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::Plugin;

	fn plugin(list: &str) -> SilenceUsers {
		let mut plugin = SilenceUsers::default();
		plugin.configure(&Values(
			[("mutedUserIds".to_string(), serde_json::json!(list))]
				.into_iter()
				.collect(),
		));
		plugin
	}

	fn pinged(author: u64) -> Message {
		let mut message = test_support::message(1, Id(7));
		message.author.id = Id(author);
		message.mention_everyone = true;
		message.mention_roles = vec![Id(10)];
		message.mentions = vec![message.author.clone()];
		message
	}

	#[test]
	fn a_muted_author_loses_every_ping() {
		let mut plugin = plugin("2, 3");
		let mut message = pinged(2);
		plugin.mutate_incoming(&mut message);
		assert!(!message.mention_everyone);
		assert!(message.mention_roles.is_empty());
		assert!(message.mentions.is_empty());
	}

	#[test]
	fn everyone_else_keeps_their_pings() {
		let mut plugin = plugin("2, 3");
		let mut message = pinged(9);
		plugin.mutate_incoming(&mut message);
		assert!(message.mention_everyone);
		assert_eq!(message.mention_roles, vec![Id(10)]);
		assert_eq!(message.mentions.len(), 1);
	}

	#[test]
	fn the_body_is_never_touched() {
		let mut plugin = plugin("2");
		let mut message = pinged(2);
		message.content = "@here look at this".into();
		let before = message.content.clone();
		plugin.mutate_incoming(&mut message);
		assert_eq!(message.content, before);
	}

	#[test]
	fn an_empty_list_silences_nobody() {
		let mut plugin = plugin("");
		let mut message = pinged(2);
		plugin.mutate_incoming(&mut message);
		assert!(message.mention_everyone);
		assert_eq!(plugin.summary().as_deref(), Some("0 users silenced"));
	}
}
