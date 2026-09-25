//! Render-time body ports: what a message looks like without changing what it says.

use crate::Meta;

/// A body rewrite applied while a message is formatted, never to the stored message, so
/// copying a message still yields what its author wrote.
pub type BodyTransform = fn(&str) -> String;

#[derive(Default)]
pub struct Unindent;

impl crate::Plugin for Unindent {
	fn meta(&self) -> Meta {
		Meta {
			id: "Unindent",
			name: "Unindent",
			description: "Trims the shared leading indentation from a code block.",
			authors: "Ven",
			tags: &["Chat", "Utility"],
			aliases: &["unindent"],
			default_enabled: false,
		}
	}

	fn body_transform(&self) -> Option<BodyTransform> {
		Some(unindent)
	}

	fn summary(&self) -> Option<String> {
		Some("Code blocks lose their shared indent".to_string())
	}
}

/// Discord turns a typed tab into spaces, but a bot can send a real one, so tabs become
/// four spaces first. The shortest indent across the block is then removed from every line,
/// which is what the fenced block wanted all along.
pub fn unindent(body: &str) -> String {
	let expanded = body.replace('\t', "    ");
	// Only lines with content count, exactly like the pattern TestCord matches; a body whose
	// least-indented line starts at column zero keeps its shape.
	let indent = expanded
		.lines()
		.filter_map(|line| {
			let content = line.trim_start_matches(' ');
			(!content.is_empty()).then(|| line.len() - content.len())
		})
		.min()
		.unwrap_or_default();
	if indent == 0 {
		return expanded;
	}
	expanded
		.lines()
		.map(|line| line.strip_prefix(&" ".repeat(indent)).unwrap_or(line))
		.collect::<Vec<_>>()
		.join("\n")
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_shared_indent_goes_and_the_relative_one_stays() {
		let body = "    fn main() {\n        println!(\"hi\");\n    }";
		assert_eq!(
			unindent(body),
			"fn main() {\n    println!(\"hi\");\n}"
		);
	}

	#[test]
	fn tabs_become_four_spaces_first() {
		// One indented line is entirely the shared indent, so it goes too, as in TestCord.
		assert_eq!(unindent("\tcode"), "code");
		assert_eq!(unindent("\t\tcode"), "code");
		assert_eq!(unindent("\tcode\n\tother"), "code\nother");
	}

	#[test]
	fn a_body_with_no_shared_indent_is_untouched() {
		let body = "one\n  two\nthree";
		assert_eq!(unindent(body), body);
	}

	#[test]
	fn blank_lines_do_not_count_towards_the_smallest_indent() {
		let body = "    a\n\n    b";
		assert_eq!(unindent(body), "a\n\nb");
	}

	#[test]
	fn the_transform_is_offered_only_while_enabled() {
		let mut registry = crate::Registry::new();
		assert!(registry.body_transform().is_none());
		registry.set_enabled("Unindent", true);
		assert_eq!(registry.body_transform().map(|(id, _)| id), Some("Unindent"));
		registry.set_enabled("Unindent", false);
		assert!(registry.body_transform().is_none());
	}

	#[test]
	fn the_transform_leaves_the_stored_message_alone() {
		let mut registry = crate::Registry::new();
		registry.set_enabled("Unindent", true);
		let mut message = test_support::message(1, model::Id(7));
		message.content = "    indented".into();
		let inbound = crate::Inbound::new(model::Id(7), None, model::Id(1), 0);
		registry.observe(&inbound, crate::InboundEvent::Created(&message));
		assert_eq!(message.content, "    indented");
		let (_, transform) = registry.body_transform().expect("enabled");
		assert_eq!(transform(message.content.as_str()), "indented");
	}

	#[test]
	fn the_plugin_declares_no_settings() {
		assert!(crate::Plugin::settings(&Unindent).is_empty());
	}
}
