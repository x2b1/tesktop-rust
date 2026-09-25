//! Settings persistence and TestCord import.
//!
//! tesktop2 keeps its own file so the two apps never fight over one, but it reads TestCord's
//! `settings.json` shape, including its id, name and alias resolution, so an existing TestCord
//! configuration can be imported instead of retyped.

use crate::{MAX_PLUGIN_BYTES, MAX_SETTINGS_BYTES, Registry};
use serde_json::{Map, Value};
use std::path::Path;

const FILE: &str = "tesktop-plugins.json";
const VERSION: i64 = 1;

/// The settings file beside the local database.
pub fn path(root: &Path) -> std::path::PathBuf {
	root.join(FILE)
}

/// Read the stored file. A file that is missing, too large or malformed is treated as absent, so a
/// damaged file never keeps the app from starting.
pub fn load(root: &Path) -> Option<Value> {
	let bytes = std::fs::metadata(path(root)).ok()?.len();
	if bytes > MAX_SETTINGS_BYTES as u64 {
		return None;
	}
	serde_json::from_slice(&std::fs::read(path(root)).ok()?).ok()
}

pub fn save(root: &Path, registry: &Registry) -> Result<(), String> {
	let value = export(registry);
	let bytes = serde_json::to_vec_pretty(&value).map_err(|error| error.to_string())?;
	if bytes.len() > MAX_SETTINGS_BYTES {
		return Err("Plugin settings are too large to save".to_string());
	}
	let target = path(root);
	let temporary = target.with_extension("json.new");
	std::fs::write(&temporary, &bytes).map_err(|error| error.to_string())?;
	std::fs::rename(&temporary, &target).map_err(|error| error.to_string())
}

/// Apply a stored file to the registry, keeping only settings a bundled plugin declares.
pub fn restore(registry: &mut Registry, value: &Value) {
	let Some(plugins) = value.get("plugins").and_then(Value::as_object) else {
		return;
	};
	for (id, entry) in plugins {
		let Some(canonical) = canonical(registry, id) else {
			continue;
		};
		let Some(entry) = entry.as_object() else {
			continue;
		};
		if let Some(enabled) = entry.get("enabled").and_then(Value::as_bool) {
			registry.set_enabled(&canonical, enabled);
		}
		let keys: Vec<String> = registry
			.settings_of(&canonical)
			.iter()
			.map(|setting| setting.key.to_string())
			.collect();
		for key in keys {
			if let Some(setting) = entry.get(&key) {
				registry.set_value(&canonical, &key, setting.clone());
			}
		}
	}
	registry.reconfigure();
}

/// Import TestCord's `settings.json`: `{ "plugins": { "<id>": { "enabled": …, … } } }`.
/// Returns how many bundled plugins were recognised, so the app can say what was not.
pub fn import_testcord(registry: &mut Registry, value: &Value) -> usize {
	let Some(plugins) = value.get("plugins").and_then(Value::as_object) else {
		return 0;
	};
	let mut imported = 0;
	for (id, entry) in plugins {
		let Some(canonical) = canonical(registry, id) else {
			continue;
		};
		let Some(entry) = entry.as_object() else {
			continue;
		};
		if serde_json::to_string(entry).map_or(MAX_PLUGIN_BYTES, |json| json.len())
			> MAX_PLUGIN_BYTES
		{
			continue;
		}
		if let Some(enabled) = entry.get("enabled").and_then(Value::as_bool) {
			registry.set_enabled(&canonical, enabled);
		}
		let keys: Vec<String> = registry
			.settings_of(&canonical)
			.iter()
			.map(|setting| setting.key.to_string())
			.collect();
		for key in keys {
			if let Some(setting) = entry.get(&key) {
				registry.set_value(&canonical, &key, setting.clone());
			}
		}
		imported += 1;
	}
	registry.reconfigure();
	imported
}

fn canonical(registry: &Registry, id: &str) -> Option<String> {
	registry
		.aliases(id)
		.into_iter()
		.next()
		.map(|resolved| resolved.to_string())
}

fn export(registry: &Registry) -> Value {
	let mut plugins = Map::new();
	for meta in registry.metas() {
		let entry = registry
			.stored_value(meta.id)
			.map(|(enabled, values)| {
				let mut entry = Map::new();
				entry.insert("enabled".to_string(), Value::Bool(enabled));
				for (key, value) in values {
					entry.insert(key.clone(), value.clone());
				}
				Value::Object(entry)
			})
			.unwrap_or(Value::Null);
		plugins.insert(meta.id.to_string(), entry);
	}
	let mut root = Map::new();
	root.insert("version".to_string(), Value::from(VERSION));
	root.insert("plugins".to_string(), Value::Object(plugins));
	Value::Object(root)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::Registry;

	fn temp_dir(name: &str) -> std::path::PathBuf {
		let dir =
			std::env::temp_dir().join(format!("tesktop-plugins-{name}-{}", std::process::id()));
		let _ = std::fs::remove_dir_all(&dir);
		std::fs::create_dir_all(&dir).unwrap();
		dir
	}

	fn registry_with_settings() -> Registry {
		let mut registry = Registry::new();
		registry.set_enabled("ClearURLs", false);
		registry.set_value(
			"BlockKeywords",
			"blockedWords",
			Value::String("spoiler, [a,b]".into()),
		);
		registry.set_value("BlockKeywords", "caseSensitive", Value::Bool(true));
		registry
	}

	#[test]
	fn settings_survive_a_round_trip() {
		let dir = temp_dir("round-trip");
		let registry = registry_with_settings();
		save(&dir, &registry).unwrap();
		let stored = load(&dir).expect("stored file");
		let mut restored = Registry::new();
		restore(&mut restored, &stored);
		assert!(!restored.enabled("ClearURLs"));
		assert_eq!(
			restored.value("BlockKeywords", "blockedWords"),
			Some(&Value::String("spoiler, [a,b]".into()))
		);
		assert_eq!(
			restored.value("BlockKeywords", "caseSensitive"),
			Some(&Value::Bool(true))
		);
		std::fs::remove_dir_all(&dir).unwrap();
	}

	#[test]
	fn testcord_settings_import_by_id_name_and_case() {
		let mut registry = Registry::new();
		let file = serde_json::json!({
			"plugins": {
				"ClearURLs": { "enabled": false },
				"blockKeywords": { "enabled": true, "blockedWords": "spoiler", "useRegex": true },
				"messagelogger": { "enabled": true, "logEdits": false },
				"SomethingElse": { "enabled": true }
			}
		});
		assert_eq!(import_testcord(&mut registry, &file), 3);
		assert!(!registry.enabled("ClearURLs"));
		assert!(registry.enabled("BlockKeywords"));
		assert!(registry.enabled("MessageLogger"));
		assert_eq!(
			registry.value("BlockKeywords", "useRegex"),
			Some(&Value::Bool(true))
		);
	}

	#[test]
	fn unknown_keys_and_settings_are_dropped() {
		let mut registry = Registry::new();
		let file = serde_json::json!({
			"plugins": {
				"ClearURLs": { "enabled": true, "notASetting": "x", "anotherOne": 1 }
			}
		});
		assert_eq!(import_testcord(&mut registry, &file), 1);
		assert!(registry.enabled("ClearURLs"));
		assert!(!registry.enabled("MessageLogger"));
		assert!(registry.value("ClearURLs", "notASetting").is_none());
	}

	#[test]
	fn a_damaged_or_foreign_file_leaves_defaults_alone() {
		let dir = temp_dir("damaged");
		std::fs::write(path(&dir), b"{not json").unwrap();
		assert!(load(&dir).is_none());
		let mut registry = Registry::new();
		registry.set_enabled("ClearURLs", true);
		restore(&mut registry, &serde_json::json!({ "unrelated": true }));
		assert!(registry.enabled("ClearURLs"));
		restore(
			&mut registry,
			&serde_json::json!({ "plugins": "not an object" }),
		);
		assert!(registry.enabled("ClearURLs"));
		std::fs::remove_dir_all(&dir).unwrap();
	}
}
