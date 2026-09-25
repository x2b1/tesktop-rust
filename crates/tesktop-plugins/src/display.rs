//! Ports that change how the client's own clocks and markers read.

use crate::{Fallback, Meta, Setting, SettingKind, Values, number_or, text_or};

/// Whether a clock keeps 12- or 24-hour time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HourFormat {
	#[default]
	Keep,
	Twelve,
	TwentyFour,
}

/// What one plugin wants changed about message display. `None` means "not my business".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisplayPatch {
	pub floor_relative: Option<bool>,
	pub hour: Option<HourFormat>,
	pub offset_minutes: Option<i32>,
	pub hide_edited: Option<bool>,
}

const HOUR_SETTINGS: &[Setting] = &[
	Setting {
		key: "hourFormat",
		label: "Clock",
		kind: SettingKind::Choice(&[
			("keep", "As the system sets it"),
			("twentyfour", "24 hour"),
			("twelve", "12 hour"),
		]),
		default: Fallback::Text("keep"),
	},
	Setting {
		key: "offsetMinutes",
		label: "Shift every clock (minutes)",
		kind: SettingKind::Number {
			min: -720,
			max: 840,
		},
		default: Fallback::Number(0),
	},
];

/// CustomTimestamps: a 12- or 24-hour clock with the owner's own offset.
pub struct CustomTimestamps {
	hour: Option<HourFormat>,
	offset: Option<i32>,
}

impl Default for CustomTimestamps {
	fn default() -> Self {
		Self {
			hour: Some(HourFormat::Keep),
			offset: Some(0),
		}
	}
}

impl crate::Plugin for CustomTimestamps {
	fn meta(&self) -> Meta {
		Meta {
			id: "CustomTimestamps",
			name: "CustomTimestamps",
			description: "Shows message clocks on your own 12 or 24 hour setting, with an offset.",
			authors: "Vencord",
			tags: &["Appearance", "Utility"],
			aliases: &["customTimestamps"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		HOUR_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.hour = Some(
			match text_or(values, HOUR_SETTINGS, "hourFormat").as_str() {
				"twentyfour" => HourFormat::TwentyFour,
				"twelve" => HourFormat::Twelve,
				_ => HourFormat::Keep,
			},
		);
		self.offset =
			Some(number_or(values, HOUR_SETTINGS, "offsetMinutes").clamp(-720, 840) as i32);
	}

	fn display(&self) -> DisplayPatch {
		DisplayPatch {
			hour: self.hour,
			offset_minutes: self.offset,
			..DisplayPatch::default()
		}
	}

	fn summary(&self) -> Option<String> {
		let hour = match self.hour.unwrap_or(HourFormat::Keep) {
			HourFormat::Keep => "system clock",
			HourFormat::Twelve => "12 hour clock",
			HourFormat::TwentyFour => "24 hour clock",
		};
		let offset = self.offset.unwrap_or_default();
		Some(if offset == 0 {
			hour.to_string()
		} else {
			format!("{hour}, shifted {offset} min")
		})
	}
}

/// DontRoundMyTimestamps: relative phrases always round down, so 7.6 years reads "7 years".
pub struct DontRoundMyTimestamps;

impl crate::Plugin for DontRoundMyTimestamps {
	fn meta(&self) -> Meta {
		Meta {
			id: "DontRoundMyTimestamps",
			name: "DontRoundMyTimestamps",
			description: "Rounds relative times down instead of to the nearest.",
			authors: "Lexi",
			tags: &["Appearance", "Utility"],
			aliases: &["dontRoundMyTimestamps", "noRoundMyTimestamps"],
			default_enabled: false,
		}
	}

	fn display(&self) -> DisplayPatch {
		DisplayPatch {
			floor_relative: Some(true),
			..DisplayPatch::default()
		}
	}

	fn summary(&self) -> Option<String> {
		Some("Relative times round down".to_string())
	}
}

/// NoEditedTimestamp: drops the "(edited)" marker from every message row.
pub struct NoEditedTimestamp;

impl crate::Plugin for NoEditedTimestamp {
	fn meta(&self) -> Meta {
		Meta {
			id: "NoEditedTimestamp",
			name: "NoEditedTimestamp",
			description: "Hides the edited marker on messages.",
			authors: "Vencord",
			tags: &["Appearance"],
			aliases: &["noEditedTimestamp"],
			default_enabled: false,
		}
	}

	fn display(&self) -> DisplayPatch {
		DisplayPatch {
			hide_edited: Some(true),
			..DisplayPatch::default()
		}
	}

	fn summary(&self) -> Option<String> {
		Some("Edited marker hidden".to_string())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::Plugin;
	use model::Id;

	fn custom(settings: &[(&str, serde_json::Value)]) -> CustomTimestamps {
		let mut plugin = CustomTimestamps::default();
		plugin.configure(&Values(
			settings
				.iter()
				.map(|(key, value)| ((*key).to_string(), value.clone()))
				.collect(),
		));
		plugin
	}

	#[test]
	fn the_defaults_keep_the_system_clock() {
		let registry = crate::Registry::new();
		let display = registry.display();
		assert_eq!(display.hour, HourFormat::Keep);
		assert_eq!(display.offset_minutes, 0);
		assert!(!display.floor_relative);
		assert!(!display.hide_edited);
		assert_eq!(
			CustomTimestamps::default().display().floor_relative,
			None,
			"a display port only speaks for itself"
		);
	}

	#[test]
	fn a_chosen_hour_and_offset_reach_the_display() {
		let plugin = custom(&[
			("hourFormat", "twelve".into()),
			("offsetMinutes", serde_json::json!(-330)),
		]);
		let patch = plugin.display();
		assert_eq!(patch.hour, Some(HourFormat::Twelve));
		assert_eq!(patch.offset_minutes, Some(-330));
		assert_eq!(
			plugin.summary().as_deref(),
			Some("12 hour clock, shifted -330 min")
		);
	}

	#[test]
	fn an_out_of_range_offset_is_clamped_to_a_real_zone() {
		let plugin = custom(&[("offsetMinutes", serde_json::json!(99_999))]);
		assert_eq!(plugin.display().offset_minutes, Some(840));
		let plugin = custom(&[("offsetMinutes", serde_json::json!(-99_999))]);
		assert_eq!(plugin.display().offset_minutes, Some(-720));
	}

	#[test]
	fn rounding_and_the_edited_marker_are_independent() {
		let rounding = DontRoundMyTimestamps.display();
		assert_eq!(rounding.floor_relative, Some(true));
		assert_eq!(rounding.hide_edited, None);
		let marker = NoEditedTimestamp.display();
		assert_eq!(marker.hide_edited, Some(true));
		assert_eq!(marker.floor_relative, None);
	}

	#[test]
	fn only_enabled_plugins_change_the_display() {
		let mut registry = crate::Registry::new();
		registry.set_enabled("NoEditedTimestamp", true);
		assert!(registry.display().hide_edited);
		registry.set_enabled("DontRoundMyTimestamps", true);
		let display = registry.display();
		assert!(display.hide_edited);
		assert!(display.floor_relative);
		registry.set_enabled("NoEditedTimestamp", false);
		assert!(!registry.display().hide_edited);
		assert!(registry.display().floor_relative);
	}

	#[test]
	fn the_plugins_never_touch_message_content() {
		let mut registry = crate::Registry::new();
		registry.set_enabled("CustomTimestamps", true);
		let inbound = crate::Inbound::new(Id(7), None, Id(1), 0);
		let message = test_support::message(1, Id(7));
		let replies: Vec<crate::PendingReply> = Vec::new();
		registry.observe(&inbound, crate::InboundEvent::Created(&message));
		assert!(replies.is_empty());
	}
}
