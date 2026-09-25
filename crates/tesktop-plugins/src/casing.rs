//! Body and casing ports: how a message reads, and how a code fence is laid out.

use crate::{Fallback, Meta, Outgoing, Setting, SettingKind, Values, text_or};

const UPPER_SETTINGS: &[Setting] = &[Setting {
	key: "blockedWords",
	label: "Sentences that start with one of these stay as they are",
	kind: SettingKind::Text { multiline: true },
	default: Fallback::Text(""),
}];

/// WriteUpperCase: a capital at the start of every sentence you send.
#[derive(Default)]
pub struct WriteUpperCase {
	blocked: Vec<String>,
}

impl crate::Plugin for WriteUpperCase {
	fn meta(&self) -> Meta {
		Meta {
			id: "WriteUpperCase",
			name: "WriteUpperCase",
			description: "Capitalizes the first letter of each sentence you send.",
			authors: "Vencord",
			tags: &["Utility"],
			aliases: &["writeUpperCase"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		UPPER_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.blocked = text_or(values, UPPER_SETTINGS, "blockedWords")
			.split(',')
			.map(str::trim)
			.filter(|word| !word.is_empty())
			.map(str::to_lowercase)
			.collect();
	}

	fn before_send(&mut self, outgoing: &mut Outgoing<'_>) -> Result<(), &'static str> {
		let body = std::mem::take(outgoing.body);
		// TestCord splits on the whitespace that follows sentence punctuation, and leaves the
		// whitespace itself alone, so the same split serves here.
		let mut out = String::with_capacity(body.len());
		let mut capitalize = true;
		let mut sentence = String::new();
		for character in body.chars() {
			sentence.push(character);
			if !character.is_whitespace() {
				continue;
			}
			if capitalize {
				out.push_str(&capitalize_sentence(&sentence, &self.blocked));
			} else {
				out.push_str(&sentence);
			}
			capitalize = ends_sentence(sentence.trim_end());
			sentence.clear();
		}
		out.push_str(&capitalize_sentence(&sentence, &self.blocked));
		*outgoing.body = out;
		Ok(())
	}

	fn summary(&self) -> Option<String> {
		Some(if self.blocked.is_empty() {
			"Every sentence capitalized".to_string()
		} else {
			format!("{} exceptions", self.blocked.len())
		})
	}
}

/// Whether a finished chunk of text ends a sentence, so the next one starts capitalized.
fn ends_sentence(text: &str) -> bool {
	let trimmed = text.trim_end();
	let Some(last) = trimmed.chars().last() else {
		return true;
	};
	if !matches!(last, '.' | '!' | '?') {
		return trimmed.contains('\n');
	}
	// A trailing run of closing punctuation still ends the sentence: "done!)"
	let mut closing = 0;
	for character in trimmed.chars().rev() {
		if matches!(character, ')' | ']' | '"' | '\'') {
			closing += 1;
			continue;
		}
		break;
	}
	trimmed
		.chars()
		.rev()
		.nth(closing)
		.is_some_and(|character| matches!(character, '.' | '!' | '?'))
}

fn capitalize_sentence(chunk: &str, blocked: &[String]) -> String {
	let trimmed = chunk.trim_start();
	if trimmed.is_empty() {
		return chunk.to_string();
	}
	let lowered = trimmed.to_lowercase();
	if blocked
		.iter()
		.any(|word| lowered.starts_with(word.as_str()))
	{
		return chunk.to_string();
	}
	let leading = chunk.len() - trimmed.len();
	let mut out = String::with_capacity(chunk.len());
	out.push_str(&chunk[..leading]);
	let mut rest = trimmed.chars();
	match rest.next() {
		Some(first) => {
			out.extend(first.to_uppercase());
			out.push_str(rest.as_str());
		}
		None => out.push_str(trimmed),
	}
	out
}

/// FixCodeblockGap: a closing fence always ends its line, so text below it is not glued on.
pub struct FixCodeblockGap;

impl crate::Plugin for FixCodeblockGap {
	fn meta(&self) -> Meta {
		Meta {
			id: "FixCodeblockGap",
			name: "FixCodeblockGap",
			description: "Keeps a line break after a closing code fence.",
			authors: "Vencord",
			tags: &["Chat", "Utility"],
			aliases: &["fixCodeblockGap"],
			default_enabled: false,
		}
	}

	fn body_transform(&self) -> Option<crate::body::BodyTransform> {
		Some(fix_codeblock_gap)
	}

	fn summary(&self) -> Option<String> {
		Some("Closing fences end their line".to_string())
	}
}

/// TestCord appends an optional line break after a fenced block; in practice the gap only
/// shows when content follows the closing fence on the same line, which is what this closes.
pub fn fix_codeblock_gap(body: &str) -> String {
	let mut out = String::with_capacity(body.len());
	let mut open = false;
	let mut rest = body;
	'lines: while !rest.is_empty() {
		let (line, consumed, had_newline) = match rest.find('\n') {
			Some(end) => (&rest[..end], end + 1, true),
			None => (rest, rest.len(), false),
		};
		let trimmed = line.trim_start();
		if !trimmed.starts_with("```") {
			out.push_str(line);
			if had_newline {
				out.push('\n');
				rest = &rest[consumed..];
				continue 'lines;
			}
			rest = "";
			continue 'lines;
		}
		// The fence is three backticks; whatever follows it starts the next line.
		let lead = line.len() - trimmed.len();
		let run = line[lead..].len() - line[lead..].trim_start_matches('`').len();
		let closing = open;
		open = !open;
		out.push_str(&line[..lead + run]);
		let after = &line[lead + run..];
		if after.is_empty() {
			if had_newline {
				out.push('\n');
				rest = &rest[consumed..];
				continue 'lines;
			}
			rest = "";
			continue 'lines;
		}
		// A closing fence runs straight into the text below it; give that text its own line.
		if closing {
			out.push('\n');
		}
		out.push_str(after);
		if had_newline {
			out.push('\n');
			rest = &rest[consumed..];
		} else {
			rest = "";
		}
		continue 'lines;
	}
	out
}

/// NormalizeMessageLinks: a canary or ptb host is shown the way everyone else sees it.
pub struct NormalizeMessageLinks;

impl crate::Plugin for NormalizeMessageLinks {
	fn meta(&self) -> Meta {
		Meta {
			id: "NormalizeMessageLinks",
			name: "NormalizeMessageLinks",
			description: "Shows canary and ptb links without their channel prefix.",
			authors: "Vencord",
			tags: &["Utility"],
			aliases: &["normalizeMessageLinks"],
			default_enabled: false,
		}
	}

	fn body_transform(&self) -> Option<crate::body::BodyTransform> {
		Some(normalize_links)
	}

	fn summary(&self) -> Option<String> {
		Some("canary. and ptb. hidden".to_string())
	}
}

pub fn normalize_links(body: &str) -> String {
	if !body.contains("canary.") && !body.contains("ptb.") {
		return body.to_string();
	}
	let mut out = String::with_capacity(body.len());
	let mut rest = body;
	while let Some(start) = rest.find("http") {
		let (before, tail) = rest.split_at(start);
		let end = tail
			.find(|character: char| {
				character.is_whitespace() || matches!(character, '<' | '>' | '"' | '\'' | '`')
			})
			.unwrap_or(tail.len());
		let (candidate, after) = tail.split_at(end);
		out.push_str(before);
		out.push_str(&normalize_host(candidate));
		rest = after;
	}
	out.push_str(rest);
	out
}

fn normalize_host(candidate: &str) -> String {
	let Some((scheme, host)) = candidate.split_once("://") else {
		return candidate.to_string();
	};
	let stripped = host
		.strip_prefix("canary.")
		.or_else(|| host.strip_prefix("ptb."))
		.unwrap_or(host);
	format!("{scheme}://{stripped}")
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::Plugin;

	fn send(plugin: &mut dyn crate::Plugin, body: &str) -> String {
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
	fn each_sentence_gets_its_capital() {
		let mut plugin = WriteUpperCase::default();
		assert_eq!(
			send(&mut plugin, "hello there. second one! third?"),
			"Hello there. Second one! Third?"
		);
	}

	#[test]
	fn a_blocked_sentence_is_left_alone() {
		let mut plugin = WriteUpperCase::default();
		plugin.configure(&Values(
			[("blockedWords".to_string(), "http".into())]
				.into_iter()
				.collect(),
		));
		assert_eq!(
			send(&mut plugin, "https://example.com/x. next"),
			"https://example.com/x. Next"
		);
	}

	#[test]
	fn closing_punctuation_still_ends_a_sentence() {
		assert!(
			ends_sentence("done!"),
			"a closing bracket does not end a sentence"
		);
		assert!(ends_sentence("what?"));
		assert!(!ends_sentence("e.g"), "a decimal point does not");
		assert!(!ends_sentence("no punctuation here"));
	}

	#[test]
	fn a_closing_fence_ends_its_line() {
		assert_eq!(
			fix_codeblock_gap("before\n```\ncode\n```after"),
			"before\n```\ncode\n```\nafter"
		);
		assert_eq!(fix_codeblock_gap("```\ncode\n```"), "```\ncode\n```");
	}

	#[test]
	fn code_and_links_are_otherwise_untouched() {
		let body = "```rust\nfn main() {}\n```\n\nand a paragraph.";
		assert_eq!(fix_codeblock_gap(body), body);
	}

	#[test]
	fn channel_prefixes_are_hidden_from_links() {
		assert_eq!(
			normalize_links(
				"see https://canary.discord.com/channels/1/2 and https://ptb.discord.com/x"
			),
			"see https://discord.com/channels/1/2 and https://discord.com/x"
		);
		assert_eq!(normalize_links("no links"), "no links");
		assert_eq!(
			normalize_links("https://example.com/x"),
			"https://example.com/x"
		);
	}

	#[test]
	fn the_body_ports_offer_their_rewrite_only_while_enabled() {
		let mut registry = crate::Registry::new();
		assert!(registry.body_transform().is_none());
		registry.set_enabled("FixCodeblockGap", true);
		assert_eq!(
			registry.body_transform().map(|(id, _)| id),
			Some("FixCodeblockGap")
		);
		registry.set_enabled("NormalizeMessageLinks", true);
		assert!(registry.body_transform().is_some());
	}
}
