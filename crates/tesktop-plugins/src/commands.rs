//! Slash commands a port expands before the message goes out, as TestCord's CommandsAPI does.

use crate::{Fallback, Setting, SettingKind, Values, flag_or};

/// A slash command the owner typed and the ports may expand.
pub struct Claim {
	/// The body to send instead of the typed line.
	pub body: String,
}

const LEET: &[(char, &str)] = &[
	('a', "4"),
	('e', "3"),
	('i', "1"),
	('o', "0"),
	('s', "5"),
	('t', "7"),
	('b', "8"),
	('g', "9"),
	('l', "1"),
];

const SMALL_CAPS: &[(char, char)] = &[
	('a', 'ᴀ'),
	('b', 'ʙ'),
	('c', 'ᴄ'),
	('d', 'ᴅ'),
	('e', 'ᴇ'),
	('f', 'ꜰ'),
	('g', 'ɢ'),
	('h', 'ʜ'),
	('i', 'ɪ'),
	('j', 'ᴊ'),
	('k', 'ᴋ'),
	('l', 'ʟ'),
	('m', 'ᴍ'),
	('n', 'ɴ'),
	('o', 'ᴏ'),
	('p', 'ᴘ'),
	('q', 'q'),
	('r', 'ʀ'),
	('s', 'ꜱ'),
	('t', 'ᴛ'),
	('u', 'ᴜ'),
	('v', 'ᴠ'),
	('w', 'ᴡ'),
	('x', 'x'),
	('y', 'ʏ'),
	('z', 'ᴢ'),
];

fn leet(text: &str) -> String {
	text.chars()
		.map(|character| {
			LEET.iter()
				.find(|(plain, _)| plain.eq_ignore_ascii_case(&character))
				.map_or(character.to_string(), |(_, swap)| (*swap).to_string())
		})
		.collect()
}

fn bold(text: &str) -> String {
	text.chars()
		.map(|character| {
			let code = character as u32;
			match character {
				'A'..='Z' => char::from_u32(0x1D400 + code - 65),
				'a'..='z' => char::from_u32(0x1D41A + code - 97),
				'0'..='9' => char::from_u32(0x1D7CE + code - 48),
				_ => Some(character),
			}
			.unwrap_or(character)
		})
		.collect()
}

fn small_caps(text: &str) -> String {
	text.to_lowercase()
		.chars()
		.map(|character| {
			SMALL_CAPS
				.iter()
				.find(|(plain, _)| *plain == character)
				.map_or(character, |(_, small)| *small)
		})
		.collect()
}

fn fullwidth(text: &str) -> String {
	text.chars()
		.map(|character| match (' '..='~').contains(&character) {
			false => character,
			true if character == ' ' => '\u{3000}',
			true => char::from_u32(character as u32 + 0xFEE0).unwrap_or(character),
		})
		.collect()
}

const LEET_SETTINGS: &[Setting] = &[Setting {
	key: "enabled",
	label: "Expand the command",
	kind: SettingKind::Toggle,
	default: Fallback::Flag(true),
}];

macro_rules! text_command {
	($name:ident, $id:literal, $label:literal, $about:literal, $fn:expr, $aliases:expr) => {
		pub struct $name {
			enabled: Option<bool>,
		}
		impl Default for $name {
			fn default() -> Self {
				Self {
					enabled: Some(true),
				}
			}
		}
		impl crate::Plugin for $name {
			fn meta(&self) -> crate::Meta {
				crate::Meta {
					id: $id,
					name: $id,
					description: $about,
					authors: "Sharp",
					tags: &["Chat", "Commands"],
					aliases: $aliases,
					default_enabled: false,
				}
			}
			fn settings(&self) -> &'static [Setting] {
				LEET_SETTINGS
			}
			fn configure(&mut self, values: &Values) {
				self.enabled = Some(flag_or(values, LEET_SETTINGS, "enabled"));
			}
			fn command(&self, argument: &str) -> Option<Claim> {
				self.enabled.unwrap_or(true).then(|| Claim {
					body: $fn(argument),
				})
			}
			fn command_names(&self) -> &'static [&'static str] {
				&[$label]
			}
			fn command_about(&self) -> &'static str {
				$about
			}
		}
	};
}

text_command!(
	BoldText,
	"BoldText",
	"bold",
	"Turns your message into unicode bold.",
	bold,
	&["boldText"]
);
text_command!(
	LeetText,
	"LeetText",
	"leet",
	"Converts your message to leetspeak.",
	leet,
	&["leetText"]
);
text_command!(
	SmallCaps,
	"SmallCaps",
	"smallcaps",
	"Turns your message into small caps.",
	small_caps,
	&["smallCaps"]
);
text_command!(
	VaporwaveText,
	"VaporwaveText",
	"vaporwave",
	"Turns your message into fullwidth characters.",
	fullwidth,
	&["vaporwaveText"]
);

/// The expansion of a typed line, if a port claims it.
pub fn expand(plugin: &dyn crate::Plugin, line: &str) -> Option<Claim> {
	let (name, argument) = line.split_once(char::is_whitespace)?;
	let name = name.strip_prefix('/')?;
	plugin
		.command_names()
		.iter()
		.any(|known| known.eq_ignore_ascii_case(name))
		.then(|| plugin.command(argument.trim()))
		.flatten()
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::Plugin;

	#[test]
	fn bold_keeps_letters_and_digits_readable() {
		assert_eq!(bold("Hi 2024!"), "𝐇𝐢 𝟐𝟎𝟐𝟒!");
	}

	#[test]
	fn leet_keeps_the_shape_of_the_original() {
		assert_eq!(leet("testing bots"), "73571n9 8075");
	}

	#[test]
	fn small_caps_lowercases_first() {
		assert_eq!(small_caps("Hello"), "ʜᴇʟʟᴏ");
	}

	#[test]
	fn fullwidth_only_touches_printable_ascii() {
		assert_eq!(fullwidth("a b~"), "ａ　ｂ～");
		assert_eq!(fullwidth("日本"), "日本");
	}

	#[test]
	fn a_typed_line_is_expanded_by_the_right_port() {
		let plugins: Vec<Box<dyn Plugin>> = vec![
			Box::new(BoldText::default()),
			Box::new(LeetText::default()),
			Box::new(SmallCaps::default()),
			Box::new(VaporwaveText::default()),
		];
		let mut claims: Vec<String> = Vec::new();
		for name in ["bold", "leet", "smallcaps", "vaporwave"] {
			let line = format!("/{name} hello");
			for plugin in &plugins {
				if let Some(claim) = expand(plugin.as_ref(), &line) {
					claims.push(claim.body);
				}
			}
		}
		assert_eq!(claims.len(), 4);
		assert!(
			claims.iter().any(|claim| claim.chars().all(|c| c != 'h')),
			"the bold expansion should carry no plain letters: {claims:?}"
		);
		assert!(claims.contains(&"ʜᴇʟʟᴏ".to_string()));
		assert!(claims.contains(&"ｈｅｌｌｏ".to_string()));
	}

	#[test]
	fn an_unknown_command_is_left_for_the_service() {
		let plugin = LeetText::default();
		assert!(expand(&plugin, "/unknown hello").is_none());
		assert!(expand(&plugin, "hello /leet").is_none());
		assert!(
			expand(&plugin, "/leet").is_none(),
			"a command needs its argument"
		);
	}

	#[test]
	fn a_disabled_port_claims_nothing() {
		let mut plugin = LeetText::default();
		plugin.configure(&Values(
			[("enabled".to_string(), serde_json::json!(false))]
				.into_iter()
				.collect(),
		));
		assert!(expand(&plugin, "/leet hello").is_none());
	}

	#[test]
	fn the_registry_picks_the_first_port_that_claims_a_line() {
		let mut registry = crate::Registry::new();
		assert!(registry.command("/leet hello").is_none());
		registry.set_enabled("LeetText", true);
		assert_eq!(
			registry.command("/leet hello").map(|claim| claim.body),
			// "hello" with TestCord's own swap table: l and i both become 1.
			Some("h3110".to_string())
		);
	}
}
