//! Burst ports: several quick messages become one instead of a wall of them.

use crate::{Fallback, Meta, Outgoing, Route, Setting, SettingKind, Values, flag_or, number_or};
#[cfg(test)]
use model::Id;

const BURST_SETTINGS: &[Setting] = &[
	Setting {
		key: "timePeriod",
		label: "How long a burst lasts (seconds)",
		kind: SettingKind::Number { min: 1, max: 300 },
		default: Fallback::Number(3),
	},
	Setting {
		key: "shouldMergeWithAttachment",
		label: "Merge into a message that has a file",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
	Setting {
		key: "useSpace",
		label: "Join with a space instead of a new line",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
];

/// MessageBurst: a second message inside the window edits the first instead of sending.
#[derive(Default)]
pub struct MessageBurst {
	window_ms: u64,
	merge_attachments: bool,
	use_space: bool,
}

impl crate::Plugin for MessageBurst {
	fn meta(&self) -> Meta {
		Meta {
			id: "MessageBurst",
			name: "MessageBurst",
			description: "Folds quick successive messages into the one before them.",
			authors: "Mavri",
			tags: &["Chat", "Utility"],
			aliases: &["messageBurst"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		BURST_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.window_ms =
			(number_or(values, BURST_SETTINGS, "timePeriod").clamp(1, 300) as u64) * 1000;
		self.merge_attachments = flag_or(values, BURST_SETTINGS, "shouldMergeWithAttachment");
		self.use_space = flag_or(values, BURST_SETTINGS, "useSpace");
	}

	fn route(&mut self, outgoing: &mut Outgoing<'_>) -> bool {
		let Some(previous) = outgoing.previous else {
			return false;
		};
		if previous.author != outgoing.me
			|| previous.age_ms > self.window_ms
			|| previous.replying
			|| outgoing.reply.is_some()
			|| (previous.attachments > 0 && !self.merge_attachments)
			// A group message names the member; folding into it would rename the group.
			|| (previous.is_group && previous.content == *outgoing.body)
		{
			return false;
		}
		let separator = if self.use_space { " " } else { "\n" };
		let previous_body = previous.content.clone();
		let merged = format!("{previous_body}{separator}{}", *outgoing.body);
		*outgoing.body = merged;
		outgoing.route = Route::EditPrevious;
		true
	}

	fn summary(&self) -> Option<String> {
		Some(format!(
			"Merges within {}s{}",
			self.window_ms / 1000,
			if self.use_space {
				", space separated"
			} else {
				""
			}
		))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::Plugin;

	fn plugin(settings: &[(&str, serde_json::Value)]) -> MessageBurst {
		let mut plugin = MessageBurst::default();
		plugin.configure(&Values(
			settings
				.iter()
				.map(|(key, value)| ((*key).to_string(), value.clone()))
				.collect(),
		));
		plugin
	}

	fn previous() -> crate::Previous {
		crate::Previous {
			id: Id(50),
			author: Id(1),
			content: "first".into(),
			attachments: 0,
			age_ms: 500,
			is_group: false,
			replying: false,
		}
	}

	fn send(
		plugin: &mut MessageBurst,
		previous: Option<crate::Previous>,
		body: &str,
	) -> (Route, String) {
		let mut text = body.to_string();
		let mut outgoing = Outgoing {
			channel: Id(7),
			me: Id(1),
			body: &mut text,
			reply: None,
			previous: previous.as_ref(),
			route: Route::Send,
		};
		plugin.route(&mut outgoing);
		(outgoing.route, outgoing.body.clone())
	}

	#[test]
	fn a_second_message_becomes_an_edit() {
		let mut burst = plugin(&[]);
		let (route, body) = send(&mut burst, Some(previous()), "second");
		assert_eq!(route, Route::EditPrevious);
		assert_eq!(body, "first\nsecond");
	}

	#[test]
	fn a_space_can_join_instead() {
		let mut burst = plugin(&[("useSpace", true.into())]);
		assert_eq!(
			send(&mut burst, Some(previous()), "second").1,
			"first second"
		);
	}

	#[test]
	fn a_stale_or_foreign_message_is_never_touched() {
		let mut burst = plugin(&[]);
		let mut old = previous();
		old.age_ms = 4_000;
		assert_eq!(send(&mut burst, Some(old), "second").0, Route::Send);
		let mut theirs = previous();
		theirs.author = Id(9);
		assert_eq!(send(&mut burst, Some(theirs), "second").0, Route::Send);
		assert_eq!(send(&mut burst, None, "second").0, Route::Send);
	}

	#[test]
	fn replies_and_attachments_block_a_merge() {
		let mut burst = plugin(&[]);
		let mut replying = previous();
		replying.replying = true;
		assert_eq!(send(&mut burst, Some(replying), "second").0, Route::Send);

		let mut attached = previous();
		attached.attachments = 1;
		assert_eq!(
			send(&mut burst, Some(attached.clone()), "second").0,
			Route::Send
		);
		let mut merging = plugin(&[("shouldMergeWithAttachment", true.into())]);
		assert_eq!(
			send(&mut merging, Some(attached), "second").0,
			Route::EditPrevious
		);
	}

	#[test]
	fn a_group_named_after_the_message_is_left_alone() {
		let mut burst = plugin(&[]);
		let mut group = previous();
		group.is_group = true;
		group.content = "kaz".into();
		assert_eq!(send(&mut burst, Some(group), "kaz").0, Route::Send);
	}

	#[test]
	fn only_the_first_plugin_that_claims_a_send_wins() {
		let mut registry = crate::Registry::new();
		registry.set_enabled("MessageBurst", true);
		let mut text = "second".to_string();
		let mut outgoing = Outgoing {
			channel: Id(7),
			me: Id(1),
			body: &mut text,
			reply: None,
			previous: Some(&previous()),
			route: Route::Send,
		};
		assert!(registry.route(&mut outgoing));
		assert_eq!(outgoing.route, Route::EditPrevious);
		registry.set_enabled("MessageBurst", false);
		let mut again = "third".to_string();
		let mut outgoing = Outgoing {
			channel: Id(7),
			me: Id(1),
			body: &mut again,
			reply: None,
			previous: Some(&previous()),
			route: Route::Send,
		};
		assert!(!registry.route(&mut outgoing));
		assert_eq!(outgoing.route, Route::Send);
	}

	#[test]
	fn the_window_is_never_absurd() {
		let burst = plugin(&[("timePeriod", 100_000.into())]);
		assert!(burst.window_ms <= 300_000);
		assert_eq!(burst.summary().as_deref(), Some("Merges within 300s"));
		let burst = plugin(&[("timePeriod", 0.into())]);
		assert_eq!(burst.window_ms, 1000);
	}
}
