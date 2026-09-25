//! MessageLogger: keeps a bounded record of what happened to messages, with edits and deletions.
//!
//! TestCord's logger also draws a searchable history window and edit diffs. This port keeps the
//! record and exports it as text; the app's own message cache and search stay the source of truth
//! for what you can browse.

use crate::{Fallback, Inbound, Meta, Setting, SettingKind, Values, flag_or, text_or};
use model::{Id, Message};
use std::collections::VecDeque;
use std::str::FromStr;

/// Logged entries kept in memory.
pub const MAX_ENTRIES: usize = 2000;
/// Bytes kept across those entries.
pub const MAX_BYTES: usize = 4 * 1024 * 1024;
/// Body characters kept per entry; Discord's own ceiling.
const MAX_BODY_CHARS: usize = 2000;
/// Bytes handed to the clipboard in one export.
pub const MAX_EXPORT_BYTES: usize = 256 * 1024;

const SETTINGS: &[Setting] = &[
	Setting {
		key: "logEdits",
		label: "Record edits",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(true),
	},
	Setting {
		key: "logDeletes",
		label: "Record deletions",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(true),
	},
	Setting {
		key: "ignoreBots",
		label: "Ignore bot messages",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(true),
	},
	Setting {
		key: "ignoreSelf",
		label: "Ignore my messages",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
	Setting {
		key: "ignoreUsers",
		label: "Ignored user ids",
		kind: SettingKind::Text { multiline: true },
		default: Fallback::Text(""),
	},
	Setting {
		key: "ignoreChannels",
		label: "Ignored channel ids",
		kind: SettingKind::Text { multiline: true },
		default: Fallback::Text(""),
	},
	Setting {
		key: "ignoreGuilds",
		label: "Ignored server ids",
		kind: SettingKind::Text { multiline: true },
		default: Fallback::Text(""),
	},
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
	Created,
	Edited,
	Deleted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
	pub kind: Kind,
	pub channel: Id,
	pub id: Id,
	pub author: String,
	pub content: String,
}

#[derive(Default)]
pub struct MessageLogger {
	entries: VecDeque<Entry>,
	bytes: usize,
	deleted: usize,
	edited: usize,
	log_edits: bool,
	log_deletes: bool,
	ignore_bots: bool,
	ignore_self: bool,
	ignore_users: Vec<Id>,
	ignore_channels: Vec<Id>,
	ignore_guilds: Vec<Id>,
}

impl MessageLogger {
	fn record(&mut self, kind: Kind, channel: Id, id: Id, author: &str, content: String) {
		let content: String = content.chars().take(MAX_BODY_CHARS).collect();
		let entry = Entry {
			kind,
			channel,
			id,
			author: author.chars().take(64).collect(),
			content,
		};
		match kind {
			Kind::Edited => self.edited += 1,
			Kind::Deleted => self.deleted += 1,
			Kind::Created => {}
		}
		self.bytes += entry.content.len() + entry.author.len() + std::mem::size_of::<Entry>();
		self.entries.push_back(entry);
		while self.entries.len() > MAX_ENTRIES || self.bytes > MAX_BYTES {
			match self.entries.pop_front() {
				Some(dropped) => {
					self.bytes = self.bytes.saturating_sub(
						dropped.content.len() + dropped.author.len() + std::mem::size_of::<Entry>(),
					);
				}
				None => {
					self.bytes = 0;
					break;
				}
			}
		}
	}

	fn ignores_author(&self, author: &model::User, me: Id) -> bool {
		if self.ignore_self && author.id == me {
			return true;
		}
		(self.ignore_bots && (author.webhook || author.kind == model::AccountKind::Bot))
			|| self.ignore_users.contains(&author.id)
	}

	fn text(&self, entry: &Entry) -> String {
		let label = match entry.kind {
			Kind::Created => "message",
			Kind::Edited => "edit",
			Kind::Deleted => "deleted",
		};
		format!(
			"[{}] {} in {}: {}\n{}",
			label, entry.author, entry.channel, entry.id, entry.content
		)
	}
}

impl crate::Plugin for MessageLogger {
	fn meta(&self) -> Meta {
		Meta {
			id: "MessageLogger",
			name: "MessageLogger",
			description: "Records messages, edits and deletions from every channel you can read.",
			authors: "Vencord",
			tags: &["Utility", "Privacy"],
			aliases: &["messageLogger"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.log_edits = flag_or(values, SETTINGS, "logEdits");
		self.log_deletes = flag_or(values, SETTINGS, "logDeletes");
		self.ignore_bots = flag_or(values, SETTINGS, "ignoreBots");
		self.ignore_self = flag_or(values, SETTINGS, "ignoreSelf");
		self.ignore_users = ids(&text_or(values, SETTINGS, "ignoreUsers"));
		self.ignore_channels = ids(&text_or(values, SETTINGS, "ignoreChannels"));
		self.ignore_guilds = ids(&text_or(values, SETTINGS, "ignoreGuilds"));
	}

	fn reset(&mut self) {
		self.entries.clear();
		self.bytes = 0;
		self.edited = 0;
		self.deleted = 0;
	}

	fn on_created(
		&mut self,
		inbound: &Inbound,
		message: &Message,
		_replies: &mut Vec<crate::PendingReply>,
	) {
		if self.ignores_author(&message.author, inbound.me) {
			return;
		}
		if self.ignore_channels.contains(&inbound.channel)
			|| inbound
				.guild
				.is_some_and(|guild| self.ignore_guilds.contains(&guild))
		{
			return;
		}
		self.record(
			Kind::Created,
			inbound.channel,
			message.id,
			&message.author.name,
			message.content.clone(),
		);
	}

	fn on_edited(&mut self, inbound: &Inbound, edit: &crate::Edit<'_>) {
		if !self.log_edits || edit.before == edit.after {
			return;
		}
		if self.ignores_author(edit.author, inbound.me)
			|| self.ignore_channels.contains(&edit.channel)
		{
			return;
		}
		self.record(
			Kind::Edited,
			edit.channel,
			edit.id,
			&edit.author.name,
			edit.after.to_string(),
		);
	}

	fn on_deleted(&mut self, inbound: &Inbound, channel: Id, id: Id, last: Option<&Message>) {
		if !self.log_deletes {
			return;
		}
		if self.ignore_channels.contains(&channel) {
			return;
		}
		let Some(last) = last else {
			return;
		};
		if self.ignores_author(&last.author, inbound.me) {
			return;
		}
		self.record(
			Kind::Deleted,
			channel,
			id,
			&last.author.name,
			last.content.clone(),
		);
	}

	fn summary(&self) -> Option<String> {
		Some(format!(
			"{} logged · {} edits · {} deletions",
			self.entries.len(),
			self.edited,
			self.deleted
		))
	}

	fn export(&self) -> Option<String> {
		if self.entries.is_empty() {
			return Some("Nothing logged yet".to_string());
		}
		let mut out = String::new();
		for entry in &self.entries {
			let line = self.text(entry);
			if out.len() + line.len() > MAX_EXPORT_BYTES {
				out.push_str("Log truncated for export.\n");
				break;
			}
			out.push_str(&line);
		}
		Some(out)
	}
}

fn ids(input: &str) -> Vec<Id> {
	let mut ids: Vec<Id> = input
		.split([' ', ',', '\n', '\t', '\r'])
		.filter_map(|part| Id::from_str(part.trim()).ok())
		.collect();
	ids.sort_unstable();
	ids.dedup();
	ids.truncate(256);
	ids
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::Plugin;
	use model::AccountKind;

	fn configured(settings: &[(&str, serde_json::Value)]) -> MessageLogger {
		let mut plugin = MessageLogger::default();
		plugin.configure(&Values(
			settings
				.iter()
				.map(|(key, value)| ((*key).to_string(), value.clone()))
				.collect(),
		));
		plugin
	}

	fn inbound() -> Inbound {
		Inbound::new(Id(7), Some(Id(10)), Id(1), 0)
	}

	fn message(id: u64, author: Id, content: &str) -> Message {
		let mut message = test_support::message(id, Id(7));
		message.author.id = author;
		message.content = content.to_string();
		message
	}

	fn author(id: Id, bot: bool) -> model::User {
		let mut author = model::User {
			id,
			name: "Someone".into(),
			avatar: None,
			webhook: false,
			kind: AccountKind::Human,
			discriminator: 0,
			primary_guild: None,
		};
		if bot {
			author.kind = AccountKind::Bot;
		}
		author
	}

	#[test]
	fn messages_are_recorded_with_their_author() {
		let mut plugin = configured(&[]);
		let mut replies = Vec::new();
		plugin.on_created(&inbound(), &message(1, Id(2), "hello"), &mut replies);
		let log = plugin.export().unwrap();
		assert!(log.contains("hello"));
		assert!(log.contains("[message]"));
	}

	#[test]
	fn bots_and_ignored_channels_are_skipped() {
		let mut plugin = configured(&[("ignoreChannels", "7, 9".into())]);
		let mut replies = Vec::new();
		plugin.on_created(&inbound(), &message(1, Id(2), "hello"), &mut replies);
		assert!(plugin.entries.is_empty());

		let mut plugin = configured(&[("ignoreBots", false.into())]);
		let mut bot = message(2, Id(3), "from a bot");
		bot.author.kind = AccountKind::Bot;
		plugin.on_created(&inbound(), &bot, &mut replies);
		assert_eq!(plugin.entries.len(), 1);
	}

	#[test]
	fn edits_and_deletions_follow_their_settings() {
		let mut plugin = configured(&[]);
		let mut replies = Vec::new();
		let message = message(1, Id(2), "first");
		plugin.on_created(&inbound(), &message, &mut replies);
		let edit = crate::Edit {
			channel: Id(7),
			id: message.id,
			author: &message.author,
			before: "first",
			after: "second",
		};
		plugin.on_edited(&inbound(), &edit);
		plugin.on_deleted(&inbound(), Id(7), message.id, Some(&message));
		assert_eq!(
			plugin.summary().as_deref(),
			Some("3 logged · 1 edits · 1 deletions")
		);

		let mut quiet = configured(&[("logEdits", false.into()), ("logDeletes", false.into())]);
		quiet.on_edited(&inbound(), &edit);
		quiet.on_deleted(&inbound(), Id(7), message.id, Some(&message));
		assert_eq!(
			quiet.summary().as_deref(),
			Some("0 logged · 0 edits · 0 deletions")
		);
	}

	#[test]
	fn an_edit_that_changed_nothing_is_not_recorded() {
		let mut plugin = configured(&[]);
		let message = message(1, Id(2), "same");
		let edit = crate::Edit {
			channel: Id(7),
			id: message.id,
			author: &message.author,
			before: "same",
			after: "same",
		};
		plugin.on_edited(&inbound(), &edit);
		assert!(plugin.entries.is_empty());
	}

	#[test]
	fn a_deletion_without_a_known_body_is_skipped() {
		let mut plugin = configured(&[]);
		plugin.on_deleted(&inbound(), Id(7), Id(99), None);
		assert!(plugin.entries.is_empty());
	}

	#[test]
	fn the_log_and_the_export_stay_bounded() {
		let mut plugin = configured(&[]);
		let mut replies = Vec::new();
		for id in 0..(MAX_ENTRIES as u64 + 500) {
			let body = "y".repeat(MAX_BODY_CHARS);
			plugin.on_created(&inbound(), &message(id, Id(2), &body), &mut replies);
		}
		assert!(plugin.entries.len() <= MAX_ENTRIES);
		assert!(plugin.bytes <= MAX_BYTES);
		let exported = plugin.export().unwrap();
		assert!(exported.len() <= MAX_EXPORT_BYTES);
	}

	#[test]
	fn my_own_messages_are_recorded_unless_ignored() {
		let mut plugin = configured(&[]);
		let mut replies = Vec::new();
		plugin.on_created(&inbound(), &message(1, Id(1), "mine"), &mut replies);
		assert_eq!(plugin.entries.len(), 1);
		let mut plugin = configured(&[("ignoreSelf", true.into())]);
		plugin.on_created(&inbound(), &message(1, Id(1), "mine"), &mut replies);
		assert!(plugin.entries.is_empty());
	}

	#[test]
	fn bot_authors_are_recognised_through_the_shared_helper() {
		let plugin = configured(&[]);
		assert!(plugin.ignores_author(&author(Id(5), true), Id(1)));
		assert!(!plugin.ignores_author(&author(Id(5), false), Id(1)));
	}
}
