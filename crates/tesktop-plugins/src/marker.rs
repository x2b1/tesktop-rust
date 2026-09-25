//! A line under a message: what the original draws as a message accessory.

use crate::{Fallback, Meta, Setting, SettingKind, Values, text_or};
use regex::Regex;
use std::sync::OnceLock;

/// The hosts that are nothing but a link to one of those videos.
const KNOWN_HOSTS: &[&str] = &["rickroll.link", "rickrolled.fr"];

/// The videos, by the id the host uses.
const KNOWN_VIDEOS: &[&str] = &[
	"dQw4w9WgXcQ",
	"oHg5SJYRHA0",
	"6_b7RDuLwcI",
	"G8iEMVr7GFg",
	"AyOqGRjVtls",
	"6mhmcwmgWbA",
	"SpZ2FsEfwP4",
	"H01BwSD9eyQ",
	"nrsnN23tmUA",
	"8mkofgRW1II",
	"rAx5LIul1N8",
	"sO4wVSA9UPs",
	"rrs0B_LM898",
	"doEqUhFiQS4",
	"epyRUp0BhrA",
	"uK5WDo_3s7s",
	"wzSVOcgKq04",
	"7B--1KArxow",
	"rbsPu1z3ugQ",
	"ptw2FLKXDQE",
	"E50L-JYWm3w",
	"8leAAwMIigI",
	"ByqFY-Boq5Y",
	"E4ihJMQUmUQ",
	"cjBHXvBYw5s",
	"xaazUgEKuVA",
	"TzXXHVhGXTQ",
	"Uj1ykZWtPYI",
	"EE-xtCF3T94",
	"V-_O7nl0Ii0",
	"cqF6M25kqq4",
	"0SoNH07Slj0",
	"xfr64zoBTAQ",
	"j5a0jTc9S10",
	"dPmZqsQNzGA",
	"nHRbZW097Uk",
	"BjDebmqFRuc",
	"Gc2u6AFImn8",
	"8VFzHYtOARw",
	"cSAp9sBzPbc",
	"Dx5i1t0mN78",
	"Oo0twK2ZbLU",
	"cvh0nX08nRw",
	"lXMskKTw3Bc",
	"7z_1E8VGJOw",
	"VgojnNgmgVs",
	"5wOXc03RwVA",
	"2xx_2XNxxfA",
	"lpiB2wMc49g",
	"H8ZH_mkfPUY",
	"Svj1bZz2mXw",
	"iik25wqIuFo",
	"hvL1339luv0",
	"N9w1lCZfaWI",
];

const SETTINGS: &[Setting] = &[
	Setting {
		key: "customLinks",
		label: "Links that count (comma separated)",
		kind: SettingKind::Text { multiline: true },
		default: Fallback::Text(""),
	},
	Setting {
		key: "customVideoIds",
		label: "Video ids that count (comma separated)",
		kind: SettingKind::Text { multiline: true },
		default: Fallback::Text(""),
	},
];

/// The links in a body: a bare one, one inside a markdown link, and one the sender masked
/// behind angle brackets, which is how they are usually hidden.
fn words() -> &'static Regex {
	static PATTERN: OnceLock<Regex> = OnceLock::new();
	PATTERN.get_or_init(|| Regex::new(r"https?://\S+").expect("a fixed pattern"))
}

/// AntiRickroll: a line under a message whose link is one you would rather not follow.
#[derive(Default)]
pub struct AntiRickroll {
	custom_links: Vec<String>,
	custom_videos: Vec<String>,
}

impl AntiRickroll {
	/// Whether a link is one of ours, and whether it is one the owner added.
	fn classify(&self, url: &str) -> Option<bool> {
		let without_markdown = url.trim_matches(|character| matches!(character, '>' | ')' | '('));
		if self
			.custom_links
			.iter()
			.any(|link| !link.is_empty() && without_markdown.contains(link.as_str()))
		{
			return Some(true);
		}
		let host = host_of(without_markdown)?;
		let host = host.trim_start_matches("www.");
		if KNOWN_HOSTS.contains(&host) {
			return Some(false);
		}
		let video = match host {
			"youtube.com" | "youtu.be" => video_id(without_markdown, host),
			_ => None,
		}?;
		// A video you added is yours, and is said as such rather than as one of theirs.
		if self.custom_videos.iter().any(|known| known == &video) {
			return Some(true);
		}
		KNOWN_VIDEOS.contains(&video.as_str()).then_some(false)
	}

	/// The line to draw under a message, if any of its links is one of ours.
	pub fn warning(&self, content: &str) -> Option<String> {
		// A masked link is the one they are hiding, so it is the one worth looking at first.
		let mut candidates: Vec<&str> = words()
			.find_iter(content)
			.map(|found| found.as_str())
			.collect();
		candidates.sort_by_key(|url| !url.starts_with('<'));
		for url in candidates {
			match self.classify(url) {
				Some(true) => {
					return Some("This link matches one of your rickroll filters.".to_string());
				}
				Some(false) => return Some("This link is a known rickroll.".to_string()),
				None => {}
			}
		}
		None
	}
}

/// The host of a link, or `None` when the link has no recognisable one.
fn host_of(url: &str) -> Option<&str> {
	let rest = url.split_once("://")?.1;
	let host = rest.split(['/', '?', '#']).next()?;
	(!host.is_empty()).then_some(host)
}

fn video_id(url: &str, host: &str) -> Option<String> {
	if host == "youtu.be" {
		let id = url.split_once("://")?.1.trim_start_matches("youtu.be/");
		let id = id.split(['?', '#']).next()?;
		return (!id.is_empty()).then(|| id.to_string());
	}
	// A watch link carries the id in the query and an embed link in its path; anything else
	// does not name a video at all.
	rest_of_query(url, "v=")
		.or_else(|| rest_of_query(url, "/embed/"))
		.map(str::to_string)
}

fn rest_of_query<'a>(url: &'a str, marker: &str) -> Option<&'a str> {
	let after = url.split_once(marker)?.1;
	let end = after
		.find(|character: char| {
			character.is_whitespace() || matches!(character, '&' | '#' | '<' | '>')
		})
		.unwrap_or(after.len());
	let id = &after[..end];
	(!id.is_empty()).then_some(id)
}

impl crate::Plugin for AntiRickroll {
	fn meta(&self) -> Meta {
		Meta {
			id: "AntiRickroll",
			name: "AntiRickroll",
			description: "Warns you under a message whose link is one you would rather not follow.",
			authors: "x2b",
			tags: &["Fun", "Utility"],
			aliases: &["antiRickroll", "vencord-antirickroll"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.custom_links = split(&text_or(values, SETTINGS, "customLinks"));
		self.custom_videos = split(&text_or(values, SETTINGS, "customVideoIds"));
	}

	fn message_marker(&self, message: &model::Message) -> Option<String> {
		self.warning(&message.content)
	}

	fn summary(&self) -> Option<String> {
		let custom = self.custom_links.len() + self.custom_videos.len();
		Some(if custom == 0 {
			format!(
				"{} videos and {} hosts",
				KNOWN_VIDEOS.len(),
				KNOWN_HOSTS.len()
			)
		} else {
			format!("{custom} of your own")
		})
	}
}

/// A comma separated list, kept as it was typed: a video id is case sensitive.
fn split(raw: &str) -> Vec<String> {
	raw.split(',')
		.map(str::trim)
		.filter(|part| !part.is_empty())
		.map(str::to_owned)
		.take(128)
		.collect()
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
	fn a_known_video_gets_a_line_under_the_message() {
		let plugin = AntiRickroll::default();
		assert_eq!(
			plugin
				.message_marker(&message("look: https://youtu.be/dQw4w9WgXcQ"))
				.as_deref(),
			Some("This link is a known rickroll.")
		);
	}

	#[test]
	fn a_masked_link_is_still_caught() {
		let plugin = AntiRickroll::default();
		assert!(
			plugin
				.message_marker(&message("nice <https://youtu.be/dQw4w9WgXcQ>"))
				.is_some()
		);
	}

	#[test]
	fn an_ordinary_video_is_left_alone() {
		let plugin = AntiRickroll::default();
		assert!(
			plugin
				.message_marker(&message("https://www.youtube.com/watch?v=aqz-KE-bpKQ"))
				.is_none()
		);
	}

	#[test]
	fn no_link_at_all_gets_no_line() {
		let plugin = AntiRickroll::default();
		assert!(plugin.message_marker(&message("just words")).is_none());
	}

	#[test]
	fn your_own_list_adds_to_theirs() {
		let mut plugin = AntiRickroll::default();
		plugin.configure(&Values(
			[(
				"customVideoIds".to_string(),
				serde_json::json!("aqz-KE-bpKQ"),
			)]
			.into_iter()
			.collect(),
		));
		assert_eq!(
			plugin
				.message_marker(&message("https://www.youtube.com/watch?v=aqz-KE-bpKQ"))
				.as_deref(),
			Some("This link matches one of your rickroll filters.")
		);
		assert!(plugin.summary().unwrap().contains("1 of your own"));
	}

	#[test]
	fn a_known_host_is_caught_without_a_video() {
		let plugin = AntiRickroll::default();
		assert!(
			plugin
				.message_marker(&message("https://rickroll.link/abc"))
				.is_some()
		);
	}

	#[test]
	fn the_registry_draws_one_line_per_message() {
		let mut registry = crate::Registry::new();
		let mut first = message("https://youtu.be/dQw4w9WgXcQ");
		first.id = model::Id(1);
		let mut second = message("nothing here");
		second.id = model::Id(2);
		assert!(
			registry
				.message_markers([&first, &second].into_iter())
				.is_empty()
		);
		registry.set_enabled("AntiRickroll", true);
		let markers = registry.message_markers([&first, &second].into_iter());
		assert_eq!(markers.len(), 1);
		assert!(markers.contains_key(&model::Id(1)));
	}
}
