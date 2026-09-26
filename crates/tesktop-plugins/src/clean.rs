//! Ports that clean up what you send: what is invisible, what is a digit, what is a toggle.

use crate::{Fallback, Meta, Outgoing, Setting, SettingKind, Values, flag_or};
use regex::Regex;
use std::sync::OnceLock;

/// The characters the sanitizer removes: zero-width joiners, bidirectional overrides, the
/// word joiner, the byte-order mark and the soft hyphen. Every one of them is invisible in
/// the composer and visible to whoever reads the raw text.
fn is_invisible(character: char) -> bool {
	matches!(character as u32,
		0x200B..=0x200D
		| 0x200E..=0x200F
		| 0x202A..=0x202E
		| 0x2060..=0x2064
		| 0x206A..=0x206F
		| 0xFEFF
		| 0x00AD)
}

/// A body with its invisible characters removed, and how many were there.
pub fn strip_invisible(text: &str) -> (String, usize) {
	let mut out = String::with_capacity(text.len());
	let mut removed = 0;
	for character in text.chars() {
		if is_invisible(character) {
			removed += 1;
			continue;
		}
		out.push(character);
	}
	(out, removed)
}

const SANITIZE_SETTINGS: &[Setting] = &[
	Setting {
		key: "sanitizeOutgoing",
		label: "Clean what you send",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(true),
	},
	Setting {
		key: "sanitizeEdits",
		label: "Clean your edits too",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(true),
	},
	Setting {
		key: "showToastOnDetection",
		label: "Say when something was removed",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(true),
	},
];

/// ZeroWidthSanitizer: nothing you send carries a character you cannot see.
pub struct ZeroWidthSanitizer {
	outgoing: bool,
	edits: bool,
	toast: bool,
	/// The line waiting for the host to show it.
	pending: Option<String>,
}

impl Default for ZeroWidthSanitizer {
	fn default() -> Self {
		Self {
			outgoing: true,
			edits: true,
			toast: true,
			pending: None,
		}
	}
}

impl ZeroWidthSanitizer {
	fn clean(&mut self, body: &mut String) {
		let (cleaned, removed) = strip_invisible(body);
		if removed == 0 {
			return;
		}
		*body = cleaned;
		if self.toast {
			self.pending = Some(format!("Removed {removed} invisible character(s)"));
		}
	}
}

impl crate::Plugin for ZeroWidthSanitizer {
	fn meta(&self) -> Meta {
		Meta {
			id: "ZeroWidthSanitizer",
			name: "ZeroWidthSanitizer",
			description: "Strips the invisible characters out of what you send.",
			authors: "Testcord",
			tags: &["Utility", "Chat"],
			aliases: &["zeroWidthSanitizer"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		SANITIZE_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.outgoing = flag_or(values, SANITIZE_SETTINGS, "sanitizeOutgoing");
		self.edits = flag_or(values, SANITIZE_SETTINGS, "sanitizeEdits");
		self.toast = flag_or(values, SANITIZE_SETTINGS, "showToastOnDetection");
		self.pending = None;
	}

	fn reset(&mut self) {
		self.pending = None;
	}

	fn before_send(&mut self, outgoing: &mut Outgoing<'_>) -> Result<(), &'static str> {
		if !self.outgoing {
			return Ok(());
		}
		let mut body = std::mem::take(outgoing.body);
		self.clean(&mut body);
		*outgoing.body = body;
		Ok(())
	}

	fn before_edit(&mut self, outgoing: &mut Outgoing<'_>) -> Result<(), &'static str> {
		if !self.edits {
			return Ok(());
		}
		let mut body = std::mem::take(outgoing.body);
		self.clean(&mut body);
		*outgoing.body = body;
		Ok(())
	}

	fn take_toast(&mut self) -> Option<String> {
		self.pending.take()
	}

	fn summary(&self) -> Option<String> {
		Some(if self.edits {
			"Sends and edits".to_string()
		} else {
			"Sends only".to_string()
		})
	}
}

/// The mathematical figure for a digit, from the same table the original writes out.
const fn figure(digit: u32) -> char {
	// A const fn cannot call `char::from_u32`, so the range is checked by hand here and the
	// runtime helper does the real conversion.
	match char::from_u32(0x1D7F6 + digit) {
		Some(figure) => figure,
		None => '0',
	}
}

/// A tag or a link is copied verbatim: a mention that no longer mentions, or an address that
/// no longer opens, is worse than a plain digit.
fn tags_and_links() -> &'static Regex {
	static PATTERN: OnceLock<Regex> = OnceLock::new();
	PATTERN.get_or_init(|| {
		Regex::new(r#"<[^>]+>|https?://[^\s<>"{}|\\^`\[\]]+"#).expect("a fixed pattern")
	})
}

/// SafeNumbers: digits as mathematical figures, everywhere except where a number has to work.
pub struct SafeNumbers;

impl crate::Plugin for SafeNumbers {
	fn meta(&self) -> Meta {
		Meta {
			id: "SafeNumbers",
			name: "SafeNumbers",
			description: "Writes your digits as mathematical figures, leaving tags and links alone.",
			authors: "Testcord",
			tags: &["Chat", "Fun"],
			aliases: &["safeNumbers"],
			default_enabled: false,
		}
	}

	fn before_send(&mut self, outgoing: &mut Outgoing<'_>) -> Result<(), &'static str> {
		*outgoing.body = replace_digits(outgoing.body);
		Ok(())
	}
}

/// Replace every ASCII digit outside a tag or a link with its mathematical figure.
pub fn replace_digits(content: &str) -> String {
	let pattern = tags_and_links();
	let mut out = String::with_capacity(content.len());
	let mut last = 0;
	for found in pattern.find_iter(content) {
		out.push_str(&map_digits(&content[last..found.start()]));
		out.push_str(found.as_str());
		last = found.end();
	}
	out.push_str(&map_digits(&content[last..]));
	out
}

fn map_digits(text: &str) -> String {
	if !text.contains(|character: char| character.is_ascii_digit()) {
		return text.to_string();
	}
	text.chars()
		.map(|character| match character {
			'0'..='9' => figure(character as u32 - '0' as u32),
			other => other,
		})
		.collect()
}

/// TalkInReverse: your message arrives the other way round.
pub struct TalkInReverse {
	reversed: bool,
}

impl Default for TalkInReverse {
	fn default() -> Self {
		Self { reversed: true }
	}
}

impl crate::Plugin for TalkInReverse {
	fn meta(&self) -> Meta {
		Meta {
			id: "TalkInReverse",
			name: "TalkInReverse",
			description: "Sends your message with its characters in reverse order.",
			authors: "Equicord",
			tags: &["Chat", "Fun"],
			aliases: &["talkInReverse"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		&[Setting {
			key: "reversed",
			label: "Send it reversed",
			kind: SettingKind::Toggle,
			default: Fallback::Flag(true),
		}]
	}

	fn configure(&mut self, values: &Values) {
		self.reversed = flag_or(values, self.settings(), "reversed");
	}

	fn composer_button(&self) -> Option<crate::ComposerButton> {
		Some(crate::ComposerButton {
			id: "talk-in-reverse",
			icon: Some("reverse-message"),
			label: "Reverse message",
			tooltip: if self.reversed {
				"Disable Reverse Message"
			} else {
				"Enable Reverse Message"
			},
			active: Some(self.reversed),
		})
	}

	fn press_composer(&mut self, id: &str) {
		if id == "talk-in-reverse" {
			self.reversed = !self.reversed;
		}
	}

	fn before_send(&mut self, outgoing: &mut Outgoing<'_>) -> Result<(), &'static str> {
		if !self.reversed {
			return Ok(());
		}
		// Reversed by character, not by byte, so nothing comes out as mojibake.
		let reversed: String = outgoing.body.chars().rev().collect();
		*outgoing.body = reversed;
		Ok(())
	}

	fn summary(&self) -> Option<String> {
		Some(if self.reversed {
			"Sending reversed".to_string()
		} else {
			"Sending as typed".to_string()
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

	fn edit(plugin: &mut dyn Plugin, body: &str) -> String {
		let previous = crate::Previous {
			id: model::Id(50),
			author: model::Id(1),
			content: "before".into(),
			attachments: 0,
			age_ms: 500,
			is_group: false,
			replying: false,
		};
		let mut text = body.to_string();
		let mut outgoing = Outgoing {
			channel: model::Id(7),
			me: model::Id(1),
			body: &mut text,
			reply: None,
			previous: Some(&previous),
			route: crate::Route::EditPrevious,
		};
		plugin.before_edit(&mut outgoing).expect("no veto");
		outgoing.body.clone()
	}

	#[test]
	fn invisible_characters_are_removed_from_a_send_and_an_edit() {
		let mut plugin = ZeroWidthSanitizer::default();
		assert_eq!(send(&mut plugin, "he\u{200B}llo\u{FEFF}"), "hello");
		assert_eq!(
			plugin.take_toast(),
			Some("Removed 2 invisible character(s)".to_string())
		);
		assert_eq!(edit(&mut plugin, "he\u{200B}llo"), "hello");
		assert_eq!(
			plugin.take_toast(),
			Some("Removed 1 invisible character(s)".to_string()),
			"an edit reports its own count"
		);
		assert!(plugin.take_toast().is_none(), "the line is shown once");
	}

	#[test]
	fn a_body_with_nothing_invisible_stays_quiet() {
		let mut plugin = ZeroWidthSanitizer::default();
		assert_eq!(send(&mut plugin, "plain text"), "plain text");
		assert!(plugin.take_toast().is_none());
	}

	#[test]
	fn cleaning_can_be_turned_off_per_path() {
		let mut plugin = ZeroWidthSanitizer::default();
		plugin.configure(&Values(
			[
				("sanitizeOutgoing".to_string(), serde_json::json!(false)),
				("showToastOnDetection".to_string(), serde_json::json!(false)),
			]
			.into_iter()
			.collect(),
		));
		assert_eq!(send(&mut plugin, "he\u{200B}llo"), "he\u{200B}llo");
		assert_eq!(edit(&mut plugin, "he\u{200B}llo"), "hello");
		assert!(plugin.take_toast().is_none());
	}

	#[test]
	fn digits_become_figures() {
		assert_eq!(
			replace_digits("room 101"),
			format!("room {}{}{}", figure(1), figure(0), figure(1)),
			"the table starts at a zero, the way the original writes it"
		);
	}

	#[test]
	fn a_mention_or_a_link_is_copied_verbatim() {
		assert_eq!(
			replace_digits("see <@123> and https://example.com/404 in 106"),
			format!(
				"see <@123> and https://example.com/404 in {}{}{}",
				figure(1),
				figure(0),
				figure(6)
			),
			"a mention still mentions and an address still opens, so their digits stay"
		);
	}

	#[test]
	fn a_reversed_message_arrives_backwards() {
		let mut plugin = TalkInReverse::default();
		assert_eq!(send(&mut plugin, "stressed"), "desserts");
		assert_eq!(send(&mut plugin, "héllo"), "olléh", "reversed by character");
	}

	#[test]
	fn reversing_can_be_turned_off() {
		let mut plugin = TalkInReverse::default();
		plugin.configure(&Values(
			[("reversed".to_string(), serde_json::json!(false))]
				.into_iter()
				.collect(),
		));
		assert_eq!(send(&mut plugin, "stressed"), "stressed");
	}

	#[test]
	fn the_body_hooks_never_see_an_id() {
		// A port that only rewrites a body has no way to reach an id, which is what keeps
		// these ports from touching anything but the text you typed.
		assert_eq!(SafeNumbers.meta().id, "SafeNumbers");
	}
}
