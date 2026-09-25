//! A filter that swaps the words you would rather not read for ones you would.

use crate::{Fallback, Meta, Setting, SettingKind, Values, flag_or};
use regex::Regex;
use std::sync::OnceLock;

/// The words each category holds, kept as the original lists them so the port and the
/// original block the same things.
const SEXUAL_VERBS: &[&str] = &["fuck", "cum"];
const SEXUAL_NOUNS: &[&str] = &[
	"cunt", "yuri", "whore", "dick", "pussy", "slut", "tit", "cum", "cock", "blowjob", "sex",
	"ass", "furry", "bewbs", "boob", "booba", "boobies", "boobs", "booby", "porn", "pron",
	"pronhub", "r34", "rape", "raped", "raping", "rapist",
];
const BRAINROT_NOUNS: &[&str] = &[
	"mewing",
	"mew",
	"skibidi",
	"gyat",
	"gyatt",
	"rizzler",
	"nettspend",
	"boykisser",
	"ohio",
	"rizz",
	"tickle my toes bruh",
	"crack my spine like a whip",
	"hawk tuah",
];
/// Spelled the way they are written, because that is how they appear in a message.
const SLUR_NOUNS: &[&str] = &[
	"retard", "faggot", "fag", "faggots", "fags", "retards", "n*g",
];
const GENERAL_VERBS: &[&str] = &["kill", "destroy"];
const GENERAL_NOUNS: &[&str] = &["shit", "bullshit", "bitch", "bastard", "die", "brainless"];
const FUN_NOUNS: &[&str] = &["kotlin", "avast"];

const VERB_REPLACEMENTS: &[&str] = &[
	"love",
	"eat",
	"deconstruct",
	"marry",
	"fart",
	"teach",
	"display",
	"plug",
	"explode",
	"undress",
	"finish",
	"freeze",
	"beat",
	"free",
	"brush",
	"allocate",
	"date",
	"melt",
	"breed",
	"educate",
	"injure",
	"change",
];
const NOUN_REPLACEMENTS: &[&str] = &[
	"pasta",
	"kebab",
	"cake",
	"potato",
	"woman",
	"computer",
	"java",
	"hamburger",
	"monster truck",
	"osu!",
	"Ukrainian ball in search of gas game",
	"Anime",
	"Anime girl",
	"good",
	"keyboard",
	"NVIDIA RTX 3090 Graphics Card",
	"storm",
	"queen",
	"single",
	"umbrella",
	"mosque",
	"physics",
	"bath",
	"virus",
	"bathroom",
	"mom",
	"owner",
	"airport",
	"Avast Antivirus Free",
];

/// The obfuscated spellings a slur also arrives as, matched the way the original matches them.
const SLUR_PATTERNS: &[&str] = &[r"\bn{1,}(i|!|1){1,}(b|g){2,}(a|@|e|3){1,}?"];

const SETTINGS: &[Setting] = &[
	Setting {
		key: "incoming",
		label: "Filter the messages you receive",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(true),
	},
	Setting {
		key: "blockSexual",
		label: "Block sexual words",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(true),
	},
	Setting {
		key: "blockBrainrot",
		label: "Block the current slang",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(true),
	},
	Setting {
		key: "blockSlurs",
		label: "Block slurs",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(true),
	},
	Setting {
		key: "blockInsults",
		label: "Block insults",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(true),
	},
	Setting {
		key: "blockOthers",
		label: "Block the words one author dislikes",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(true),
	},
];

/// GoodPerson: the words you would rather not read arrive as something else.
#[derive(Default)]
pub struct GoodPerson {
	incoming: bool,
	sexual: bool,
	brainrot: bool,
	slurs: bool,
	insults: bool,
	others: bool,
}

impl GoodPerson {
	/// The nouns the enabled categories hold, longest first so a longer word wins its match.
	fn nouns(&self) -> Vec<&'static str> {
		let mut words: Vec<&'static str> = Vec::new();
		if self.brainrot {
			words.extend_from_slice(BRAINROT_NOUNS);
		}
		if self.insults {
			words.extend_from_slice(GENERAL_NOUNS);
		}
		if self.others {
			words.extend_from_slice(FUN_NOUNS);
		}
		if self.sexual {
			words.extend_from_slice(SEXUAL_NOUNS);
		}
		if self.slurs {
			words.extend_from_slice(SLUR_NOUNS);
		}
		words.sort_unstable_by_key(|word| std::cmp::Reverse(word.len()));
		words.dedup();
		words
	}

	fn verbs(&self) -> Vec<&'static str> {
		let mut words: Vec<&'static str> = Vec::new();
		if self.sexual {
			words.extend_from_slice(SEXUAL_VERBS);
		}
		if self.insults {
			words.extend_from_slice(GENERAL_VERBS);
		}
		words
	}

	/// The pattern the enabled nouns match, built once per configuration.
	fn noun_pattern(&self) -> Option<Regex> {
		let words = self.nouns();
		if words.is_empty() {
			return None;
		}
		let alternatives: Vec<String> = words
			.iter()
			// A multi-word phrase contains a space, so its parts are the boundaries instead.
			.map(|word| {
				if word.contains(' ') {
					word.split(' ')
						.map(regex::escape)
						.collect::<Vec<_>>()
						.join(r"\s+")
				} else {
					regex::escape(word)
				}
			})
			.collect();
		Some(Regex::new(&format!(r"\b({})\b", alternatives.join("|"))).expect("escaped words"))
	}

	fn verb_pattern(&self) -> Option<Regex> {
		let words = self.verbs();
		if words.is_empty() {
			return None;
		}
		let alternatives: Vec<String> = words.iter().map(|word| regex::escape(word)).collect();
		Some(Regex::new(&format!(r"\b({})\b", alternatives.join("|"))).expect("escaped words"))
	}

	fn slur_pattern() -> &'static Regex {
		static PATTERN: OnceLock<Regex> = OnceLock::new();
		PATTERN.get_or_init(|| Regex::new(&SLUR_PATTERNS.join("|")).expect("a fixed pattern"))
	}

	/// Replace every blocked word in a body.
	///
	/// The replacement is chosen from the word itself rather than at random, so the same
	/// message always reads the same way and a message is never rewritten twice.
	pub fn rewrite(&self, content: &str) -> String {
		let mut out = content.to_string();
		if let Some(pattern) = self.verb_pattern() {
			out = pattern
				.replace_all(&out, |found: &regex::Captures<'_>| {
					pick(VERB_REPLACEMENTS, &found[0])
				})
				.into_owned();
		}
		if let Some(pattern) = self.noun_pattern() {
			out = pattern
				.replace_all(&out, |found: &regex::Captures<'_>| {
					pick(NOUN_REPLACEMENTS, &found[0])
				})
				.into_owned();
		}
		if self.slurs {
			out = Self::slur_pattern()
				.replace_all(&out, |found: &regex::Captures<'_>| {
					pick(NOUN_REPLACEMENTS, &found[0])
				})
				.into_owned();
		}
		out
	}
}

/// One replacement, chosen by the word it stands in for.
fn pick(table: &[&str], word: &str) -> String {
	if table.is_empty() {
		return word.to_string();
	}
	table[hash(word) as usize % table.len()].to_string()
}

fn hash(text: &str) -> u64 {
	let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
	for byte in text.as_bytes() {
		hash ^= u64::from(*byte);
		hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
	}
	hash
}

impl crate::Plugin for GoodPerson {
	fn meta(&self) -> Meta {
		Meta {
			id: "GoodPerson",
			name: "GoodPerson",
			description: "Swaps the words you would rather not read for ones you would.",
			authors: "nin0dev, mantikafasi",
			tags: &["Utility", "Fun"],
			aliases: &["goodPerson", "vc-goodperson"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.incoming = flag_or(values, SETTINGS, "incoming");
		self.sexual = flag_or(values, SETTINGS, "blockSexual");
		self.brainrot = flag_or(values, SETTINGS, "blockBrainrot");
		self.slurs = flag_or(values, SETTINGS, "blockSlurs");
		self.insults = flag_or(values, SETTINGS, "blockInsults");
		self.others = flag_or(values, SETTINGS, "blockOthers");
	}

	fn mutate_incoming(&mut self, message: &mut model::Message) {
		if !self.incoming {
			return;
		}
		message.content = self.rewrite(&message.content);
	}

	fn before_send(&mut self, outgoing: &mut crate::Outgoing<'_>) -> Result<(), &'static str> {
		let body = std::mem::take(outgoing.body);
		*outgoing.body = self.rewrite(&body);
		Ok(())
	}

	fn summary(&self) -> Option<String> {
		let count = self.nouns().len() + self.verbs().len();
		(count > 0).then(|| {
			if self.incoming {
				format!("{count} words, both ways")
			} else {
				format!("{count} words, sent only")
			}
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::Plugin;

	/// A port with every category at its declared default, then the given overrides applied
	/// in one pass, which is the only way `configure` is ever called.
	fn plugin(overrides: &[(&str, bool)]) -> GoodPerson {
		let mut values: Vec<(String, serde_json::Value)> = SETTINGS
			.iter()
			.map(|setting| (setting.key.to_string(), serde_json::json!(true)))
			.collect();
		for (key, value) in overrides {
			match values.iter_mut().find(|(name, _)| name == key) {
				Some(entry) => entry.1 = serde_json::json!(value),
				None => values.push((key.to_string(), serde_json::json!(value))),
			}
		}
		let mut plugin = GoodPerson::default();
		plugin.configure(&Values(values.into_iter().collect()));
		plugin
	}

	#[test]
	fn a_blocked_word_arrives_as_another_one() {
		let plugin = plugin(&[]);
		let rewritten = plugin.rewrite("what the fuck");
		assert!(!rewritten.contains("fuck"), "{rewritten}");
		assert!(rewritten.starts_with("what the "), "{rewritten}");
	}

	#[test]
	fn the_same_word_always_arrives_the_same_way() {
		let plugin = plugin(&[]);
		assert_eq!(plugin.rewrite("kotlin"), plugin.rewrite("kotlin"));
	}

	#[test]
	fn a_word_inside_another_word_is_left_alone() {
		let plugin = plugin(&[]);
		assert_eq!(plugin.rewrite("Scunthorpe problem"), "Scunthorpe problem");
	}

	#[test]
	fn a_phrase_matches_whole() {
		let plugin = plugin(&[]);
		assert_ne!(plugin.rewrite("ohio"), "ohio");
		assert!(!plugin.rewrite("ohio").contains("ohio"));
	}

	#[test]
	fn a_category_can_be_left_out() {
		let plugin = plugin(&[("blockBrainrot", false)]);
		assert_eq!(plugin.rewrite("ohio"), "ohio");
		assert_ne!(plugin.rewrite("ass"), "ass");
	}

	#[test]
	fn every_category_off_leaves_the_body_alone() {
		let plugin = plugin(&[
			("blockSexual", false),
			("blockBrainrot", false),
			("blockSlurs", false),
			("blockInsults", false),
			("blockOthers", false),
		]);
		assert_eq!(plugin.rewrite("what the fuck, ohio"), "what the fuck, ohio");
		assert!(plugin.summary().is_none());
	}

	#[test]
	fn the_obfuscated_spelling_is_caught_too() {
		let plugin = plugin(&[]);
		let rewritten = plugin.rewrite("n1gga");
		assert_ne!(rewritten, "n1gga");
	}

	#[test]
	fn an_incoming_message_is_filtered_and_a_sent_one_too() {
		let mut plugin = plugin(&[]);
		let mut inbound = test_support::message(1, model::Id(7));
		inbound.content = "kotlin".to_string();
		plugin.mutate_incoming(&mut inbound);
		assert_ne!(inbound.content, "kotlin");

		let mut text = "kotlin".to_string();
		let mut outgoing = crate::Outgoing {
			channel: model::Id(7),
			me: model::Id(1),
			body: &mut text,
			reply: None,
			previous: None,
			route: crate::Route::Send,
		};
		plugin.before_send(&mut outgoing).expect("no veto");
		assert_ne!(outgoing.body, "kotlin");
	}

	#[test]
	fn the_incoming_half_can_be_turned_off() {
		let mut plugin = plugin(&[("incoming", false)]);
		let mut inbound = test_support::message(1, model::Id(7));
		inbound.content = "kotlin".to_string();
		plugin.mutate_incoming(&mut inbound);
		assert_eq!(inbound.content, "kotlin");
		assert!(plugin.summary().unwrap().contains("sent only"));
	}
}
