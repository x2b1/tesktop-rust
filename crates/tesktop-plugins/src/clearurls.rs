//! ClearURLs: strips tracking parameters from links before you send them.
//!
//! TestCord downloads ClearURLs' full rule set at startup. A native client ships its own table so
//! cleaning works offline and never blocks a send on a network round trip, so this port covers
//! the widely used providers rather than all of them.

use crate::{Meta, SendContext};
use url::Url;

struct Provider {
	hosts: &'static [&'static str],
	drop: &'static [&'static str],
	drop_prefix: &'static [&'static str],
}

const PROVIDERS: &[Provider] = &[
	Provider {
		hosts: &["youtube.com", "youtu.be", "youtube-nocookie.com"],
		drop: &["feature", "si", "pp", "ab_channel"],
		drop_prefix: &["utm_"],
	},
	Provider {
		hosts: &[
			"amazon.com",
			"amazon.de",
			"amazon.co.uk",
			"amazon.co.jp",
			"amzn.to",
		],
		drop: &[
			"ref",
			"ref_",
			"tag",
			"linkCode",
			"creative",
			"creativeASIN",
			"ascsubtag",
			"th",
			"pd_rd_i",
			"pd_rd_r",
			"pd_rd_w",
			"pd_rd_wg",
			"pf_rd_p",
			"pf_rd_r",
			"pf_rd_s",
			"pf_rd_t",
			"pf_rd_i",
			"pf_rd_m",
			"pf_rd_c",
			"psc",
			"qid",
			"sr",
			"smid",
			"crid",
			"sprefix",
			"ie",
			"_encoding",
			"language",
		],
		drop_prefix: &["pd_rd_", "pf_rd_", "ref_", "utm_"],
	},
	Provider {
		hosts: &[
			"google.com",
			"google.de",
			"google.co.uk",
			"goo.gl",
			"youtube.com",
		],
		drop: &[
			"gclid", "dclid", "gbraid", "wbraid", "ved", "usqp", "oq", "sa", "uact", "sourceid",
			"source", "ei",
		],
		drop_prefix: &["utm_"],
	},
	Provider {
		hosts: &["bing.com"],
		drop: &[
			"form", "sp", "pq", "sc", "sk", "qp", "qs", "ghc", "ghpl", "ghsh", "ghacc", "cvid",
		],
		drop_prefix: &["utm_"],
	},
	Provider {
		hosts: &[
			"facebook.com",
			"m.facebook.com",
			"web.facebook.com",
			"fb.watch",
		],
		drop: &[
			"fbclid", "ref", "refsrc", "rs", "referrer", "_rdr", "mibextid", "hc_loc",
		],
		drop_prefix: &["__cft__", "utm_"],
	},
	Provider {
		hosts: &["instagram.com"],
		drop: &["igshid", "igsh"],
		drop_prefix: &["utm_"],
	},
	Provider {
		hosts: &["x.com", "twitter.com", "t.co", "mobile.twitter.com"],
		drop: &["s", "t", "ref_src", "ref_url", "src", "mx"],
		drop_prefix: &["utm_"],
	},
	Provider {
		hosts: &["tiktok.com"],
		drop: &[
			"is_from_webapp",
			"sender_device",
			"web_id",
			"msToken",
			"_r",
			"checksum",
			"share_app_id",
			"share_link_id",
			"share_item_id",
			"sec_user_id",
			"share_app_name",
			"tt_from",
			"u_code",
			"timestamp",
			"user_id",
			"_d",
			"_t",
		],
		drop_prefix: &["utm_"],
	},
	Provider {
		hosts: &["reddit.com", "redd.it", "old.reddit.com"],
		drop: &[
			"ref",
			"ref_source",
			"ref_campaign",
			"correlation_id",
			"post_fullname",
			"rdt",
			"share_id",
			"share",
			"utm_name",
		],
		drop_prefix: &["utm_"],
	},
	Provider {
		hosts: &["store.steampowered.com", "steamcommunity.com"],
		drop: &["l", "cc", "ts", "agecheckage", "snr"],
		drop_prefix: &["snr", "utm_"],
	},
	Provider {
		hosts: &["ebay.com", "ebay.co.uk", "ebay.de"],
		drop: &[
			"mkcid",
			"mkevt",
			"mkrid",
			"campid",
			"toolid",
			"customid",
			"siteid",
			"ff",
			"hash",
			"_trkparms",
			"_trksid",
			"amdata",
		],
		drop_prefix: &["utm_"],
	},
	Provider {
		hosts: &["microsoft.com", "bing.com", "msn.com", "xbox.com"],
		drop: &["cvid", "ocid"],
		drop_prefix: &["wt.mc_id", "utm_"],
	},
	Provider {
		hosts: &["epicgames.com"],
		drop: &["lang", "locale", "country", "appToken", "redirect"],
		drop_prefix: &["utm_", "lang_"],
	},
];

/// Applies everywhere, the way the ClearURLs rules for common ad parameters do.
const GENERIC_DROP: &[&str] = &[
	"utm_source",
	"utm_medium",
	"utm_campaign",
	"utm_term",
	"utm_content",
	"utm_name",
	"utm",
	"fbclid",
	"gclid",
	"msclkid",
	"dclid",
	"igshid",
	"mc_cid",
	"mc_eid",
	"yclid",
	"_openstat",
];
const GENERIC_DROP_PREFIX: &[&str] = &["utm_"];

pub struct ClearUrls;

impl crate::Plugin for ClearUrls {
	fn meta(&self) -> Meta {
		Meta {
			id: "ClearURLs",
			name: "ClearURLs",
			description: "Removes tracking parameters from links before you send them.",
			authors: "adryd, thororen",
			tags: &["Privacy", "Utility"],
			aliases: &["clearurls", "ClearUrls"],
			default_enabled: false,
		}
	}

	fn before_send(
		&mut self,
		_context: &SendContext,
		content: &mut String,
	) -> Result<(), &'static str> {
		*content = clean(content);
		Ok(())
	}

	fn before_edit(
		&mut self,
		_context: &SendContext,
		content: &mut String,
	) -> Result<(), &'static str> {
		*content = clean(content);
		Ok(())
	}

	fn summary(&self) -> Option<String> {
		Some(format!("{} providers offline", PROVIDERS.len()))
	}
}

fn host_matches(host: &str, candidate: &str) -> bool {
	host == candidate || host.ends_with(&format!(".{candidate}"))
}

fn clean_url(candidate: &str) -> Option<String> {
	let mut url = Url::parse(candidate).ok()?;
	if url.cannot_be_a_base() {
		return None;
	}
	let host = url.host_str()?.to_lowercase();
	let mut doomed = Vec::new();
	{
		let params: Vec<String> = url.query_pairs().map(|(key, _)| key.into_owned()).collect();
		for param in params {
			let generic = GENERIC_DROP.contains(&param.as_str())
				|| GENERIC_DROP_PREFIX
					.iter()
					.any(|prefix| param.starts_with(prefix));
			let provider = PROVIDERS.iter().any(|provider| {
				provider
					.hosts
					.iter()
					.any(|candidate| host_matches(&host, candidate))
					&& (provider.drop.contains(&param.as_str())
						|| provider
							.drop_prefix
							.iter()
							.any(|prefix| param.starts_with(prefix)))
			});
			if generic || provider {
				doomed.push(param);
			}
		}
	}
	if doomed.is_empty() {
		return None;
	}
	let kept: Vec<(String, String)> = url
		.query_pairs()
		.filter(|(key, _)| !doomed.iter().any(|param| param == key))
		.map(|(key, value)| (key.into_owned(), value.into_owned()))
		.collect();
	url.set_query(None);
	if !kept.is_empty() {
		url.query_pairs_mut().extend_pairs(kept);
	}
	Some(url.to_string())
}

/// Rewrite every link in `text`, leaving the rest of the message untouched.
pub fn clean(text: &str) -> String {
	if !text.contains("http") {
		return text.to_string();
	}
	let mut out = String::with_capacity(text.len());
	let mut rest = text;
	while let Some(start) = rest.find("http") {
		let (before, tail) = rest.split_at(start);
		let end = tail
			.find(|character: char| {
				character.is_whitespace() || matches!(character, '<' | '>' | '"' | '\'' | '`' | '|')
			})
			.unwrap_or(tail.len());
		let (candidate, after) = tail.split_at(end);
		let trimmed = candidate.trim_end_matches(['.', ',', ';', ':', '!', '?']);
		let punctuation = &candidate[trimmed.len()..];
		let (unbalanced, tail_paren) = match trimmed.strip_suffix(')') {
			Some(head) if !head.contains('(') => (head, true),
			_ => (trimmed, false),
		};
		out.push_str(before);
		match clean_url(unbalanced) {
			Some(cleaned) => out.push_str(&cleaned),
			None => out.push_str(unbalanced),
		}
		if tail_paren {
			out.push(')');
		}
		out.push_str(punctuation);
		rest = after;
	}
	out.push_str(rest);
	out
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn common_tracking_parameters_are_removed() {
		assert_eq!(
			clean("look https://www.youtube.com/watch?v=abc123&feature=share&t=30"),
			"look https://www.youtube.com/watch?v=abc123&t=30"
		);
		assert_eq!(
			clean("https://www.amazon.com/dp/B000000000?psc=1&th=1&linkCode=ogi"),
			"https://www.amazon.com/dp/B000000000"
		);
		assert_eq!(
			clean("https://example.com/x?utm_source=news&keep=1"),
			"https://example.com/x?keep=1"
		);
	}

	#[test]
	fn provider_scopes_are_respected() {
		assert_eq!(
			clean("https://twitter.com/x/status/1?s=20&t=abc"),
			"https://twitter.com/x/status/1"
		);
		// `feature` is a YouTube rule and must survive on another host.
		assert_eq!(
			clean("https://example.com/watch?feature=share&v=2"),
			"https://example.com/watch?feature=share&v=2"
		);
	}

	#[test]
	fn several_links_and_punctuation_survive() {
		assert_eq!(
			clean("a https://a.example/?utm_medium=x b https://b.example/?utm_medium=y."),
			"a https://a.example/ b https://b.example/."
		);
		assert_eq!(
			clean("markdown [link](https://c.example/?fbclid=1)"),
			"markdown [link](https://c.example/)"
		);
	}

	#[test]
	fn text_without_links_is_untouched() {
		let text = "http is a scheme, not a link";
		assert_eq!(clean(text), text);
	}
}
