//! The bundled-ports page, built once and used by both the app and the screenshot fixture.
//!
//! The app fills this in every time the enabled set changes; the offline preview asks for the
//! same list so a screenshot of the page is a screenshot of the page.

use ui::testcord::Entry;

/// One settings row for the ports page, falling back to the value the port declares.
pub fn tesktop_field(
	registry: &tesktop_plugins::Registry,
	id: &str,
	setting: &tesktop_plugins::Setting,
) -> ui::testcord::Field {
	use tesktop_plugins::{Fallback, SettingKind};
	let stored = registry.value(id, setting.key);
	let (kind, fallback) = match (setting.kind, setting.default) {
		(SettingKind::Toggle, Fallback::Flag(value)) => {
			(ui::testcord::Kind::Toggle, ui::testcord::Value::Flag(value))
		}
		(SettingKind::Text { multiline }, Fallback::Text(value)) => (
			ui::testcord::Kind::Text { multiline },
			ui::testcord::Value::Text(value.to_string()),
		),
		(SettingKind::Number { min, max }, Fallback::Number(value)) => (
			ui::testcord::Kind::Number { min, max },
			ui::testcord::Value::Number(value),
		),
		(SettingKind::Choice(options), Fallback::Text(value)) => (
			ui::testcord::Kind::Choice { options },
			ui::testcord::Value::Text(value.to_string()),
		),
		_ => (ui::testcord::Kind::Toggle, ui::testcord::Value::Flag(false)),
	};
	let value = match (&kind, stored) {
		(ui::testcord::Kind::Toggle, Some(serde_json::Value::Bool(stored))) => {
			ui::testcord::Value::Flag(*stored)
		}
		(ui::testcord::Kind::Number { .. }, Some(serde_json::Value::Number(stored))) => {
			ui::testcord::Value::Number(stored.as_i64().unwrap_or_default())
		}
		(_, Some(serde_json::Value::String(stored))) => ui::testcord::Value::Text(stored.clone()),
		_ => fallback,
	};
	ui::testcord::Field {
		key: setting.key.to_string(),
		label: setting.label.to_string(),
		kind,
		value,
	}
}

/// Every bundled port, with the settings it declares and whatever it says about itself.
pub fn page(registry: &tesktop_plugins::Registry) -> Vec<Entry> {
	registry
		.metas()
		.iter()
		.map(|meta| {
			let mut entry = Entry::new(
				meta.id,
				meta.name,
				meta.description,
				meta.authors,
				meta.tags,
				registry.enabled(meta.id),
			);
			entry.summary = registry.summary(meta.id).unwrap_or_default();
			entry.log = registry.export(meta.id).is_some();
			entry.log_tail = if entry.log {
				registry.tail(meta.id, 40)
			} else {
				String::new()
			};
			entry.fields = registry
				.settings_of(meta.id)
				.iter()
				.map(|setting| tesktop_field(registry, meta.id, setting))
				.collect();
			entry
		})
		.collect()
}
