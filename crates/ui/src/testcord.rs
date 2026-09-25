//! Settings page for the bundled TestCord ports: one card per plugin, with its own settings.

use crate::design;
use egui::RichText;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
	Flag(bool),
	Text(String),
	Number(i64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
	Toggle,
	Text { multiline: bool },
	Number { min: i64, max: i64 },
}

#[derive(Clone, Debug)]
pub struct Field {
	pub key: String,
	pub label: String,
	pub kind: Kind,
	pub value: Value,
}

#[derive(Clone, Debug)]
pub struct Entry {
	pub id: String,
	pub name: String,
	pub description: String,
	pub authors: String,
	pub tags: String,
	pub enabled: bool,
	pub summary: String,
	pub fields: Vec<Field>,
	/// Whether the plugin can hand out a record to copy.
	pub log: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Request {
	SetEnabled {
		id: String,
		enabled: bool,
	},
	SetValue {
		id: String,
		key: String,
		value: Value,
	},
	CopyLog {
		id: String,
	},
	Import,
}

#[derive(Default)]
pub struct TestCord {
	/// Refreshed by the app every frame from the live registry.
	pub entries: Vec<Entry>,
	/// Drained by the app; one request per control the owner touched.
	pub requests: Vec<Request>,
	pub notice: String,
	expanded: Option<String>,
	buffers: BTreeMap<(String, String), String>,
}

impl Entry {
	pub fn new(
		id: &str,
		name: &str,
		description: &str,
		authors: &str,
		tags: &[&str],
		enabled: bool,
	) -> Self {
		Self {
			id: id.to_string(),
			name: name.to_string(),
			description: description.to_string(),
			authors: authors.to_string(),
			tags: tags.join(", "),
			enabled,
			summary: String::new(),
			fields: Vec::new(),
			log: false,
		}
	}
}

const MAX_REQUESTS: usize = 32;

impl TestCord {
	fn request(&mut self, request: Request) {
		if self.requests.len() < MAX_REQUESTS {
			self.requests.push(request);
		}
	}

	/// The app reports what an import or an export did, so the page can say it out loud.
	pub fn report(&mut self, notice: impl Into<String>) {
		self.notice = notice.into();
	}

	pub fn show(&mut self, ui: &mut egui::Ui) {
		design::group(ui, "TestCord plugins", |ui| {
			design::hint(
				ui,
				"Ports of TestCord plugins that run natively here. Community add-ons live on the Extensions page.",
			);
			ui.add_space(6.0);
			design::row(
				ui,
				"Import TestCord settings",
				Some("Reads the plugins and settings from a TestCord settings.json file."),
				|ui| {
					if design::button(ui, "Import", design::ButtonKind::Outline).clicked() {
						self.request(Request::Import);
					}
				},
			);
			if !self.notice.is_empty() {
				design::hint(ui, &self.notice);
			}
		});
		ui.add_space(12.0);
		if self.entries.is_empty() {
			design::hint(ui, "No plugins are available in this build.");
			return;
		}
		for index in 0..self.entries.len() {
			if index > 0 {
				ui.add_space(10.0);
			}
			self.plugin(ui, index);
		}
	}

	fn plugin(&mut self, ui: &mut egui::Ui, index: usize) {
		let entry = self.entries[index].clone();
		let mut enabled = entry.enabled;
		let open = self.expanded.as_deref() == Some(entry.id.as_str());
		let colors = design::palette(ui);
		design::card(ui, |ui| {
			design::switch(
				ui,
				&entry.name,
				Some(&format!(
					"{}{}",
					entry.description,
					if entry.summary.is_empty() {
						String::new()
					} else {
						format!(" {}", entry.summary)
					}
				)),
				&mut enabled,
			);
			if enabled != entry.enabled {
				self.request(Request::SetEnabled {
					id: entry.id.clone(),
					enabled,
				});
			}
			ui.add_space(2.0);
			ui.label(
				RichText::new(format!(
					"Ported from TestCord · {} · {}",
					entry.authors, entry.tags
				))
				.size(11.5)
				.color(colors.muted),
			);
			if !entry.fields.is_empty() || entry.log {
				design::card_divider(ui);
				let response = design::disclosure(ui, "Settings", open);
				if response.clicked() {
					self.expanded = (!open).then(|| entry.id.clone());
				}
				if open {
					self.settings(ui, &entry);
				}
			}
		});
	}

	fn settings(&mut self, ui: &mut egui::Ui, entry: &Entry) {
		for field in &entry.fields {
			ui.label(design::medium(ui, field.label.clone(), 13.0));
			match (&field.kind, &field.value) {
				(Kind::Toggle, Value::Flag(value)) => {
					let mut value = *value;
					if design::switch(ui, "", None, &mut value).changed() {
						self.request(Request::SetValue {
							id: entry.id.clone(),
							key: field.key.clone(),
							value: Value::Flag(value),
						});
					}
				}
				(_, Value::Text(value)) => {
					let buffer = self
						.buffers
						.entry((entry.id.clone(), field.key.clone()))
						.or_insert_with(|| value.clone());
					let multiline = matches!(field.kind, Kind::Text { multiline: true });
					let edit = if multiline {
						egui::TextEdit::multiline(buffer).desired_rows(4)
					} else {
						egui::TextEdit::singleline(buffer)
					};
					if design::input(ui, edit.hint_text(&field.label)).changed() {
						let updated = buffer.clone();
						self.request(Request::SetValue {
							id: entry.id.clone(),
							key: field.key.clone(),
							value: Value::Text(updated),
						});
					}
				}
				(Kind::Number { min, max }, Value::Number(value)) => {
					let mut value = (*value).clamp(*min, *max);
					let response = ui.add(
						egui::Slider::new(&mut value, *min..=*max).suffix(if *max >= 1000 {
							" ms"
						} else {
							""
						}),
					);
					if response.changed() {
						self.request(Request::SetValue {
							id: entry.id.clone(),
							key: field.key.clone(),
							value: Value::Number(value),
						});
					}
				}
				_ => {}
			}
			ui.add_space(6.0);
		}
		if entry.log && design::button(ui, "Copy log", design::ButtonKind::Outline).clicked() {
			self.request(Request::CopyLog {
				id: entry.id.clone(),
			});
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn drawn(page: &mut TestCord) {
		let ctx = egui::Context::default();
		crate::design::apply(&ctx);
		let output = ctx.run_ui(egui::RawInput::default(), |ui| page.show(ui));
		output.drop_without_applying_deltas();
	}

	fn entry() -> Entry {
		Entry::new(
			"MessageLogger",
			"MessageLogger",
			"Records messages",
			"Vencord",
			&["Utility"],
			false,
		)
	}

	#[test]
	fn a_page_without_entries_says_so_instead_of_drawing_nothing() {
		let mut page = TestCord::default();
		drawn(&mut page);
		assert!(page.requests.is_empty());
	}

	#[test]
	fn a_page_with_entries_draws_without_panicking() {
		let mut page = TestCord {
			entries: vec![entry()],
			..TestCord::default()
		};
		drawn(&mut page);
		assert!(page.requests.is_empty());
	}

	#[test]
	fn requests_are_bounded() {
		let mut page = TestCord::default();
		for _ in 0..(MAX_REQUESTS * 3) {
			page.request(Request::Import);
		}
		assert_eq!(page.requests.len(), MAX_REQUESTS);
	}
}
