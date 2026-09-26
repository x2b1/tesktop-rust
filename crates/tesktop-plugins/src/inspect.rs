//! Ports that read a message you point at: what is in it, and what a link in it means.

use crate::{Fallback, Meta, Setting, SettingKind, Values, flag_or, text_or};
use model::Id;
use regex::Regex;
use std::str::FromStr;
use std::sync::OnceLock;

const BLOCK_SETTINGS: &[Setting] = &[Setting {
	key: "usersToBlock",
	label: "People to block (ids, comma separated)",
	kind: SettingKind::Text { multiline: true },
	default: Fallback::Text(""),
}];

/// ClientSideBlock: nobody on this list reaches your screen.
#[derive(Default)]
pub struct ClientSideBlock {
	blocked: Vec<Id>,
}

impl ClientSideBlock {
	pub fn blocked(&self) -> &[Id] {
		&self.blocked
	}
}

impl crate::Plugin for ClientSideBlock {
	fn meta(&self) -> Meta {
		Meta {
			id: "ClientSideBlock",
			name: "ClientSideBlock",
			description: "Keeps the messages of the people you list off your screen.",
			authors: "Testcord",
			tags: &["Utility", "Chat"],
			aliases: &["clientSideBlock"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		BLOCK_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.blocked = text_or(values, BLOCK_SETTINGS, "usersToBlock")
			.split([' ', ',', '\n', '\t', '\r'])
			.filter_map(|part| Id::from_str(part.trim()).ok())
			.take(1024)
			.collect();
	}

	fn ignore(&self, message: &model::Message) -> bool {
		if self.blocked.is_empty() {
			return false;
		}
		self.blocked.contains(&message.author.id)
	}

	fn summary(&self) -> Option<String> {
		(!self.blocked.is_empty()).then(|| format!("{} blocked", self.blocked.len()))
	}
}

/// The engines the port offers, in the order the original lists them.
pub const ENGINES: &[(&str, &str)] = &[
	("Google", "https://www.google.com/search?q="),
	("DuckDuckGo", "https://duckduckgo.com/?q="),
	("Brave", "https://search.brave.com/search?q="),
	("Bing", "https://www.bing.com/search?q="),
	("Yahoo", "https://search.yahoo.com/search?p="),
	("Yandex", "https://yandex.com/search/?text="),
	("GitHub", "https://github.com/search?q="),
	("Reddit", "https://www.reddit.com/search?q="),
	("Wikipedia", "https://wikipedia.org/w/index.php?search="),
	("Startpage", "https://www.startpage.com/sp/search?query="),
	("Kagi", "https://kagi.com/search?q="),
];

const SEARCH_SETTINGS: &[Setting] = &[
	Setting {
		key: "replacementEngine",
		label: "Replace a Google link with",
		kind: SettingKind::Choice(&[
			("Off", "Off"),
			("Custom", "Custom engine"),
			("Google", "Google"),
			("DuckDuckGo", "DuckDuckGo"),
			("Brave", "Brave"),
			("Bing", "Bing"),
			("Yahoo", "Yahoo"),
			("Yandex", "Yandex"),
			("GitHub", "GitHub"),
			("Reddit", "Reddit"),
			("Wikipedia", "Wikipedia"),
			("Startpage", "Startpage"),
			("Kagi", "Kagi"),
		]),
		default: Fallback::Text("Off"),
	},
	Setting {
		key: "customEngineName",
		label: "Custom engine name",
		kind: SettingKind::Text { multiline: false },
		default: Fallback::Text(""),
	},
	Setting {
		key: "customEngineURL",
		label: "Custom engine address",
		kind: SettingKind::Text { multiline: false },
		default: Fallback::Text(""),
	},
];

/// ReplaceGoogleSearch: a Google result link in a message can open in the engine you read.
#[derive(Default)]
pub struct ReplaceGoogleSearch {
	engine: String,
	custom: Option<(String, String)>,
}

impl crate::Plugin for ReplaceGoogleSearch {
	fn meta(&self) -> Meta {
		Meta {
			id: "ReplaceGoogleSearch",
			name: "ReplaceGoogleSearch",
			description: "Opens a Google link in a message with the search engine you read.",
			authors: "Vencord",
			tags: &["Chat", "Utility"],
			aliases: &["replaceGoogleSearch"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		SEARCH_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.engine = crate::text_or(values, SEARCH_SETTINGS, "replacementEngine");
		let name = crate::text_or(values, SEARCH_SETTINGS, "customEngineName")
			.trim()
			.chars()
			.take(32)
			.filter(|character| !character.is_control())
			.collect::<String>();
		let url = crate::text_or(values, SEARCH_SETTINGS, "customEngineURL")
			.trim()
			.to_string();
		self.custom = (!name.is_empty() && !url.is_empty()).then_some((name, url));
	}

	fn mutate_incoming(&mut self, message: &mut model::Message) {
		if self.engine == "Off" || self.engine == "Custom" && self.custom.is_none() {
			return;
		}
		if !message.content.contains("google.com/search") {
			return;
		}
		let rewritten = rewrite_links(&message.content, |url| self.replace(url));
		message.content = rewritten;
	}

	fn summary(&self) -> Option<String> {
		(self.engine != "Off").then(|| format!("Google links open in {}", self.engine))
	}
}

/// Rewrite every link in a body through `replace`, leaving everything else exactly as it was.
pub fn rewrite_links(content: &str, replace: impl Fn(&str) -> Option<String>) -> String {
	if !content.contains("http") {
		return content.to_string();
	}
	let mut out = String::with_capacity(content.len());
	let mut rest = content;
	while let Some(start) = rest.find("http") {
		let (before, tail) = rest.split_at(start);
		let end = tail
			.find(|character: char| {
				character.is_whitespace() || matches!(character, '<' | '>' | '"' | '\'' | '`')
			})
			.unwrap_or(tail.len());
		let (candidate, after) = tail.split_at(end);
		out.push_str(before);
		out.push_str(&replace(candidate).unwrap_or_else(|| candidate.to_string()));
		rest = after;
	}
	out.push_str(rest);
	out
}

impl ReplaceGoogleSearch {
	/// The prefix an engine search uses, if the setting names one that exists.
	fn prefix(&self) -> Option<String> {
		if self.engine == "Off" {
			return None;
		}
		if self.engine == "Custom" {
			let (name, url) = self.custom.clone()?;
			// A custom engine is only usable when it is a real web address.
			return (url.starts_with("https://") && !name.is_empty()).then_some(url);
		}
		ENGINES
			.iter()
			.find(|(name, _)| *name == self.engine)
			.map(|(_, url)| (*url).to_string())
	}

	/// Rewrite one Google search link, leaving the query exactly as it was.
	pub fn replace(&self, url: &str) -> Option<String> {
		let prefix = self.prefix()?;
		let query = google_query(url)?;
		Some(format!("{prefix}{query}"))
	}
}

/// The query of a Google search link, or `None` when the link is not one.
fn google_query(url: &str) -> Option<String> {
	let rest = url
		.strip_prefix("https://www.google.com/search?")
		.or_else(|| url.strip_prefix("https://google.com/search?"))?;
	let query = rest
		.split('&')
		.find_map(|pair| pair.strip_prefix("q="))
		.or_else(|| {
			rest.split('&')
				.find(|pair| pair.starts_with("q="))
				.map(|pair| &pair[2..])
		})?;
	(!query.is_empty() && !query.chars().any(char::is_control)).then(|| query.to_string())
}

/// A run of base64 as the original looks for it: at least four characters, with the padding
/// where base64 puts it.
fn base64_runs() -> &'static Regex {
	static PATTERN: OnceLock<Regex> = OnceLock::new();
	PATTERN.get_or_init(|| {
		Regex::new(r"\b[A-Za-z0-9+/]{4,}(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?\b")
			.expect("a fixed pattern")
	})
}

const DECODE_SETTINGS: &[Setting] = &[Setting {
	key: "showAction",
	label: "Show the decode entry in the message menu",
	kind: SettingKind::Toggle,
	default: Fallback::Flag(true),
}];

/// DecodeBase64: reads the base64 in a message and hands you the text inside it.
pub struct DecodeBase64 {
	show_action: bool,
}

impl Default for DecodeBase64 {
	fn default() -> Self {
		Self { show_action: true }
	}
}

impl crate::Plugin for DecodeBase64 {
	fn meta(&self) -> Meta {
		Meta {
			id: "DecodeBase64",
			name: "DecodeBase64",
			description: "Decodes the base64 in a message so you can read it.",
			authors: "Equicord",
			tags: &["Utility", "Chat"],
			// TestCord's folder is `baseDecoder`; both spellings, and the id this port
			// used to have, are accepted so a settings file from either finds it.
			aliases: &["baseDecoder", "BaseDecoder", "base-decoder"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		DECODE_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.show_action = flag_or(values, DECODE_SETTINGS, "showAction");
	}

	fn message_actions(&self) -> Vec<crate::MessageAction> {
		vec![crate::MessageAction {
			id: "decode-base64",
			label: "Decode the base64 here",
		}]
	}

	fn run_action(&self, action: &str, message: &model::Message) -> Option<crate::ActionResult> {
		if action != "decode-base64" || !self.show_action {
			return None;
		}
		let mut decoded = decode_all(&message.content);
		match decoded.len() {
			0 => Some(crate::ActionResult::Notice(
				"No base64 in that message".to_string(),
			)),
			1 => Some(crate::ActionResult::Clipboard(decoded.remove(0))),
			// More than one is ambiguous, so the longest is offered and the count is said.
			count => {
				let longest = decoded.remove(0);
				Some(crate::ActionResult::Clipboard(format!(
					"{}\n\n({count} base64 strings; this is the longest)",
					longest
				)))
			}
		}
	}

	fn summary(&self) -> Option<String> {
		Some("Decodes on request".to_string())
	}
}

/// Every readable run of base64 in a body, longest first so the most likely one is offered.
pub fn decode_all(content: &str) -> Vec<String> {
	let mut decoded: Vec<String> = base64_runs()
		.find_iter(content)
		.filter_map(|found| decode_base64(found.as_str()))
		.collect();
	decoded.sort_by_key(|text| std::cmp::Reverse(text.len()));
	decoded.dedup();
	decoded.truncate(8);
	decoded
}

/// Decode one run of base64 into UTF-8 text, or `None` when it is not base64 or not text.
///
/// The bytes must be valid UTF-8 as well as valid base64: a random word decodes to bytes that
/// are almost never text, and guessing at those is how a decode button turns into noise.
pub fn decode_base64(run: &str) -> Option<String> {
	// A group of one leftover character cannot be base64; two or three can, unpadded, which
	// is what most people paste.
	if run.len() % 4 == 1 {
		return None;
	}
	let bytes = decode_base64_bytes(run)?;
	String::from_utf8(bytes)
		.ok()
		.filter(|text| !text.is_empty())
}

fn decode_base64_bytes(run: &str) -> Option<Vec<u8>> {
	let mut out = Vec::with_capacity(run.len() / 4 * 3);
	let mut accumulator: u32 = 0;
	let mut bits = 0;
	for character in run.chars() {
		let value = match character {
			'A'..='Z' => u32::from(character as u8 - b'A'),
			'a'..='z' => u32::from(character as u8 - b'a') + 26,
			'0'..='9' => u32::from(character as u8 - b'0') + 52,
			'+' => 62,
			'/' => 63,
			'=' => break,
			// A run the pattern let through but base64 does not define.
			other => {
				let _ = other;
				return None;
			}
		};
		accumulator = (accumulator << 6) | value;
		bits += 6;
		if bits >= 8 {
			bits -= 8;
			out.push((accumulator >> bits) as u8);
		}
	}
	Some(out)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::Plugin;

	fn message(content: &str) -> model::Message {
		let mut message = test_support::message(1, model::Id(7));
		message.content = content.to_string();
		message
	}

	#[test]
	fn a_blocked_person_is_not_shown() {
		let mut plugin = ClientSideBlock::default();
		plugin.configure(&Values(
			[("usersToBlock".to_string(), serde_json::json!("5, 6"))]
				.into_iter()
				.collect(),
		));
		let mut theirs = message("hi");
		theirs.author.id = model::Id(5);
		let mut mine = message("hi");
		mine.author.id = model::Id(9);
		assert!(plugin.ignore(&theirs));
		assert!(!plugin.ignore(&mine));
		assert_eq!(plugin.summary().as_deref(), Some("2 blocked"));
	}

	#[test]
	fn an_empty_list_hides_nothing() {
		let plugin = ClientSideBlock::default();
		let mut theirs = message("hi");
		theirs.author.id = model::Id(5);
		assert!(!plugin.ignore(&theirs));
		assert!(plugin.summary().is_none());
	}

	#[test]
	fn an_incoming_link_is_rewritten_where_it_is_read() {
		let mut plugin = ReplaceGoogleSearch::default();
		plugin.configure(&Values(
			[("replacementEngine".to_string(), serde_json::json!("Kagi"))]
				.into_iter()
				.collect(),
		));
		let mut inbound = message("look at https://www.google.com/search?q=rust and tell me");
		plugin.mutate_incoming(&mut inbound);
		assert_eq!(
			inbound.content,
			"look at https://kagi.com/search?q=rust and tell me"
		);
	}

	#[test]
	fn a_google_link_can_move_to_the_engine_you_read() {
		let mut plugin = ReplaceGoogleSearch::default();
		plugin.configure(&Values(
			[(
				"replacementEngine".to_string(),
				serde_json::json!("DuckDuckGo"),
			)]
			.into_iter()
			.collect(),
		));
		assert_eq!(
			plugin
				.replace("https://www.google.com/search?q=rust+lang")
				.as_deref(),
			Some("https://duckduckgo.com/?q=rust+lang"),
			"the query is copied exactly as it was, the way the original copies it"
		);
		assert_eq!(plugin.replace("https://example.com/x"), None);
	}

	#[test]
	fn the_engine_can_be_turned_off() {
		let mut plugin = ReplaceGoogleSearch::default();
		plugin.configure(&Values(
			[("replacementEngine".to_string(), serde_json::json!("Off"))]
				.into_iter()
				.collect(),
		));
		assert_eq!(plugin.replace("https://www.google.com/search?q=x"), None);
	}

	#[test]
	fn a_custom_engine_needs_a_real_address() {
		let mut plugin = ReplaceGoogleSearch::default();
		plugin.configure(&Values(
			[
				("replacementEngine".to_string(), serde_json::json!("Custom")),
				("customEngineName".to_string(), serde_json::json!("Mine")),
				(
					"customEngineURL".to_string(),
					serde_json::json!("file:///etc/passwd"),
				),
			]
			.into_iter()
			.collect(),
		));
		assert_eq!(plugin.replace("https://www.google.com/search?q=x"), None);
		plugin.configure(&Values(
			[
				("replacementEngine".to_string(), serde_json::json!("Custom")),
				("customEngineName".to_string(), serde_json::json!("Mine")),
				(
					"customEngineURL".to_string(),
					serde_json::json!("https://mine.example/?q="),
				),
			]
			.into_iter()
			.collect(),
		));
		assert_eq!(
			plugin
				.replace("https://www.google.com/search?q=x")
				.as_deref(),
			Some("https://mine.example/?q=x")
		);
	}

	#[test]
	fn base64_in_a_message_decodes() {
		assert_eq!(decode_base64("aGVsbG8=").as_deref(), Some("hello"));
		let plugin = DecodeBase64::default();
		assert_eq!(
			plugin.run_action("decode-base64", &message("look: aGVsbG8=")),
			Some(crate::ActionResult::Clipboard("hello".to_string()))
		);
	}

	#[test]
	fn a_word_that_is_not_base64_is_left_alone() {
		assert_eq!(
			decode_base64("hello"),
			None,
			"five characters is not a group"
		);
		assert_eq!(
			decode_base64("aGVsbG8"),
			Some("hello".to_string()),
			"padding is optional"
		);
		let plugin = DecodeBase64::default();
		assert_eq!(
			plugin.run_action("decode-base64", &message("just words here")),
			Some(crate::ActionResult::Notice(
				"No base64 in that message".to_string()
			))
		);
	}

	#[test]
	fn the_decode_entry_can_be_turned_off() {
		let mut plugin = DecodeBase64::default();
		plugin.configure(&Values(
			[("showAction".to_string(), serde_json::json!(false))]
				.into_iter()
				.collect(),
		));
		assert!(
			plugin
				.run_action("decode-base64", &message("aGVsbG8="))
				.is_none()
		);
	}
}
