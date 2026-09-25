//! Outgoing ports that rewrite links or stamp a body with where it was typed.

use crate::{Fallback, Meta, Outgoing, Setting, SettingKind, Values, flag_or, text_or};
use model::Id;

/// TestCord rewrites an embedded link to the form that renders as a player rather than a
/// preview card, per origin. The map is short because only these hosts are recognised.
const EMBED_HOSTS: &[(&str, &str)] = &[
	("https://www.youtube.com", "https://youtu.be"),
	("https://m.youtube.com", "https://youtu.be"),
	("https://music.youtube.com", "https://youtu.be"),
	("https://www.youtube-nocookie.com", "https://youtu.be"),
	("https://open.spotify.com", "https://open.spotify.com"),
	(
		"https://podcasters.spotify.com",
		"https://podcasters.spotify.com",
	),
];

const EMBEDDED_SETTINGS: &[Setting] = &[Setting {
	key: "enabled",
	label: "Rewrite embedded links",
	kind: SettingKind::Toggle,
	default: Fallback::Flag(true),
}];

/// EmbeddedURLs: links that embed inline instead of turning into a card.
#[derive(Default)]
pub struct EmbeddedUrls {
	enabled: Option<bool>,
}

impl crate::Plugin for EmbeddedUrls {
	fn meta(&self) -> Meta {
		Meta {
			id: "EmbeddedURLs",
			name: "EmbeddedURLs",
			description: "Rewrites links to the form that embeds inline.",
			authors: "Dadian1",
			tags: &["Utility", "Media"],
			aliases: &["embeddedURLs"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		EMBEDDED_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.enabled = Some(flag_or(values, EMBEDDED_SETTINGS, "enabled"));
	}

	fn before_send(&mut self, outgoing: &mut Outgoing<'_>) -> Result<(), &'static str> {
		if !self.enabled.unwrap_or(true) {
			return Ok(());
		}
		let body = std::mem::take(outgoing.body);
		*outgoing.body = rewrite_links(&body);
		Ok(())
	}

	fn summary(&self) -> Option<String> {
		Some(format!("{} link forms", EMBED_HOSTS.len()))
	}
}

/// Rewrite every recognised link in `body`; anything unparseable is left exactly as it was.
pub fn rewrite_links(body: &str) -> String {
	if !body.contains("http") {
		return body.to_string();
	}
	let mut out = String::with_capacity(body.len());
	let mut rest = body;
	while let Some(start) = rest.find("http") {
		let (before, tail) = rest.split_at(start);
		let end = tail
			.find(|character: char| {
				character.is_whitespace() || matches!(character, '<' | '>' | '"' | '\'' | '`' | '|')
			})
			.unwrap_or(tail.len());
		let (candidate, after) = tail.split_at(end);
		out.push_str(before);
		out.push_str(&rewrite_one(candidate));
		rest = after;
	}
	out.push_str(rest);
	out
}

fn rewrite_one(candidate: &str) -> String {
	let Ok(url) = url::Url::parse(candidate) else {
		return candidate.to_string();
	};
	let origin = format!("{}://{}", url.scheme(), url.host_str().unwrap_or_default());
	let Some((_, replacement)) = EMBED_HOSTS
		.iter()
		.find(|(known, _)| known.eq_ignore_ascii_case(&origin))
	else {
		return candidate.to_string();
	};
	format!(
		"{replacement}{}?{}",
		url.path(),
		url.query().unwrap_or_default()
	)
	.trim_end_matches('?')
	.to_string()
}

const SENT_FROM_SETTINGS: &[Setting] = &[
	Setting {
		key: "what",
		label: "What follows \"Sent from my\"",
		kind: SettingKind::Text { multiline: false },
		default: Fallback::Text("uname"),
	},
	Setting {
		key: "channelWhitelist",
		label: "Only in these channel ids",
		kind: SettingKind::Text { multiline: true },
		default: Fallback::Text(""),
	},
];

/// SentFromMyUname: stamp a body with where it was typed. A leading `nouname ` opts out of
/// that one message, exactly as TestCord does.
#[derive(Default)]
pub struct SentFromMyUname {
	what: Option<String>,
	channels: Vec<Id>,
}

impl crate::Plugin for SentFromMyUname {
	fn meta(&self) -> Meta {
		Meta {
			id: "SentFromMyUname",
			name: "SentFromMyUname",
			description: "Adds a \"Sent from my\" line to messages you send.",
			authors: "Testcord",
			tags: &["Utility"],
			aliases: &["sentfrommyuname"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		SENT_FROM_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.what = Some(text_or(values, SENT_FROM_SETTINGS, "what"));
		self.channels = ids(&text_or(values, SENT_FROM_SETTINGS, "channelWhitelist"));
	}

	fn before_send(&mut self, outgoing: &mut Outgoing<'_>) -> Result<(), &'static str> {
		if !self.channels.is_empty() && !self.channels.contains(&outgoing.channel) {
			return Ok(());
		}
		let body = std::mem::take(outgoing.body);
		if let Some(without) = body.strip_prefix("nouname ") {
			*outgoing.body = without.to_string();
			return Ok(());
		}
		if body.is_empty() {
			*outgoing.body = body;
			return Ok(());
		}
		let what = self.what.clone().unwrap_or_default();
		*outgoing.body = format!("{body}\n\nSent from my {what}");
		Ok(())
	}

	fn summary(&self) -> Option<String> {
		Some(
			self.what
				.clone()
				.filter(|what| !what.is_empty())
				.unwrap_or_else(|| "Nothing to stamp".to_string()),
		)
	}
}

fn ids(input: &str) -> Vec<Id> {
	use std::str::FromStr;
	let mut ids: Vec<Id> = input
		.split([' ', ',', '\n', '\t', '\r'])
		.filter_map(|part| Id::from_str(part.trim()).ok())
		.collect();
	ids.sort_unstable();
	ids.dedup();
	ids.truncate(256);
	ids
}

#[cfg(test)]
mod tests {
	use super::*;

	fn send(plugin: &mut dyn crate::Plugin, body: &str) -> String {
		let mut text = body.to_string();
		let mut outgoing = Outgoing {
			channel: Id(7),
			me: Id(1),
			body: &mut text,
			reply: None,
			previous: None,
			route: crate::Route::Send,
		};
		plugin.before_send(&mut outgoing).expect("no veto");
		outgoing.body.clone()
	}

	fn configured<T: crate::Plugin + Default>(settings: &[(&str, serde_json::Value)]) -> T {
		let mut plugin = T::default();
		plugin.configure(&Values(
			settings
				.iter()
				.map(|(key, value)| ((*key).to_string(), value.clone()))
				.collect(),
		));
		plugin
	}

	#[test]
	fn a_youtube_watch_link_becomes_its_short_form() {
		let mut plugin = configured::<EmbeddedUrls>(&[("enabled", true.into())]);
		assert_eq!(
			send(&mut plugin, "watch https://www.youtube.com/watch?v=abc123"),
			"watch https://youtu.be/watch?v=abc123"
		);
	}

	#[test]
	fn other_links_and_junk_are_untouched() {
		let mut plugin = configured::<EmbeddedUrls>(&[("enabled", true.into())]);
		assert_eq!(
			send(&mut plugin, "see https://example.com/x?y=1"),
			"see https://example.com/x?y=1"
		);
		assert_eq!(send(&mut plugin, "not a link at all"), "not a link at all");
	}

	#[test]
	fn the_rewrite_can_be_switched_off() {
		let mut plugin = configured::<EmbeddedUrls>(&[("enabled", false.into())]);
		assert_eq!(
			send(&mut plugin, "https://www.youtube.com/watch?v=abc123"),
			"https://www.youtube.com/watch?v=abc123"
		);
	}

	#[test]
	fn the_stamp_names_what_it_was_typed_on() {
		let mut plugin = configured::<SentFromMyUname>(&[("what", "a Linux box".into())]);
		assert_eq!(
			send(&mut plugin, "hello"),
			"hello\n\nSent from my a Linux box"
		);
	}

	#[test]
	fn nouname_opts_out_of_one_message() {
		let mut plugin = configured::<SentFromMyUname>(&[]);
		assert_eq!(send(&mut plugin, "nouname plain"), "plain");
		// The opt-out is not remembered for the next message.
		assert_eq!(send(&mut plugin, "again"), "again\n\nSent from my uname");
	}

	#[test]
	fn a_whitelist_keeps_the_stamp_out_of_other_channels() {
		let mut plugin = configured::<SentFromMyUname>(&[("channelWhitelist", "9".into())]);
		assert_eq!(send(&mut plugin, "hello"), "hello");
	}

	#[test]
	fn an_empty_body_is_never_stamped() {
		let mut plugin = configured::<SentFromMyUname>(&[]);
		assert_eq!(send(&mut plugin, ""), "");
	}
}
