//! AutoDeleteDMs: the conversations that have gone quiet, and StartupTimings: how long this
//! took to get going.

use crate::{
	Fallback, Inbound, Intent, IntentContext, Meta, PendingReply, Setting, SettingKind, Values,
	flag_or, number_or,
};
use model::Id;
use std::collections::BTreeMap;

/// Conversations remembered at once.
pub const MAX_DMS: usize = 512;
/// A day in milliseconds, which is the unit the delay is written in.
const DAY_MS: u64 = 24 * 60 * 60 * 1000;

const DELETE_SETTINGS: &[Setting] = &[
	Setting {
		key: "afterDays",
		label: "Leave a conversation after this many quiet days",
		kind: SettingKind::Number { min: 1, max: 365 },
		default: Fallback::Number(30),
	},
	Setting {
		key: "ignoreGroups",
		label: "Leave group conversations too",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
	Setting {
		key: "minMessages",
		label: "Only consider a conversation with at least this many messages",
		kind: SettingKind::Number { min: 1, max: 100 },
		default: Fallback::Number(1),
	},
];

/// One conversation, and the last time anything was said in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Conversation {
	pub last: u64,
	pub messages: u32,
	pub group: bool,
}

/// AutoDeleteDMs: the quiet ones go, on their own, without you doing anything.
pub struct AutoDeleteDms {
	after_ms: u64,
	ignore_groups: bool,
	min_messages: u32,
	conversations: BTreeMap<Id, Conversation>,
	ready: Vec<Intent>,
}

impl Default for AutoDeleteDms {
	fn default() -> Self {
		Self {
			// Nothing is left until the delay is set, so a fresh install keeps everything.
			after_ms: 0,
			ignore_groups: true,
			min_messages: 1,
			conversations: BTreeMap::new(),
			ready: Vec::new(),
		}
	}
}

impl AutoDeleteDms {
	/// Note that a conversation is alive, oldest first out when the list is full.
	fn touch(&mut self, channel: Id, now: u64, group: bool) {
		while self.conversations.len() >= MAX_DMS {
			if let Some(quietest) = self
				.conversations
				.iter()
				.min_by_key(|(_, seen)| seen.last)
				.map(|(id, _)| *id)
			{
				self.conversations.remove(&quietest);
			} else {
				break;
			}
		}
		let entry = self.conversations.entry(channel).or_insert(Conversation {
			last: now,
			messages: 0,
			group,
		});
		entry.last = now;
		entry.messages = entry.messages.saturating_add(1);
	}

	/// The conversations that have been quiet long enough, oldest first.
	fn due(&self, now: u64) -> Vec<Id> {
		let mut due: Vec<(Id, u64)> = self
			.conversations
			.iter()
			.filter(|(_, seen)| {
				self.after_ms > 0
					&& seen.messages >= self.min_messages
					&& !(self.ignore_groups && seen.group)
					&& now.saturating_sub(seen.last) >= self.after_ms
			})
			.map(|(id, seen)| (*id, seen.last))
			.collect();
		due.sort_by_key(|(_, last)| *last);
		due.into_iter().map(|(id, _)| id).collect()
	}
}

impl crate::Plugin for AutoDeleteDms {
	fn meta(&self) -> Meta {
		Meta {
			id: "AutoDeleteDMs",
			name: "AutoDeleteDMs",
			description: "Leaves the conversations that have gone quiet, after the delay you set.",
			authors: "Kitty",
			tags: &["Utility", "Friends"],
			aliases: &["autoDeleteDms", "autoDeleteDMs"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		DELETE_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		let days = number_or(values, DELETE_SETTINGS, "afterDays").clamp(1, 365) as u64;
		self.after_ms = days.saturating_mul(DAY_MS);
		self.ignore_groups = flag_or(values, DELETE_SETTINGS, "ignoreGroups");
		self.min_messages = number_or(values, DELETE_SETTINGS, "minMessages").clamp(1, 100) as u32;
		self.conversations.clear();
		self.ready.clear();
	}

	fn reset(&mut self) {
		self.conversations.clear();
		self.ready.clear();
	}

	fn on_created(
		&mut self,
		inbound: &Inbound,
		_message: &model::Message,
		_replies: &mut Vec<PendingReply>,
	) {
		let group = inbound.guild.is_some();
		self.touch(inbound.channel, inbound.now, group);
	}

	fn tick(&mut self, now_ms: u64) {
		for channel in self.due(now_ms) {
			self.conversations.remove(&channel);
			self.ready.push(Intent::Leave { channel });
		}
	}

	fn take_intent(&mut self, _context: &IntentContext<'_>) -> Option<Intent> {
		if self.ready.is_empty() {
			return None;
		}
		Some(self.ready.remove(0))
	}

	fn summary(&self) -> Option<String> {
		(self.after_ms > 0).then(|| {
			format!(
				"{} quiet days · {} conversations watched",
				self.after_ms / DAY_MS,
				self.conversations.len()
			)
		})
	}
}

const TIMING_SETTINGS: &[Setting] = &[Setting {
	key: "show",
	label: "Report how long the ports took to start",
	kind: SettingKind::Toggle,
	default: Fallback::Flag(true),
}];

/// StartupTimings: how long the runtime took before it could do anything.
#[derive(Default)]
pub struct StartupTimings {
	show: bool,
	first_tick: Option<u64>,
	seen: u64,
}

impl StartupTimings {
	/// The number of ticks it took to be asked anything, which is what the original's page
	/// shows in its own terms: how long before the client was ready.
	pub fn ticks_before_ready(&self) -> u64 {
		self.seen
	}
}

impl crate::Plugin for StartupTimings {
	fn meta(&self) -> Meta {
		Meta {
			id: "StartupTimings",
			name: "StartupTimings",
			description: "Reports how long the bundled ports took to come up.",
			authors: "Megu",
			tags: &["Developers", "Utility"],
			aliases: &["startupTimings"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		TIMING_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.show = flag_or(values, TIMING_SETTINGS, "show");
		self.first_tick = None;
		self.seen = 0;
	}

	fn reset(&mut self) {
		self.first_tick = None;
		self.seen = 0;
	}

	fn tick(&mut self, now_ms: u64) {
		if self.first_tick.is_none() {
			self.first_tick = Some(now_ms);
		}
		self.seen += 1;
	}

	fn summary(&self) -> Option<String> {
		if !self.show {
			return None;
		}
		Some(match self.first_tick {
			Some(_) => format!("Ready after {} frame(s)", self.seen),
			None => "Not asked yet".to_string(),
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{Plugin, Registry};

	fn context() -> IntentContext<'static> {
		IntentContext {
			channel: model::Id(7),
			me: model::Id(1),
			previous: None,
		}
	}

	fn message(channel: u64) -> model::Message {
		test_support::message(1, model::Id(channel))
	}

	fn configured(days: i64, groups: bool) -> AutoDeleteDms {
		let mut plugin = AutoDeleteDms::default();
		plugin.configure(&Values(
			[
				("afterDays".to_string(), serde_json::json!(days)),
				("ignoreGroups".to_string(), serde_json::json!(groups)),
			]
			.into_iter()
			.collect(),
		));
		plugin
	}

	#[test]
	fn a_quiet_conversation_is_left_after_the_delay() {
		let mut plugin = configured(30, true);
		let mut replies = Vec::new();
		plugin.on_created(
			&Inbound::new(model::Id(7), None, model::Id(2), 0),
			&message(7),
			&mut replies,
		);
		plugin.tick(29 * DAY_MS);
		assert!(
			plugin.take_intent(&context()).is_none(),
			"not yet quiet enough"
		);
		plugin.tick(30 * DAY_MS);
		assert_eq!(
			plugin.take_intent(&context()),
			Some(Intent::Leave {
				channel: model::Id(7)
			})
		);
	}

	#[test]
	fn a_conversation_that_speaks_again_starts_over() {
		let mut plugin = configured(30, true);
		let mut replies = Vec::new();
		plugin.on_created(
			&Inbound::new(model::Id(7), None, model::Id(2), 0),
			&message(7),
			&mut replies,
		);
		plugin.tick(29 * DAY_MS);
		plugin.on_created(
			&Inbound::new(model::Id(7), None, model::Id(2), 29 * DAY_MS),
			&message(7),
			&mut replies,
		);
		plugin.tick(50 * DAY_MS);
		assert!(plugin.take_intent(&context()).is_none());
	}

	#[test]
	fn nothing_is_left_before_the_delay_is_set() {
		let mut plugin = AutoDeleteDms::default();
		let mut replies = Vec::new();
		plugin.on_created(
			&Inbound::new(model::Id(7), None, model::Id(2), 0),
			&message(7),
			&mut replies,
		);
		plugin.tick(365 * DAY_MS);
		assert!(plugin.take_intent(&context()).is_none());
		assert!(plugin.summary().is_none());
	}

	#[test]
	fn a_group_can_be_left_out() {
		let mut out = configured(1, true);
		let mut kept = configured(1, false);
		let mut replies = Vec::new();
		out.on_created(
			&Inbound::new(model::Id(7), Some(model::Id(10)), model::Id(2), 0),
			&message(7),
			&mut replies,
		);
		kept.on_created(
			&Inbound::new(model::Id(7), Some(model::Id(10)), model::Id(2), 0),
			&message(7),
			&mut replies,
		);
		out.tick(2 * DAY_MS);
		kept.tick(2 * DAY_MS);
		assert!(
			out.take_intent(&context()).is_none(),
			"groups are left alone"
		);
		assert_eq!(
			kept.take_intent(&context()),
			Some(Intent::Leave {
				channel: model::Id(7)
			})
		);
	}

	#[test]
	fn a_conversation_with_too_little_in_it_is_left_alone() {
		let mut plugin = configured(1, true);
		plugin.configure(&Values(
			[
				("afterDays".to_string(), serde_json::json!(1)),
				("minMessages".to_string(), serde_json::json!(3)),
			]
			.into_iter()
			.collect(),
		));
		let mut replies = Vec::new();
		plugin.on_created(
			&Inbound::new(model::Id(7), None, model::Id(2), 0),
			&message(7),
			&mut replies,
		);
		plugin.tick(2 * DAY_MS);
		assert!(
			plugin.take_intent(&context()).is_none(),
			"one message is not a conversation"
		);
	}

	#[test]
	fn the_conversations_watched_are_bounded() {
		let mut plugin = configured(1, true);
		let mut replies = Vec::new();
		for id in 0..(MAX_DMS as u64 + 20) {
			plugin.on_created(
				&Inbound::new(model::Id(id), None, model::Id(2), id),
				&message(id),
				&mut replies,
			);
		}
		assert_eq!(plugin.conversations.len(), MAX_DMS);
	}

	#[test]
	fn the_quietest_goes_first_when_the_list_is_full() {
		let mut plugin = configured(1, true);
		let mut replies = Vec::new();
		// The oldest conversation is the one that should go when room runs out.
		plugin.on_created(
			&Inbound::new(model::Id(1), None, model::Id(2), 0),
			&message(1),
			&mut replies,
		);
		for id in 2..(MAX_DMS as u64 + 2) {
			plugin.on_created(
				&Inbound::new(model::Id(id), None, model::Id(2), 1_000),
				&message(id),
				&mut replies,
			);
		}
		assert!(
			!plugin.conversations.contains_key(&model::Id(1)),
			"the quietest conversation was forgotten first"
		);
	}

	#[test]
	fn the_startup_report_counts_the_frames_it_took() {
		let mut plugin = StartupTimings::default();
		plugin.configure(&Values::default());
		assert_eq!(plugin.summary().as_deref(), Some("Not asked yet"));
		plugin.tick(0);
		plugin.tick(16);
		assert_eq!(plugin.ticks_before_ready(), 2);
		assert_eq!(plugin.summary().as_deref(), Some("Ready after 2 frame(s)"));
	}

	#[test]
	fn the_startup_report_can_be_turned_off() {
		let mut plugin = StartupTimings::default();
		plugin.configure(&Values(
			[("show".to_string(), serde_json::json!(false))]
				.into_iter()
				.collect(),
		));
		plugin.tick(0);
		assert!(plugin.summary().is_none());
	}

	#[test]
	fn the_registry_carries_both_through_their_intent() {
		let mut registry = Registry::new();
		registry.set_enabled("AutoDeleteDMs", true);
		registry.set_value("AutoDeleteDMs", "afterDays", 1.into());
		let mut replies: Vec<PendingReply> = Vec::new();
		let _ = registry.observe(
			&Inbound::new(model::Id(7), None, model::Id(2), 0),
			crate::InboundEvent::Created(&message(7)),
		);
		let _ = &mut replies;
		registry.tick(2 * DAY_MS);
		assert_eq!(
			registry.take_intent(&context()),
			Some(Intent::Leave {
				channel: model::Id(7)
			})
		);
	}
}
