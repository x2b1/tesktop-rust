//! NoReplyMention: decides whether your reply pings the person you are replying to.
//!
//! TestCord reads the Shift key at send time and decides from it. This client shows an explicit
//! mention switch in the reply header instead, so the port applies a policy at send time and the
//! switch keeps working the way it does for every other send.

use crate::{Fallback, Meta, Outgoing, Setting, SettingKind, Values, text_or};
use model::Id;

const SETTINGS: &[Setting] = &[
	Setting {
		key: "policy",
		label: "When replying",
		kind: SettingKind::Choice(&[
			("suppress", "Never ping"),
			("onlyListed", "Ping only the listed users or roles"),
			("leave", "Leave the mention as chosen"),
		]),
		default: Fallback::Text("suppress"),
	},
	Setting {
		key: "userList",
		label: "Exempt user ids",
		kind: SettingKind::Text { multiline: true },
		default: Fallback::Text(""),
	},
	Setting {
		key: "roleList",
		label: "Exempt role ids",
		kind: SettingKind::Text { multiline: true },
		default: Fallback::Text(""),
	},
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Policy {
	#[default]
	Suppress,
	OnlyListed,
	Leave,
}

#[derive(Default)]
pub struct NoReplyMention {
	policy: Policy,
	users: Vec<Id>,
	roles: Vec<Id>,
}

impl NoReplyMention {
	fn is_exempt(&self, author: Id, roles: &[Id]) -> bool {
		self.users.contains(&author) || roles.iter().any(|role| self.roles.contains(role))
	}
}

impl crate::Plugin for NoReplyMention {
	fn meta(&self) -> Meta {
		Meta {
			id: "NoReplyMention",
			name: "NoReplyMention",
			description: "Decides whether your reply pings the author.",
			authors: "DustyAngel47, rae, pylix, outfoxxed",
			tags: &["Chat", "Notifications"],
			aliases: &["noReplyMention"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.policy = match text_or(values, SETTINGS, "policy").as_str() {
			"onlyListed" => Policy::OnlyListed,
			"leave" => Policy::Leave,
			_ => Policy::Suppress,
		};
		self.users = ids(&text_or(values, SETTINGS, "userList"));
		self.roles = ids(&text_or(values, SETTINGS, "roleList"));
	}

	fn before_send(&mut self, outgoing: &mut Outgoing<'_>) -> Result<(), &'static str> {
		let Some(reply) = outgoing.reply.as_mut() else {
			return Ok(());
		};
		match self.policy {
			Policy::Leave => {}
			Policy::Suppress => *reply.mention = false,
			Policy::OnlyListed => *reply.mention = self.is_exempt(reply.author, reply.roles),
		}
		Ok(())
	}

	fn summary(&self) -> Option<String> {
		match self.policy {
			Policy::Leave => Some("Your mention choice is kept".to_string()),
			_ => Some(format!(
				"{} exempt user{}, {} exempt role{}",
				self.users.len(),
				if self.users.len() == 1 { "" } else { "s" },
				self.roles.len(),
				if self.roles.len() == 1 { "" } else { "s" }
			)),
		}
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

	fn plugin(policy: &str, users: &str, roles: &str) -> NoReplyMention {
		let mut plugin = NoReplyMention::default();
		plugin.configure(&Values(
			[
				("policy".to_string(), serde_json::json!(policy)),
				("userList".to_string(), serde_json::json!(users)),
				("roleList".to_string(), serde_json::json!(roles)),
			]
			.into_iter()
			.collect(),
		));
		plugin
	}

	fn send(plugin: &mut NoReplyMention, author: Id, roles: &[Id], mention: bool) -> bool {
		let mut body = "hello".to_string();
		let mut chosen = mention;
		let mut outgoing = Outgoing {
			channel: Id(7),
			me: Id(1),
			body: &mut body,
			reply: Some(crate::Reply {
				message: Id(99),
				author,
				roles,
				mention: &mut chosen,
			}),
		};
		plugin.before_send(&mut outgoing).unwrap();
		chosen
	}

	#[test]
	fn the_default_suppresses_the_ping() {
		assert!(!send(&mut plugin("suppress", "", ""), Id(2), &[], true));
	}

	#[test]
	fn only_listed_mentions_the_exempted_people() {
		let mut plugin = plugin("onlyListed", "2 3", "10");
		assert!(send(&mut plugin, Id(2), &[], false));
		assert!(send(&mut plugin, Id(9), &[Id(10)], false));
		assert!(!send(&mut plugin, Id(9), &[Id(11)], true));
	}

	#[test]
	fn leaving_the_mention_alone_changes_nothing() {
		let mut plugin = plugin("leave", "", "");
		assert!(send(&mut plugin, Id(2), &[], true));
		assert!(!send(&mut plugin, Id(2), &[], false));
	}

	#[test]
	fn a_message_that_is_not_a_reply_is_untouched() {
		let mut plugin = plugin("suppress", "", "");
		let mut body = "hello".to_string();
		let mut outgoing = Outgoing {
			channel: Id(7),
			me: Id(1),
			body: &mut body,
			reply: None,
		};
		plugin.before_send(&mut outgoing).unwrap();
		assert_eq!(body, "hello");
	}

	#[test]
	fn the_lists_are_bounded_and_deduplicated() {
		let many: String = (1..1000).map(|id| format!("{id} ")).collect();
		let plugin = plugin("onlyListed", &many, &many);
		assert!(plugin.users.len() <= 256);
		assert_eq!(plugin.users, plugin.roles);
		assert_eq!(plugin.users[0], Id(1));
	}

	#[test]
	fn an_unknown_policy_falls_back_to_the_safe_one() {
		assert_eq!(
			plugin("nonsense", "", "").summary().as_deref(),
			Some("0 exempt users, 0 exempt roles")
		);
	}
}
