//! SplitLargeMessages: sends one oversized body as several Discord-sized messages.
//!
//! TestCord cancels the send and posts the chunks itself, with a delay between them. Here the
//! plugin only decides the split; the app keeps ownership of every send, so the first chunk is
//! the command the composer produced and the rest go through the same send path in order.

use crate::{Fallback, Meta, SendContext, Setting, SettingKind, Values, number_or, text_or};

/// Discord's limit for an account without Nitro.
pub const MESSAGE_LIMIT: usize = 2000;
/// Longest chunk the owner may configure; Discord rejects anything beyond this.
pub const MAX_LIMIT: usize = 4000;
/// Chunks one split may produce, so a pasted wall of text cannot flood a channel.
pub const MAX_CHUNKS: usize = 8;
/// Queued chunks the app holds, oldest first.
pub const MAX_QUEUED_CHUNKS: usize = 16;

const SETTINGS: &[Setting] = &[
	Setting {
		key: "splitMode",
		label: "Prefer this boundary",
		kind: SettingKind::Choice(&[
			("newlines", "Newlines"),
			("spaces", "Spaces"),
			("characters", "Exact length"),
		]),
		default: Fallback::Text("newlines"),
	},
	Setting {
		key: "chunkLimit",
		label: "Maximum characters per chunk",
		kind: SettingKind::Number {
			min: MESSAGE_LIMIT as i64,
			max: MAX_LIMIT as i64,
		},
		default: Fallback::Number(MESSAGE_LIMIT as i64),
	},
	Setting {
		key: "sendDelayMs",
		label: "Delay between chunks (ms)",
		kind: SettingKind::Number {
			min: 0,
			max: 60_000,
		},
		default: Fallback::Number(1000),
	},
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SplitMode {
	#[default]
	Newlines,
	Spaces,
	Characters,
}

impl SplitMode {
	fn parse(value: &str) -> Self {
		match value {
			"spaces" => Self::Spaces,
			"characters" => Self::Characters,
			_ => Self::Newlines,
		}
	}
}

pub struct SplitLargeMessages {
	mode: SplitMode,
	limit: usize,
	/// Delay the app should leave between chunks, read from the settings.
	delay_ms: u64,
}

impl Default for SplitLargeMessages {
	fn default() -> Self {
		Self {
			mode: SplitMode::Newlines,
			limit: MESSAGE_LIMIT,
			delay_ms: 1000,
		}
	}
}

impl SplitLargeMessages {
	/// How long the app should wait before sending the next chunk.
	pub fn delay_ms(&self) -> u64 {
		self.delay_ms
	}

	/// Split `content` into chunks of at most `limit` characters, preferring the chosen boundary.
	pub fn split_message(&self, content: &str) -> Vec<String> {
		split_message(content, self.limit, self.mode)
	}
}

impl crate::Plugin for SplitLargeMessages {
	fn meta(&self) -> Meta {
		Meta {
			id: "SplitLargeMessages",
			name: "SplitLargeMessages",
			description: "Sends an oversized message as several smaller ones.",
			authors: "Reycko, justjxke",
			tags: &["Chat", "Utility"],
			aliases: &["splitLargeMessages"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.mode = SplitMode::parse(&text_or(values, SETTINGS, "splitMode"));
		self.limit =
			(number_or(values, SETTINGS, "chunkLimit").clamp(1, MAX_LIMIT as i64)) as usize;
		self.delay_ms = number_or(values, SETTINGS, "sendDelayMs").max(0) as u64;
	}

	/// Empty means the body already fits and must be sent as it is.
	fn chunk_delay_ms(&self) -> Option<u64> {
		Some(self.delay_ms)
	}

	fn split(&self, _context: &SendContext, body: &str) -> Vec<String> {
		let parts = self.split_message(body);
		if parts.len() > 1 { parts } else { Vec::new() }
	}

	fn summary(&self) -> Option<String> {
		Some(format!(
			"{} characters per chunk, {} ms apart",
			self.limit.clamp(1, MAX_LIMIT),
			self.delay_ms
		))
	}
}

/// Split `content` into chunks of at most `limit` characters, preferring the chosen boundary.
pub fn split_message(content: &str, limit: usize, mode: SplitMode) -> Vec<String> {
	let total = content.chars().count();
	if limit == 0 || total <= limit {
		return vec![content.to_string()];
	}
	let characters: Vec<char> = content.chars().collect();
	let mut chunks: Vec<String> = Vec::new();
	let mut start = 0;
	while start < characters.len() && chunks.len() < MAX_CHUNKS {
		let remaining = characters.len() - start;
		let take = if remaining <= limit {
			remaining
		} else {
			split_index(&characters[start..], limit, mode)
		};
		let end = (start + take.max(1)).min(characters.len());
		chunks.push(characters[start..end].iter().collect());
		start = end;
	}
	if start < characters.len() {
		chunks.push(characters[start..].iter().collect());
	}
	chunks
}

fn split_index(content: &[char], limit: usize, mode: SplitMode) -> usize {
	if mode == SplitMode::Characters || limit == 0 || content.len() <= limit {
		return limit;
	}
	let window = &content[..limit.min(content.len())];
	let boundary = if mode == SplitMode::Newlines {
		'\n'
	} else {
		' '
	};
	if let Some(position) = window.iter().rposition(|character| *character == boundary) {
		return position + 1;
	}
	if mode == SplitMode::Newlines
		&& let Some(position) = window.iter().rposition(|character| *character == ' ')
	{
		return position + 1;
	}
	limit
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::Plugin;
	use model::Id;

	fn plugin(mode: &str, limit: i64, delay: i64) -> SplitLargeMessages {
		let mut plugin = SplitLargeMessages::default();
		plugin.configure(&Values(
			[
				("splitMode".to_string(), serde_json::json!(mode)),
				("chunkLimit".to_string(), serde_json::json!(limit)),
				("sendDelayMs".to_string(), serde_json::json!(delay)),
			]
			.into_iter()
			.collect(),
		));
		plugin
	}

	#[test]
	fn short_bodies_are_left_alone() {
		assert_eq!(
			split_message("hello", 2000, SplitMode::Newlines),
			vec!["hello"]
		);
	}

	#[test]
	fn newline_mode_cuts_on_the_last_line_break() {
		let body = format!("{}\n{}", "a".repeat(12), "b".repeat(20));
		// TestCord keeps the boundary with the chunk it ends.
		assert_eq!(
			split_message(&body, 20, SplitMode::Newlines),
			vec![
				format!(
					"{}
",
					"a".repeat(12)
				),
				"b".repeat(20)
			]
		);
	}

	#[test]
	fn space_mode_cuts_on_the_last_space() {
		assert_eq!(
			split_message("aaa bbb ccc ddd", 8, SplitMode::Spaces),
			vec!["aaa bbb ", "ccc ddd"]
		);
	}

	#[test]
	fn character_mode_is_exact() {
		assert_eq!(
			split_message("abcdefghij", 4, SplitMode::Characters),
			vec!["abcd", "efgh", "ij"]
		);
	}

	#[test]
	fn every_chunk_stays_within_the_limit() {
		let body = "word ".repeat(2000);
		for mode in [
			SplitMode::Newlines,
			SplitMode::Spaces,
			SplitMode::Characters,
		] {
			for part in split_message(&body, 2000, mode) {
				assert!(
					part.chars().count() <= 2000,
					"{mode:?} produced an oversized chunk"
				);
			}
		}
	}

	#[test]
	fn a_body_with_no_boundary_still_splits() {
		let parts = split_message(&"x".repeat(2500), 2000, SplitMode::Newlines);
		assert_eq!(parts.len(), 2);
		assert_eq!(parts[1].chars().count(), 500);
	}

	#[test]
	fn multibyte_bodies_keep_their_text() {
		let body = "日本語".repeat(1000);
		let parts = split_message(&body, 2000, SplitMode::Characters);
		assert_eq!(parts.concat(), body);
		assert!(parts.iter().all(|part| part.chars().count() <= 2000));
	}

	#[test]
	fn a_pasted_wall_of_text_is_capped() {
		let parts = split_message(&"x".repeat(60_000), 2000, SplitMode::Characters);
		assert!(parts.len() <= MAX_CHUNKS + 1);
	}

	#[test]
	fn only_an_oversized_body_asks_for_a_split() {
		let plugin = plugin("newlines", 2000, 1000);
		let context = SendContext::new(Id(7), Id(1));
		assert!(plugin.split(&context, "short").is_empty());
		assert_eq!(plugin.split(&context, &"y".repeat(2500)).len(), 2);
		assert_eq!(plugin.delay_ms(), 1000);
	}

	#[test]
	fn the_configured_limit_is_clamped_to_what_discord_accepts() {
		let plugin = plugin("newlines", 100_000, 0);
		assert!(plugin.split_message(&"z".repeat(5000)).len() >= 2);
		assert_eq!(
			plugin.split_message(&"z".repeat(5000))[0].chars().count(),
			4000
		);
	}

	#[test]
	fn the_defaults_match_testcord() {
		let plugin = SplitLargeMessages::default();
		assert_eq!(plugin.mode, SplitMode::Newlines);
		assert_eq!(plugin.limit, MESSAGE_LIMIT);
		assert_eq!(plugin.delay_ms(), 1000);
	}
}
