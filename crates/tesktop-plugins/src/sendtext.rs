//! Outgoing text ports: what a message says on its way out, without touching the draft the
//! owner is still editing.

use crate::{Fallback, Meta, Outgoing, Setting, SettingKind, Values, flag_or, text_or};
use regex::Regex;

/// A body a port would send that the owner did not write is refused with this reason.
const PROFANITY_EMPTY: &str = "Message was emptied by the profanity filter";

/// One pass of the capitalization rules: uppercase every letter's first word unless the word
/// is on the blocked list or the segment starts a link.
fn capitalize(body: &str, blocked: &[String]) -> String {
	let mut out = String::with_capacity(body.len());
	for (index, part) in split_sentences(body).into_iter().enumerate() {
		if index % 2 == 1 {
			// The delimiter between two sentences is kept as it was written.
			out.push_str(&part);
			continue;
		}
		if part.trim().is_empty() || part.starts_with("http") {
			out.push_str(&part);
			continue;
		}
		let first_word = part
			.trim_start()
			.split(|character: char| !character.is_alphanumeric() && character != '\'')
			.next()
			.unwrap_or_default()
			.to_lowercase();
		if !first_word.is_empty() && blocked.contains(&first_word) {
			out.push_str(&part);
			continue;
		}
		let leading = part.len() - part.trim_start().len();
		out.push_str(&part[..leading]);
		let mut rest = &part[leading..];
		if let Some(first) = rest.chars().next() {
			out.extend(first.to_uppercase());
			rest = &rest[first.len_utf8()..];
		}
		out.push_str(rest);
	}
	out
}

/// Split into alternating content and delimiter parts, like TestCord's capture group does.
fn split_sentences(body: &str) -> Vec<String> {
	let mut parts = vec![String::new()];
	let mut characters = body.chars().peekable();
	while let Some(character) = characters.next() {
		if character == '\n' {
			// A run of newlines is one delimiter, like `\n+`.
			parts.push(character.to_string());
			while characters.peek() == Some(&'\n') {
				parts.last_mut().expect("delimiter part").push('\n');
				characters.next();
			}
			parts.push(String::new());
			continue;
		}
		parts.last_mut().expect("content part").push(character);
		if matches!(character, '.' | '?' | '!') {
			// `(?<=[.?!])\s+`, minus TestCord's four negative lookbehinds: an ellipsis, an
			// initial such as "A.", and a decimal such as "1.5" do not end a sentence.
			let before: Vec<char> = {
				let part = parts.last().expect("content part");
				let trimmed: String = part[..part.len() - character.len_utf8()].to_string();
				trimmed.chars().rev().take(3).collect()
			};
			let is_ellipsis = before.first() == Some(&'.');
			let is_initial = matches!(
				(before.first(), before.get(1)),
				(Some(upper), Some('.')) if upper.is_uppercase()
			);
			let is_decimal = matches!(
				(before.first(), before.get(1), before.get(2)),
				(Some(last), Some('.'), Some(first))
					if last.is_numeric() && first.is_numeric()
			);
			let abbreviation = is_ellipsis || is_initial || is_decimal;
			if characters.peek().is_some_and(|next| next.is_whitespace()) && !abbreviation {
				let mut delimiter = String::new();
				while characters.peek().is_some_and(|next| next.is_whitespace()) {
					delimiter.push(characters.next().expect("peeked"));
				}
				parts.push(delimiter);
				parts.push(String::new());
				continue;
			}
		}
	}
	parts
}

#[derive(Default)]
pub struct PolishWording {
	quick_disable: Option<bool>,
	blocked_words: Vec<String>,
	fix_apostrophes: bool,
	expand_contractions: bool,
	contractions: Vec<(&'static str, &'static str)>,
	missing: Vec<(String, &'static str)>,
	capitalize: bool,
	add_periods: bool,
}

const CONTRACTIONS: &[(&str, &str)] = &[
	("wasn't", "was not"),
	("can't", "cannot"),
	("don't", "do not"),
	("won't", "will not"),
	("isn't", "is not"),
	("aren't", "are not"),
	("haven't", "have not"),
	("hasn't", "has not"),
	("hadn't", "had not"),
	("doesn't", "does not"),
	("didn't", "did not"),
	("shouldn't", "should not"),
	("wouldn't", "would not"),
	("couldn't", "could not"),
	("that's", "that is"),
	("what's", "what is"),
	("there's", "there is"),
	("how's", "how is"),
	("where's", "where is"),
	("when's", "when is"),
	("who's", "who is"),
	("why's", "why is"),
	("you'll", "you will"),
	("i'll", "I will"),
	("they'll", "they will"),
	("it'll", "it will"),
	("i'm", "I am"),
	("you're", "you are"),
	("they're", "they are"),
	("he's", "he is"),
	("she's", "she is"),
	("i've", "I have"),
	("you've", "you have"),
	("we've", "we have"),
	("they've", "they have"),
	("you'd", "you would"),
	("he'd", "he would"),
	("she'd", "she would"),
	("it'd", "it would"),
	("we'd", "we would"),
	("they'd", "they would"),
	("y'all", "you all"),
	("here's", "here is"),
];

const POLISH_SETTINGS: &[Setting] = &[
	Setting {
		key: "quickDisable",
		label: "Disabled",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
	Setting {
		key: "blockedWords",
		label: "Words that stay lowercase",
		kind: SettingKind::Text { multiline: true },
		default: Fallback::Text(""),
	},
	Setting {
		key: "fixApostrophes",
		label: "Put the apostrophe back",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(true),
	},
	Setting {
		key: "expandContractions",
		label: "Expand contractions",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
	Setting {
		key: "capitalize",
		label: "Capitalize sentences",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
	Setting {
		key: "addPeriods",
		label: "End sentences with a period",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
];

impl crate::Plugin for PolishWording {
	fn meta(&self) -> Meta {
		Meta {
			id: "PolishWording",
			name: "PolishWording",
			description: "Fixes apostrophes, expands contractions and tidies sentence case.",
			authors: "Kaqar",
			tags: &["Utility"],
			aliases: &["polishWording"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		POLISH_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.quick_disable = Some(flag_or(values, POLISH_SETTINGS, "quickDisable"));
		self.blocked_words = text_or(values, POLISH_SETTINGS, "blockedWords")
			.split(',')
			.map(str::trim)
			.filter(|word| !word.is_empty())
			.map(str::to_lowercase)
			.collect();
		self.fix_apostrophes = flag_or(values, POLISH_SETTINGS, "fixApostrophes");
		self.expand_contractions = flag_or(values, POLISH_SETTINGS, "expandContractions");
		self.capitalize = flag_or(values, POLISH_SETTINGS, "capitalize");
		self.add_periods = flag_or(values, POLISH_SETTINGS, "addPeriods");
		self.contractions = CONTRACTIONS.to_vec();
		self.missing = CONTRACTIONS
			.iter()
			.map(|(short, _)| (short.replace('\'', ""), *short))
			.collect();
	}

	fn before_send(&mut self, outgoing: &mut Outgoing<'_>) -> Result<(), &'static str> {
		if self.quick_disable.unwrap_or(false) {
			return Ok(());
		}
		let body = std::mem::take(outgoing.body);
		let mut out = body;
		if self.fix_apostrophes {
			out = replace_whole_words(&out, &self.missing, false);
		}
		if self.expand_contractions {
			out = replace_whole_words(&out, &self.contractions, false);
		}
		if self.capitalize {
			out = capitalize(&out, &self.blocked_words);
		}
		if self.add_periods {
			out = add_periods(&out);
		}
		*outgoing.body = out;
		Ok(())
	}

	fn summary(&self) -> Option<String> {
		let mut parts = Vec::new();
		if self.fix_apostrophes {
			parts.push("apostrophes");
		}
		if self.expand_contractions {
			parts.push("contractions");
		}
		if self.capitalize {
			parts.push("capitalization");
		}
		if self.add_periods {
			parts.push("periods");
		}
		Some(if parts.is_empty() {
			"Nothing to change yet".to_string()
		} else {
			format!("Fixes {}", parts.join(", "))
		})
	}
}

/// Whole-word replacement that keeps the letters' capitalization of the match, as TestCord's
/// `getCapData`/`restoreCap` pair does.
fn replace_whole_words<S: AsRef<str>>(body: &str, rules: &[(S, &str)], match_case: bool) -> String {
	let mut out = body.to_string();
	for (find, replace) in rules {
		let pattern = regex::RegexBuilder::new(&format!(r"\b{}\b", regex::escape(find.as_ref())))
			.case_insensitive(!match_case)
			.size_limit(64 * 1024)
			.build()
			.expect("a literal pattern always compiles");
		out = replace_keeping_case(&pattern, &out, replace);
	}
	out
}

fn replace_keeping_case(pattern: &Regex, body: &str, replace: &str) -> String {
	let mut out = String::with_capacity(body.len());
	let mut last = 0;
	for found in pattern.find_iter(body) {
		out.push_str(&body[last..found.start()]);
		out.push_str(&apply_case(replace, found.as_str()));
		last = found.end();
	}
	out.push_str(&body[last..]);
	out
}

fn apply_case(replacement: &str, matched: &str) -> String {
	let letters: Vec<char> = matched
		.chars()
		.filter(|character| character.is_alphabetic())
		.collect();
	let upper: Vec<bool> = letters
		.iter()
		.map(|character| character.is_uppercase())
		.collect();
	let mut index = 0;
	let mut out = String::with_capacity(replacement.len());
	for character in replacement.chars() {
		if !character.is_alphabetic() {
			out.push(character);
			continue;
		}
		let was_upper = upper
			.get(index)
			.copied()
			.unwrap_or_else(|| upper.last().copied().unwrap_or(false));
		if was_upper {
			out.extend(character.to_uppercase());
		} else {
			out.extend(character.to_lowercase());
		}
		if index + 1 < upper.len() {
			index += 1;
		}
	}
	out
}

/// Give a sentence a final period when it ends in a word character.
fn add_periods(body: &str) -> String {
	body.split_inclusive('\n')
		.map(|line| {
			let (content, ending) = match line.strip_suffix('\n') {
				Some(content) => (content, "\n"),
				None => (line, ""),
			};
			let trimmed = content.trim_end();
			let ends_with_word = trimmed
				.chars()
				.last()
				.is_some_and(|character| character.is_alphanumeric() || character == '\'');
			if ends_with_word {
				format!("{trimmed}.{ending}")
			} else {
				line.to_string()
			}
		})
		.collect()
}

const PROFANITY_SETTINGS: &[Setting] = &[
	Setting {
		key: "words",
		label: "Filtered words, separated by commas or newlines",
		kind: SettingKind::Text { multiline: true },
		default: Fallback::Text(""),
	},
	Setting {
		key: "duckOnEmpty",
		label: "Send a duck when nothing is left",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(true),
	},
];

pub struct ProfanityFilter {
	words: Vec<Regex>,
	duck: bool,
}

impl Default for ProfanityFilter {
	fn default() -> Self {
		Self {
			words: Vec::new(),
			duck: true,
		}
	}
}

impl crate::Plugin for ProfanityFilter {
	fn meta(&self) -> Meta {
		Meta {
			id: "ProfanityFilter",
			name: "ProfanityFilter",
			description: "Removes filtered whole words from messages you send.",
			authors: "Testcord",
			tags: &["Utility", "Privacy"],
			aliases: &["profanityFilter"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		PROFANITY_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.duck = flag_or(values, PROFANITY_SETTINGS, "duckOnEmpty");
		self.words =
			crate::blockkeywords::split_patterns(&text_or(values, PROFANITY_SETTINGS, "words"))
				.into_iter()
				.filter_map(|word| Regex::new(&format!(r"\b{}\b", regex::escape(&word))).ok())
				.take(crate::blockkeywords::MAX_PATTERNS)
				.collect();
	}

	fn before_send(&mut self, outgoing: &mut Outgoing<'_>) -> Result<(), &'static str> {
		if self.words.is_empty() {
			return Ok(());
		}
		let body = std::mem::take(outgoing.body);
		let mut filtered = body.clone();
		for word in &self.words {
			filtered = word.replace_all(&filtered, "").into_owned();
		}
		// Collapse what removal left behind, then tidy the punctuation it left stranded.
		let filtered = collapse_spaces(&filtered);
		let filtered = Regex::new(r"\s+([,.!?;:])")
			.expect("a literal class always compiles")
			.replace_all(&filtered, "$1")
			.into_owned();
		let filtered = filtered.trim().to_string();
		*outgoing.body = if filtered.is_empty() {
			if self.duck {
				":duck:".to_string()
			} else {
				return Err(PROFANITY_EMPTY);
			}
		} else {
			filtered
		};
		Ok(())
	}

	fn before_edit(&mut self, outgoing: &mut Outgoing<'_>) -> Result<(), &'static str> {
		self.before_send(outgoing)
	}

	fn summary(&self) -> Option<String> {
		Some(format!(
			"{} {} filtered",
			self.words.len(),
			if self.words.len() == 1 {
				"word"
			} else {
				"words"
			}
		))
	}
}

fn collapse_spaces(body: &str) -> String {
	let mut out = String::with_capacity(body.len());
	let mut spaces = 0;
	for character in body.chars() {
		if character == ' ' || character == '\t' {
			spaces += 1;
			continue;
		}
		if spaces > 0 && !out.is_empty() {
			out.push(' ');
		}
		spaces = 0;
		out.push(character);
	}
	out
}

const REPLACE_SETTINGS: &[Setting] = &[
	Setting {
		key: "rules",
		label: "One rule per line: find => replace, with an optional `| if: text`",
		kind: SettingKind::Text { multiline: true },
		default: Fallback::Text(""),
	},
	Setting {
		key: "useRegex",
		label: "Treat each rule's find as a regular expression",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
];

/// JsTextReplace: the owner's own find and replace rules.
///
/// TestCord keeps these in a repeating settings component; a native client has no such widget
/// yet, so they live in one multiline field, one rule per line.
#[derive(Default)]
pub struct JsTextReplace {
	rules: Vec<ReplaceRule>,
	use_regex: bool,
}

struct ReplaceRule {
	find: String,
	replace: String,
	only_if_includes: String,
}

impl crate::Plugin for JsTextReplace {
	fn meta(&self) -> Meta {
		Meta {
			id: "JsTextReplace",
			name: "JsTextReplace",
			description: "Applies your own find and replace rules to messages you send.",
			authors: "nin0",
			tags: &["Utility"],
			aliases: &["jstextreplace", "textReplace"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		REPLACE_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.use_regex = flag_or(values, REPLACE_SETTINGS, "useRegex");
		self.rules = text_or(values, REPLACE_SETTINGS, "rules")
			.lines()
			.filter_map(|line| {
				let line = line.trim();
				if line.is_empty() {
					return None;
				}
				let (rule, condition) = match line.split_once("| if:") {
					Some((rule, condition)) => (rule, condition.trim()),
					None => (line, ""),
				};
				let (find, replace) = rule.split_once("=>")?;
				let find = find.trim();
				(!find.is_empty()).then(|| ReplaceRule {
					find: find.to_string(),
					replace: replace.trim().to_string(),
					only_if_includes: condition.to_string(),
				})
			})
			.take(64)
			.collect();
	}

	fn before_send(&mut self, outgoing: &mut Outgoing<'_>) -> Result<(), &'static str> {
		if self.rules.is_empty() {
			return Ok(());
		}
		let body = std::mem::take(outgoing.body);
		let mut out = body;
		for rule in &self.rules {
			if !rule.only_if_includes.is_empty() && !out.contains(&rule.only_if_includes) {
				continue;
			}
			out = if self.use_regex {
				match Regex::new(&rule.find) {
					Ok(pattern) => pattern
						.replace_all(&out, rule.replace.as_str())
						.into_owned(),
					// A rule that no longer compiles is skipped, never fatal to the send.
					Err(_) => continue,
				}
			} else {
				out.replace(&rule.find, &rule.replace)
			};
		}
		*outgoing.body = out;
		Ok(())
	}

	fn summary(&self) -> Option<String> {
		Some(format!(
			"{} {}",
			self.rules.len(),
			if self.rules.len() == 1 {
				"rule"
			} else {
				"rules"
			}
		))
	}
}

const SIGNATURE_SETTINGS: &[Setting] = &[
	Setting {
		key: "name",
		label: "Signature",
		kind: SettingKind::Text { multiline: false },
		default: Fallback::Text("a chronic discord user"),
	},
	Setting {
		key: "textHeader",
		label: "Header before the signature",
		kind: SettingKind::Text { multiline: false },
		default: Fallback::Text(">"),
	},
	Setting {
		key: "isEnabled",
		label: "Add the signature",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(true),
	},
];

/// Signature: a fixed line under everything you send.
#[derive(Default)]
pub struct Signature {
	name: Option<String>,
	header: Option<String>,
	enabled: Option<bool>,
}

impl crate::Plugin for Signature {
	fn meta(&self) -> Meta {
		Meta {
			id: "Signature",
			name: "Signature",
			description: "Appends your signature to every message you send.",
			authors: "Cyn",
			tags: &["Utility"],
			aliases: &["signature"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		SIGNATURE_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.name = Some(text_or(values, SIGNATURE_SETTINGS, "name"));
		self.header = Some(text_or(values, SIGNATURE_SETTINGS, "textHeader"));
		self.enabled = Some(flag_or(values, SIGNATURE_SETTINGS, "isEnabled"));
	}

	fn before_send(&mut self, outgoing: &mut Outgoing<'_>) -> Result<(), &'static str> {
		if !self.enabled.unwrap_or(true) {
			return Ok(());
		}
		let name = self.name.clone().unwrap_or_default();
		if name.is_empty() {
			return Ok(());
		}
		let header = self.header.clone().unwrap_or_default();
		let body = std::mem::take(outgoing.body);
		if body.is_empty() {
			*outgoing.body = body;
			return Ok(());
		}
		*outgoing.body = if header.is_empty() {
			format!("{body}\n{name}")
		} else {
			format!("{body}\n{header} {name}")
		};
		Ok(())
	}

	fn summary(&self) -> Option<String> {
		Some(match &self.name {
			Some(name) if !name.is_empty() => name.clone(),
			_ => "No signature set".to_string(),
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;

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

	fn send(plugin: &mut dyn crate::Plugin, body: &str) -> Result<String, &'static str> {
		let mut text = body.to_string();
		let mut outgoing = Outgoing {
			channel: model::Id(7),
			previous: None,
			route: crate::Route::Send,
			me: model::Id(1),
			body: &mut text,
			reply: None,
		};
		plugin.before_send(&mut outgoing)?;
		Ok(outgoing.body.clone())
	}

	#[test]
	fn a_missing_apostrophe_comes_back() {
		let mut polish = plugin::<PolishWording>(&[]);
		assert_eq!(send(&mut polish, "dont do that").unwrap(), "don't do that");
		assert_eq!(send(&mut polish, "Dont do that").unwrap(), "Don't do that");
		assert_eq!(
			send(&mut polish, "we couldnt go").unwrap(),
			"we couldn't go"
		);
	}

	#[test]
	fn contractions_expand_only_when_asked() {
		let mut off = plugin::<PolishWording>(&[]);
		assert_eq!(send(&mut off, "it's fine").unwrap(), "it's fine");
		let mut on = plugin::<PolishWording>(&[("expandContractions", true.into())]);
		assert_eq!(send(&mut on, "don't stop").unwrap(), "do not stop");
		// TestCord's table has no entry for "it's", so neither does this port.
		assert_eq!(send(&mut on, "it's fine").unwrap(), "it's fine");
		assert_eq!(send(&mut on, "DON'T stop").unwrap(), "DO NOT stop");
	}

	#[test]
	fn capitalization_skips_blocked_words_and_links() {
		let mut polish = plugin::<PolishWording>(&[
			("capitalize", true.into()),
			("blockedWords", "i, discord".into()),
		]);
		assert_eq!(
			send(&mut polish, "hello there. second one!").unwrap(),
			"Hello there. Second one!"
		);
		assert_eq!(send(&mut polish, "i am here").unwrap(), "i am here");
		assert_eq!(
			send(&mut polish, "discord is fine").unwrap(),
			"discord is fine"
		);
		assert_eq!(
			send(&mut polish, "https://example.com/x").unwrap(),
			"https://example.com/x"
		);
	}

	#[test]
	fn the_quick_disable_stops_every_rule() {
		let mut polish = plugin::<PolishWording>(&[
			("quickDisable", true.into()),
			("expandContractions", true.into()),
		]);
		assert_eq!(send(&mut polish, "dont touch me").unwrap(), "dont touch me");
	}

	#[test]
	fn periods_land_only_after_words() {
		let mut polish = plugin::<PolishWording>(&[("addPeriods", true.into())]);
		assert_eq!(send(&mut polish, "done").unwrap(), "done.");
		assert_eq!(send(&mut polish, "what!").unwrap(), "what!");
		assert_eq!(send(&mut polish, "one\ntwo").unwrap(), "one.\ntwo.");
	}

	#[test]
	fn filtered_words_leave_readable_text() {
		let mut filter = plugin::<ProfanityFilter>(&[("words", "bad, worse".into())]);
		assert_eq!(send(&mut filter, "this is bad, ok").unwrap(), "this is, ok");
		assert_eq!(send(&mut filter, "nothing here").unwrap(), "nothing here");
	}

	#[test]
	fn an_emptied_message_becomes_a_duck_or_nothing() {
		let mut ducking = plugin::<ProfanityFilter>(&[("words", "bad".into())]);
		assert_eq!(send(&mut ducking, "bad").unwrap(), ":duck:");
		let mut refusing =
			plugin::<ProfanityFilter>(&[("words", "bad".into()), ("duckOnEmpty", false.into())]);
		assert_eq!(send(&mut refusing, "bad"), Err(PROFANITY_EMPTY));
	}

	#[test]
	fn combined_words_are_never_filtered() {
		let mut filter = plugin::<ProfanityFilter>(&[("words", "bad".into())]);
		assert_eq!(
			send(&mut filter, "badminton is fine").unwrap(),
			"badminton is fine"
		);
	}

	#[test]
	fn own_rules_apply_in_order() {
		let mut replace = plugin::<JsTextReplace>(&[(
			"rules",
			"teh => the\nfoo => bar\nbaz => qux | if: quux".into(),
		)]);
		assert_eq!(send(&mut replace, "teh foo").unwrap(), "the bar");
		assert_eq!(send(&mut replace, "baz alone").unwrap(), "baz alone");
		assert_eq!(send(&mut replace, "baz quux").unwrap(), "qux quux");
	}

	#[test]
	fn a_broken_regex_rule_is_skipped() {
		let mut replace = plugin::<JsTextReplace>(&[
			("rules", "( => x\nfoo => bar".into()),
			("useRegex", true.into()),
		]);
		assert_eq!(send(&mut replace, "foo").unwrap(), "bar");
	}

	#[test]
	fn the_signature_lands_under_the_message() {
		let mut signature = plugin::<Signature>(&[("name", "- me".into())]);
		assert_eq!(send(&mut signature, "hello").unwrap(), "hello\n> - me");
		let mut quiet = plugin::<Signature>(&[("isEnabled", false.into())]);
		assert_eq!(send(&mut quiet, "hello").unwrap(), "hello");
		let mut bare = plugin::<Signature>(&[("name", "me".into()), ("textHeader", "".into())]);
		assert_eq!(send(&mut bare, "hello").unwrap(), "hello\nme");
	}

	#[test]
	fn an_empty_message_never_grows_a_signature() {
		let mut signature = plugin::<Signature>(&[]);
		assert_eq!(send(&mut signature, "").unwrap(), "");
	}

	#[test]
	fn every_port_is_off_until_it_is_enabled() {
		let mut registry = crate::Registry::new();
		let message = model::Id(7);
		let mut body = "dont".to_string();
		let mut outgoing = Outgoing {
			channel: message,
			previous: None,
			route: crate::Route::Send,
			me: model::Id(1),
			body: &mut body,
			reply: None,
		};
		registry.before_send(&mut outgoing).unwrap();
		assert_eq!(outgoing.body, "dont");
	}
}
