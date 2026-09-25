//! AutoDeleter: your own messages, taken back after the delay you set.

use crate::{
	Delivery, Fallback, Inbound, Intent, IntentContext, Meta, Setting, SettingKind, Values,
	number_or, text_or,
};
use model::Id;
use std::collections::VecDeque;
use std::str::FromStr;

/// Messages waiting to be taken back. A session does not keep more than this.
pub const MAX_PENDING: usize = 256;
/// The longest delay the setting can ask for, which is the original's own ceiling.
pub const MAX_DELAY_MS: u64 = 24 * 60 * 60 * 1000;

const SETTINGS: &[Setting] = &[
	Setting {
		key: "defaultDelay",
		label: "How long a message stays",
		kind: SettingKind::Number {
			min: 5,
			max: 86_400,
		},
		default: Fallback::Number(300),
	},
	Setting {
		key: "delayUnit",
		label: "That number is in",
		kind: SettingKind::Choice(&[
			("seconds", "Seconds"),
			("minutes", "Minutes"),
			("hours", "Hours"),
		]),
		default: Fallback::Text("seconds"),
	},
	Setting {
		key: "channels",
		label: "Only these conversations (ids; empty means all)",
		kind: SettingKind::Text { multiline: true },
		default: Fallback::Text(""),
	},
	Setting {
		key: "keep",
		label: "Never delete a message containing one of these",
		kind: SettingKind::Text { multiline: true },
		default: Fallback::Text(""),
	},
	Setting {
		key: "maxChars",
		label: "Only messages up to this many characters",
		kind: SettingKind::Number { min: 0, max: 4_000 },
		default: Fallback::Number(0),
	},
];

/// One message waiting to be taken back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Due {
	pub channel: Id,
	pub message: Id,
	pub at: u64,
}

/// AutoDeleter: a message of yours goes away on its own, which is a privacy switch.
#[derive(Default)]
pub struct AutoDeleter {
	delay_ms: u64,
	channels: Vec<Id>,
	keep: Vec<String>,
	max_chars: usize,
	pending: VecDeque<Due>,
	outstanding: std::collections::BTreeSet<Id>,
	/// The deletes waiting to be handed over, which is what the host carries out.
	ready: Vec<Intent>,
	/// Nothing is taken back until the delay is set, so a fresh install deletes nothing.
	/// The monotonic clock of the last frame, so a send answered between two frames is
	/// remembered from the frame it was answered in and not from a wall clock.
	last_tick: u64,
}

impl AutoDeleter {
	/// Whether a message of yours should be taken back at all.
	fn should_delete(&self, content: &str) -> bool {
		if self.delay_ms == 0 {
			return false;
		}
		if self.max_chars > 0 && content.chars().count() > self.max_chars {
			return false;
		}
		if self
			.keep
			.iter()
			.any(|word| !word.is_empty() && content.to_lowercase().contains(word))
		{
			return false;
		}
		true
	}

	/// Remember a message, dropping the oldest when the list is full.
	fn remember(&mut self, channel: Id, message: Id, now: u64) {
		while self.pending.len() >= MAX_PENDING {
			if let Some(oldest) = self.pending.pop_front() {
				self.outstanding.remove(&oldest.message);
			} else {
				break;
			}
		}
		self.outstanding.insert(message);
		self.pending.push_back(Due {
			channel,
			message,
			at: now.saturating_add(self.delay_ms),
		});
	}
}

impl crate::Plugin for AutoDeleter {
	fn meta(&self) -> Meta {
		Meta {
			id: "AutoDeleter",
			name: "AutoDeleter",
			description: "Deletes your own messages after the delay you set.",
			authors: "Vencord",
			tags: &["Utility", "Chat"],
			aliases: &["autoDeleter"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		let amount = number_or(values, SETTINGS, "defaultDelay").clamp(5, 86_400) as u64;
		let unit = match text_or(values, SETTINGS, "delayUnit").as_str() {
			"minutes" => 60_000,
			"hours" => 3_600_000,
			_ => 1_000,
		};
		self.delay_ms = amount.saturating_mul(unit).min(MAX_DELAY_MS);
		self.channels = text_or(values, SETTINGS, "channels")
			.split([' ', ',', '\n', '\t', '\r'])
			.filter_map(|part| Id::from_str(part.trim()).ok())
			.take(256)
			.collect();
		self.keep = text_or(values, SETTINGS, "keep")
			.split(['\n', ','])
			.map(str::trim)
			.filter(|part| !part.is_empty())
			.map(str::to_lowercase)
			.take(64)
			.collect();
		self.max_chars = number_or(values, SETTINGS, "maxChars").clamp(0, 4_000) as usize;
		self.pending.clear();
		self.outstanding.clear();
		self.ready.clear();
	}

	fn reset(&mut self) {
		// A message that is already gone from the service needs no remembering.
		self.pending.clear();
		self.outstanding.clear();
		self.ready.clear();
	}

	fn on_created(
		&mut self,
		inbound: &Inbound,
		message: &model::Message,
		_replies: &mut Vec<crate::PendingReply>,
	) {
		// Only your own messages, and only the conversations you named when you named any.
		if message.author.id != inbound.me {
			return;
		}
		if !self.channels.is_empty() && !self.channels.contains(&inbound.channel) {
			return;
		}
		if !self.should_delete(&message.content) {
			return;
		}
		self.remember(inbound.channel, message.id, inbound.now);
	}

	fn on_deleted(
		&mut self,
		_inbound: &Inbound,
		_channel: Id,
		id: Id,
		_last: Option<&model::Message>,
	) {
		// You deleted it yourself, or it was edited away; either way there is nothing left
		// to take back and no intent to raise.
		self.outstanding.remove(&id);
		self.pending.retain(|due| due.message != id);
	}

	fn delivered(&mut self, event: &Delivery<'_>) {
		let Delivery::Sent { message, .. } = event else {
			return;
		};
		if !self.should_delete(&message.content) {
			return;
		}
		// The echo of your own send arrives as a created message, which already remembered
		// it; this covers the conversation where it does not.
		if self.outstanding.contains(&message.id) {
			return;
		}
		self.remember(message.channel, message.id, self.last_tick);
	}

	fn tick(&mut self, now_ms: u64) {
		self.last_tick = now_ms;
		while self.pending.front().is_some_and(|due| due.at <= now_ms) {
			let Some(due) = self.pending.pop_front() else {
				break;
			};
			if self.outstanding.remove(&due.message) {
				self.ready.push(Intent::Delete {
					channel: due.channel,
					message: due.message,
				});
			}
			// The list is bounded already, but a burst of due messages is still handed
			// over a few at a time so one frame is not spent on all of them.
			if self.ready.len() >= MAX_PER_FRAME {
				break;
			}
		}
	}

	fn take_intent(&mut self, _context: &IntentContext<'_>) -> Option<Intent> {
		if self.ready.is_empty() {
			return None;
		}
		Some(self.ready.remove(0))
	}

	fn summary(&self) -> Option<String> {
		(self.delay_ms > 0).then(|| {
			format!(
				"{} waiting, {} each",
				self.pending.len(),
				seconds(self.delay_ms)
			)
		})
	}
}

const MAX_PER_FRAME: usize = 8;

fn seconds(ms: u64) -> String {
	if ms >= 3_600_000 && ms.is_multiple_of(3_600_000) {
		format!("{}h", ms / 3_600_000)
	} else if ms >= 60_000 && ms.is_multiple_of(60_000) {
		format!("{}m", ms / 60_000)
	} else {
		format!("{}s", ms / 1_000)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{Plugin, Registry};

	fn mine(id: u64, content: &str) -> model::Message {
		let mut message = test_support::message(id, model::Id(7));
		message.author.id = model::Id(1);
		message.content = content.to_string();
		message
	}

	fn context() -> IntentContext<'static> {
		IntentContext {
			channel: model::Id(7),
			me: model::Id(1),
			previous: None,
		}
	}

	fn configured(delay: &str, unit: &str) -> AutoDeleter {
		let mut plugin = AutoDeleter::default();
		plugin.configure(&Values(
			[
				("defaultDelay".to_string(), serde_json::json!(delay)),
				("delayUnit".to_string(), serde_json::json!(unit)),
			]
			.into_iter()
			.collect(),
		));
		plugin
	}

	#[test]
	fn a_message_goes_away_once_the_delay_has_passed() {
		let mut plugin = configured("30", "seconds");
		let mut replies = Vec::new();
		plugin.on_created(
			&Inbound::new(model::Id(7), None, model::Id(1), 0),
			&mine(5, "regrettable"),
			&mut replies,
		);
		plugin.tick(29_000);
		assert!(plugin.take_intent(&context()).is_none(), "not yet");
		plugin.tick(30_000);
		assert_eq!(
			plugin.take_intent(&context()),
			Some(Intent::Delete {
				channel: model::Id(7),
				message: model::Id(5)
			})
		);
	}

	#[test]
	fn what_the_settings_read_back() {
		let values = Values(
			[
				("defaultDelay".to_string(), serde_json::json!(2)),
				("delayUnit".to_string(), serde_json::json!("minutes")),
			]
			.into_iter()
			.collect(),
		);
		assert_eq!(number_or(&values, SETTINGS, "defaultDelay"), 2);
		assert_eq!(text_or(&values, SETTINGS, "delayUnit"), "minutes");
	}

	#[test]
	fn the_delay_is_read_in_the_unit_you_choose() {
		// The original's own floor is five, so a smaller number is raised to it rather
		// than quietly becoming a shorter delay than the page allows.
		assert_eq!(configured("2", "minutes").delay_ms, 5 * 60_000);
		assert_eq!(configured("1", "hours").delay_ms, 5 * 3_600_000);
		assert_eq!(configured("30", "seconds").delay_ms, 30_000);
	}

	#[test]
	fn a_delay_beyond_a_day_is_capped() {
		assert_eq!(
			configured("999999", "hours").delay_ms,
			MAX_DELAY_MS,
			"a day at most"
		);
	}

	#[test]
	fn nothing_is_deleted_before_the_delay_is_set() {
		let mut plugin = AutoDeleter::default();
		let mut replies = Vec::new();
		plugin.on_created(
			&Inbound::new(model::Id(7), None, model::Id(1), 0),
			&mine(5, "regrettable"),
			&mut replies,
		);
		plugin.tick(1_000_000_000);
		assert!(plugin.take_intent(&context()).is_none());
		assert!(plugin.summary().is_none());
	}

	#[test]
	fn someone_elses_message_is_never_taken_back() {
		let mut plugin = configured("5", "seconds");
		let mut replies = Vec::new();
		let mut theirs = mine(5, "theirs");
		theirs.author.id = model::Id(2);
		plugin.on_created(
			&Inbound::new(model::Id(7), None, model::Id(1), 0),
			&theirs,
			&mut replies,
		);
		plugin.tick(60_000);
		assert!(plugin.take_intent(&context()).is_none());
	}

	#[test]
	fn a_message_you_already_deleted_is_not_deleted_again() {
		let mut plugin = configured("5", "seconds");
		let mut replies = Vec::new();
		plugin.on_created(
			&Inbound::new(model::Id(7), None, model::Id(1), 0),
			&mine(5, "regrettable"),
			&mut replies,
		);
		plugin.on_deleted(
			&Inbound::new(model::Id(7), None, model::Id(1), 0),
			model::Id(7),
			model::Id(5),
			None,
		);
		plugin.tick(60_000);
		assert!(plugin.take_intent(&context()).is_none());
	}

	#[test]
	fn a_kept_word_spares_the_message() {
		let mut plugin = configured("5", "seconds");
		plugin.configure(&Values(
			[
				("defaultDelay".to_string(), serde_json::json!(5)),
				("delayUnit".to_string(), serde_json::json!("seconds")),
				("keep".to_string(), serde_json::json!("announcement")),
			]
			.into_iter()
			.collect(),
		));
		let mut replies = Vec::new();
		plugin.on_created(
			&Inbound::new(model::Id(7), None, model::Id(1), 0),
			&mine(5, "an Announcement to keep"),
			&mut replies,
		);
		plugin.tick(60_000);
		assert!(plugin.take_intent(&context()).is_none());
	}

	#[test]
	fn only_the_conversations_you_named_are_covered() {
		let mut plugin = configured("5", "seconds");
		plugin.configure(&Values(
			[
				("defaultDelay".to_string(), serde_json::json!(5)),
				("delayUnit".to_string(), serde_json::json!("seconds")),
				("channels".to_string(), serde_json::json!("9")),
			]
			.into_iter()
			.collect(),
		));
		let mut replies = Vec::new();
		plugin.on_created(
			&Inbound::new(model::Id(7), None, model::Id(1), 0),
			&mine(5, "regrettable"),
			&mut replies,
		);
		plugin.tick(60_000);
		assert!(plugin.take_intent(&context()).is_none());
	}

	#[test]
	fn a_long_message_is_left_alone_when_you_say_so() {
		let mut plugin = configured("5", "seconds");
		plugin.configure(&Values(
			[
				("defaultDelay".to_string(), serde_json::json!(5)),
				("delayUnit".to_string(), serde_json::json!("seconds")),
				("maxChars".to_string(), serde_json::json!(10)),
			]
			.into_iter()
			.collect(),
		));
		let mut replies = Vec::new();
		plugin.on_created(
			&Inbound::new(model::Id(7), None, model::Id(1), 0),
			&mine(5, "far too long to be a throwaway"),
			&mut replies,
		);
		plugin.tick(60_000);
		assert!(plugin.take_intent(&context()).is_none());
	}

	#[test]
	fn the_list_waiting_is_bounded() {
		let mut plugin = configured("5", "seconds");
		let mut replies = Vec::new();
		for id in 0..(MAX_PENDING as u64 + 20) {
			plugin.on_created(
				&Inbound::new(model::Id(7), None, model::Id(1), 0),
				&mine(id, "regrettable"),
				&mut replies,
			);
		}
		assert_eq!(plugin.pending.len(), MAX_PENDING);
		plugin.tick(60_000);
		let mut deleted = 0;
		while plugin.take_intent(&context()).is_some() {
			deleted += 1;
		}
		assert!(
			deleted <= MAX_PER_FRAME,
			"a frame hands over a few at a time"
		);
	}

	#[test]
	fn turning_it_off_forgets_what_it_was_waiting_for() {
		let mut registry = Registry::new();
		registry.set_enabled("AutoDeleter", true);
		registry.set_value("AutoDeleter", "defaultDelay", 5.into());
		registry.set_value("AutoDeleter", "delayUnit", "seconds".into());
		let mut replies: Vec<crate::PendingReply> = Vec::new();
		let _ = registry.observe(
			&Inbound::new(model::Id(7), None, model::Id(1), 0),
			crate::InboundEvent::Created(&mine(5, "regrettable")),
		);
		let _ = &mut replies;
		registry.tick(60_000);
		assert!(registry.take_intent(&context()).is_some());
		registry.set_enabled("AutoDeleter", false);
		registry.tick(600_000);
		assert!(registry.take_intent(&context()).is_none());
	}
}
