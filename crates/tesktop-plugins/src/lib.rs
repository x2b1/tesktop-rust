//! TestCord plugin contract in Rust, as used by tesktop2.
//!
//! TestCord plugins patch Discord's own JavaScript at runtime, so the patch engine itself does
//! not transfer to a native client. What transfers is the contract those plugins share: a
//! manifest, an enable flag, per-plugin settings, and hooks on the message pipeline. This crate
//! implements that contract natively; the sandboxed community extension runtime in
//! `crates/extensions` stays the path for third-party add-ons.
//!
//! Every hook is bounded by items and bytes, so message traffic cannot grow memory without limit.

pub mod autoreply;
pub mod blockkeywords;
pub mod body;
pub mod clearurls;
pub mod copy;
pub mod display;
pub mod messagelogger;
pub mod noreplymention;
pub mod sendtext;
pub mod silenceusers;
pub mod splitlarge;
pub mod store;

use model::{Id, Message};
use serde_json::Value;
use std::collections::BTreeMap;

/// Bundled plugins are compiled in, so a settings file cannot add entries. The ceiling is
/// generous because the port set keeps growing; the file size is the real limit.
pub const MAX_PLUGINS: usize = 256;
/// Setting keys kept per plugin.
pub const MAX_PLUGIN_VALUES: usize = 48;
/// Serialized setting bytes accepted per plugin.
pub const MAX_PLUGIN_BYTES: usize = 16 * 1024;
/// Serialized bytes accepted from a settings file.
pub const MAX_SETTINGS_BYTES: usize = 256 * 1024;
/// Outbound messages plugins may hold for the host to send.
pub const MAX_PENDING_REPLIES: usize = 8;
/// Bytes across those pending messages.
pub const MAX_PENDING_REPLY_BYTES: usize = 8 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Meta {
	/// TestCord plugin id, which is also the settings key.
	pub id: &'static str,
	pub name: &'static str,
	pub description: &'static str,
	pub authors: &'static str,
	pub tags: &'static [&'static str],
	/// Extra TestCord-side names that resolve to this plugin.
	pub aliases: &'static [&'static str],
	pub default_enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingKind {
	Toggle,
	Text {
		multiline: bool,
	},
	Number {
		min: i64,
		max: i64,
	},
	/// A closed list of `(value, label)` pairs; the stored value is the first element.
	Choice(&'static [(&'static str, &'static str)]),
}

/// The value a setting falls back to, and the single place it is declared.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fallback {
	Flag(bool),
	Text(&'static str),
	Number(i64),
}

/// One setting the settings page renders for a plugin.
#[derive(Clone, Copy, Debug)]
pub struct Setting {
	pub key: &'static str,
	pub label: &'static str,
	pub kind: SettingKind,
	pub default: Fallback,
}

pub fn flag_or(values: &Values, table: &'static [Setting], key: &str) -> bool {
	values.flag(key).unwrap_or_else(|| {
		table
			.iter()
			.find(|setting| setting.key == key)
			.is_some_and(|setting| matches!(setting.default, Fallback::Flag(value) if value))
	})
}

pub fn text_or(values: &Values, table: &'static [Setting], key: &str) -> String {
	values
		.text(key)
		.map(str::to_string)
		.unwrap_or_else(|| {
			table
				.iter()
				.find(|setting| setting.key == key)
				.map_or(String::new(), |setting| match setting.default {
					Fallback::Text(value) => value.to_string(),
					_ => String::new(),
				})
		})
		.trim()
		.to_string()
}

pub fn number_or(values: &Values, table: &'static [Setting], key: &str) -> i64 {
	values.number(key).unwrap_or_else(|| {
		table
			.iter()
			.find(|setting| setting.key == key)
			.map_or(0, |setting| match setting.default {
				Fallback::Number(value) => value,
				_ => 0,
			})
	})
}

/// A plugin's stored settings, read through each plugin's own TestCord defaults. The map is
/// cloned out of the registry so a plugin can reconfigure itself without borrowing the registry.
#[derive(Debug, Default)]
pub struct Values(BTreeMap<String, Value>);

impl Values {
	pub fn flag(&self, key: &str) -> Option<bool> {
		match self.0.get(key) {
			Some(Value::Bool(value)) => Some(*value),
			_ => None,
		}
	}
	pub fn text(&self, key: &str) -> Option<&str> {
		match self.0.get(key) {
			Some(Value::String(value)) => Some(value),
			_ => None,
		}
	}
	pub fn number(&self, key: &str) -> Option<i64> {
		match self.0.get(key) {
			Some(Value::Number(value)) => value.as_i64(),
			Some(Value::String(value)) => value.parse().ok(),
			_ => None,
		}
	}
}

/// What the host should do with an inbound message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
	Show,
	/// The message never entered the timeline, as if the service had not sent it.
	Ignore,
}

/// Conversation facts a plugin cannot learn from a message alone.
#[derive(Clone, Copy, Debug)]
pub struct Inbound {
	pub channel: Id,
	pub guild: Option<Id>,
	pub me: Id,
	/// Monotonic milliseconds, so cooldowns never read a wall clock.
	pub now: u64,
}

impl Inbound {
	pub fn new(channel: Id, guild: Option<Id>, me: Id, now: u64) -> Self {
		Self {
			channel,
			guild,
			me,
			now,
		}
	}
	pub fn is_direct(&self) -> bool {
		self.guild.is_none()
	}
}

/// Conversation facts for a message the owner is about to send.
#[derive(Clone, Copy, Debug)]
pub struct SendContext {
	pub channel: Id,
	pub me: Id,
}

impl SendContext {
	pub fn new(channel: Id, me: Id) -> Self {
		Self { channel, me }
	}
}

/// A reply the owner is sending, including the decision the composer made about the mention.
pub struct Reply<'a> {
	pub message: Id,
	pub author: Id,
	pub roles: &'a [Id],
	pub mention: &'a mut bool,
}

/// The body and reply of one outgoing message, which plugins may rewrite.
pub struct Outgoing<'a> {
	pub channel: Id,
	pub me: Id,
	pub body: &'a mut String,
	/// `None` unless the owner is replying to a message.
	pub reply: Option<Reply<'a>>,
}

/// A message edit as the host saw it: the timeline body before and after the patch.
#[derive(Clone, Copy)]
pub struct Edit<'a> {
	pub channel: Id,
	pub id: Id,
	pub author: &'a model::User,
	pub before: &'a str,
	pub after: &'a str,
}

/// A message a plugin asked the host to send once its delay elapsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingReply {
	pub channel: Id,
	pub content: String,
	/// Monotonic millisecond the reply became due.
	pub due: u64,
}

/// How the app should render clocks and markers, after every active plugin has had its say.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Display {
	/// Round relative phrases down instead of to the nearest.
	pub floor_relative: bool,
	pub hour: display::HourFormat,
	/// Minutes to shift every clock by, within a real time zone.
	pub offset_minutes: i32,
	pub hide_edited: bool,
	/// Keep messages from being marked as read while they are on screen.
	pub hold_read_ack: bool,
	/// The composer's counter, or `None` for the app's own near-limit counter.
	pub counter: Option<display::Counter>,
}

pub enum InboundEvent<'a> {
	Created(&'a Message),
	Edited(&'a Edit<'a>),
	Deleted {
		channel: Id,
		id: Id,
		last: Option<&'a Message>,
	},
	Other,
}

/// One entry a plugin adds to a message's own menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MessageAction {
	pub id: &'static str,
	pub label: &'static str,
}

/// What running an action produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionResult {
	Clipboard(String),
	Notice(&'static str),
}

pub trait Plugin {
	fn meta(&self) -> Meta;
	fn settings(&self) -> &'static [Setting] {
		&[]
	}
	/// Apply stored settings. Called on load and whenever one of them changes.
	fn configure(&mut self, _values: &Values) {}
	/// Forget remembered state, so a re-enabled plugin starts clean.
	fn reset(&mut self) {}
	/// Whether this message should be ignored, as TestCord's BlockKeywords does.
	fn ignore(&self, _message: &Message) -> bool {
		false
	}
	fn on_created(
		&mut self,
		_inbound: &Inbound,
		_message: &Message,
		_replies: &mut Vec<PendingReply>,
	) {
	}
	fn on_edited(&mut self, _inbound: &Inbound, _edit: &Edit<'_>) {}
	fn on_deleted(&mut self, _inbound: &Inbound, _channel: Id, _id: Id, _last: Option<&Message>) {}
	/// Rewrite the body or the reply mention, or refuse it with a reason the app shows
	/// instead of sending.
	fn before_send(&mut self, _outgoing: &mut Outgoing<'_>) -> Result<(), &'static str> {
		Ok(())
	}
	fn before_edit(&mut self, _outgoing: &mut Outgoing<'_>) -> Result<(), &'static str> {
		Ok(())
	}
	/// Split one oversized body into several bodies. The app sends them in order.
	fn split(&self, _context: &SendContext, _body: &str) -> Vec<String> {
		Vec::new()
	}
	/// How long the app should wait between the parts of one message.
	fn chunk_delay_ms(&self) -> Option<u64> {
		None
	}
	/// Rewrite an accepted inbound message before it enters the timeline.
	fn mutate_incoming(&mut self, _message: &mut Message) {}
	/// A rewrite applied while a message is formatted, never to the stored message.
	fn body_transform(&self) -> Option<body::BodyTransform> {
		None
	}
	/// What this plugin wants changed about message display.
	fn display(&self) -> display::DisplayPatch {
		display::DisplayPatch::default()
	}
	/// Entries this plugin adds to a message's menu.
	fn message_actions(&self) -> &'static [MessageAction] {
		&[]
	}
	/// Run one of this plugin's actions against a message.
	fn run_action(&self, _action: &str, _message: &Message) -> Option<ActionResult> {
		None
	}
	/// One-line status for the settings page.
	fn summary(&self) -> Option<String> {
		None
	}
	/// Plain-text log for the settings page, bounded by the plugin itself.
	fn export(&self) -> Option<String> {
		None
	}
}

pub(crate) struct Entry {
	pub enabled: bool,
	pub values: BTreeMap<String, Value>,
}

pub struct Registry {
	plugins: Vec<Box<dyn Plugin>>,
	entries: BTreeMap<String, Entry>,
	pending: Vec<PendingReply>,
}

impl Default for Registry {
	fn default() -> Self {
		Self::new()
	}
}

impl Registry {
	/// The bundled TestCord ports. Only pipelined plugins need host hooks, so only those ship.
	pub fn new() -> Self {
		let plugins: Vec<Box<dyn Plugin>> = vec![
			Box::new(clearurls::ClearUrls),
			Box::new(copy::CopyUserUrls),
			Box::new(copy::CopyUserMention),
			Box::new(copy::CopyStickerLinks::default()),
			Box::new(display::CustomTimestamps::default()),
			Box::new(display::DontRoundMyTimestamps),
			Box::new(display::NoEditedTimestamp),
			Box::new(display::CharacterCounter::default()),
			Box::new(display::StopAutoUnread),
			Box::new(body::Unindent),
			Box::new(sendtext::PolishWording::default()),
			Box::new(sendtext::ProfanityFilter::default()),
			Box::new(sendtext::JsTextReplace::default()),
			Box::new(sendtext::Signature::default()),
			Box::new(blockkeywords::BlockKeywords::default()),
			Box::new(silenceusers::SilenceUsers::default()),
			Box::new(splitlarge::SplitLargeMessages::default()),
			Box::new(noreplymention::NoReplyMention::default()),
			Box::new(autoreply::AutoReplyContent::default()),
			Box::new(messagelogger::MessageLogger::default()),
		];
		debug_assert!(plugins.len() <= MAX_PLUGINS);
		let entries = plugins
			.iter()
			.map(|plugin| {
				let meta = plugin.meta();
				(
					meta.id.to_string(),
					Entry {
						enabled: meta.default_enabled,
						values: BTreeMap::new(),
					},
				)
			})
			.collect();
		Self {
			plugins,
			entries,
			pending: Vec::new(),
		}
	}

	pub fn metas(&self) -> Vec<Meta> {
		self.plugins.iter().map(|plugin| plugin.meta()).collect()
	}

	pub fn meta(&self, id: &str) -> Option<Meta> {
		self.resolve(id).map(|index| self.plugins[index].meta())
	}

	pub fn settings_of(&self, id: &str) -> &'static [Setting] {
		self.resolve(id)
			.map_or(&[][..], |index| self.plugins[index].settings())
	}

	pub fn enabled(&self, id: &str) -> bool {
		self.resolve(id).is_some_and(|index| {
			self.entries
				.get(self.plugins[index].meta().id)
				.is_some_and(|entry| entry.enabled)
		})
	}

	pub fn set_enabled(&mut self, id: &str, enabled: bool) {
		let Some(index) = self.resolve(id) else {
			return;
		};
		let meta = self.plugins[index].meta();
		let entry = self.entries.entry(meta.id.to_string()).or_insert(Entry {
			enabled,
			values: BTreeMap::new(),
		});
		if entry.enabled == enabled {
			return;
		}
		entry.enabled = enabled;
		if !enabled {
			self.plugins[index].reset();
		}
	}

	pub fn value(&self, id: &str, key: &str) -> Option<&Value> {
		self.entries.get(id)?.values.get(key)
	}

	pub fn set_value(&mut self, id: &str, key: &str, value: Value) {
		let Some(index) = self.resolve(id) else {
			return;
		};
		let meta = self.plugins[index].meta();
		let entry = self.entries.entry(meta.id.to_string()).or_insert(Entry {
			enabled: false,
			values: BTreeMap::new(),
		});
		if entry.values.len() >= MAX_PLUGIN_VALUES && !entry.values.contains_key(key) {
			return;
		}
		entry.values.insert(key.to_string(), value);
		let values = Values(entry.values.clone());
		self.plugins[index].configure(&values);
	}

	/// Ids this registry answers to, including the names and aliases an imported file may use.
	pub fn aliases(&self, id: &str) -> Vec<&'static str> {
		let Some(index) = self.resolve(id) else {
			return Vec::new();
		};
		let meta = self.plugins[index].meta();
		let mut ids = vec![meta.id, meta.name];
		ids.extend(meta.aliases);
		ids.dedup();
		ids
	}

	/// Observe an inbound message. A plugin that ignores it stops the rest from seeing it.
	pub fn observe(&mut self, inbound: &Inbound, event: InboundEvent<'_>) -> Verdict {
		if !self.any_enabled() {
			return Verdict::Show;
		}
		let active = self.active();
		match event {
			InboundEvent::Created(message) => {
				if active
					.iter()
					.any(|&index| self.plugins[index].ignore(message))
				{
					return Verdict::Ignore;
				}
				let mut replies = std::mem::take(&mut self.pending);
				for index in active {
					self.plugins[index].on_created(inbound, message, &mut replies);
				}
				self.trim_replies(replies);
			}
			InboundEvent::Edited(edit) => {
				for index in active {
					self.plugins[index].on_edited(inbound, edit);
				}
			}
			InboundEvent::Deleted { channel, id, last } => {
				for index in active {
					self.plugins[index].on_deleted(inbound, channel, id, last);
				}
			}
			InboundEvent::Other => {}
		}
		Verdict::Show
	}

	pub fn before_send(&mut self, outgoing: &mut Outgoing<'_>) -> Result<(), &'static str> {
		if !self.any_enabled() {
			return Ok(());
		}
		for index in self.active() {
			self.plugins[index].before_send(outgoing)?;
		}
		Ok(())
	}

	pub fn before_edit(&mut self, outgoing: &mut Outgoing<'_>) -> Result<(), &'static str> {
		if !self.any_enabled() {
			return Ok(());
		}
		for index in self.active() {
			self.plugins[index].before_edit(outgoing)?;
		}
		Ok(())
	}

	/// The first plugin that wants a body split wins; the rest see nothing.
	pub fn split(&self, context: &SendContext, body: &str) -> Vec<String> {
		if !self.any_enabled() {
			return Vec::new();
		}
		self.active()
			.into_iter()
			.find_map(|index| {
				let parts = self.plugins[index].split(context, body);
				(!parts.is_empty()).then_some(parts)
			})
			.unwrap_or_default()
	}

	/// The first active plugin that wants bodies rewritten before they are formatted, with the
	/// id that owns it, so a host can tell one rewrite from another.
	pub fn body_transform(&self) -> Option<(&'static str, body::BodyTransform)> {
		if !self.any_enabled() {
			return None;
		}
		self.active().into_iter().find_map(|index| {
			self.plugins[index]
				.body_transform()
				.map(|transform| (self.plugins[index].meta().id, transform))
		})
	}

	/// Fold every active plugin's display wishes into one resolved view.
	pub fn display(&self) -> Display {
		let mut display = Display::default();
		for index in self.active() {
			let patch = self.plugins[index].display();
			display.floor_relative |= patch.floor_relative.unwrap_or(false);
			display.hide_edited |= patch.hide_edited.unwrap_or(false);
			display.hold_read_ack |= patch.hold_read_ack.unwrap_or(false);
			if let Some(counter) = patch.counter {
				display.counter = Some(counter);
			}
			if let Some(hour) = patch.hour {
				display.hour = hour;
			}
			if let Some(offset) = patch.offset_minutes {
				display.offset_minutes = offset;
			}
		}
		display
	}

	/// Every message-menu entry the active plugins offer, with the plugin that owns it.
	pub fn message_actions(&self) -> Vec<(&'static str, MessageAction)> {
		if !self.any_enabled() {
			return Vec::new();
		}
		self.active()
			.into_iter()
			.flat_map(|index| {
				let id = self.plugins[index].meta().id;
				self.plugins[index]
					.message_actions()
					.iter()
					.map(move |action| (id, *action))
					.collect::<Vec<_>>()
			})
			.collect()
	}

	/// Run one action, if the plugin that advertised it is enabled and answers.
	pub fn run_action(
		&mut self,
		plugin: &str,
		action: &str,
		message: &Message,
	) -> Option<ActionResult> {
		let index = self.resolve(plugin)?;
		if !self.enabled(plugin) {
			return None;
		}
		self.plugins[index].run_action(action, message)
	}

	/// The slowest delay any active plugin asked for between message parts.
	pub fn chunk_delay_ms(&self) -> u64 {
		if !self.any_enabled() {
			return 0;
		}
		self.active()
			.into_iter()
			.filter_map(|index| self.plugins[index].chunk_delay_ms())
			.max()
			.unwrap_or_default()
	}

	/// Let plugins rewrite an accepted message before the state owner sees it.
	pub fn mutate_incoming(&mut self, message: &mut Message) {
		if !self.any_enabled() {
			return;
		}
		for index in self.active() {
			self.plugins[index].mutate_incoming(message);
		}
	}

	/// Replies whose delay has elapsed. Later ones stay queued for a later frame.
	pub fn take_replies(&mut self, now: u64) -> Vec<PendingReply> {
		let (due, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending)
			.into_iter()
			.partition(|reply| reply.due <= now);
		self.pending = waiting;
		due
	}

	pub fn summary(&self, id: &str) -> Option<String> {
		self.resolve(id)
			.and_then(|index| self.plugins[index].summary())
	}

	pub fn export(&self, id: &str) -> Option<String> {
		self.resolve(id)
			.and_then(|index| self.plugins[index].export())
	}

	/// Re-apply stored settings to every plugin, after an import changed the whole file.
	pub fn reconfigure(&mut self) {
		for index in 0..self.plugins.len() {
			let meta = self.plugins[index].meta();
			let values = self
				.entries
				.get(meta.id)
				.map(|entry| Values(entry.values.clone()))
				.unwrap_or_default();
			self.plugins[index].configure(&values);
		}
	}

	pub(crate) fn stored_value(&self, id: &str) -> Option<(bool, &BTreeMap<String, Value>)> {
		self.entries
			.get(id)
			.map(|entry| (entry.enabled, &entry.values))
	}

	#[cfg(test)]
	pub(crate) fn stored_mut(&mut self) -> &mut BTreeMap<String, Entry> {
		&mut self.entries
	}

	fn resolve(&self, id: &str) -> Option<usize> {
		if let Some(index) = self.index_of(id) {
			return Some(index);
		}
		let lowered = id.to_lowercase();
		self.index_of(&lowered)
	}

	/// TestCord resolves a settings key by id, then name, then alias, then lowercase name.
	fn index_of(&self, id: &str) -> Option<usize> {
		self.plugins.iter().position(|plugin| {
			let meta = plugin.meta();
			meta.id == id
				|| meta
					.aliases
					.iter()
					.any(|alias| alias.eq_ignore_ascii_case(id))
		})
	}

	/// Keeps the message path allocation-free while every plugin is off, which is the default.
	pub fn any_enabled(&self) -> bool {
		self.entries.values().any(|entry| entry.enabled)
	}

	fn active(&self) -> Vec<usize> {
		self.plugins
			.iter()
			.enumerate()
			.filter(|(_, plugin)| self.enabled(plugin.meta().id))
			.map(|(index, _)| index)
			.collect()
	}

	fn trim_replies(&mut self, mut replies: Vec<PendingReply>) {
		let bytes = |replies: &[PendingReply]| {
			replies
				.iter()
				.map(|reply| reply.content.len())
				.sum::<usize>()
		};
		while replies.len() > MAX_PENDING_REPLIES
			|| bytes(&self.pending) + bytes(&replies) > MAX_PENDING_REPLY_BYTES
		{
			if replies.pop().is_none() {
				break;
			}
		}
		self.pending = replies;
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use model::Message;

	#[derive(Default)]
	struct Probe {
		created: usize,
		edits: usize,
		deletes: usize,
		ignores: bool,
		reply: Option<PendingReply>,
		reset: bool,
	}

	impl Plugin for Probe {
		fn meta(&self) -> Meta {
			Meta {
				id: "Probe",
				name: "Probe",
				description: "Counts the hooks it received.",
				authors: "tesktop2",
				tags: &["Utility"],
				aliases: &["ProbeAlias"],
				default_enabled: true,
			}
		}
		fn reset(&mut self) {
			self.reset = true;
		}
		fn ignore(&self, _message: &Message) -> bool {
			self.ignores
		}
		fn on_created(
			&mut self,
			_inbound: &Inbound,
			_message: &Message,
			replies: &mut Vec<PendingReply>,
		) {
			self.created += 1;
			replies.extend(self.reply.clone());
		}
		fn on_edited(&mut self, _inbound: &Inbound, _edit: &Edit<'_>) {
			self.edits += 1;
		}
		fn on_deleted(
			&mut self,
			_inbound: &Inbound,
			_channel: Id,
			_id: Id,
			_last: Option<&Message>,
		) {
			self.deletes += 1;
		}
		fn summary(&self) -> Option<String> {
			Some(format!(
				"{} created, {} edits, {} deletes, reset {}",
				self.created, self.edits, self.deletes, self.reset
			))
		}
	}

	fn registry(probe: Probe) -> Registry {
		let mut registry = Registry::new();
		registry.plugins.push(Box::new(probe));
		registry.entries.insert(
			"Probe".to_string(),
			Entry {
				enabled: true,
				values: BTreeMap::new(),
			},
		);
		registry
	}

	fn message(id: u64) -> Message {
		test_support::message(id, Id(7))
	}

	#[test]
	fn ignored_messages_reach_no_plugin() {
		let mut registry = registry(Probe {
			ignores: true,
			reply: Some(PendingReply {
				channel: Id(7),
				content: "hi".into(),
				due: 0,
			}),
			..Probe::default()
		});
		let inbound = Inbound::new(Id(7), None, Id(1), 0);
		let message = message(5);
		assert_eq!(
			registry.observe(&inbound, InboundEvent::Created(&message)),
			Verdict::Ignore
		);
		assert!(registry.take_replies(0).is_empty());
	}

	#[test]
	fn replies_wait_for_their_delay() {
		let mut registry = registry(Probe {
			reply: Some(PendingReply {
				channel: Id(7),
				content: "hi".into(),
				due: 500,
			}),
			..Probe::default()
		});
		let inbound = Inbound::new(Id(7), None, Id(1), 0);
		let message = message(5);
		assert_eq!(
			registry.observe(&inbound, InboundEvent::Created(&message)),
			Verdict::Show
		);
		assert!(registry.take_replies(499).is_empty());
		assert_eq!(registry.take_replies(500).len(), 1);
	}

	#[test]
	fn pending_replies_stay_within_their_ceiling() {
		let reply = PendingReply {
			channel: Id(7),
			content: "x".repeat(1024),
			due: 0,
		};
		let mut registry = registry(Probe {
			reply: Some(reply.clone()),
			..Probe::default()
		});
		let inbound = Inbound::new(Id(7), None, Id(1), 0);
		for id in 1..=40 {
			let message = message(id);
			registry.observe(&inbound, InboundEvent::Created(&message));
		}
		let mut sent = 0;
		while sent < MAX_PENDING_REPLIES * 2 {
			let due = registry.take_replies(0);
			if due.is_empty() {
				break;
			}
			assert!(due.len() <= MAX_PENDING_REPLIES);
			sent += due.len();
		}
	}

	#[test]
	fn disabling_forgets_state_and_stops_hooks() {
		let mut registry = registry(Probe::default());
		let inbound = Inbound::new(Id(7), None, Id(1), 0);
		let message = message(5);
		registry.observe(&inbound, InboundEvent::Created(&message));
		assert_eq!(
			registry.summary("Probe").as_deref(),
			Some("1 created, 0 edits, 0 deletes, reset false")
		);
		registry.set_enabled("ProbeAlias", false);
		assert!(!registry.enabled("Probe"));
		assert!(!registry.stored_mut()["Probe"].enabled);
		registry.observe(&inbound, InboundEvent::Created(&message));
		assert_eq!(
			registry.summary("Probe").as_deref(),
			Some("1 created, 0 edits, 0 deletes, reset true")
		);
	}

	#[test]
	fn message_actions_follow_the_enabled_set() {
		let mut registry = Registry::new();
		assert!(registry.message_actions().is_empty());
		registry.set_enabled("CopyUserURLs", true);
		let actions = registry.message_actions();
		assert_eq!(actions.len(), 1);
		assert_eq!(actions[0].0, "CopyUserURLs");
		assert_eq!(actions[0].1.id, "user-url");

		let mut message = test_support::message(5, Id(7));
		message.author.id = Id(3);
		assert_eq!(
			registry.run_action("CopyUserURLs", "user-url", &message),
			Some(ActionResult::Clipboard(
				"<https://discord.com/users/3>".into()
			))
		);
		assert!(
			registry
				.run_action("CopyUserURLs", "nope", &message)
				.is_none()
		);

		registry.set_enabled("CopyUserURLs", false);
		assert!(
			registry
				.run_action("CopyUserURLs", "user-url", &message)
				.is_none()
		);
		assert!(registry.message_actions().is_empty());
	}

	#[test]
	fn an_all_off_registry_stays_out_of_the_message_path() {
		let mut registry = Registry::new();
		for meta in registry.metas() {
			registry.set_enabled(meta.id, false);
		}
		assert!(!registry.any_enabled());
		let inbound = Inbound::new(Id(7), None, Id(1), 0);
		let message = message(5);
		assert_eq!(
			registry.observe(&inbound, InboundEvent::Created(&message)),
			Verdict::Show
		);
		let mut body = "https://example.com/?utm_source=x".to_string();
		let mut outgoing = Outgoing {
			channel: Id(7),
			me: Id(1),
			body: &mut body,
			reply: None,
		};
		assert!(registry.before_send(&mut outgoing).is_ok());
		assert_eq!(body, "https://example.com/?utm_source=x");
	}

	#[test]
	fn ids_resolve_by_alias_name_and_case() {
		let registry = Registry::new();
		assert_eq!(
			registry.meta("clearurls").map(|meta| meta.id),
			Some("ClearURLs")
		);
		assert_eq!(
			registry.meta("ClearURLs").map(|meta| meta.id),
			Some("ClearURLs")
		);
		assert!(registry.meta("NothingHere").is_none());
	}
}
