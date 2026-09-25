//! A reaction you can put on a message from the message menu, and the abbreviations you
//! would rather not type.

use crate::{
	ActionResult, Fallback, Intent, IntentContext, Message, Meta, Setting, SettingKind, Values,
	flag_or, text_or,
};
use model::Id;
use std::str::FromStr;

/// Reactions offered at once, which is all a popover can hold.
pub const MAX_BUTTONS: usize = 8;

const BUTTON_SETTINGS: &[Setting] = &[Setting {
	key: "buttons",
	label: "One per line: label=emoji",
	kind: SettingKind::Text { multiline: true },
	default: Fallback::Text("Like=👍"),
}];

/// One reaction button: what it is called and what it puts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Button {
	pub label: String,
	pub emoji: model::ReactionEmoji,
}

/// Read a reaction the way the original does: a plain emoji, a custom one written
/// `<:name:id>` or `<a:name:id>`, or the same without the brackets.
pub fn parse_emoji(raw: &str) -> Option<model::ReactionEmoji> {
	let trimmed = raw.trim();
	if trimmed.is_empty() || trimmed.chars().count() > 128 {
		return None;
	}
	let body = trimmed
		.strip_prefix('<')
		.and_then(|rest| rest.strip_suffix('>'))
		.unwrap_or(trimmed);
	// A custom emoji is `name:id`, where an animated one keeps its `a:` on the name.
	let Some((name, id)) = body.rsplit_once(':') else {
		// A plain emoji is a name with no id, as long as it is not a colon pair.
		return (!body.contains(':')).then(|| model::ReactionEmoji {
			id: None,
			name: Some(body.to_string()),
		});
	};
	// `:wave:123` wraps the name in colons of its own, which are not part of the name.
	let name = name.trim_matches(':');
	if name.is_empty() {
		return None;
	}
	let id = Id::from_str(id).ok()?;
	Some(model::ReactionEmoji {
		id: Some(id),
		name: Some(name.to_string()),
	})
}

/// Read the buttons, one per line, skipping a line that does not make sense.
pub fn parse_buttons(text: &str) -> Vec<Button> {
	let mut buttons: Vec<Button> = Vec::new();
	for line in text.lines() {
		let line = line.trim();
		if line.is_empty() || line.starts_with('#') {
			continue;
		}
		let Some((label, emoji)) = line.split_once('=') else {
			continue;
		};
		let label: String = label.trim().chars().take(32).collect();
		let Some(emoji) = parse_emoji(emoji) else {
			continue;
		};
		if label.is_empty() {
			continue;
		}
		buttons.push(Button { label, emoji });
		if buttons.len() >= MAX_BUTTONS {
			break;
		}
	}
	buttons
}

/// CustomReactionButtons: your own reactions on the message menu.
#[derive(Default)]
pub struct CustomReactionButtons {
	buttons: Vec<Button>,
	pending: std::cell::RefCell<Option<Intent>>,
}

impl crate::Plugin for CustomReactionButtons {
	fn meta(&self) -> Meta {
		Meta {
			id: "CustomReactionButtons",
			name: "CustomReactionButtons",
			description: "Puts your own reactions on the message menu.",
			authors: "x2b",
			tags: &["Chat", "Utility", "Reactions"],
			aliases: &["customReactionButtons"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		BUTTON_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.buttons = parse_buttons(&text_or(values, BUTTON_SETTINGS, "buttons"));
		self.pending.borrow_mut().take();
	}

	fn reset(&mut self) {
		self.pending.borrow_mut().take();
	}

	fn message_actions(&self) -> Vec<crate::MessageAction> {
		// One entry per button the owner wrote, which is why the list is owned: the labels
		// are theirs, not literals here. The action is the button's own position, so a
		// rename in the list cannot aim at the wrong reaction.
		self.buttons
			.iter()
			.enumerate()
			.map(|(index, button)| crate::MessageAction {
				id: Box::leak(action_id(index).into_boxed_str()),
				label: Box::leak(button.label.clone().into_boxed_str()),
			})
			.collect()
	}

	fn run_action(&self, action: &str, message: &Message) -> Option<ActionResult> {
		let index = action.strip_prefix(REACT_PREFIX)?.parse::<usize>().ok()?;
		let button = self.buttons.get(index)?;
		if !button.emoji.valid() {
			return Some(ActionResult::Notice(
				"That reaction is not one the service accepts".to_string(),
			));
		}
		*self.pending.borrow_mut() = Some(Intent::React {
			channel: message.channel,
			message: message.id,
			emoji: format_emoji(&button.emoji),
			add: true,
		});
		Some(ActionResult::Notice(format!(
			"{} is on its way",
			button.label
		)))
	}

	fn take_intent(&mut self, _context: &IntentContext<'_>) -> Option<Intent> {
		self.pending.borrow_mut().take()
	}

	fn summary(&self) -> Option<String> {
		(!self.buttons.is_empty()).then(|| format!("{} reactions on the menu", self.buttons.len()))
	}
}

/// The action a button runs, which is its own position in the list.
const REACT_PREFIX: &str = "react:";

fn action_id(index: usize) -> String {
	format!("{REACT_PREFIX}{index}")
}

/// The reaction as the service is asked for it, which is what an intent carries.
fn format_emoji(emoji: &model::ReactionEmoji) -> String {
	match (emoji.id, emoji.name.as_deref()) {
		(Some(id), Some(name)) => format!("<:{name}:{id}>"),
		(_, Some(name)) => name.to_string(),
		_ => String::new(),
	}
}

const ABBREVIATION_SETTINGS: &[Setting] = &[
	Setting {
		key: "abbreviations",
		label: "Abbreviations, as abbrev=full text separated by |",
		kind: SettingKind::Text { multiline: true },
		default: Fallback::Text(DEFAULT_ABBREVIATIONS),
	},
	Setting {
		key: "customAbbreviations",
		label: "Your own, in the same form",
		kind: SettingKind::Text { multiline: true },
		default: Fallback::Text(""),
	},
	Setting {
		key: "caseSensitive",
		label: "Only expand an abbreviation typed in the same case",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
	Setting {
		key: "enabled",
		label: "Expand what you send",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(true),
	},
];

/// The list the original ships, copied as it stands.
const DEFAULT_ABBREVIATIONS: &str = "btw=by the way|omg=oh my god|brb=be right back|afk=away from keyboard|imo=in my opinion|tbh=to be honest|lol=laughing out loud|wtf=what the f*ck|nvm=never mind|thx=thanks|pls=please|u=you|ur=your|bc=because|rn=right now|irl=in real life|fyi=for your information|asap=as soon as possible|ttyl=talk to you later|gtg=got to go|idk=I don't know|ikr=I know right|smh=shaking my head|dm=direct message|gm=good morning|gn=good night|gl=good luck|hf=have fun|wp=well played|gg=good game|ez=easy|op=overpowered|nerf=reduce power|buff=increase power|meta=most effective tactics available";

/// Abbreviations kept, so a long list cannot cost a scan per send.
pub const MAX_ABBREVIATIONS: usize = 256;

/// Parse a list, which is `abbrev=full text` separated by `|`. Yours win over theirs.
pub fn parse_abbreviations(
	text: &str,
	case_sensitive: bool,
) -> std::collections::BTreeMap<String, String> {
	let mut map = std::collections::BTreeMap::new();
	for pair in text.split('|') {
		let Some((abbrev, expansion)) = pair.split_once('=') else {
			continue;
		};
		let abbrev = abbrev.trim();
		let expansion = expansion.trim();
		if abbrev.is_empty() || expansion.is_empty() {
			continue;
		}
		if abbrev.chars().count() > 32 || expansion.chars().count() > 128 {
			continue;
		}
		let key = if case_sensitive {
			abbrev.to_string()
		} else {
			abbrev.to_lowercase()
		};
		map.insert(key, expansion.to_string());
		if map.len() >= MAX_ABBREVIATIONS {
			break;
		}
	}
	map
}

/// Abbreviation: the short forms you type, written out in full.
pub struct Abbreviation {
	abbreviations: std::collections::BTreeMap<String, String>,
	case_sensitive: bool,
	enabled: bool,
	pending: std::cell::RefCell<Option<String>>,
}

impl Default for Abbreviation {
	fn default() -> Self {
		Self {
			abbreviations: std::collections::BTreeMap::new(),
			case_sensitive: false,
			enabled: true,
			pending: std::cell::RefCell::new(None),
		}
	}
}

impl Abbreviation {
	/// Expand a body, keeping the punctuation that was around each word.
	pub fn expand(&self, body: &str) -> String {
		if !self.enabled || self.abbreviations.is_empty() {
			return body.to_string();
		}
		let mut out = String::with_capacity(body.len());
		let mut expanded = 0;
		// Split on the runs of whitespace, keeping them, the way the original does, so the
		// spacing of what you typed survives.
		let mut word = String::new();
		for character in body.chars() {
			if character.is_whitespace() {
				self.push(&mut out, &mut word, &mut expanded);
				out.push(character);
				continue;
			}
			word.push(character);
		}
		self.push(&mut out, &mut word, &mut expanded);
		if expanded > 0 {
			*self.pending.borrow_mut() = Some(format!("Expanded {expanded} abbreviation(s)"));
		}
		out
	}

	fn push(&self, out: &mut String, word: &mut String, expanded: &mut usize) {
		if word.is_empty() {
			return;
		}
		// The word without its punctuation is what is looked up, and the punctuation keeps
		// its side of it, so "(btw)" becomes "(by the way)" and "btw!" becomes "by the way!".
		let is_word = |character: &char| character.is_alphanumeric() || *character == '_';
		let leading: String = word.chars().take_while(|c| !is_word(c)).collect();
		let trailing: String = word.chars().rev().take_while(|c| !is_word(c)).collect();
		let trailing: String = trailing.chars().rev().collect();
		let bare: String = word.chars().filter(is_word).collect();
		if bare.is_empty() {
			out.push_str(word);
			word.clear();
			return;
		}
		let key = if self.case_sensitive {
			bare
		} else {
			bare.to_lowercase()
		};
		match self.abbreviations.get(&key) {
			Some(expansion) => {
				out.push_str(&leading);
				out.push_str(expansion);
				out.push_str(&trailing);
				*expanded += 1;
			}
			None => out.push_str(word),
		}
		word.clear();
	}
}

impl crate::Plugin for Abbreviation {
	fn meta(&self) -> Meta {
		Meta {
			id: "Abbreviation",
			name: "Abbreviation",
			description: "Writes out the short forms you type before you send them.",
			authors: "Vencord",
			tags: &["Chat", "Utility"],
			aliases: &["abbreviation", "abreviation"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		ABBREVIATION_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.case_sensitive = flag_or(values, ABBREVIATION_SETTINGS, "caseSensitive");
		self.enabled = flag_or(values, ABBREVIATION_SETTINGS, "enabled");
		// Yours are read second so they win, which is what the original does.
		let mut map = parse_abbreviations(
			&text_or(values, ABBREVIATION_SETTINGS, "abbreviations"),
			self.case_sensitive,
		);
		for (key, expansion) in parse_abbreviations(
			&text_or(values, ABBREVIATION_SETTINGS, "customAbbreviations"),
			self.case_sensitive,
		) {
			map.insert(key, expansion);
		}
		self.abbreviations = map;
		self.pending.borrow_mut().take();
	}

	fn before_send(&mut self, outgoing: &mut crate::Outgoing<'_>) -> Result<(), &'static str> {
		let body = std::mem::take(outgoing.body);
		*outgoing.body = self.expand(&body);
		Ok(())
	}

	fn take_toast(&mut self) -> Option<String> {
		self.pending.borrow_mut().take()
	}

	fn summary(&self) -> Option<String> {
		(!self.abbreviations.is_empty())
			.then(|| format!("{} abbreviations", self.abbreviations.len()))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{Plugin, Registry, Verdict};

	fn message(author: u64) -> Message {
		let mut message = test_support::message(5, model::Id(7));
		message.author.id = model::Id(author);
		message
	}

	#[test]
	fn a_plain_emoji_is_read_as_a_name() {
		assert_eq!(
			parse_emoji("👍"),
			Some(model::ReactionEmoji {
				id: None,
				name: Some("👍".to_string())
			})
		);
	}

	#[test]
	fn a_custom_emoji_keeps_its_id_and_its_animation() {
		assert_eq!(
			parse_emoji("<a:party:123456789>"),
			Some(model::ReactionEmoji {
				id: Some(model::Id(123456789)),
				name: Some("a:party".to_string())
			})
		);
		assert_eq!(
			parse_emoji(":wave:123456789"),
			Some(model::ReactionEmoji {
				id: Some(model::Id(123456789)),
				name: Some("wave".to_string())
			})
		);
	}

	#[test]
	fn an_emoji_that_is_not_one_is_refused() {
		assert_eq!(parse_emoji(""), None);
		assert_eq!(parse_emoji("   "), None);
		assert_eq!(parse_emoji(":"), None);
		assert_eq!(parse_emoji("<:name:notanid>"), None);
		assert_eq!(parse_emoji("a:b:c"), None, "two colons is not a reaction");
	}

	#[test]
	fn the_buttons_are_read_a_line_at_a_time() {
		let buttons = parse_buttons("# a comment\nLike=👍\nParty=<a:party:123>\nbroken\n");
		assert_eq!(buttons.len(), 2);
		assert_eq!(buttons[0].label, "Like");
		assert_eq!(buttons[1].emoji.id, Some(model::Id(123)));
	}

	#[test]
	fn a_pressed_button_reacts_once() {
		let mut plugin = CustomReactionButtons::default();
		plugin.configure(&Values(
			[("buttons".to_string(), serde_json::json!("Like=👍"))]
				.into_iter()
				.collect(),
		));
		let offered = plugin.message_actions();
		assert_eq!(offered.len(), 1);
		assert_eq!(offered[0].id, "react:0");
		assert_eq!(offered[0].label, "Like");
		assert!(
			plugin
				.run_action("react:0", &message(2))
				.is_some_and(|result| matches!(result, ActionResult::Notice(_)))
		);
		let context = IntentContext {
			channel: model::Id(7),
			me: model::Id(1),
			previous: None,
		};
		assert_eq!(
			plugin.take_intent(&context),
			Some(Intent::React {
				channel: model::Id(7),
				message: model::Id(5),
				emoji: "👍".to_string(),
				add: true,
			})
		);
		assert!(plugin.take_intent(&context).is_none());
	}

	#[test]
	fn a_button_that_is_not_there_reacts_to_nothing() {
		let plugin = CustomReactionButtons::default();
		assert!(plugin.run_action("react:0", &message(2)).is_none());
		assert!(plugin.run_action("other", &message(2)).is_none());
	}

	#[test]
	fn the_registry_offers_the_buttons_in_the_message_menu() {
		let mut registry = Registry::new();
		registry.set_enabled("CustomReactionButtons", true);
		let actions = registry.message_actions();
		assert_eq!(actions.len(), 1);
		assert_eq!(actions[0].0, "CustomReactionButtons");
		assert_eq!(actions[0].1.id, "react:0");
	}

	#[test]
	fn an_abbreviation_is_written_out() {
		let mut plugin = Abbreviation::default();
		plugin.configure(&Values(
			[(
				"abbreviations".to_string(),
				serde_json::json!("btw=by the way"),
			)]
			.into_iter()
			.collect(),
		));
		assert_eq!(plugin.expand("btw, i am late"), "by the way, i am late");
	}

	#[test]
	fn the_punctuation_around_a_word_survives() {
		let mut plugin = Abbreviation::default();
		plugin.configure(&Values(
			[(
				"abbreviations".to_string(),
				serde_json::json!("omg=oh my god"),
			)]
			.into_iter()
			.collect(),
		));
		assert_eq!(plugin.expand("omg!"), "oh my god!");
		assert_eq!(plugin.expand("(omg)"), "(oh my god)");
	}

	#[test]
	fn case_can_be_respected() {
		let mut plugin = Abbreviation::default();
		plugin.configure(&Values(
			[
				(
					"abbreviations".to_string(),
					serde_json::json!("omg=oh my god"),
				),
				("caseSensitive".to_string(), serde_json::json!(true)),
			]
			.into_iter()
			.collect(),
		));
		assert_eq!(plugin.expand("omg"), "oh my god");
		assert_eq!(plugin.expand("OMG"), "OMG");
	}

	#[test]
	fn a_word_that_is_not_an_abbreviation_is_left_alone() {
		let mut plugin = Abbreviation::default();
		plugin.configure(&Values(
			[("abbreviations".to_string(), serde_json::json!("u=you"))]
				.into_iter()
				.collect(),
		));
		assert_eq!(plugin.expand("us and them"), "us and them");
	}

	#[test]
	fn your_own_abbreviations_win() {
		let mut plugin = Abbreviation::default();
		plugin.configure(&Values(
			[
				("abbreviations".to_string(), serde_json::json!("u=you")),
				(
					"customAbbreviations".to_string(),
					serde_json::json!("u=uwu"),
				),
			]
			.into_iter()
			.collect(),
		));
		assert_eq!(plugin.expand("u"), "uwu");
	}

	#[test]
	fn the_expansion_says_how_many_it_did() {
		let mut plugin = Abbreviation::default();
		plugin.configure(&Values(
			[(
				"abbreviations".to_string(),
				serde_json::json!("btw=by the way"),
			)]
			.into_iter()
			.collect(),
		));
		assert_eq!(plugin.expand("btw"), "by the way");
		assert_eq!(
			plugin.take_toast().as_deref(),
			Some("Expanded 1 abbreviation(s)")
		);
		assert!(plugin.take_toast().is_none());
	}

	#[test]
	fn nothing_expanded_means_nothing_said() {
		let mut plugin = Abbreviation::default();
		plugin.configure(&Values(
			[(
				"abbreviations".to_string(),
				serde_json::json!("btw=by the way"),
			)]
			.into_iter()
			.collect(),
		));
		plugin.expand("hello");
		assert!(plugin.take_toast().is_none());
	}

	#[test]
	fn the_send_path_uses_it() {
		let mut registry = Registry::new();
		registry.set_enabled("Abbreviation", true);
		let mut text = "brb".to_string();
		let mut outgoing = crate::Outgoing {
			channel: model::Id(7),
			me: model::Id(1),
			body: &mut text,
			reply: None,
			previous: None,
			route: crate::Route::Send,
		};
		registry.before_send(&mut outgoing).expect("no veto");
		assert_eq!(outgoing.body, "be right back");
		let _ = Verdict::Show;
	}
}
