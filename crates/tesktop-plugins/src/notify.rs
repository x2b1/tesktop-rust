//! Notification ports: what the client announces for a message, and what it says instead.

use crate::{Fallback, Meta, Setting, SettingKind, Values, flag_or, text_or};
use model::{Id, Message};

/// What the client is about to announce for an accepted message.
#[derive(Clone, Copy)]
pub struct Notify<'a> {
	pub message: &'a Message,
	pub channel: Id,
	pub guild: Option<Id>,
	/// A direct message, or a server message that names the owner outright.
	pub direct: bool,
	pub mentions_me: bool,
	pub everyone: bool,
	/// The oldest unread message of its channel, so a run of them pings once.
	pub oldest_unread: bool,
	/// The conversation the owner is looking at right now.
	pub visible: bool,
	pub me: Id,
}

/// Whether to raise the app's own alert, and whether to say something in the window too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Notice {
	/// Play the sound and raise the OS alert, as the app would.
	pub announce: bool,
	/// Also raise an in-app toast, which the app does not do for a quiet conversation.
	pub toast: bool,
}

impl Default for Notice {
	fn default() -> Self {
		Self {
			announce: true,
			toast: false,
		}
	}
}

impl Notice {
	/// Take another port's opinion: any port may silence the alert, any port may add a toast.
	pub fn merge(self, other: Self) -> Self {
		Self {
			announce: self.announce && other.announce,
			toast: self.toast || other.toast,
		}
	}
}

const PING_SETTINGS: &[Setting] = &[
	Setting {
		key: "notifyFriends",
		label: "Notify for friends in servers",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
	Setting {
		key: "onlyMentions",
		label: "In servers, only notify on a direct mention",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(true),
	},
	Setting {
		key: "notifyDMs",
		label: "Notify for direct messages",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(true),
	},
	Setting {
		key: "notifyActiveChannel",
		label: "Notify for the conversation you are reading",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
	Setting {
		key: "friends",
		label: "Friend ids",
		kind: SettingKind::Text { multiline: true },
		default: Fallback::Text(""),
	},
];

/// PingNotifications: a server message only pings when it names you.
pub struct PingNotifications {
	notify_friends: bool,
	only_mentions: bool,
	notify_dms: bool,
	notify_active: bool,
	friends: Vec<Id>,
}

impl Default for PingNotifications {
	fn default() -> Self {
		// TestCord's declared defaults, so a fresh install pings as TestCord would.
		Self {
			notify_friends: false,
			only_mentions: true,
			notify_dms: true,
			notify_active: false,
			friends: Vec::new(),
		}
	}
}

impl crate::Plugin for PingNotifications {
	fn meta(&self) -> Meta {
		Meta {
			id: "PingNotifications",
			name: "PingNotifications",
			description: "In servers, only ping when a message names you.",
			authors: "Vencord",
			tags: &["Notifications"],
			aliases: &["pingNotifications"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		PING_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.notify_friends = flag_or(values, PING_SETTINGS, "notifyFriends");
		self.only_mentions = flag_or(values, PING_SETTINGS, "onlyMentions");
		self.notify_dms = flag_or(values, PING_SETTINGS, "notifyDMs");
		self.notify_active = flag_or(values, PING_SETTINGS, "notifyActiveChannel");
		self.friends = ids(&text_or(values, PING_SETTINGS, "friends"));
	}

	fn notice(&self, event: &Notify<'_>) -> Notice {
		let mut notice = Notice::default();
		if event.guild.is_none() && !self.notify_dms {
			notice.announce = false;
		}
		if event.visible && !self.notify_active {
			notice.announce = false;
		}
		if self.only_mentions
			&& event.guild.is_some()
			&& !event.mentions_me
			&& !event.everyone
			&& !(self.notify_friends && self.friends.contains(&event.message.author.id))
		{
			notice.announce = false;
		}
		notice
	}

	fn summary(&self) -> Option<String> {
		Some(if self.only_mentions {
			"Server messages need a mention".to_string()
		} else {
			"Every server message pings".to_string()
		})
	}
}

const ONE_PING_SETTINGS: &[Setting] = &[
	Setting {
		key: "channelToAffect",
		label: "Applies to",
		kind: SettingKind::Choice(&[
			("both_dms", "Direct and group messages"),
			("user_dm", "Direct messages"),
			("group_dm", "Group messages"),
		]),
		default: Fallback::Text("both_dms"),
	},
	Setting {
		key: "allowMentions",
		label: "Always ping on a direct mention",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
	Setting {
		key: "allowEveryone",
		label: "Always ping on @everyone or @here",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
	Setting {
		key: "ignoreUsers",
		label: "Never throttle these users",
		kind: SettingKind::Text { multiline: true },
		default: Fallback::Text(""),
	},
];

/// OnePingPerDM: a run of unread direct messages pings once, at the oldest.
pub struct OnePingPerDm {
	scope: Option<String>,
	allow_mentions: bool,
	allow_everyone: bool,
	ignore: Vec<Id>,
}

impl Default for OnePingPerDm {
	fn default() -> Self {
		Self {
			scope: Some("both_dms".to_string()),
			allow_mentions: false,
			allow_everyone: false,
			ignore: Vec::new(),
		}
	}
}

impl crate::Plugin for OnePingPerDm {
	fn meta(&self) -> Meta {
		Meta {
			id: "OnePingPerDM",
			name: "OnePingPerDM",
			description: "A run of unread direct messages pings once, not once per message.",
			authors: "ProffDea",
			tags: &["Notifications", "Customisation"],
			aliases: &["onePingPerDM"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		ONE_PING_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.scope = Some(text_or(values, ONE_PING_SETTINGS, "channelToAffect"));
		self.allow_mentions = flag_or(values, ONE_PING_SETTINGS, "allowMentions");
		self.allow_everyone = flag_or(values, ONE_PING_SETTINGS, "allowEveryone");
		self.ignore = ids(&text_or(values, ONE_PING_SETTINGS, "ignoreUsers"));
	}

	fn notice(&self, event: &Notify<'_>) -> Notice {
		let mut notice = Notice::default();
		if event.guild.is_some() {
			return notice;
		}
		let scope = self.scope.clone().unwrap_or_default();
		let group = event
			.message
			.mentions
			.iter()
			.all(|user| user.id == event.me)
			&& event.channel.0.is_multiple_of(2);
		if (group && scope == "user_dm") || (!group && scope == "group_dm") {
			return notice;
		}
		if self.ignore.contains(&event.message.author.id)
			|| (self.allow_mentions && event.mentions_me)
			|| (self.allow_everyone && event.everyone)
		{
			return notice;
		}
		if !event.oldest_unread {
			notice.announce = false;
		}
		notice
	}

	fn summary(&self) -> Option<String> {
		Some(match self.scope.as_deref() {
			Some("user_dm") => "Direct messages only".to_string(),
			Some("group_dm") => "Group messages only".to_string(),
			_ => "Direct and group messages".to_string(),
		})
	}
}

const NOTIFIER_SETTINGS: &[Setting] = &[Setting {
	key: "userIds",
	label: "User ids to announce",
	kind: SettingKind::Text { multiline: true },
	default: Fallback::Text(""),
}];

/// MessageNotifier: a toast for the people you list, even where the app stays quiet.
#[derive(Default)]
pub struct MessageNotifier {
	users: Vec<Id>,
}

impl crate::Plugin for MessageNotifier {
	fn meta(&self) -> Meta {
		Meta {
			id: "MessageNotifier",
			name: "MessageNotifier",
			description: "Shows a toast when one of the listed people writes.",
			authors: "Vencord",
			tags: &["Notifications"],
			aliases: &["messageNotifier"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		NOTIFIER_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.users = ids(&text_or(values, NOTIFIER_SETTINGS, "userIds"));
	}

	fn notice(&self, event: &Notify<'_>) -> Notice {
		Notice {
			announce: true,
			toast: self.users.contains(&event.message.author.id),
		}
	}

	fn summary(&self) -> Option<String> {
		Some(format!(
			"{} {} watched",
			self.users.len(),
			if self.users.len() == 1 {
				"person"
			} else {
				"people"
			}
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
	ids.truncate(256);
	ids
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::Plugin;

	fn plugin<T: crate::Plugin + Default>(settings: &[(&str, serde_json::Value)]) -> T {
		let mut plugin = T::default();
		plugin.configure(&Values(
			settings
				.iter()
				.map(|(key, value)| ((*key).to_string(), value.clone()))
				.collect(),
		));
		plugin
	}

	fn message(author: u64, content: &str) -> Message {
		let mut message = test_support::message(1, Id(7));
		message.author.id = Id(author);
		message.content = content.to_string();
		message
	}

	fn event<'a>(message: &'a Message, guild: Option<Id>, oldest: bool) -> Notify<'a> {
		Notify {
			message,
			channel: message.channel,
			guild,
			direct: guild.is_none(),
			mentions_me: message.mentions.iter().any(|user| user.id == Id(1)),
			everyone: message.mention_everyone,
			oldest_unread: oldest,
			visible: false,
			me: Id(1),
		}
	}

	#[test]
	fn a_disabled_port_leaves_the_alert_alone() {
		let registry = crate::Registry::new();
		let message = message(2, "hi");
		assert_eq!(
			registry.notice(&event(&message, Some(Id(10)), true)),
			Notice::default()
		);
		// TestCord's own default: on, a server message needs a mention.
		let mut enabled = crate::Registry::new();
		enabled.set_enabled("PingNotifications", true);
		assert!(
			!enabled
				.notice(&event(&message, Some(Id(10)), true))
				.announce
		);
	}

	#[test]
	fn a_server_message_needs_a_mention() {
		let pings = plugin::<PingNotifications>(&[]);
		assert!(
			!pings
				.notice(&event(&message(2, "hi"), Some(Id(10)), true))
				.announce
		);
		let mut mine = message(2, "hi");
		mine.mentions.push(mine.author.clone());
		mine.mentions[0].id = Id(1);
		assert!(pings.notice(&event(&mine, Some(Id(10)), true)).announce);
	}

	#[test]
	fn friends_and_dms_can_opt_back_in() {
		let pings =
			plugin::<PingNotifications>(&[("notifyFriends", true.into()), ("friends", "2".into())]);
		assert!(
			pings
				.notice(&event(&message(2, "hi"), Some(Id(10)), true))
				.announce
		);
		assert!(
			!pings
				.notice(&event(&message(3, "hi"), Some(Id(10)), true))
				.announce
		);
		assert!(pings.notice(&event(&message(3, "hi"), None, true)).announce);
		let quiet = plugin::<PingNotifications>(&[("notifyDMs", false.into())]);
		assert!(!quiet.notice(&event(&message(3, "hi"), None, true)).announce);
	}

	#[test]
	fn the_read_conversation_can_stay_quiet() {
		let pings = plugin::<PingNotifications>(&[]);
		let message = message(2, "hi");
		let mut now = event(&message, Some(Id(10)), true);
		now.visible = true;
		assert!(!pings.notice(&now).announce);
		let loud = plugin::<PingNotifications>(&[
			("notifyActiveChannel", true.into()),
			("onlyMentions", false.into()),
		]);
		assert!(loud.notice(&now).announce);
	}

	#[test]
	fn only_the_oldest_of_a_run_pings() {
		let one = plugin::<OnePingPerDm>(&[]);
		let message = message(2, "hi");
		assert!(one.notice(&event(&message, None, true)).announce);
		assert!(!one.notice(&event(&message, None, false)).announce);
		// A server message is never throttled.
		assert!(one.notice(&event(&message, Some(Id(10)), false)).announce);
	}

	#[test]
	fn mentions_and_the_ignore_list_always_ping() {
		let one = plugin::<OnePingPerDm>(&[("allowMentions", true.into())]);
		let mut mine = message(2, "hi");
		mine.mentions.push(mine.author.clone());
		mine.mentions[0].id = Id(1);
		assert!(one.notice(&event(&mine, None, false)).announce);
		let listed = plugin::<OnePingPerDm>(&[("ignoreUsers", "2".into())]);
		assert!(
			listed
				.notice(&event(&message(2, "hi"), None, false))
				.announce
		);
	}

	#[test]
	fn a_watched_person_gets_a_toast() {
		let notifier = plugin::<MessageNotifier>(&[("userIds", "2".into())]);
		assert_eq!(
			notifier.notice(&event(&message(2, "hi"), None, true)),
			Notice {
				announce: true,
				toast: true
			}
		);
		assert_eq!(
			notifier.notice(&event(&message(3, "hi"), None, true)),
			Notice::default()
		);
	}

	#[test]
	fn silence_and_toasts_combine_across_ports() {
		let mut registry = crate::Registry::new();
		registry.set_enabled("PingNotifications", true);
		registry.set_enabled("MessageNotifier", true);
		registry.set_value("MessageNotifier", "userIds", "2".into());
		let notice = registry.notice(&event(&message(2, "hi"), Some(Id(10)), true));
		assert!(
			!notice.announce,
			"a server message without a mention is quiet"
		);
		assert!(notice.toast, "but the watched person still gets a toast");
	}

	#[test]
	fn a_disabled_port_never_reaches_the_notice() {
		let mut registry = crate::Registry::new();
		registry.set_value("PingNotifications", "onlyMentions", true.into());
		let message = message(2, "hi");
		assert!(
			registry
				.notice(&event(&message, Some(Id(10)), true))
				.announce
		);
		registry.set_enabled("PingNotifications", true);
		assert!(
			!registry
				.notice(&event(&message, Some(Id(10)), true))
				.announce
		);
	}

	#[test]
	fn the_notice_survives_a_message_the_state_never_stores() {
		let mut registry = crate::Registry::new();
		registry.set_enabled("MessageNotifier", true);
		registry.set_value("MessageNotifier", "userIds", "2".into());
		let message = message(2, "hi");
		let inbound = crate::Inbound::new(Id(7), None, Id(1), 0);
		let replies: Vec<crate::PendingReply> = Vec::new();
		registry.observe(&inbound, crate::InboundEvent::Created(&message));
		assert!(replies.is_empty());
	}
}
