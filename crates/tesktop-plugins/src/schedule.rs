//! Time-of-day and activity ports, judged against what the app already knows.

use crate::{Fallback, Meta, Setting, SettingKind, Values, flag_or, number_or};

const QUIET_SETTINGS: &[Setting] = &[
	Setting {
		key: "start",
		label: "Quiet hours start (hour, 0 to 23)",
		kind: SettingKind::Number { min: 0, max: 23 },
		default: Fallback::Number(23),
	},
	Setting {
		key: "end",
		label: "Quiet hours end (hour, 0 to 23)",
		kind: SettingKind::Number { min: 0, max: 23 },
		default: Fallback::Number(8),
	},
];

/// QuietHours: no alerts between the hours you pick, wrapping past midnight.
#[derive(Default)]
pub struct QuietHours {
	start: u32,
	end: u32,
}

impl QuietHours {
	/// Whether `hour` falls in the quiet window. A window that ends where it starts is empty.
	pub fn is_quiet(&self, hour: u32) -> bool {
		let hour = hour.min(23);
		if self.start == self.end {
			return false;
		}
		if self.start < self.end {
			(self.start..self.end).contains(&hour)
		} else {
			hour >= self.start || hour < self.end
		}
	}
}

impl crate::Plugin for QuietHours {
	fn meta(&self) -> Meta {
		Meta {
			id: "QuietHours",
			name: "QuietHours",
			description: "Silences alerts during the hours you choose.",
			authors: "Testcord",
			tags: &["Notifications"],
			aliases: &["quietHours"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		QUIET_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.start = number_or(values, QUIET_SETTINGS, "start").clamp(0, 23) as u32;
		self.end = number_or(values, QUIET_SETTINGS, "end").clamp(0, 23) as u32;
	}

	fn notice(&self, event: &crate::notify::Notify<'_>) -> crate::notify::Notice {
		if !self.is_quiet(u32::from(event.hour)) {
			return crate::notify::Notice::default();
		}
		// A watched person still gets their toast; only the sound and the alert stop.
		crate::notify::Notice {
			announce: false,
			toast: event.visible,
		}
	}

	fn summary(&self) -> Option<String> {
		Some(format!(
			"quiet from {:02}:00 to {:02}:00",
			self.start, self.end
		))
	}
}

const ACTIVITY_SETTINGS: &[Setting] = &[
	Setting {
		key: "alertWhilePlaying",
		label: "Still alert while a game is running",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
	Setting {
		key: "showStatus",
		label: "Change your status while playing",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(false),
	},
];

/// AutoDNDWhilePlaying: quiet the alerts while a game is running.
#[derive(Default)]
pub struct AutoDndWhilePlaying {
	alert_while_playing: bool,
	show_status: bool,
}

impl crate::Plugin for AutoDndWhilePlaying {
	fn meta(&self) -> Meta {
		Meta {
			id: "AutoDNDWhilePlaying",
			name: "AutoDNDWhilePlaying",
			description: "Keeps the alerts quiet while you are playing.",
			authors: "Mavri",
			tags: &["Notifications", "Activity"],
			aliases: &["autoDNDWhilePlaying", "autoDndWhilePlaying"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		ACTIVITY_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.alert_while_playing = flag_or(values, ACTIVITY_SETTINGS, "alertWhilePlaying");
		self.show_status = flag_or(values, ACTIVITY_SETTINGS, "showStatus");
	}

	fn notice(&self, event: &crate::notify::Notify<'_>) -> crate::notify::Notice {
		if !event.playing || self.alert_while_playing {
			return crate::notify::Notice::default();
		}
		crate::notify::Notice {
			announce: false,
			toast: true,
		}
	}

	/// While a game is running the app can set your status to do not disturb, which is what
	/// TestCord's plugin arranges through the service. The app owns presence, so this port only
	/// reports the intent.
	fn presence(&self) -> crate::Presence {
		if self.show_status {
			crate::Presence::DoNotDisturbWhilePlaying
		} else {
			crate::Presence::Keep
		}
	}

	fn summary(&self) -> Option<String> {
		Some(if self.alert_while_playing {
			"Alerts still come through while playing".to_string()
		} else {
			"Alerts are quiet while a game is running".to_string()
		})
	}
}

/// The hour inside the day, for schedules that read better that way.
pub fn hour_of(seconds: i64) -> u32 {
	seconds.div_euclid(3600).rem_euclid(24) as u32
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::Plugin;

	fn quiet(start: i64, end: i64) -> QuietHours {
		let mut plugin = QuietHours::default();
		plugin.configure(&Values(
			[
				("start".to_string(), serde_json::json!(start)),
				("end".to_string(), serde_json::json!(end)),
			]
			.into_iter()
			.collect(),
		));
		plugin
	}

	fn event(hour: u8, playing: bool, visible: bool) -> crate::notify::Notify<'static> {
		let message: &'static model::Message =
			Box::leak(Box::new(test_support::message(1, model::Id(7))));
		crate::notify::Notify {
			message,
			channel: model::Id(7),
			guild: Some(model::Id(10)),
			direct: false,
			mentions_me: false,
			everyone: false,
			oldest_unread: true,
			visible,
			me: model::Id(1),
			hour,
			playing,
		}
	}

	#[test]
	fn a_window_inside_one_day_is_quiet() {
		let plugin = quiet(2, 5);
		assert!(!plugin.is_quiet(1));
		assert!(plugin.is_quiet(2));
		assert!(plugin.is_quiet(4));
		assert!(!plugin.is_quiet(5));
	}

	#[test]
	fn a_window_wrapping_midnight_is_quiet() {
		let plugin = quiet(23, 8);
		assert!(plugin.is_quiet(23));
		assert!(plugin.is_quiet(0));
		assert!(plugin.is_quiet(7));
		assert!(!plugin.is_quiet(8));
		assert!(!plugin.is_quiet(22));
	}

	#[test]
	fn an_empty_window_is_never_quiet() {
		let plugin = quiet(3, 3);
		for hour in 0..24 {
			assert!(!plugin.is_quiet(hour));
		}
	}

	#[test]
	fn hours_outside_the_range_are_clamped() {
		let plugin = quiet(-5, 99);
		assert!(plugin.is_quiet(0));
		assert!(plugin.is_quiet(22));
		assert!(!plugin.is_quiet(23));
	}

	#[test]
	fn the_hour_helper_is_always_inside_the_day() {
		assert_eq!(hour_of(0), 0);
		assert_eq!(hour_of(3600 * 5), 5);
		assert_eq!(hour_of(-3600), 23);
	}

	#[test]
	fn quiet_hours_silence_the_alert_and_keep_a_toast() {
		let plugin = quiet(3, 4);
		let quiet_hour = plugin.notice(&event(3, false, true));
		assert!(!quiet_hour.announce);
		assert!(quiet_hour.toast);
		let loud_hour = plugin.notice(&event(9, false, true));
		assert!(loud_hour.announce);
	}

	#[test]
	fn playing_silences_alerts_unless_you_ask_otherwise() {
		let mut plugin = AutoDndWhilePlaying::default();
		assert!(!plugin.notice(&event(9, true, false)).announce);
		assert!(plugin.notice(&event(9, false, false)).announce);
		plugin.configure(&Values(
			[("alertWhilePlaying".to_string(), serde_json::json!(true))]
				.into_iter()
				.collect(),
		));
		assert!(plugin.notice(&event(9, true, false)).announce);
	}

	#[test]
	fn a_status_change_is_only_intent() {
		let mut plugin = AutoDndWhilePlaying::default();
		assert_eq!(plugin.presence(), crate::Presence::Keep);
		plugin.configure(&Values(
			[("showStatus".to_string(), serde_json::json!(true))]
				.into_iter()
				.collect(),
		));
		assert_eq!(plugin.presence(), crate::Presence::DoNotDisturbWhilePlaying);
	}
}
