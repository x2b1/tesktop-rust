//! AutoReplyContent: answers a message that matches your trigger with a canned response.
//!
//! TestCord fires this from a JavaScript timeout. Here the host sends the queued reply on a
//! later frame, so the delay is honoured without a thread and the reply still goes through the
//! app's own send path and its permission checks.

use crate::{
	Fallback, Inbound, Meta, PendingReply, Setting, SettingKind, Values, flag_or, number_or,
	text_or,
};
use model::{Id, Message};
use std::collections::{BTreeMap, VecDeque};
use std::str::FromStr;

/// Discord's own message ceiling.
const MAX_CONTENT: usize = 2000;
/// Triggered message ids remembered, so a replayed event cannot answer twice.
const MAX_PROCESSED: usize = 1024;
/// How long a processed id is remembered.
const PROCESSED_TTL_MS: u64 = 5 * 60 * 1000;
/// Tracked users and channels for cooldowns.
const MAX_TRACKED: usize = 256;
/// Reply timestamps kept for the per-minute ceiling.
const MAX_RATE_WINDOW: usize = 64;

const SETTINGS: &[Setting] = &[
	Setting {
		key: "triggerPrefix",
		label: "Trigger",
		kind: SettingKind::Text { multiline: false },
		default: Fallback::Text(""),
	},
	Setting {
		key: "responseMessage",
		label: "Response",
		kind: SettingKind::Text { multiline: true },
		default: Fallback::Text(""),
	},
	Setting {
		key: "responseMessages",
		label: "Responses, one per line",
		kind: SettingKind::Text { multiline: true },
		default: Fallback::Text(""),
	},
	Setting {
		key: "channelId",
		label: "Only in this channel",
		kind: SettingKind::Text { multiline: false },
		default: Fallback::Text(""),
	},
	Setting {
		key: "channelWhitelist",
		label: "Only in these channels",
		kind: SettingKind::Text { multiline: true },
		default: Fallback::Text(""),
	},
	Setting {
		key: "delayMs",
		label: "Delay before replying (ms)",
		kind: SettingKind::Number {
			min: 0,
			max: 60_000,
		},
		default: Fallback::Number(500),
	},
	Setting {
		key: "caseInsensitive",
		label: "Ignore case",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
	Setting {
		key: "exactMatch",
		label: "Match the whole message",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
	Setting {
		key: "useRegex",
		label: "Treat the trigger as a regular expression",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
	Setting {
		key: "onlyInDMs",
		label: "Only in direct messages",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
	Setting {
		key: "excludeDMs",
		label: "Never in direct messages",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
	Setting {
		key: "onlyWhenMentioned",
		label: "Only when mentioned",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
	Setting {
		key: "triggerOnSelfMessages",
		label: "React to my own messages",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
	Setting {
		key: "triggerOnBotMessages",
		label: "React to bot messages",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
	Setting {
		key: "perUserCooldownMs",
		label: "Per user cooldown (ms)",
		kind: SettingKind::Number {
			min: 0,
			max: 3_600_000,
		},
		default: Fallback::Number(10_000),
	},
	Setting {
		key: "perChannelCooldownMs",
		label: "Per channel cooldown (ms)",
		kind: SettingKind::Number {
			min: 0,
			max: 3_600_000,
		},
		default: Fallback::Number(0),
	},
	Setting {
		key: "maxRepliesPerMinute",
		label: "Replies per minute, 0 for no ceiling",
		kind: SettingKind::Number { min: 0, max: 60 },
		default: Fallback::Number(0),
	},
];

#[derive(Default)]
pub struct AutoReplyContent {
	trigger: String,
	responses: Vec<String>,
	channels: Vec<Id>,
	delay_ms: u64,
	case_insensitive: bool,
	exact_match: bool,
	use_regex: bool,
	only_in_dms: bool,
	exclude_dms: bool,
	only_when_mentioned: bool,
	trigger_on_self: bool,
	trigger_on_bots: bool,
	user_cooldown_ms: u64,
	channel_cooldown_ms: u64,
	max_replies_per_minute: usize,
	regex: Option<regex::Regex>,
	regex_invalid: bool,
	processed: VecDeque<(Id, u64)>,
	user_cooldowns: BTreeMap<Id, u64>,
	channel_cooldowns: BTreeMap<Id, u64>,
	rate_window: VecDeque<u64>,
}

impl crate::Plugin for AutoReplyContent {
	fn meta(&self) -> Meta {
		Meta {
			id: "AutoReplyContent",
			name: "AutoReplyContent",
			description: "Answers a matching message with a response you wrote.",
			authors: "SirPhantom89",
			tags: &["Chat"],
			aliases: &["autoReplyContent"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.trigger = text_or(values, SETTINGS, "triggerPrefix");
		self.responses = responses(
			&text_or(values, SETTINGS, "responseMessages"),
			&text_or(values, SETTINGS, "responseMessage"),
		);
		self.channels = channels(
			&text_or(values, SETTINGS, "channelWhitelist"),
			&text_or(values, SETTINGS, "channelId"),
		);
		self.delay_ms = number_or(values, SETTINGS, "delayMs").max(0) as u64;
		self.case_insensitive = flag_or(values, SETTINGS, "caseInsensitive");
		self.exact_match = flag_or(values, SETTINGS, "exactMatch");
		self.use_regex = flag_or(values, SETTINGS, "useRegex");
		self.only_in_dms = flag_or(values, SETTINGS, "onlyInDMs");
		self.exclude_dms = flag_or(values, SETTINGS, "excludeDMs");
		self.only_when_mentioned = flag_or(values, SETTINGS, "onlyWhenMentioned");
		self.trigger_on_self = flag_or(values, SETTINGS, "triggerOnSelfMessages");
		self.trigger_on_bots = flag_or(values, SETTINGS, "triggerOnBotMessages");
		self.user_cooldown_ms = number_or(values, SETTINGS, "perUserCooldownMs").max(0) as u64;
		self.channel_cooldown_ms =
			number_or(values, SETTINGS, "perChannelCooldownMs").max(0) as u64;
		self.max_replies_per_minute =
			number_or(values, SETTINGS, "maxRepliesPerMinute").max(0) as usize;
		self.regex = if self.use_regex {
			compile(&self.trigger, self.case_insensitive)
		} else {
			None
		};
		self.regex_invalid = self.use_regex && self.regex.is_none();
	}

	fn reset(&mut self) {
		self.processed.clear();
		self.user_cooldowns.clear();
		self.channel_cooldowns.clear();
		self.rate_window.clear();
	}

	fn on_created(
		&mut self,
		inbound: &Inbound,
		message: &Message,
		replies: &mut Vec<PendingReply>,
	) {
		if self.trigger.is_empty() || self.responses.is_empty() || message.content.is_empty() {
			return;
		}
		if self.only_in_dms && self.exclude_dms {
			return;
		}
		if self.only_in_dms && !inbound.is_direct() {
			return;
		}
		if self.exclude_dms && inbound.is_direct() {
			return;
		}
		if !self.channels.is_empty() && !self.channels.contains(&inbound.channel) {
			return;
		}
		if !self.trigger_on_bots && is_bot(&message.author) {
			return;
		}
		if !self.trigger_on_self && message.author.id == inbound.me {
			return;
		}
		if self.only_when_mentioned && !mentions_me(message, inbound.me) {
			return;
		}
		if self.seen(message.id, inbound.now) {
			return;
		}
		if !self.matches(&message.content) {
			return;
		}
		if self
			.user_cooldowns
			.get(&message.author.id)
			.is_some_and(|last| inbound.now.saturating_sub(*last) < self.user_cooldown_ms)
			|| self
				.channel_cooldowns
				.get(&inbound.channel)
				.is_some_and(|last| inbound.now.saturating_sub(*last) < self.channel_cooldown_ms)
		{
			return;
		}
		self.prune_rate_window(inbound.now);
		if self.max_replies_per_minute > 0 && self.rate_window.len() >= self.max_replies_per_minute
		{
			return;
		}
		self.rate_window.push_back(inbound.now);
		remember(&mut self.user_cooldowns, message.author.id, inbound.now);
		remember(&mut self.channel_cooldowns, inbound.channel, inbound.now);
		let choice = self.responses[hash(message.id.0) as usize % self.responses.len()].clone();
		replies.push(PendingReply {
			channel: inbound.channel,
			content: clamp(apply_template(&choice, message)),
			due: inbound.now + self.delay_ms,
		});
	}

	fn summary(&self) -> Option<String> {
		if self.trigger.is_empty() || self.responses.is_empty() {
			return Some("Set a trigger and a response".to_string());
		}
		let mut summary = format!(
			"{} {}",
			self.responses.len(),
			if self.responses.len() == 1 {
				"response"
			} else {
				"responses"
			}
		);
		if self.regex_invalid {
			summary.push_str(" · trigger is not a valid pattern");
		}
		Some(summary)
	}
}

impl AutoReplyContent {
	fn matches(&self, content: &str) -> bool {
		if self.use_regex {
			return self
				.regex
				.as_ref()
				.is_some_and(|regex| regex.is_match(content));
		}
		if self.case_insensitive {
			let content = content.to_lowercase();
			let trigger = self.trigger.to_lowercase();
			return if self.exact_match {
				content == trigger
			} else {
				content.starts_with(&trigger)
			};
		}
		if self.exact_match {
			content == self.trigger
		} else {
			content.starts_with(&self.trigger)
		}
	}

	/// Whether this message id was already answered, remembering it when it was not.
	fn seen(&mut self, id: Id, now: u64) -> bool {
		while self
			.processed
			.front()
			.is_some_and(|(_, seen)| now.saturating_sub(*seen) >= PROCESSED_TTL_MS)
		{
			self.processed.pop_front();
		}
		if self.processed.iter().any(|(seen, _)| *seen == id) {
			return true;
		}
		self.processed.push_back((id, now));
		while self.processed.len() > MAX_PROCESSED {
			self.processed.pop_front();
		}
		false
	}

	fn prune_rate_window(&mut self, now: u64) {
		while self
			.rate_window
			.front()
			.is_some_and(|sent| now.saturating_sub(*sent) >= 60_000)
		{
			self.rate_window.pop_front();
		}
		while self.rate_window.len() > MAX_RATE_WINDOW {
			self.rate_window.pop_front();
		}
	}
}

fn is_bot(author: &model::User) -> bool {
	author.webhook || author.kind == model::AccountKind::Bot
}

fn mentions_me(message: &Message, me: Id) -> bool {
	message.mentions.iter().any(|user| user.id == me)
		|| message.content.contains(&format!("<@{me}>"))
		|| message.content.contains(&format!("<@!{me}>"))
}

fn compile(trigger: &str, case_insensitive: bool) -> Option<regex::Regex> {
	// TestCord accepts `/pattern/flags` alongside a bare pattern.
	let slash = trigger
		.strip_prefix('/')
		.and_then(|rest| rest.rfind('/').map(|end| (&rest[..end], &rest[end + 1..])))
		.filter(|(pattern, _)| !pattern.is_empty());
	let (pattern, flags) = match slash {
		Some((pattern, flags)) => (
			pattern.to_string(),
			if flags.is_empty() {
				String::new()
			} else {
				format!("(?{flags})")
			},
		),
		None => (trigger.to_string(), String::new()),
	};
	regex::RegexBuilder::new(&format!("{flags}{pattern}"))
		.case_insensitive(case_insensitive)
		.size_limit(512 * 1024)
		.build()
		.ok()
}

fn responses(list: &str, single: &str) -> Vec<String> {
	let lines: Vec<String> = list
		.lines()
		.map(str::trim)
		.filter(|line| !line.is_empty())
		.map(str::to_string)
		.collect();
	if !lines.is_empty() {
		return lines;
	}
	let single = single.trim();
	if single.is_empty() {
		Vec::new()
	} else {
		vec![single.to_string()]
	}
}

fn channels(list: &str, single: &str) -> Vec<Id> {
	let mut ids: Vec<Id> = list
		.split([' ', ',', '\n', '\t', '\r'])
		.filter_map(|part| Id::from_str(part.trim()).ok())
		.collect();
	if let Ok(id) = Id::from_str(single.trim()) {
		ids.push(id);
	}
	ids.sort_unstable();
	ids.dedup();
	ids.truncate(MAX_TRACKED);
	ids
}

fn apply_template(template: &str, message: &Message) -> String {
	template
		.replace("{user}", &message.author.name)
		.replace("{mention}", &format!("<@{}>", message.author.id))
		.replace("{channel}", &format!("<#{}>", message.channel))
		.replace("{message}", &message.content)
}

fn clamp(content: String) -> String {
	if content.chars().count() <= MAX_CONTENT {
		return content;
	}
	content
		.chars()
		.take(MAX_CONTENT)
		.collect::<String>()
		.trim_end()
		.to_string()
}

fn remember(map: &mut BTreeMap<Id, u64>, id: Id, now: u64) {
	if map.len() >= MAX_TRACKED
		&& !map.contains_key(&id)
		&& let Some(oldest) = map.iter().min_by_key(|(_, at)| **at).map(|(id, _)| *id)
	{
		map.remove(&oldest);
	}
	map.insert(id, now);
}

fn hash(value: u64) -> u64 {
	let mut hash = value ^ 0x9e37_79b9_7f4a_7c15;
	hash = (hash ^ (hash >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
	hash = (hash ^ (hash >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
	hash ^ (hash >> 31)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::Plugin;
	use model::AccountKind;

	fn configured(settings: &[(&str, serde_json::Value)]) -> AutoReplyContent {
		let mut plugin = AutoReplyContent::default();
		plugin.configure(&Values(
			settings
				.iter()
				.map(|(key, value)| ((*key).to_string(), value.clone()))
				.collect(),
		));
		plugin
	}

	fn base() -> Vec<(&'static str, serde_json::Value)> {
		vec![
			("triggerPrefix", "ping".into()),
			("responseMessage", "pong {user}".into()),
			("delayMs", 0.into()),
		]
	}

	fn inbound(channel: Id, now: u64) -> Inbound {
		Inbound::new(channel, Some(Id(10)), Id(1), now)
	}

	fn message(id: u64, author: Id, content: &str) -> Message {
		let mut message = test_support::message(id, Id(7));
		message.author.id = author;
		message.content = content.to_string();
		message
	}

	fn replies(
		plugin: &mut AutoReplyContent,
		inbound: &Inbound,
		message: &Message,
	) -> Vec<PendingReply> {
		let mut replies = Vec::new();
		plugin.on_created(inbound, message, &mut replies);
		replies
	}

	#[test]
	fn a_matching_message_gets_one_reply() {
		let mut plugin = configured(&base());
		let message = message(1, Id(2), "ping there");
		let sent = replies(&mut plugin, &inbound(Id(7), 0), &message);
		assert_eq!(sent.len(), 1);
		assert_eq!(sent[0].content, "pong Robin (synthetic)");
		assert_eq!(sent[0].channel, Id(7));
	}

	#[test]
	fn the_same_message_is_never_answered_twice() {
		let mut plugin = configured(&base());
		let message = message(1, Id(2), "ping");
		assert_eq!(replies(&mut plugin, &inbound(Id(7), 0), &message).len(), 1);
		assert!(replies(&mut plugin, &inbound(Id(7), 10), &message).is_empty());
	}

	#[test]
	fn own_and_bot_messages_need_their_own_opt_in() {
		let mut plugin = configured(&base());
		assert!(replies(&mut plugin, &inbound(Id(7), 0), &message(1, Id(1), "ping")).is_empty());
		let mut bot = message(2, Id(3), "ping");
		bot.author.kind = AccountKind::Bot;
		assert!(replies(&mut plugin, &inbound(Id(7), 0), &bot).is_empty());

		let mut opt_in = base();
		opt_in.push(("triggerOnSelfMessages", true.into()));
		opt_in.push(("triggerOnBotMessages", true.into()));
		let mut plugin = configured(&opt_in);
		assert_eq!(
			replies(&mut plugin, &inbound(Id(7), 0), &message(3, Id(1), "ping")).len(),
			1
		);
		assert_eq!(replies(&mut plugin, &inbound(Id(7), 0), &bot).len(), 1);
	}

	#[test]
	fn cooldowns_and_the_minute_ceiling_hold() {
		let mut settings = base();
		settings.push(("perUserCooldownMs", 10_000.into()));
		settings.push(("maxRepliesPerMinute", 1.into()));
		let mut plugin = configured(&settings);
		let first = inbound(Id(7), 0);
		assert_eq!(
			replies(&mut plugin, &first, &message(1, Id(2), "ping")).len(),
			1
		);
		assert!(replies(&mut plugin, &first, &message(2, Id(2), "ping")).is_empty());
		assert!(replies(&mut plugin, &first, &message(3, Id(9), "ping")).is_empty());
		let later = inbound(Id(7), 61_000);
		assert_eq!(
			replies(&mut plugin, &later, &message(4, Id(9), "ping")).len(),
			1
		);
	}

	#[test]
	fn the_delay_is_reported_to_the_host() {
		let mut settings = base();
		settings.push(("delayMs", 750.into()));
		let mut plugin = configured(&settings);
		let sent = replies(
			&mut plugin,
			&inbound(Id(7), 1_000),
			&message(1, Id(2), "ping"),
		);
		assert_eq!(sent[0].due, 1_750);
	}

	#[test]
	fn channel_and_direct_message_rules_apply() {
		let mut settings = base();
		settings.push(("channelId", "7".into()));
		let mut plugin = configured(&settings);
		assert_eq!(
			replies(&mut plugin, &inbound(Id(7), 0), &message(1, Id(2), "ping")).len(),
			1
		);
		assert!(replies(&mut plugin, &inbound(Id(8), 0), &message(2, Id(2), "ping")).is_empty());

		let mut settings = base();
		settings.push(("onlyInDMs", true.into()));
		let mut plugin = configured(&settings);
		let guild = inbound(Id(7), 0);
		let direct = Inbound::new(Id(7), None, Id(1), 0);
		assert!(replies(&mut plugin, &guild, &message(3, Id(2), "ping")).is_empty());
		assert_eq!(
			replies(&mut plugin, &direct, &message(4, Id(2), "ping")).len(),
			1
		);
	}

	#[test]
	fn regex_triggers_accept_slash_form() {
		let mut settings = base();
		settings.push(("triggerPrefix", "^ping(\\s|$)".into()));
		settings.push(("useRegex", true.into()));
		let mut plugin = configured(&settings);
		assert_eq!(
			replies(
				&mut plugin,
				&inbound(Id(7), 0),
				&message(1, Id(2), "ping you")
			)
			.len(),
			1
		);
		assert!(replies(&mut plugin, &inbound(Id(7), 0), &message(2, Id(2), "nope")).is_empty());

		let mut settings = base();
		settings.push(("triggerPrefix", "/^PING$/i".into()));
		settings.push(("useRegex", true.into()));
		let mut plugin = configured(&settings);
		assert_eq!(
			replies(&mut plugin, &inbound(Id(7), 0), &message(3, Id(2), "ping")).len(),
			1
		);
	}

	#[test]
	fn a_broken_trigger_is_reported_instead_of_panicking() {
		let mut settings = base();
		settings.push(("triggerPrefix", "(".into()));
		settings.push(("useRegex", true.into()));
		let mut plugin = configured(&settings);
		assert!(replies(&mut plugin, &inbound(Id(7), 0), &message(1, Id(2), "ping")).is_empty());
		assert!(plugin.summary().unwrap().contains("not a valid pattern"));
	}

	#[test]
	fn a_response_longer_than_discord_allows_is_trimmed() {
		let mut settings = base();
		settings.push(("responseMessage", "x".repeat(2500).into()));
		let mut plugin = configured(&settings);
		let sent = replies(&mut plugin, &inbound(Id(7), 0), &message(1, Id(2), "ping"));
		assert_eq!(sent[0].content.chars().count(), MAX_CONTENT);
	}

	#[test]
	fn one_of_several_responses_is_picked_per_message() {
		let mut settings = base();
		settings.push(("responseMessages", "one\ntwo\nthree".into()));
		settings.push(("perUserCooldownMs", 0.into()));
		let mut plugin = configured(&settings);
		let mut seen = std::collections::BTreeSet::new();
		for id in 1..=30 {
			let sent = replies(
				&mut plugin,
				&inbound(Id(7), id * 1000),
				&message(id, Id(id + 100), "ping"),
			);
			assert_eq!(sent.len(), 1);
			assert!(["one", "two", "three"].contains(&sent[0].content.as_str()));
			seen.insert(sent[0].content.clone());
		}
		assert_eq!(seen.len(), 3);
	}

	#[test]
	fn tracked_cooldowns_stay_bounded() {
		let mut settings = base();
		settings.push(("perUserCooldownMs", 0.into()));
		let mut plugin = configured(&settings);
		for id in 1..=(MAX_TRACKED as u64 * 2) {
			let message = message(id, Id(id + 100), "ping");
			replies(&mut plugin, &inbound(Id(7), id), &message);
		}
		assert!(plugin.user_cooldowns.len() <= MAX_TRACKED);
		assert!(plugin.processed.len() <= MAX_PROCESSED);
		plugin.reset();
		assert!(plugin.user_cooldowns.is_empty() && plugin.processed.is_empty());
	}
}
