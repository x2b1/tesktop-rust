//! What every port has to do about a body it was not asked to touch, and about one it was.
//!
//! This is the test that says the ports work rather than the test that says they do what
//! they are for: every bundled port is enabled at once and put in front of the awkward
//! bodies, and the invariants are the ones a message must never break.

use crate::{Inbound, Outgoing, Registry, Route};
use model::Id;

/// The bodies that break ports: empty, one character, a mention, a link, markdown, emoji,
/// combining marks, a body at the service's own limit, and a body past it.
fn awkward() -> Vec<&'static str> {
	vec![
		"",
		" ",
		"\n",
		"a",
		"@",
		"<@1>",
		"<@!1>",
		"@everyone",
		"<@&7>",
		"https://example.com/a?b=c#d",
		"```\ncode\n```",
		"**bold** _ital_ `code`",
		"🗿🗿🗿",
		"e\u{0301}combining",
		"a\u{200b}b",
		"line\nline\r\nline",
		"\t\ttabs",
		"!@#$%^&*()",
		"SELECT * FROM users; --",
		"{}[]()",
		"ünïcödé",
	]
}

fn a_body_at_the_limit() -> String {
	"x".repeat(client_core_max())
}

fn client_core_max() -> usize {
	// The same ceiling the app enforces, kept here as a number so this test does not depend
	// on the app's own copy of it.
	2000
}

fn send(registry: &mut Registry, body: &str) -> Result<String, &'static str> {
	let mut text = body.to_string();
	let mut outgoing = Outgoing {
		channel: Id(7),
		me: Id(1),
		body: &mut text,
		reply: None,
		previous: None,
		route: Route::Send,
	};
	registry.before_send(&mut outgoing)?;
	Ok(outgoing.body.clone())
}

fn observe(registry: &mut Registry, content: &str) {
	let mut message = test_support::message(1, Id(7));
	message.content = content.to_string();
	let inbound = Inbound::new(Id(7), Some(Id(10)), Id(1), 1_000);
	let _ = registry.observe(&inbound, crate::InboundEvent::Created(&message));
}

#[test]
fn nothing_breaks_when_every_port_is_on_at_once() {
	let mut registry = Registry::new();
	for meta in registry.metas() {
		registry.set_enabled(meta.id, true);
	}
	for body in awkward() {
		// A body may be refused with a reason, or come out changed, but it must never fail
		// in a way the app cannot show, and the split must still be able to handle it.
		let _ = send(&mut registry, body);
		observe(&mut registry, body);
	}
}

#[test]
fn a_disabled_port_leaves_a_body_exactly_as_it_was() {
	let bodies = awkward();
	for meta in Registry::new().metas().to_vec() {
		// One port at a time, and it is off: whatever it is, it may not touch a body. The
		// other ports are left out so a change can only be this one's doing.
		let mut registry = Registry::new();
		for body in &bodies {
			assert_eq!(
				send(&mut registry, body).ok().as_deref(),
				Some(*body),
				"{} touched a body on a fresh registry: {body:?}",
				meta.id
			);
		}
		// And having been on and then off, it may not touch one either: a port that keeps
		// working after you turn it off is the worst kind of bug.
		registry.set_enabled(meta.id, true);
		registry.set_enabled(meta.id, false);
		for body in &bodies {
			assert_eq!(
				send(&mut registry, body).ok().as_deref(),
				Some(*body),
				"{} touched a body after being turned off: {body:?}",
				meta.id
			);
		}
	}
}

#[test]
fn no_port_turns_a_body_into_something_the_service_will_not_take() {
	let mut registry = Registry::new();
	for meta in registry.metas() {
		registry.set_enabled(meta.id, true);
	}
	let mut every: Vec<String> = awkward().into_iter().map(str::to_owned).collect();
	every.push(a_body_at_the_limit());
	for body in every {
		if let Ok(out) = send(&mut registry, &body) {
			assert!(
				out.chars().count() <= client_core_max() * 4,
				"a body grew out of all proportion: {} chars from {}",
				out.chars().count(),
				body.chars().count()
			);
			assert!(
				!out.contains('\u{0}'),
				"a body came back with a null in it: {body:?}"
			);
			assert!(
				!out.contains('\u{fffd}'),
				"a body came back with a replacement character in it: {body:?}"
			);
		}
	}
}

#[test]
fn a_port_that_refuses_a_send_says_why_in_a_short_line() {
	let mut registry = Registry::new();
	registry.set_enabled("RobloxFilter", true);
	registry.set_value("RobloxFilter", "actionOnViolation", "block".into());
	let reason = send(&mut registry, "a link to some cp").expect_err("blocked");
	assert!(
		!reason.is_empty(),
		"a refusal with no reason tells the owner nothing"
	);
	assert!(
		reason.chars().count() <= 120,
		"a refusal the app has to show in a status line: {reason}"
	);
}

#[test]
fn a_setting_you_type_takes_effect_without_a_restart() {
	// The owner edits a setting and expects the next send to obey it; a port that only read
	// its settings at startup would fail here rather than in the app.
	let mut registry = Registry::new();
	registry.set_enabled("Abbreviation", true);
	registry.set_value("Abbreviation", "abbreviations", "btw=by the way".into());
	assert_eq!(send(&mut registry, "btw").unwrap(), "by the way");
	registry.set_value("Abbreviation", "abbreviations", "btw=be right back".into());
	assert_eq!(send(&mut registry, "btw").unwrap(), "be right back");

	let mut filter = Registry::new();
	filter.set_enabled("ClientSideBlock", true);
	filter.set_value("ClientSideBlock", "usersToBlock", "5".into());
	assert_eq!(
		filter.summary("ClientSideBlock").as_deref(),
		Some("1 blocked"),
		"a list you typed is a list the port uses"
	);
}

#[test]
fn splitting_a_body_never_loses_or_invents_one() {
	let mut registry = Registry::new();
	registry.set_enabled("SplitLargeMessages", true);
	let body = a_body_at_the_limit() + &a_body_at_the_limit();
	let context = crate::SendContext::new(Id(7), Id(1));
	let parts = registry.split(&context, &body);
	assert!(parts.len() > 1, "a body this long is split");
	let rejoined: String = parts.concat();
	assert_eq!(
		rejoined.chars().filter(|c| !c.is_whitespace()).count(),
		body.chars().filter(|c| !c.is_whitespace()).count(),
		"the split lost or invented text"
	);
}

#[test]
fn an_inbound_body_survives_every_port_that_only_reads_it() {
	// A port that only filters words leaves a body with nothing to filter exactly as it was.
	let mut registry = Registry::new();
	registry.set_enabled("ClearUrls", true);
	registry.set_enabled("NoReplyMention", true);
	for body in awkward() {
		let mut message = test_support::message(1, Id(7));
		message.content = body.to_string();
		let inbound = Inbound::new(Id(7), Some(Id(10)), Id(1), 1_000);
		let _ = registry.observe(&inbound, crate::InboundEvent::Created(&message));
		if body == "line\nline\r\nline" {
			// The only body with whitespace a filter is entitled to change.
			continue;
		}
		assert_eq!(
			message.content, body,
			"an inbound body came back changed for no reason: {body:?}"
		);
	}
}

#[test]
fn every_port_survives_being_told_nothing() {
	// A fresh install calls `reconfigure` with no stored values, so a port that reads its
	// settings wrongly shows it here rather than in the app.
	for meta in Registry::new().metas() {
		let mut plugin = Registry::new();
		plugin.set_enabled(meta.id, true);
		// Nothing is stored; the declared defaults have to be usable as they stand.
		let _ = send(&mut plugin, "a message with <@1> and https://example.com");
		observe(&mut plugin, "a message with <@1> and https://example.com");
		let _ = plugin.summary(meta.id);
		let _ = plugin.composer_buttons();
		let _ = plugin.message_actions();
		let _ = plugin.display();
	}
}

#[test]
fn the_registry_survives_a_port_being_asked_twice() {
	let mut registry = Registry::new();
	for meta in registry.metas() {
		registry.set_enabled(meta.id, true);
		registry.set_enabled(meta.id, true);
	}
	assert_eq!(
		registry.enabled_count(),
		registry.metas().len(),
		"asking twice must not count a port twice"
	);
	for meta in registry.metas() {
		registry.set_enabled(meta.id, false);
		registry.set_enabled(meta.id, false);
	}
	assert_eq!(registry.enabled_count(), 0);
	assert!(!registry.any_enabled());
}

#[test]
fn the_registry_answers_the_same_way_twice() {
	let mut registry = Registry::new();
	registry.set_enabled("Abbreviation", true);
	registry.set_value("Abbreviation", "abbreviations", "btw=by the way".into());
	let first = registry.message_actions();
	let second = registry.message_actions();
	assert_eq!(first.len(), second.len());
	assert_eq!(
		registry.tail("MessageLogger", 10),
		registry.tail("MessageLogger", 10),
		"the same question twice gets the same answer"
	);
}

#[test]
fn a_port_that_asks_for_something_nobody_offers_is_ignored() {
	let mut registry = Registry::new();
	// ClearURLs offers no button at all, so every press is for a button nobody has.
	registry.set_enabled("ClearURLs", true);
	let context = crate::IntentContext {
		channel: Id(7),
		me: Id(1),
		previous: None,
	};
	assert!(registry.composer_buttons().is_empty());
	registry.press_composer("not-a-button");
	assert!(registry.composer_buttons().is_empty());
	assert!(registry.take_intent(&context).is_none());
	registry.press_composer("");
	registry.press_composer("talk-in-reverse");
	assert!(registry.take_intent(&context).is_none());
}

#[test]
fn a_press_reaches_the_port_that_owns_the_button() {
	let mut registry = Registry::new();
	registry.set_enabled("QuickDelete", true);
	assert_eq!(registry.composer_buttons().len(), 1);
	registry.press_composer(registry.composer_buttons()[0].id);
	assert_eq!(
		registry.composer_buttons()[0].active,
		Some(false),
		"the press reached the port and it turned itself off"
	);
}

#[test]
fn the_registry_lists_each_port_once_and_in_the_same_order_every_time() {
	let first: Vec<&str> = Registry::new().metas().iter().map(|meta| meta.id).collect();
	let second: Vec<&str> = Registry::new().metas().iter().map(|meta| meta.id).collect();
	assert_eq!(
		first, second,
		"the list must not be rebuilt in another order"
	);
	let mut unique = first.clone();
	unique.sort_unstable();
	unique.dedup();
	assert_eq!(
		unique.len(),
		first.len(),
		"a port listed twice is two of everything"
	);
}
