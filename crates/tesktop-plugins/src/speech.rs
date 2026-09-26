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

/// Words the Ingtoninator will not touch: a pronoun, or anything ending in a vowel or `y`,
/// which reads badly once it carries the suffix.
pub fn ington_legal(word: &str) -> bool {
	if word.eq_ignore_ascii_case("i") {
		return false;
	}
	!matches!(
		word.chars()
			.last()
			.map(|character| character.to_ascii_lowercase()),
		Some('a' | 'e' | 'i' | 'o' | 'u' | 'y')
	)
}

/// The suffix that goes after a word, which shortens the endings it would otherwise double up.
pub fn ington(word: &str) -> String {
	let upper = word.to_ascii_uppercase();
	for (tail, replacement) in [
		("INGTON", ""),
		("INGTO", "N"),
		("INGT", "ON"),
		("ING", "TON"),
		("IN", "GTON"),
		("I", "NGTON"),
	] {
		if upper.ends_with(tail) {
			return replacement.to_string();
		}
	}
	"INGTON".to_string()
}

/// The words of a body that the port may choose from: letters only, and never inside a link.
pub fn ington_words(content: &str) -> Vec<(usize, String)> {
	let mut links: Vec<(usize, usize)> = Vec::new();
	let mut rest = content;
	while let Some(start) = rest.find("http") {
		let (before, tail) = rest.split_at(start);
		let end = tail.find(char::is_whitespace).unwrap_or(tail.len());
		links.push((start, start + end));
		rest = &tail[end..];
		let _ = before;
	}
	let mut words = Vec::new();
	let mut index = 0;
	let mut current = String::new();
	let mut start = 0;
	for character in content.chars() {
		if character.is_alphabetic() {
			if current.is_empty() {
				start = index;
			}
			current.push(character);
		} else if !current.is_empty() {
			words.push((start, std::mem::take(&mut current)));
		}
		index += character.len_utf8();
	}
	if !current.is_empty() {
		words.push((start, current));
	}
	words
		.into_iter()
		.filter(|(at, _)| !links.iter().any(|(from, to)| *at >= *from && *at < *to))
		.collect()
}

const INGTON_SETTINGS: &[Setting] = &[Setting {
	key: "isEnabled",
	label: "Add the suffix",
	kind: SettingKind::Toggle,
	default: Fallback::Flag(true),
}];

/// Ingtoninator: one word in every message you send grows a suffix.
pub struct Ingtoninator {
	enabled: bool,
}

impl Default for Ingtoninator {
	fn default() -> Self {
		Self { enabled: true }
	}
}

impl Ingtoninator {
	/// Add the suffix after one word of the body, chosen by the body itself so the same text
	/// always gets the same word and a message is never rewritten twice.
	pub fn rewrite(&self, body: &str) -> String {
		if !self.enabled {
			return body.to_string();
		}
		let words: Vec<(usize, String)> = ington_words(body)
			.into_iter()
			.filter(|(_, word)| ington_legal(word))
			.collect();
		if words.is_empty() {
			return body.to_string();
		}
		let pick = (fnv(body) as usize) % words.len();
		let (at, word) = &words[pick];
		let insertion = if word.chars().all(|character| !character.is_lowercase()) {
			ington(word)
		} else {
			ington(word).to_lowercase()
		};
		let at = at + word.len();
		format!("{}{insertion}{}", &body[..at], &body[at..])
	}
}

fn fnv(text: &str) -> u64 {
	let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
	for byte in text.as_bytes() {
		hash ^= u64::from(*byte);
		hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
	}
	hash
}

impl crate::Plugin for Ingtoninator {
	fn meta(&self) -> Meta {
		Meta {
			id: "Ingtoninator",
			name: "Ingtoninator",
			description: "Adds the Ington suffix to one word of every message you send.",
			authors: "Equicord",
			tags: &["Chat", "Fun"],
			aliases: &["ingtoninator"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		INGTON_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.enabled = flag_or(values, INGTON_SETTINGS, "isEnabled");
	}

	fn composer_button(&self) -> Option<crate::ComposerButton> {
		Some(crate::ComposerButton {
			id: "ingtoninator",
			icon: Some("ingtoninator"),
			label: "Ingtoninator",
			tooltip: if self.enabled {
				"Disable Ingtoninator"
			} else {
				"Enable Ingtoninator"
			},
			active: Some(self.enabled),
		})
	}

	fn press_composer(&mut self, id: &str) {
		if id == "ingtoninator" {
			self.enabled = !self.enabled;
		}
	}

	fn before_send(&mut self, outgoing: &mut Outgoing<'_>) -> Result<(), &'static str> {
		let body = std::mem::take(outgoing.body);
		*outgoing.body = self.rewrite(&body);
		Ok(())
	}

	fn summary(&self) -> Option<String> {
		Some(if self.enabled {
			"Adding the suffix".to_string()
		} else {
			"Standing by".to_string()
		})
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

	#[test]
	fn one_word_grows_the_suffix() {
		let plugin = Ingtoninator::default();
		// "hello" ends in a vowel and cannot take it; "world" can.
		assert_eq!(plugin.rewrite("hello world"), "hello worldington");
	}

	#[test]
	fn the_same_text_always_gets_the_same_word() {
		let plugin = Ingtoninator::default();
		assert_eq!(
			plugin.rewrite("hello world friend"),
			plugin.rewrite("hello world friend")
		);
	}

	#[test]
	fn a_word_that_cannot_take_it_is_skipped() {
		let plugin = Ingtoninator::default();
		assert_eq!(plugin.rewrite("a e o u y i"), "a e o u y i");
	}

	#[test]
	fn a_link_is_never_touched() {
		let plugin = Ingtoninator::default();
		let body = "https://example.com/some/path";
		assert_eq!(plugin.rewrite(body), body);
	}

	#[test]
	fn an_all_caps_word_keeps_its_case() {
		// The suffix goes after the whole word, and shortens the ending it would double up.
		assert_eq!(format!("BRING{}", ington("BRING")), "BRINGTON");
		assert_eq!(format!("BRINGTO{}", ington("BRINGTO")), "BRINGTON");
		assert_eq!(format!("INGTON{}", ington("INGTON")), "INGTON");
		assert_eq!(format!("THING{}", ington("THING")), "THINGTON");
	}

	#[test]
	fn the_suffix_can_be_turned_off() {
		let mut plugin = Ingtoninator::default();
		plugin.configure(&Values(
			[("isEnabled".to_string(), serde_json::json!(false))]
				.into_iter()
				.collect(),
		));
		assert_eq!(plugin.rewrite("hello there"), "hello there");
	}

	#[test]
	fn words_are_counted_in_bytes_so_a_link_range_holds() {
		let words = ington_words("héllo wörld");
		assert_eq!(words.len(), 2);
		assert_eq!(words[1].0, "héllo ".len());
	}
}

#[cfg(test)]
mod button_tests {
	use super::*;
	use crate::Registry;

	#[test]
	fn a_fresh_registry_reports_what_its_defaults_enable() {
		// The count has to agree with the seeded entries, or the host treats an untouched
		// install as "nothing enabled" and no buttons or entries are ever offered.
		let registry = Registry::new();
		let expected = registry
			.metas()
			.iter()
			.filter(|meta| meta.default_enabled)
			.count();
		assert_eq!(registry.enabled_count(), expected);
		assert_eq!(registry.any_enabled(), expected > 0);
	}

	#[test]
	fn a_port_with_a_button_offers_it_only_while_it_is_on() {
		let mut registry = Registry::new();
		assert!(registry.composer_buttons().is_empty());
		registry.set_enabled("Ingtoninator", true);
		let buttons = registry.composer_buttons();
		assert_eq!(buttons.len(), 1);
		assert_eq!(buttons[0].id, "ingtoninator");
		assert_eq!(buttons[0].active, Some(true));
	}

	#[test]
	fn a_press_reaches_the_port_that_owns_the_button() {
		let mut registry = Registry::new();
		registry.set_enabled("TalkInReverse", true);
		assert_eq!(
			registry.composer_buttons()[0].active,
			Some(true),
			"the port starts the way it declares"
		);
		registry.press_composer("talk-in-reverse");
		assert_eq!(registry.composer_buttons()[0].active, Some(false));
		registry.press_composer("talk-in-reverse");
		assert_eq!(registry.composer_buttons()[0].active, Some(true));
	}

	#[test]
	fn a_press_for_a_button_nobody_offers_goes_nowhere() {
		let mut registry = Registry::new();
		registry.set_enabled("Ingtoninator", true);
		registry.press_composer("talk-in-reverse");
		assert_eq!(registry.composer_buttons().len(), 1);
		assert_eq!(registry.composer_buttons()[0].id, "ingtoninator");
	}

	#[test]
	fn the_button_and_the_send_path_agree() {
		let mut registry = Registry::new();
		registry.set_enabled("TalkInReverse", true);
		registry.press_composer("talk-in-reverse");
		let mut text = "stressed".to_string();
		let mut outgoing = Outgoing {
			channel: model::Id(7),
			me: model::Id(1),
			body: &mut text,
			reply: None,
			previous: None,
			route: crate::Route::Send,
		};
		registry.before_send(&mut outgoing).expect("no veto");
		assert_eq!(outgoing.body, "stressed", "the toggle really turned it off");
	}
}
