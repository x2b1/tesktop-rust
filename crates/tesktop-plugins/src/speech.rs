//! Ports that change what a message says, or count what it says.

use crate::{Fallback, Meta, Outgoing, Setting, SettingKind, Values, flag_or, text_or};
use model::Id;
use std::str::FromStr;

/// Printable ASCII a fullwidth character exists for, as TestCord's own table maps them.
pub fn fullwidth(text: &str) -> String {
	text.chars()
		.map(|character| match (' '..='~').contains(&character) {
			false => character,
			// A space becomes an ideographic one, which is the whole look of the thing.
			true if character == ' ' => '\u{3000}',
			true => char::from_u32(character as u32 + 0xFEE0).unwrap_or(character),
		})
		.collect()
}

const VAPORWAVE_SETTINGS: &[Setting] = &[Setting {
	key: "enabled",
	label: "Convert what you send",
	kind: SettingKind::Toggle,
	default: Fallback::Flag(true),
}];

/// AutoVaporwave: everything you send comes out fullwidth, without typing a command.
#[derive(Default)]
pub struct AutoVaporwave {
	enabled: Option<bool>,
}

impl crate::Plugin for AutoVaporwave {
	fn meta(&self) -> Meta {
		Meta {
			id: "AutoVaporwave",
			name: "AutoVaporwave",
			description: "Turns every message you send into fullwidth text.",
			authors: "Sharp",
			tags: &["Chat", "Fun"],
			aliases: &["autoVaporwave"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		VAPORWAVE_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.enabled = Some(flag_or(values, VAPORWAVE_SETTINGS, "enabled"));
	}

	fn before_send(&mut self, outgoing: &mut Outgoing<'_>) -> Result<(), &'static str> {
		if self.enabled == Some(false) {
			return Ok(());
		}
		// Everything goes, code blocks included, which is what the original does.
		*outgoing.body = fullwidth(outgoing.body);
		Ok(())
	}

	fn summary(&self) -> Option<String> {
		Some("Everything, code blocks included".to_string())
	}
}

/// SpaceOut: a letter at a time, joined with spaces.
pub struct SpaceOut {
	enabled: Option<bool>,
}

impl Default for SpaceOut {
	fn default() -> Self {
		Self {
			enabled: Some(true),
		}
	}
}

impl crate::Plugin for SpaceOut {
	fn meta(&self) -> Meta {
		Meta {
			id: "SpaceOut",
			name: "SpaceOut",
			description: "Puts spaces between every letter you send.",
			authors: "Sharp",
			tags: &["Chat", "Commands"],
			aliases: &["spaceOut"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		&[Setting {
			key: "enabled",
			label: "Expand the command",
			kind: SettingKind::Toggle,
			default: Fallback::Flag(true),
		}]
	}

	fn configure(&mut self, values: &Values) {
		self.enabled = Some(flag_or(values, VAPORWAVE_SETTINGS, "enabled"));
	}

	fn command(&self, argument: &str) -> Option<crate::commands::Claim> {
		self.enabled
			.unwrap_or(true)
			.then(|| crate::commands::Claim {
				// Every character is separated, spaces included, so the gap between two words
				// becomes three.
				body: argument
					.chars()
					.map(|character| character.to_string())
					.collect::<Vec<_>>()
					.join(" "),
			})
	}

	fn command_names(&self) -> &'static [&'static str] {
		&["spaceout"]
	}

	fn command_about(&self) -> &'static str {
		"Puts spaces between every letter."
	}
}

const NAME_CHANGE_SETTINGS: &[Setting] = &[
	Setting {
		key: "ids",
		label: "People to rename (ids, comma separated)",
		kind: SettingKind::Text { multiline: true },
		default: Fallback::Text(""),
	},
	Setting {
		key: "alias",
		label: "The alias their mention is rewritten to",
		kind: SettingKind::Text { multiline: false },
		default: Fallback::Text(""),
	},
];

/// AntiNameChange: a person who keeps changing their name is still the person you know, so
/// their mention keeps the alias you gave them.
#[derive(Default)]
pub struct AntiNameChange {
	ids: Vec<Id>,
	alias: String,
}

impl AntiNameChange {
	/// Rewrite a mention of one of the people you track, leaving every other mention alone.
	pub fn rewrite(&self, content: &str) -> String {
		if self.ids.is_empty() || self.alias.is_empty() || self.alias.contains('@') {
			return content.to_string();
		}
		let mut out = String::with_capacity(content.len());
		let mut rest = content;
		while let Some(start) = rest.find("<@") {
			let (before, tail) = rest.split_at(start);
			let Some(end) = tail.find('>') else {
				break;
			};
			let mention = &tail[..=end];
			out.push_str(before);
			let tracked = mention
				.strip_prefix("<@")
				.and_then(|rest| rest.strip_suffix('>'))
				.and_then(|id| Id::from_str(id.trim()).ok())
				.is_some_and(|id| self.ids.contains(&id));
			if tracked {
				out.push_str(&format!("<@{}:{}>", self.alias, self.alias));
			} else {
				out.push_str(mention);
			}
			rest = &tail[end + 1..];
		}
		out.push_str(rest);
		out
	}
}

impl crate::Plugin for AntiNameChange {
	fn meta(&self) -> Meta {
		Meta {
			id: "AntiNameChange",
			name: "AntiNameChange",
			description: "Keeps the alias you gave someone when they change their name.",
			authors: "nin0",
			tags: &["Chat", "Utility"],
			aliases: &["antiNameChange"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		NAME_CHANGE_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.ids = text_or(values, NAME_CHANGE_SETTINGS, "ids")
			.split([' ', ',', '\n', '\t', '\r'])
			.filter_map(|part| Id::from_str(part.trim()).ok())
			.take(256)
			.collect();
		// An alias is a plain word; a mention inside it would produce a malformed tag.
		self.alias = text_or(values, NAME_CHANGE_SETTINGS, "alias")
			.trim()
			.chars()
			.take(32)
			.filter(|character| !character.is_control() && *character != '<')
			.collect();
	}

	fn mutate_incoming(&mut self, message: &mut model::Message) {
		message.content = self.rewrite(&message.content);
	}

	fn summary(&self) -> Option<String> {
		(self.ids.is_empty() || self.alias.is_empty())
			.then(|| "Names an id and an alias to start".to_string())
	}
}

/// WordCount: a count under every message long enough for one to be worth reading.
pub struct WordCount {
	minimum: Option<i64>,
}

impl Default for WordCount {
	fn default() -> Self {
		Self { minimum: Some(5) }
	}
}

impl crate::Plugin for WordCount {
	fn meta(&self) -> Meta {
		Meta {
			id: "WordCount",
			name: "WordCount",
			description: "Counts the words and characters under a message.",
			authors: "Vencord",
			tags: &["Chat", "Utility"],
			aliases: &["wordCount"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		&[Setting {
			key: "minimum",
			label: "Words before a message is counted",
			kind: SettingKind::Number { min: 1, max: 200 },
			default: Fallback::Number(5),
		}]
	}

	fn configure(&mut self, values: &Values) {
		self.minimum = Some(crate::number_or(values, self.settings(), "minimum"));
	}

	fn display(&self) -> crate::display::DisplayPatch {
		crate::display::DisplayPatch {
			word_count: self.minimum.map(|_| true),
			..crate::display::DisplayPatch::default()
		}
	}

	fn summary(&self) -> Option<String> {
		Some(format!("Counted from {} words", self.minimum.unwrap_or(5)))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::Plugin;

	fn send(plugin: &mut dyn Plugin, body: &str) -> String {
		let mut text = body.to_string();
		let mut outgoing = Outgoing {
			channel: model::Id(7),
			me: model::Id(1),
			body: &mut text,
			reply: None,
			previous: None,
			route: crate::Route::Send,
		};
		plugin.before_send(&mut outgoing).expect("no veto");
		outgoing.body.clone()
	}

	#[test]
	fn everything_sent_becomes_fullwidth() {
		let mut plugin = AutoVaporwave::default();
		assert_eq!(send(&mut plugin, "hello there"), "ｈｅｌｌｏ　ｔｈｅｒｅ");
		assert_eq!(send(&mut plugin, "日本語"), "日本語");
	}

	#[test]
	fn code_blocks_are_converted_too() {
		let mut plugin = AutoVaporwave::default();
		assert_eq!(
			send(&mut plugin, "```\nlet x = 1;\n```"),
			"｀｀｀\nｌｅｔ\u{3000}ｘ\u{3000}＝\u{3000}１；\n｀｀｀"
		);
	}

	#[test]
	fn the_conversion_can_be_turned_off() {
		let mut plugin = AutoVaporwave::default();
		plugin.configure(&Values(
			[("enabled".to_string(), serde_json::json!(false))]
				.into_iter()
				.collect(),
		));
		assert_eq!(send(&mut plugin, "plain"), "plain");
	}

	#[test]
	fn letters_come_out_one_at_a_time() {
		let plugin = SpaceOut::default();
		assert_eq!(
			crate::commands::expand(&plugin, "/spaceout hi there").map(|claim| claim.body),
			Some("h i   t h e r e".to_string()),
			"a space becomes three, because the space itself is separated too"
		);
	}

	#[test]
	fn only_the_people_you_track_are_renamed() {
		let mut plugin = AntiNameChange::default();
		plugin.configure(&Values(
			[
				("ids".to_string(), serde_json::json!("42, 43")),
				("alias".to_string(), serde_json::json!("friend")),
			]
			.into_iter()
			.collect(),
		));
		assert_eq!(
			plugin.rewrite("hey <@42> and <@99> and <@43>"),
			"hey <@friend:friend> and <@99> and <@friend:friend>"
		);
	}

	#[test]
	fn an_alias_cannot_smuggle_a_mention() {
		let mut plugin = AntiNameChange::default();
		plugin.configure(&Values(
			[
				("ids".to_string(), serde_json::json!("42")),
				("alias".to_string(), serde_json::json!("<@7>")),
			]
			.into_iter()
			.collect(),
		));
		assert_eq!(plugin.rewrite("hey <@42>"), "hey <@42>");
	}

	#[test]
	fn nothing_configured_rewrites_nothing() {
		let plugin = AntiNameChange::default();
		assert_eq!(plugin.rewrite("hey <@42>"), "hey <@42>");
		assert_eq!(
			plugin.summary().as_deref(),
			Some("Names an id and an alias to start")
		);
	}

	#[test]
	fn the_count_appears_once_it_is_worth_reading() {
		let mut registry = crate::Registry::new();
		assert!(!registry.display().word_count);
		registry.set_enabled("WordCount", true);
		assert!(registry.display().word_count);
		assert_eq!(
			registry.summary("WordCount").as_deref(),
			Some("Counted from 5 words")
		);
	}
}
