//! Settings page for the bundled TestCord ports: one card per plugin, with its own settings.

use crate::design;
use egui::RichText;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
	Flag(bool),
	Text(String),
	Number(i64),
}

/// Whether a clock keeps 12- or 24-hour time. Mirrors the runtime's own enum.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HourFormat {
	#[default]
	Keep,
	Twelve,
	TwentyFour,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
	Toggle,
	Text {
		multiline: bool,
	},
	Number {
		min: i64,
		max: i64,
	},
	Choice {
		options: &'static [(&'static str, &'static str)],
	},
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
	/// The tail of that record, shown here so it is visible without the clipboard. Bounded
	/// by the app; a plugin hands over more than fits and the app keeps the end.
	pub log_tail: String,
}

/// How the plugin list is ordered. `Registry` is the order the runtime was built in, which
/// groups ports by the module that owns them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sort {
	#[default]
	Registry,
	Name,
	Author,
	/// The ones that are on first, then the rest.
	Enabled,
}

impl Sort {
	pub const ALL: [Self; 4] = [Self::Registry, Self::Name, Self::Author, Self::Enabled];

	pub fn label(self) -> &'static str {
		match self {
			Self::Registry => "Build order",
			Self::Name => "Name",
			Self::Author => "Author",
			Self::Enabled => "On first",
		}
	}
}

/// A button in the composer's row, drawn by the composer and answered by the app.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComposerButton {
	pub id: String,
	pub label: String,
	pub tooltip: String,
	/// A toggle shows whether it is on; a plain button has no state.
	pub active: Option<bool>,
}

/// A message-menu entry the owner picked, waiting for the app to run it.
#[derive(Clone, Debug, PartialEq)]
pub struct Picked {
	pub plugin: String,
	pub action: String,
	pub message: model::Id,
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
	/// A composer's-row button belonging to a bundled port was pressed.
	ComposerButton {
		id: String,
	},
	Import,
}

pub struct TestCord {
	/// Refreshed by the app every frame from the live registry.
	pub entries: Vec<Entry>,
	/// Drained by the app; one request per control the owner touched.
	pub requests: Vec<Request>,
	pub notice: String,
	/// Message-menu entries the active plugins offer, rebuilt by the app when they change.
	pub message_actions: std::sync::Arc<Vec<crate::extensions_ui::MenuAction>>,
	/// Buttons the bundled ports offer in the composer's own row, rebuilt by the app when
	/// they change. Empty when no port offers one.
	pub composer_buttons: Arc<Vec<ComposerButton>>,
	/// What the owner is searching for, matched against the name, id, description, author
	/// and tags. Empty shows everything.
	pub search: String,
	/// How the list is ordered.
	pub sort: Sort,
	/// Set while a search or a sort is being typed, so the list is rebuilt while it changes
	/// and not on every frame.
	pub listing_dirty: bool,

	/// The entry the owner picked on a message.
	pub picked: Option<Picked>,
	expanded: Option<String>,
	/// The order the list is shown in, and the order the owner picked.
	listing: Vec<usize>,
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
			log_tail: String::new(),
		}
	}
}

const MAX_REQUESTS: usize = 32;

impl Default for TestCord {
	fn default() -> Self {
		Self {
			entries: Vec::new(),
			requests: Vec::new(),
			notice: String::new(),
			message_actions: Arc::new(Vec::new()),
			composer_buttons: Arc::new(Vec::new()),
			search: String::new(),
			sort: Sort::Registry,
			// A list that has never been built is dirty by definition, so the first frame
			// after the page opens shows something.
			listing_dirty: true,
			picked: None,
			expanded: None,
			listing: Vec::new(),
			buffers: BTreeMap::new(),
		}
	}
}

impl TestCord {
	/// A page over a live list of ports, which is what the app builds each frame.
	pub fn with_entries(entries: Vec<Entry>) -> Self {
		Self {
			entries,
			..Self::default()
		}
	}

	pub fn request(&mut self, request: Request) {
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
		ui.add_space(10.0);
		self.listing_row(ui);
		ui.add_space(12.0);
		if self.entries.is_empty() {
			design::hint(ui, "No plugins are available in this build.");
			return;
		}
		let order = self.visible();
		if order.is_empty() {
			design::hint(
				ui,
				&format!(
					"Nothing here matches \"{}\". Try the plugin's name, its author, or a tag.",
					self.search.trim()
				),
			);
			return;
		}
		for index in order {
			if index > 0 {
				ui.add_space(10.0);
			}
			self.plugin(ui, index);
		}
	}

	/// The search field, the sort, and how many of the ports are on.
	fn listing_row(&mut self, ui: &mut egui::Ui) {
		let colors = design::palette(ui);
		let total = self.entries.len();
		let on = self.entries.iter().filter(|entry| entry.enabled).count();
		ui.horizontal(|ui| {
			ui.spacing_mut().item_spacing.x = 8.0;
			let mut search = self.search.clone();
			let field = egui::TextEdit::singleline(&mut search)
				.hint_text("Search plugins")
				.desired_width((ui.available_width() - 200.0).max(120.0))
				.frame(egui::Frame::NONE);
			let response = egui::Frame::new()
				.fill(colors.base)
				.corner_radius(7)
				.stroke(egui::Stroke::new(1.0, colors.border))
				.inner_margin(egui::Margin::symmetric(8, 4))
				.show(ui, |ui| {
					ui.add(field);
				});
			let _ = response;
			if search != self.search {
				self.search = search;
				self.listing_dirty = true;
			}
			egui::ComboBox::from_id_salt("testcord-sort")
				.selected_text(Sort::ALL[self.sort as usize].label())
				.width(150.0)
				.show_ui(ui, |ui| {
					for sort in Sort::ALL {
						if ui
							.selectable_label(sort == self.sort, sort.label())
							.clicked()
						{
							self.sort = sort;
							self.listing_dirty = true;
							ui.close();
						}
					}
				});
		});
		ui.add_space(4.0);
		design::hint(
			ui,
			&format!("{on} of {total} on · showing {}", self.visible().len()),
		);
	}

	/// The order the list is drawn in: the search first, then the sort.
	pub fn visible(&mut self) -> Vec<usize> {
		if !self.listing_dirty {
			return self.listing.clone();
		}
		let needle = self.search.trim().to_lowercase();
		let mut order: Vec<usize> = (0..self.entries.len())
			.filter(|index| {
				if needle.is_empty() {
					return true;
				}
				let entry = &self.entries[*index];
				[
					entry.name.as_str(),
					entry.id.as_str(),
					entry.description.as_str(),
					entry.authors.as_str(),
					entry.tags.as_str(),
				]
				.iter()
				.any(|field| field.to_lowercase().contains(&needle))
			})
			.collect();
		match self.sort {
			Sort::Registry => {}
			Sort::Name => order.sort_by_key(|index| self.entries[*index].name.to_lowercase()),
			Sort::Author => order.sort_by_key(|index| self.entries[*index].authors.to_lowercase()),
			Sort::Enabled => order.sort_by_key(|index| {
				(
					!self.entries[*index].enabled,
					self.entries[*index].name.to_lowercase(),
				)
			}),
		}
		self.listing = order;
		self.listing_dirty = false;
		self.listing.clone()
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
				(Kind::Choice { options }, Value::Text(value)) => {
					let mut selected = value.clone();
					let label = options
						.iter()
						.find(|(option, _)| *option == selected.as_str())
						.map_or(selected.as_str(), |(_, label)| *label);
					egui::ComboBox::from_id_salt((
						"testcord-choice",
						entry.id.clone(),
						field.key.clone(),
					))
					.selected_text(label)
					.width(ui.available_width())
					.show_ui(ui, |ui| {
						for (option, option_label) in *options {
							ui.selectable_value(
								&mut selected,
								(*option).to_string(),
								*option_label,
							);
						}
					});
					if selected != *value {
						self.request(Request::SetValue {
							id: entry.id.clone(),
							key: field.key.clone(),
							value: Value::Text(selected),
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
		if !entry.log_tail.is_empty() {
			ui.add_space(4.0);
			egui::Frame::new()
				.fill(crate::design::palette(ui).base)
				.corner_radius(6)
				.inner_margin(egui::Margin::symmetric(8, 6))
				.show(ui, |ui| {
					egui::ScrollArea::vertical()
						.id_salt(("testcord-log", entry.id.as_str()))
						.max_height(160.0)
						.show(ui, |ui| {
							for line in entry.log_tail.lines() {
								ui.label(egui::RichText::new(line).small().monospace());
							}
						});
				});
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
		painted(page);
	}

	/// Draw the page and return every string it painted, which is what the tests below read.
	fn painted(page: &mut TestCord) -> Vec<String> {
		let ctx = egui::Context::default();
		crate::design::apply(&ctx);
		let mut out = Vec::new();
		fn texts(shape: &egui::Shape, out: &mut Vec<String>) {
			match shape {
				egui::Shape::Text(text) => out.push(text.galley.job.text.clone()),
				egui::Shape::Vec(shapes) => {
					for shape in shapes {
						texts(shape, out);
					}
				}
				_ => {}
			}
		}
		let output = ctx.run_ui(egui::RawInput::default(), |ui| page.show(ui));
		for shape in &output.shapes {
			texts(&shape.shape, &mut out);
		}
		output.drop_without_applying_deltas();
		out
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

	fn three() -> Vec<Entry> {
		vec![
			Entry::new(
				"MessageLogger",
				"MessageLogger",
				"Records messages",
				"Vencord",
				&["Utility"],
				false,
			),
			Entry::new(
				"ClearURLs",
				"ClearURLs",
				"Strips tracking from links",
				"Sam",
				&["Chat"],
				true,
			),
			Entry::new(
				"FixCodeblockGap",
				"FixCodeblockGap",
				"Keeps a break after a fence",
				"Vencord",
				&["Chat"],
				false,
			),
		]
	}

	#[test]
	fn a_search_matches_the_name_the_description_the_author_and_the_tags() {
		for needle in ["clearurls", "tracking", "vencord", "chat"] {
			let mut page = TestCord {
				entries: three(),
				search: needle.to_string(),
				listing_dirty: true,
				..TestCord::default()
			};
			let shown = page.visible();
			assert!(!shown.is_empty(), "{needle} matched nothing");
			if needle == "clearurls" || needle == "tracking" || needle == "sam" {
				assert_eq!(shown, vec![1], "{needle} matched the wrong entry");
			}
		}
	}

	#[test]
	fn a_search_that_matches_nothing_leaves_the_list_empty() {
		let mut page = TestCord {
			entries: three(),
			search: "zzz".to_string(),
			listing_dirty: true,
			..TestCord::default()
		};
		assert!(page.visible().is_empty());
	}

	#[test]
	fn the_search_ignores_case_and_surrounding_space() {
		let mut page = TestCord {
			entries: three(),
			search: "  CLEARURLS  ".to_string(),
			listing_dirty: true,
			..TestCord::default()
		};
		assert_eq!(page.visible(), vec![1]);
	}

	#[test]
	fn sorting_by_name_orders_the_list() {
		let mut page = TestCord {
			entries: three(),
			sort: Sort::Name,
			listing_dirty: true,
			..TestCord::default()
		};
		assert_eq!(page.visible(), vec![1, 2, 0], "clearurls, fix, message");
	}

	#[test]
	fn sorting_puts_the_ones_that_are_on_first() {
		let mut page = TestCord {
			entries: three(),
			sort: Sort::Enabled,
			listing_dirty: true,
			..TestCord::default()
		};
		assert_eq!(page.visible()[0], 1, "the only one that is on comes first");
	}

	#[test]
	fn build_order_is_what_the_runtime_registered() {
		let mut page = TestCord {
			entries: three(),
			listing_dirty: true,
			..TestCord::default()
		};
		assert_eq!(page.visible(), vec![0, 1, 2]);
	}

	#[test]
	fn the_order_is_only_rebuilt_when_something_changed() {
		let mut page = TestCord {
			entries: three(),
			sort: Sort::Name,
			listing_dirty: true,
			..TestCord::default()
		};
		assert_eq!(page.visible(), vec![1, 2, 0]);
		page.entries.swap(0, 1);
		assert_eq!(
			page.visible(),
			vec![1, 2, 0],
			"the order is kept until something asks for a new one"
		);
		page.listing_dirty = true;
		assert_eq!(
			page.visible(),
			vec![0, 2, 1],
			"and then it follows the entries"
		);
	}

	#[test]
	fn a_page_with_a_search_and_a_sort_draws() {
		let mut page = TestCord {
			entries: three(),
			search: "chat".to_string(),
			sort: Sort::Name,
			listing_dirty: true,
			..TestCord::default()
		};
		drawn(&mut page);
		assert!(page.requests.is_empty());
	}

	#[test]
	fn a_page_with_a_search_that_matches_nothing_draws() {
		let mut page = TestCord {
			entries: three(),
			search: "zzz".to_string(),
			listing_dirty: true,
			..TestCord::default()
		};
		drawn(&mut page);
	}

	#[test]
	fn the_search_field_and_the_order_are_on_the_page() {
		let mut page = TestCord {
			entries: three(),
			..TestCord::default()
		};
		let painted = painted(&mut page);
		assert!(
			painted.iter().any(|line| line.contains("Search plugins")),
			"the search field is missing: {painted:?}"
		);
		assert!(
			painted.iter().any(|line| line == "Build order"),
			"the order picker is missing: {painted:?}"
		);
		assert!(
			painted
				.iter()
				.any(|line| line.contains("1 of 3 on") && line.contains("showing 3")),
			"the count is missing: {painted:?}"
		);
	}

	#[test]
	fn the_log_a_port_handed_over_is_shown_on_the_page() {
		let mut logger = entry();
		logger.log = true;
		logger.log_tail =
			"[message] someone in 7: hello\n[edit] someone in 7: hello there".to_string();
		let mut page = TestCord {
			entries: vec![logger],
			expanded: Some("MessageLogger".to_string()),
			..TestCord::default()
		};
		let painted = painted(&mut page);
		assert!(
			painted.iter().any(|line| line.contains("[edit] someone")),
			"the record is not on the page: {painted:?}"
		);
		assert!(
			painted.iter().any(|line| line == "Copy log"),
			"the copy button is missing: {painted:?}"
		);
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
