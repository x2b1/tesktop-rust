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
/// Ids remembered as recorded, so the echo of your own send is not logged twice.
const MAX_SEEN: usize = 4096;
/// Bytes handed to the clipboard in one export.
pub const MAX_EXPORT_BYTES: usize = 256 * 1024;
/// Files named on a deletion before the list is cut, so one message cannot fill the record.
const MAX_LISTED_FILES: usize = 10;

/// The changed lines of an edit, as text: what went and what replaced it.
///
/// A removal is always marked. An addition is marked too when `separated` is on, which is the
/// original's more readable differential; with it off an addition is just the new line, so a
/// record reads as the message with its old lines taken out.
fn diff_text(before: &str, after: &str, separated: bool) -> String {
	let mut out = String::new();
	let mut line = |marker: &str, text: &str| {
		out.push_str(marker);
		out.push_str(text);
		out.push('\n');
	};
	for gone in before.lines().filter(|line| !after.contains(line)) {
		line("- ", gone);
	}
	for added in after.lines().filter(|line| !before.contains(line)) {
		line(if separated { "+ " } else { "" }, added);
	}
	let trimmed = out.trim_end_matches('\n');
	if trimmed.is_empty() {
		// Nothing line-shaped differs, so the whole change is one line of both.
		format!("- {before}\n+ {after}")
	} else {
		trimmed.to_string()
	}
}

/// The first line of `text`, which is what a collapsed record keeps.
fn first_line(text: &str) -> String {
	let line = text.lines().next().unwrap_or_default().trim();
	line.chars().take(MAX_BODY_CHARS).collect()
}

const SETTINGS: &[Setting] = &[
	Setting {
		key: "deleteStyle",
		label: "How a deletion is shown",
		kind: SettingKind::Choice(&[("text", "Red text"), ("overlay", "Red overlay")]),
		default: Fallback::Text("text"),
	},
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
		key: "logDeletedAttachments",
		label: "Record the files on a message that is deleted",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(true),
	},
	Setting {
		key: "ignoreSelfEdits",
		label: "Ignore my own edits",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
	Setting {
		key: "collapseDeleted",
		label: "Collapse a deleted message to a single line",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
	Setting {
		key: "showEditDiffs",
		label: "Show what an edit changed",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
	Setting {
		key: "separatedDiffs",
		label: "Separate additions from removals in a diff",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
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
	log_deleted_attachments: bool,
	ignore_self_edits: bool,
	collapse_deleted: bool,
	show_edit_diffs: bool,
	separated_diffs: bool,
	log_deletes: bool,
	ignore_bots: bool,
	ignore_self: bool,
	ignore_users: Vec<Id>,
	ignore_channels: Vec<Id>,
	ignore_guilds: Vec<Id>,
	/// Ids already recorded, so a message that arrives twice is logged once.
	seen: std::collections::BTreeSet<Id>,
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

	/// One record: a header line naming what happened and to which message, then the text
	/// as it was. The trailing newline is what keeps two records from running into one
	/// another, and a message that ends in a newline of its own does not get two.
	fn text(&self, entry: &Entry) -> String {
		let label = match entry.kind {
			Kind::Created => "message",
			Kind::Edited => "edit",
			Kind::Deleted => "deleted",
		};
		let text = format!(
			"[{}] {} in {}: {}\n{}",
			label, entry.author, entry.channel, entry.id, entry.content
		);
		if text.ends_with('\n') {
			text
		} else {
			format!("{text}\n")
		}
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
		self.log_deleted_attachments = flag_or(values, SETTINGS, "logDeletedAttachments");
		self.ignore_self_edits = flag_or(values, SETTINGS, "ignoreSelfEdits");
		self.collapse_deleted = flag_or(values, SETTINGS, "collapseDeleted");
		self.show_edit_diffs = flag_or(values, SETTINGS, "showEditDiffs");
		self.separated_diffs = flag_or(values, SETTINGS, "separatedDiffs");
		self.ignore_bots = flag_or(values, SETTINGS, "ignoreBots");
		self.ignore_self = flag_or(values, SETTINGS, "ignoreSelf");
		self.ignore_users = ids(&text_or(values, SETTINGS, "ignoreUsers"));
		self.ignore_channels = ids(&text_or(values, SETTINGS, "ignoreChannels"));
		self.ignore_guilds = ids(&text_or(values, SETTINGS, "ignoreGuilds"));
	}

	fn reset(&mut self) {
		self.seen.clear();
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

	fn delivered(&mut self, event: &crate::Delivery<'_>) {
		// The service usually echoes your own message back, and then the created hook has
		// it already. Where it does not, this is the only place the record can come from.
		let crate::Delivery::Sent {
			channel,
			message,
			me,
		} = event
		else {
			return;
		};
		if self.seen.contains(&message.id) {
			return;
		}
		if self.ignores_author(&message.author, *me) {
			return;
		}
		self.seen.insert(message.id);
		while self.seen.len() > MAX_SEEN {
			if let Some(oldest) = self.seen.iter().next().copied() {
				self.seen.remove(&oldest);
			} else {
				break;
			}
		}
		self.record(
			Kind::Created,
			*channel,
			message.id,
			&message.author.name,
			message.content.clone(),
		);
	}

	fn on_edited(&mut self, inbound: &Inbound, edit: &crate::Edit<'_>) {
		if !self.log_edits || edit.before == edit.after {
			return;
		}
		if self.ignore_self_edits && edit.author.id == inbound.me {
			return;
		}
		if self.ignores_author(edit.author, inbound.me)
			|| self.ignore_channels.contains(&edit.channel)
		{
			return;
		}
		// What the original shows as a diff, the record keeps as text: the line before and the
		// line after, with the removals and the additions marked so a diff can be read in a
		// plain record. Without the setting it is just the new text, as before.
		let content = if self.show_edit_diffs {
			diff_text(edit.before, edit.after, self.separated_diffs)
		} else {
			edit.after.to_string()
		};
		self.record(
			Kind::Edited,
			edit.channel,
			edit.id,
			&edit.author.name,
			content,
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
		let mut content = last.content.clone();
		if self.log_deleted_attachments && !last.attachments.is_empty() {
			let files: Vec<&str> = last
				.attachments
				.iter()
				.map(|attachment| attachment.filename.as_str())
				.take(MAX_LISTED_FILES)
				.collect();
			content.push('\n');
			content.push_str(&files.join(", "));
		}
		// A collapsed deletion is one line, the way a blocked message is, so the record stays
		// readable when something is deleted often.
		if self.collapse_deleted {
			content = first_line(&content);
		}
		self.record(Kind::Deleted, channel, id, &last.author.name, content);
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

#[cfg(test)]
mod delivery_tests {
	use super::*;
	use crate::{Delivery, Plugin, Registry};

	fn mine(id: u64, content: &str) -> model::Message {
		let mut message = test_support::message(id, model::Id(7));
		message.author.id = model::Id(1);
		message.content = content.to_string();
		message
	}

	#[test]
	fn a_send_is_recorded_even_without_an_echo() {
		let mut plugin = MessageLogger::default();
		let message = mine(1, "mine");
		plugin.delivered(&Delivery::Sent {
			channel: model::Id(7),
			message: &message,
			me: model::Id(1),
		});
		let log = plugin.export().expect("a log");
		assert!(log.contains("mine"), "{log}");
	}

	#[test]
	fn the_echo_of_a_send_is_not_recorded_twice() {
		let mut plugin = MessageLogger::default();
		let message = mine(1, "mine");
		let event = Delivery::Sent {
			channel: model::Id(7),
			message: &message,
			me: model::Id(1),
		};
		plugin.delivered(&event);
		plugin.delivered(&event);
		let log = plugin.export().expect("a log");
		assert_eq!(log.matches("mine").count(), 1, "{log}");
	}

	#[test]
	fn a_failure_is_not_a_record() {
		let mut plugin = MessageLogger::default();
		plugin.delivered(&Delivery::Failed {
			channel: model::Id(7),
			me: model::Id(1),
			content: "never sent",
			failure: "Connection failed",
		});
		assert!(plugin.export().unwrap().contains("Nothing logged"));
	}

	#[test]
	fn the_registry_hands_back_the_tail_the_page_shows() {
		let mut registry = Registry::new();
		registry.set_enabled("MessageLogger", true);
		for id in 1..5 {
			let message = mine(id, &format!("line {id}"));
			registry.delivered(&Delivery::Sent {
				channel: model::Id(7),
				message: &message,
				me: model::Id(1),
			});
		}
		let tail = registry.tail("MessageLogger", 2);
		assert_eq!(tail.lines().count(), 2);
		assert!(tail.contains("line 4"), "{tail}");
		assert!(!tail.contains("line 1"), "{tail}");
	}

	/// Two records must not run into one another: every header starts a line of its own,
	/// or the text of one message ends up with the next one's header glued to it.
	/// The settings the original has and this had not: a deletion names the files that went
	/// with it, a collapsed one is a single line, your own edits can be left out, and an edit
	/// can be kept as what changed rather than as the new text.
	#[test]
	fn the_settings_the_original_has_change_what_is_recorded() {
		fn logger(settings: &[(&str, serde_json::Value)]) -> MessageLogger {
			let mut plugin = MessageLogger::default();
			plugin.configure(&crate::Values(
				settings
					.iter()
					.map(|(key, value)| ((*key).to_string(), value.clone()))
					.collect(),
			));
			plugin
		}
		let mut with_file = test_support::message(9, Id(7));
		with_file.attachments = vec![model::Attachment {
			id: Id(90),
			filename: "notes.txt".to_string(),
			description: None,
			content_type: None,
			size: 12,
			media: model::EmbedMedia::default(),
			spoiler: false,
			duration_ms: None,
			waveform: Vec::new(),
		}];
		let inbound = Inbound::new(Id(7), None, Id(1), 1_000);

		// A file that went with the message is named, because losing it is the point. The
		// original has this on by default, so the test turns it off to see the difference.
		let mut plain = logger(&[("logDeletedAttachments", serde_json::json!(false))]);
		plain.on_deleted(&inbound, Id(7), Id(9), Some(&with_file));
		let mut named = logger(&[("logDeletedAttachments", serde_json::json!(true))]);
		named.on_deleted(&inbound, Id(7), Id(9), Some(&with_file));
		assert!(
			!plain.export().unwrap().contains("notes.txt"),
			"with the setting off the files are not named"
		);
		assert!(
			named.export().unwrap().contains("notes.txt"),
			"a deletion that took a file has to say which: {:?}",
			named.export()
		);

		// A collapsed deletion is one line, however long the message was.
		let mut tall = logger(&[("collapseDeleted", serde_json::json!(true))]);
		tall.on_deleted(&inbound, Id(7), Id(9), Some(&with_file));
		let record = tall.export().unwrap();
		assert_eq!(
			record.lines().count(),
			2,
			"a header and one line: {record:?}"
		);

		// Your own edits can be left out.
		let mine = model::User {
			id: Id(1),
			name: "You".into(),
			..test_support::message(1, Id(7)).author
		};
		let edit = crate::Edit {
			channel: Id(7),
			id: Id(9),
			author: &mine,
			before: "one",
			after: "two",
		};
		let mut all = logger(&[]);
		all.on_edited(&inbound, &edit);
		let mut not_mine = logger(&[("ignoreSelfEdits", serde_json::json!(true))]);
		not_mine.on_edited(&inbound, &edit);
		assert_eq!(all.edited, 1);
		assert_eq!(not_mine.edited, 0, "your own edit was left out");

		// An edit kept as a diff says what changed.
		let mut diff = logger(&[("showEditDiffs", serde_json::json!(true))]);
		diff.on_edited(&inbound, &edit);
		let record = diff.export().unwrap();
		assert!(record.contains("- one"), "{record:?}");
		assert!(record.contains("two"), "{record:?}");

		// And the separated form marks the addition as well.
		let mut separated = logger(&[
			("showEditDiffs", serde_json::json!(true)),
			("separatedDiffs", serde_json::json!(true)),
		]);
		separated.on_edited(&inbound, &edit);
		assert!(
			separated.export().unwrap().contains("+ two"),
			"a separated diff marks what was added: {:?}",
			separated.export()
		);
	}

	#[test]
	fn one_record_never_runs_into_the_next() {
		let mut registry = Registry::new();
		registry.set_enabled("MessageLogger", true);
		for id in 1..5 {
			let message = mine(id, &format!("line {id}"));
			registry.delivered(&Delivery::Sent {
				channel: model::Id(7),
				message: &message,
				me: model::Id(1),
			});
		}
		let export = registry.export("MessageLogger").unwrap();
		for line in export.lines() {
			for header in ["[message]", "[edit]", "[deleted]"] {
				assert!(
					line.starts_with(header) || !line.contains(header),
					"a header glued to the end of a record: {line:?}"
				);
			}
		}
		// A message that ends in a newline of its own must not leave a blank line behind.
		let mut registry = Registry::new();
		registry.set_enabled("MessageLogger", true);
		for id in 5..7 {
			let message = mine(id, "two lines\nand a blank one\n");
			registry.delivered(&Delivery::Sent {
				channel: model::Id(7),
				message: &message,
				me: model::Id(1),
			});
		}
		let export = registry.export("MessageLogger").unwrap();
		assert!(
			!export.contains("\n\n"),
			"a record's own trailing newline must not be doubled: {export:?}"
		);
	}
}
