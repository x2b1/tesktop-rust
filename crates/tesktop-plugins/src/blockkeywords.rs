//! BlockKeywords: ignores messages that contain words you blocked, as if the author was blocked.
//!
//! TestCord keeps two modes: dropping the message before it reaches the store, and marking it
//! blocked while still showing it. Only the dropping mode is ported, which is TestCord's default.

use crate::{Fallback, Meta, Setting, SettingKind, Values, flag_or, text_or};
use model::Message;
use regex::{Regex, RegexBuilder};

/// Patterns kept from one settings string.
pub const MAX_PATTERNS: usize = 256;
/// Bytes kept across those patterns.
pub const MAX_PATTERN_BYTES: usize = 8 * 1024;
/// Ceiling the regex engine may use while compiling one pattern.
const REGEX_SIZE_LIMIT: usize = 512 * 1024;

const SETTINGS: &[Setting] = &[
	Setting {
		key: "blockedWords",
		label: "Blocked words, one per entry",
		kind: SettingKind::Text { multiline: true },
		default: Fallback::Text(""),
	},
	Setting {
		key: "caseSensitive",
		label: "Case sensitive",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
	Setting {
		key: "useRegex",
		label: "Treat each value as a regular expression",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
];

#[derive(Default)]
pub struct BlockKeywords {
	patterns: Vec<Regex>,
	invalid: usize,
	words: String,
	case_sensitive: bool,
	use_regex: bool,
}

impl BlockKeywords {
	fn matches(&self, text: &str) -> bool {
		self.patterns.iter().any(|pattern| pattern.is_match(text))
	}

	fn contains(&self, message: &Message) -> bool {
		if self.patterns.is_empty() {
			return false;
		}
		if message.content.is_empty() && message.embeds.is_empty() {
			return false;
		}
		if self.matches(&message.content) {
			return true;
		}
		message.embeds.iter().any(|embed| {
			embed
				.title
				.as_deref()
				.is_some_and(|title| self.matches(title))
				|| embed
					.description
					.as_deref()
					.is_some_and(|description| self.matches(description))
		})
	}
}

impl crate::Plugin for BlockKeywords {
	fn meta(&self) -> Meta {
		Meta {
			id: "BlockKeywords",
			name: "BlockKeywords",
			description: "Hides messages that contain words you blocked.",
			authors: "catcraft, secp192k1",
			tags: &["Appearance", "Privacy"],
			aliases: &["blockKeywords"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.words = text_or(values, SETTINGS, "blockedWords");
		self.case_sensitive = flag_or(values, SETTINGS, "caseSensitive");
		self.use_regex = flag_or(values, SETTINGS, "useRegex");
		self.compile();
	}

	fn reset(&mut self) {
		self.patterns.clear();
		self.invalid = 0;
	}

	fn ignore(&self, message: &Message) -> bool {
		self.contains(message)
	}

	fn summary(&self) -> Option<String> {
		let count = self.patterns.len();
		if count == 0 {
			return Some("No words blocked yet".to_string());
		}
		let mut summary = format!(
			"{count} {} active",
			if count == 1 { "pattern" } else { "patterns" }
		);
		if self.invalid > 0 {
			summary.push_str(&format!(", {} could not compile", self.invalid));
		}
		Some(summary)
	}
}

impl BlockKeywords {
	fn compile(&mut self) {
		self.patterns.clear();
		self.invalid = 0;
		let mut budget = MAX_PATTERN_BYTES;
		for word in split_patterns(&self.words) {
			if self.patterns.len() >= MAX_PATTERNS || budget == 0 {
				break;
			}
			budget = budget.saturating_sub(word.len());
			let pattern = if self.use_regex {
				word.clone()
			} else {
				format!(r"\b{}\b", regex::escape(&word))
			};
			match RegexBuilder::new(&pattern)
				.case_insensitive(!self.case_sensitive)
				.size_limit(REGEX_SIZE_LIMIT)
				.build()
			{
				Ok(regex) => self.patterns.push(regex),
				Err(_) => self.invalid += 1,
			}
		}
	}
}

/// Split TestCord's comma-separated list, protecting commas inside `[a,b]` and `{n,m}`.
pub fn split_patterns(input: &str) -> Vec<String> {
	let mut patterns = Vec::new();
	let mut current = String::new();
	let mut group: Option<char> = None;
	for character in input.chars() {
		match character {
			'[' | '{' if group.is_none() => {
				group = Some(character);
				current.push(character);
			}
			'[' | '{' => current.push(character),
			']' | '}' if group.take().is_some() => current.push(character),
			',' if group.is_none() => {
				patterns.push(std::mem::take(&mut current));
			}
			_ => current.push(character),
		}
	}
	patterns.push(current);
	patterns
		.into_iter()
		.map(|pattern| pattern.trim().to_string())
		.filter(|pattern| !pattern.is_empty())
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::Plugin;
	use model::Id;

	fn configured(words: &str, case_sensitive: bool, use_regex: bool) -> BlockKeywords {
		let mut plugin = BlockKeywords::default();
		plugin.configure(&Values(
			[
				("blockedWords".to_string(), serde_json::json!(words)),
				(
					"caseSensitive".to_string(),
					serde_json::json!(case_sensitive),
				),
				("useRegex".to_string(), serde_json::json!(use_regex)),
			]
			.into_iter()
			.collect(),
		));
		plugin
	}

	fn message(content: &str) -> Message {
		let mut message = test_support::message(4, Id(9));
		message.content = content.to_string();
		message
	}

	#[test]
	fn words_are_matched_on_word_boundaries() {
		let plugin = configured("spoiler", false, false);
		assert!(plugin.ignore(&message("big spoiler ahead")));
		assert!(!plugin.ignore(&message("spoilers")));
		assert!(!plugin.ignore(&message("nothing here")));
	}

	#[test]
	fn case_sensitivity_follows_the_setting() {
		assert!(!configured("Spoiler", true, false).ignore(&message("spoiler")));
		assert!(configured("Spoiler", false, false).ignore(&message("spoiler")));
	}

	#[test]
	fn regex_mode_uses_the_pattern_as_written() {
		let plugin = configured("sp[oa]iler\\d+", false, true);
		assert!(plugin.ignore(&message("spoiler12")));
		assert!(!plugin.ignore(&message("spoiler")));
		assert_eq!(plugin.summary().as_deref(), Some("1 pattern active"));
	}

	#[test]
	fn invalid_patterns_are_counted_not_fatal() {
		let plugin = configured("valid,(", false, true);
		assert_eq!(
			plugin.summary().as_deref(),
			Some("1 pattern active, 1 could not compile")
		);
	}

	#[test]
	fn embed_fields_are_searched_too() {
		let plugin = configured("casino", false, false);
		let mut message = message("look at this");
		message.embeds.push(model::Embed {
			title: Some("Big casino night".to_string()),
			..model::Embed::default()
		});
		assert!(plugin.ignore(&message));
	}

	#[test]
	fn the_pattern_list_survives_groups_with_commas() {
		assert_eq!(
			split_patterns("one, [a,b], {2,3}, ,two"),
			vec!["one", "[a,b]", "{2,3}", "two"]
		);
	}

	#[test]
	fn empty_settings_block_nothing() {
		let plugin = configured("", false, false);
		assert!(!plugin.ignore(&message("anything")));
	}
}
