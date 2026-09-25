//! The clipboard ports that hang off a message: its author's link, a mention for it, and the
//! links of the stickers it carries.

use crate::{Fallback, MessageAction, Meta, Setting, SettingKind, Values, flag_or};
use model::Message;

/// Discord's CDN host; the native client never reads `window.GLOBAL_ENV`, so it is fixed here.
const CDN: &str = "https://cdn.discordapp.com";
const STICKER_QUERY: &str = "?size=512&lossless=true";

/// `<https://discord.com/users/1>` is Discord's own user permalink form.
pub fn user_url(id: model::Id) -> String {
	format!("<https://discord.com/users/{id}>")
}

pub fn user_mention(id: model::Id) -> String {
	format!("<@{id}>")
}

/// Sticker format types map to file extensions the way Discord's client builds them.
pub fn sticker_url(id: model::Id, format_type: u8) -> Option<String> {
	let extension = match format_type {
		1 | 2 => "png",
		3 => "json",
		4 => "gif",
		_ => return None,
	};
	Some(format!("{CDN}/stickers/{id}.{extension}{STICKER_QUERY}"))
}

pub struct CopyUserUrls;

impl crate::Plugin for CopyUserUrls {
	fn meta(&self) -> Meta {
		Meta {
			id: "CopyUserURLs",
			name: "CopyUserURLs",
			description: "Adds a Copy user link entry to a message.",
			authors: "castdrian",
			tags: &["Utility", "Friends"],
			aliases: &["copyUserURLs"],
			default_enabled: false,
		}
	}

	fn message_actions(&self) -> &'static [MessageAction] {
		&[MessageAction {
			id: "user-url",
			label: "Copy user link",
		}]
	}

	fn run_action(&self, action: &str, message: &Message) -> Option<crate::ActionResult> {
		(action == "user-url").then(|| crate::ActionResult::Clipboard(user_url(message.author.id)))
	}
}

pub struct CopyUserMention;

impl crate::Plugin for CopyUserMention {
	fn meta(&self) -> Meta {
		Meta {
			id: "CopyUserMention",
			name: "CopyUserMention",
			description: "Adds a Copy mention entry to a message.",
			authors: "Vencord",
			tags: &["Utility", "Chat"],
			aliases: &["copyUserMention"],
			default_enabled: false,
		}
	}

	fn message_actions(&self) -> &'static [MessageAction] {
		&[MessageAction {
			id: "user-mention",
			label: "Copy mention",
		}]
	}

	fn run_action(&self, action: &str, message: &Message) -> Option<crate::ActionResult> {
		(action == "user-mention")
			.then(|| crate::ActionResult::Clipboard(user_mention(message.author.id)))
	}
}

const SETTINGS: &[Setting] = &[Setting {
	key: "copyAnimatedAsPng",
	label: "Copy animated stickers as a still image",
	kind: SettingKind::Toggle,
	default: Fallback::Flag(false),
}];

#[derive(Default)]
pub struct CopyStickerLinks {
	still: Option<bool>,
}

impl crate::Plugin for CopyStickerLinks {
	fn meta(&self) -> Meta {
		Meta {
			id: "CopyStickerLinks",
			name: "CopyStickerLinks",
			description: "Adds Copy sticker link entries to a message that carries stickers.",
			authors: "Vencord",
			tags: &["Emotes", "Utility"],
			aliases: &["copyStickerLinks"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.still = Some(flag_or(values, SETTINGS, "copyAnimatedAsPng"));
	}

	fn message_actions(&self) -> &'static [MessageAction] {
		&[MessageAction {
			id: "sticker-link",
			label: "Copy sticker link",
		}]
	}

	fn run_action(&self, action: &str, message: &Message) -> Option<crate::ActionResult> {
		if action != "sticker-link" {
			return None;
		}
		let still = self.still.unwrap_or(false);
		message
			.sticker_items
			.iter()
			.find_map(|sticker| {
				let format = if still && sticker.format_type == 4 {
					2
				} else {
					sticker.format_type
				};
				sticker_url(sticker.id, format)
			})
			.map(crate::ActionResult::Clipboard)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{ActionResult, Inbound, Plugin};
	use model::Id;

	fn message_with_sticker(format_type: u8) -> Message {
		let mut message = test_support::message(3, Id(7));
		message.sticker_items.push(model::Sticker {
			id: Id(42),
			name: "Wave".into(),
			description: String::new(),
			tags: String::new(),
			format_type,
			guild_id: None,
			pack_id: None,
			available: true,
		});
		message
	}

	fn run(plugin: &dyn crate::Plugin, action: &str, message: &Message) -> Option<String> {
		match plugin.run_action(action, message) {
			Some(ActionResult::Clipboard(text)) => Some(text),
			_ => None,
		}
	}

	#[test]
	fn user_links_and_mentions_match_discord() {
		assert_eq!(user_url(Id(1)), "<https://discord.com/users/1>");
		assert_eq!(user_mention(Id(1)), "<@1>");
	}

	#[test]
	fn the_user_actions_answer_with_the_author() {
		let message = test_support::message(1, Id(7));
		assert_eq!(
			run(&CopyUserUrls, "user-url", &message),
			Some(format!("<https://discord.com/users/{}>", message.author.id))
		);
		assert_eq!(
			run(&CopyUserMention, "user-mention", &message),
			Some(format!("<@{}>", message.author.id))
		);
	}

	#[test]
	fn an_unknown_action_answers_nothing() {
		let message = test_support::message(1, Id(7));
		assert!(run(&CopyUserUrls, "something-else", &message).is_none());
	}

	#[test]
	fn sticker_links_follow_the_format_type() {
		assert_eq!(
			sticker_url(Id(42), 1),
			Some("https://cdn.discordapp.com/stickers/42.png?size=512&lossless=true".into())
		);
		assert_eq!(
			sticker_url(Id(42), 4),
			Some("https://cdn.discordapp.com/stickers/42.gif?size=512&lossless=true".into())
		);
		assert_eq!(sticker_url(Id(42), 9), None);
	}

	#[test]
	fn a_still_image_can_be_asked_for() {
		let message = message_with_sticker(4);
		let mut plugin = CopyStickerLinks::default();
		plugin.configure(&Values(
			[("copyAnimatedAsPng".to_string(), serde_json::json!(true))]
				.into_iter()
				.collect(),
		));
		assert!(
			run(&plugin, "sticker-link", &message)
				.unwrap()
				.ends_with("42.png?size=512&lossless=true")
		);
	}

	#[test]
	fn a_message_without_stickers_has_no_sticker_link() {
		let message = test_support::message(1, Id(7));
		assert!(run(&CopyStickerLinks::default(), "sticker-link", &message).is_none());
	}

	#[test]
	fn every_action_is_listed_and_runnable() {
		let message = message_with_sticker(2);
		let plugins: Vec<Box<dyn Plugin>> = vec![
			Box::new(CopyUserUrls),
			Box::new(CopyUserMention),
			Box::new(CopyStickerLinks::default()),
		];
		for plugin in &plugins {
			for action in plugin.message_actions() {
				let _ = plugin.meta();
				assert!(
					run(plugin.as_ref(), action.id, &message).is_some(),
					"{} advertises {} but cannot run it",
					plugin.meta().name,
					action.id
				);
			}
		}
		let _ = Inbound::new(Id(7), None, Id(1), 0);
	}
}
