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

/// The same table the automatic port uses, so `/vaporwave` and AutoVaporwave agree.
use crate::speech::fullwidth;

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

/// The expansion of a typed line, if a port claims it. A command may take no argument.
pub fn expand(plugin: &dyn crate::Plugin, line: &str) -> Option<Claim> {
	let (name, argument) = line
		.strip_prefix('/')
		.map(|rest| rest.split_once(char::is_whitespace).unwrap_or((rest, "")))?;
	plugin
		.command_names()
		.iter()
		.any(|known| known.eq_ignore_ascii_case(name))
		.then(|| plugin.command(argument.trim()))
		.flatten()
}

/// Annoiler: every character wrapped in a spoiler, as Kyza's original did.
pub struct Annoiler {
	enabled: Option<bool>,
}

impl Default for Annoiler {
	fn default() -> Self {
		Self {
			enabled: Some(true),
		}
	}
}

impl crate::Plugin for Annoiler {
	fn meta(&self) -> crate::Meta {
		crate::Meta {
			id: "Annoiler",
			name: "Annoiler",
			description: "Puts a spoiler around every character you send.",
			authors: "x2b",
			tags: &["Chat", "Fun"],
			aliases: &["annoiler"],
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
			body: argument.chars().map(|c| format!("||{c}||")).collect(),
		})
	}

	fn command_names(&self) -> &'static [&'static str] {
		&["annoil"]
	}

	fn command_about(&self) -> &'static str {
		"Puts a spoiler around every character."
	}
}

/// ClapText: a clap between every word.
pub struct ClapText {
	enabled: Option<bool>,
}

impl Default for ClapText {
	fn default() -> Self {
		Self {
			enabled: Some(true),
		}
	}
}

impl crate::Plugin for ClapText {
	fn meta(&self) -> crate::Meta {
		crate::Meta {
			id: "ClapText",
			name: "ClapText",
			description: "Puts a clap between every word you send.",
			authors: "Sharp",
			tags: &["Fun", "Commands"],
			aliases: &["clapText"],
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
			body: argument
				.split(char::is_whitespace)
				.filter(|word| !word.is_empty())
				.collect::<Vec<_>>()
				.join(" 👏 "),
		})
	}

	fn command_names(&self) -> &'static [&'static str] {
		&["clap"]
	}

	fn command_about(&self) -> &'static str {
		"Puts a clap between every word."
	}
}

const VIBES: &[&str] = &[
	"✦ the vibes are immaculate ✦",
	"🌴 endless summer, endless mall 🌴",
	"📼 rewinding to a time that never was 📼",
	"🛍️ welcome to the mall, population: you 🛍️",
	"🌅 chasing a sunset that never sets 🌅",
	"💾 saving your aesthetic... done 💾",
	"🪩 disco ball energy detected 🪩",
	"🌊 floating on a sea of neon 🌊",
	"☎️ this call is being routed through 1987 ☎️",
	"🍹 sipping something pink by the fountain 🍹",
];

/// VibeCheck: a random mood from TestCord's own list. The pick is by the clock rather than a
/// random number, so the same second always gives the same line.
pub struct VibeCheck {
	enabled: Option<bool>,
}

impl Default for VibeCheck {
	fn default() -> Self {
		Self {
			enabled: Some(true),
		}
	}
}

impl crate::Plugin for VibeCheck {
	fn meta(&self) -> crate::Meta {
		crate::Meta {
			id: "VibeCheck",
			name: "VibeCheck",
			description: "Drops a random vaporwave mood into chat.",
			authors: "Sharp",
			tags: &["Commands", "Fun"],
			aliases: &["vibeCheck"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		LEET_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.enabled = Some(flag_or(values, LEET_SETTINGS, "enabled"));
	}

	fn command(&self, _argument: &str) -> Option<Claim> {
		self.enabled.unwrap_or(true).then(|| Claim {
			body: vibe().to_string(),
		})
	}

	fn command_names(&self) -> &'static [&'static str] {
		&["vibe"]
	}

	fn command_about(&self) -> &'static str {
		"Sends a random vaporwave vibe."
	}
}

/// A mood from the list, chosen by the wall clock so it is stable within the second.
pub fn vibe() -> &'static str {
	let seconds = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|age| age.as_secs())
		.unwrap_or_default();
	VIBES[seconds as usize % VIBES.len()]
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
	fn every_character_gets_its_own_spoiler() {
		let annoiler = Annoiler::default();
		assert_eq!(
			expand(&annoiler, "/annoil hi").map(|claim| claim.body),
			Some("||h||||i||".to_string())
		);
	}

	#[test]
	fn a_clap_goes_between_the_words() {
		let clap = ClapText::default();
		assert_eq!(
			expand(&clap, "/clap  two   words ").map(|claim| claim.body),
			Some("two 👏 words".to_string())
		);
	}

	#[test]
	fn a_mood_comes_from_the_list() {
		let vibes: Vec<&str> = (0..40).map(|_| vibe()).collect();
		for mood in &vibes {
			assert!(VIBES.contains(mood), "{mood} is not one of ours");
		}
		let check = VibeCheck::default();
		assert!(expand(&check, "/vibe").is_some());
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
		// A command with no argument still expands, to nothing useful.
		assert_eq!(
			expand(&plugin, "/leet").map(|claim| claim.body),
			Some(String::new())
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
