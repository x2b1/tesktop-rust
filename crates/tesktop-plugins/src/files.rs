//! Ports that act on the files in a message: what they are called when they go out, and
//! getting them down here.

use crate::{Fallback, Meta, Setting, SettingKind, Values, flag_or, text_or};

/// The extensions that only work once they are named like the format inside them.
const RENAMES: &[(&str, &[&str])] = &[
	(
		"ogg",
		&[".ogv", ".oga", ".ogx", ".ogm", ".spx", ".aac", ".wma"],
	),
	(
		"jpg",
		&[
			".jpe", ".jif", ".jfi", ".pjpeg", ".pjp", ".bmp", ".tiff", ".tif",
		],
	),
	("svg", &[".svgz", ".ai", ".eps"]),
	(
		"mp4",
		&[
			".m4v", ".m4r", ".m4p", ".avi", ".mkv", ".wmv", ".flv", ".3gp",
		],
	),
	("m4a", &[".m4b", ".aiff"]),
	("mov", &[".movie", ".qt", ".asf", ".rm", ".rmvb"]),
	("png", &[".ico", ".cur"]),
];

/// The extension a name should end in, if it should end in a different one.
///
/// The mapping is the original's, in both directions: a name ending in `.mkv` is sent as
/// `.mp4`, and a name ending in `.mp4` is left alone.
pub fn corrected(name: &str) -> Option<&'static str> {
	let (_, extension) = name.rsplit_once('.')?;
	let extension = format!(".{}", extension.to_ascii_lowercase());
	for (target, sources) in RENAMES {
		if sources.contains(&extension.as_str()) {
			return Some(target);
		}
	}
	None
}

const EXTENSION_SETTINGS: &[Setting] = &[
	Setting {
		key: "enabled",
		label: "Rename the files you send",
		kind: SettingKind::Toggle,
		default: Fallback::Flag(true),
	},
	Setting {
		key: "except",
		label: "Names containing one of these are left alone",
		kind: SettingKind::Text { multiline: true },
		default: Fallback::Text(""),
	},
];

/// FixFileExtensions: a file goes out under a name the service will actually accept.
pub struct FixFileExtensions {
	enabled: bool,
	except: Vec<String>,
}

impl Default for FixFileExtensions {
	fn default() -> Self {
		Self {
			enabled: true,
			except: Vec::new(),
		}
	}
}

impl crate::Plugin for FixFileExtensions {
	fn meta(&self) -> Meta {
		Meta {
			id: "FixFileExtensions",
			name: "FixFileExtensions",
			description: "Renames a file to a compatible extension when it needs one.",
			authors: "thororen",
			tags: &["Utility"],
			aliases: &["fixFileExtensions"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		EXTENSION_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.enabled = flag_or(values, EXTENSION_SETTINGS, "enabled");
		self.except = text_or(values, EXTENSION_SETTINGS, "except")
			.split(['\n', ','])
			.map(str::trim)
			.filter(|part| !part.is_empty())
			.map(str::to_lowercase)
			.take(64)
			.collect();
	}

	fn stage_files(&mut self, files: &mut Vec<crate::Staged>) {
		if !self.enabled {
			return;
		}
		for file in files.iter_mut() {
			if self
				.except
				.iter()
				.any(|part| file.name.to_lowercase().contains(part))
			{
				continue;
			}
			// The name is only changed where the extension actually is, so a name with no
			// extension and a name with several dots keep their shape.
			let Some((stem, _)) = file.name.rsplit_once('.') else {
				continue;
			};
			let Some(target) = corrected(&file.name) else {
				continue;
			};
			file.name = format!("{stem}.{target}");
		}
	}

	fn summary(&self) -> Option<String> {
		self.enabled.then(|| {
			if self.except.is_empty() {
				"Renaming what you send".to_string()
			} else {
				format!("Renaming, {} names exempt", self.except.len())
			}
		})
	}
}

const DOWNLOAD_SETTINGS: &[Setting] = &[Setting {
	key: "images",
	label: "Include images as well as files",
	kind: SettingKind::Toggle,
	default: Fallback::Flag(true),
}];

/// DownloadAllAttachments: the files on a message, all of them, in one go.
pub struct DownloadAllAttachments {
	images: bool,
}

impl Default for DownloadAllAttachments {
	fn default() -> Self {
		Self { images: true }
	}
}

impl crate::Plugin for DownloadAllAttachments {
	fn meta(&self) -> Meta {
		Meta {
			id: "DownloadAllAttachments",
			name: "DownloadAllAttachments",
			description: "Downloads every file on a message at once.",
			authors: "Equicord",
			tags: &["Utility"],
			aliases: &["downloadAllAttachments"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		DOWNLOAD_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		self.images = flag_or(values, DOWNLOAD_SETTINGS, "images");
	}

	fn message_actions(&self) -> &'static [crate::MessageAction] {
		&[crate::MessageAction {
			id: "download-all",
			label: "Download every file here",
		}]
	}

	fn run_action(&self, action: &str, message: &model::Message) -> Option<crate::ActionResult> {
		if action != "download-all" {
			return None;
		}
		let wanted: Vec<model::Attachment> = message
			.attachments
			.iter()
			.filter(|file| self.images || !file.is_image())
			.cloned()
			.collect();
		if wanted.is_empty() {
			return Some(crate::ActionResult::Notice(
				"Nothing to download on that message".to_string(),
			));
		}
		Some(crate::ActionResult::Download(wanted))
	}

	fn summary(&self) -> Option<String> {
		Some(if self.images {
			"Files and images".to_string()
		} else {
			"Files only".to_string()
		})
	}
}

/// The name a file lands under when several on one message share it, the way the original
/// numbers them: `photo.jpg`, then `photo_1.jpg`.
pub fn unique_name(used: &mut std::collections::BTreeMap<String, u32>, original: &str) -> String {
	let count = used.entry(original.to_owned()).or_insert(0);
	let name = if *count == 0 {
		original.to_owned()
	} else {
		match original.rsplit_once('.') {
			Some((stem, extension)) => format!("{stem}_{count}.{extension}"),
			None => format!("{original}_{count}"),
		}
	};
	*count += 1;
	name
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{Plugin, Staged};

	/// A file with the name and kind the filter reads; nothing else about it matters here.
	fn file(name: &str, image: bool) -> model::Attachment {
		model::Attachment {
			id: model::Id(700 + name.len() as u64),
			filename: name.to_string(),
			description: None,
			content_type: Some(if image { "image/png" } else { "text/plain" }.to_string()),
			size: 128,
			media: model::EmbedMedia::default(),
			spoiler: false,
			duration_ms: None,
			waveform: Vec::new(),
		}
	}

	fn staged(name: &str) -> Vec<Staged> {
		vec![Staged {
			name: name.to_string(),
			bytes: 1,
		}]
	}

	#[test]
	fn an_extension_that_needs_changing_is_changed() {
		let mut plugin = FixFileExtensions::default();
		let mut files = staged("holiday.mkv");
		plugin.stage_files(&mut files);
		assert_eq!(files[0].name, "holiday.mp4");
	}

	#[test]
	fn an_extension_that_is_already_right_is_left_alone() {
		let mut plugin = FixFileExtensions::default();
		let mut files = staged("holiday.mp4");
		plugin.stage_files(&mut files);
		assert_eq!(files[0].name, "holiday.mp4");
	}

	#[test]
	fn a_name_without_an_extension_is_left_alone() {
		let mut plugin = FixFileExtensions::default();
		let mut files = staged("README");
		plugin.stage_files(&mut files);
		assert_eq!(files[0].name, "README");
	}

	#[test]
	fn several_dots_keep_their_shape() {
		let mut plugin = FixFileExtensions::default();
		let mut files = staged("my.holiday.2024.mkv");
		plugin.stage_files(&mut files);
		assert_eq!(files[0].name, "my.holiday.2024.mp4");
	}

	#[test]
	fn a_named_file_can_be_exempt() {
		let mut plugin = FixFileExtensions::default();
		plugin.configure(&Values(
			[("except".to_string(), serde_json::json!("holiday"))]
				.into_iter()
				.collect(),
		));
		let mut files = staged("holiday.mkv");
		plugin.stage_files(&mut files);
		assert_eq!(files[0].name, "holiday.mkv");
	}

	#[test]
	fn renaming_can_be_turned_off() {
		let mut plugin = FixFileExtensions::default();
		plugin.configure(&Values(
			[("enabled".to_string(), serde_json::json!(false))]
				.into_iter()
				.collect(),
		));
		let mut files = staged("holiday.mkv");
		plugin.stage_files(&mut files);
		assert_eq!(files[0].name, "holiday.mkv");
		assert!(plugin.summary().is_none());
	}

	#[test]
	fn the_mapping_is_the_originals() {
		assert_eq!(corrected("a.oga"), Some("ogg"));
		assert_eq!(corrected("a.tif"), Some("jpg"));
		assert_eq!(corrected("a.cur"), Some("png"));
		assert_eq!(corrected("a.mkv"), Some("mp4"));
		assert_eq!(corrected("a.m4b"), Some("m4a"));
		assert_eq!(corrected("a.qt"), Some("mov"));
		assert_eq!(corrected("a.ogv"), Some("ogg"));
		assert_eq!(corrected("a.avi"), Some("mp4"));
		assert_eq!(corrected("a.bmp"), Some("jpg"));
		assert_eq!(corrected("a.ico"), Some("png"));
		assert_eq!(corrected("a.eps"), Some("svg"));
		assert_eq!(corrected("a.aiff"), Some("m4a"));
		assert_eq!(corrected("a.3gp"), Some("mp4"));
		assert_eq!(corrected("a.wma"), Some("ogg"));
		assert_eq!(corrected("a.rmvb"), Some("mov"));
		assert_eq!(
			corrected("a.MKV"),
			Some("mp4"),
			"the extension's case does not matter"
		);
		assert_eq!(corrected("a.txt"), None);
		assert_eq!(corrected("noextension"), None);
	}

	#[test]
	fn every_file_on_a_message_is_offered_at_once() {
		let plugin = DownloadAllAttachments::default();
		let mut message = test_support::message(1, model::Id(7));
		message.attachments.push(file("a.txt", false));
		message.attachments.push(file("b.txt", false));
		message.attachments.push(file("c.png", true));
		match plugin.run_action("download-all", &message) {
			Some(crate::ActionResult::Download(files)) => {
				assert_eq!(files.len(), 3);
			}
			other => panic!("expected the files, got {other:?}"),
		}
	}

	#[test]
	fn images_can_be_left_out() {
		let mut plugin = DownloadAllAttachments::default();
		plugin.configure(&Values(
			[("images".to_string(), serde_json::json!(false))]
				.into_iter()
				.collect(),
		));
		let mut message = test_support::message(1, model::Id(7));
		message.attachments.push(file("a.txt", false));
		message.attachments.push(file("c.png", true));
		match plugin.run_action("download-all", &message) {
			Some(crate::ActionResult::Download(files)) => {
				assert_eq!(files.len(), 1);
				assert_eq!(files[0].filename, "a.txt");
			}
			other => panic!("expected the file, got {other:?}"),
		}
	}

	#[test]
	fn a_message_with_no_files_says_so() {
		let plugin = DownloadAllAttachments::default();
		let message = test_support::message(1, model::Id(7));
		assert!(matches!(
			plugin.run_action("download-all", &message),
			Some(crate::ActionResult::Notice(_))
		));
	}

	#[test]
	fn files_sharing_a_name_are_numbered() {
		let mut used = std::collections::BTreeMap::new();
		assert_eq!(unique_name(&mut used, "photo.jpg"), "photo.jpg");
		assert_eq!(unique_name(&mut used, "photo.jpg"), "photo_1.jpg");
		assert_eq!(unique_name(&mut used, "photo.jpg"), "photo_2.jpg");
		assert_eq!(unique_name(&mut used, "README"), "README");
		assert_eq!(unique_name(&mut used, "README"), "README_1");
	}
}

/// QuickMention: a mention of the person whose message you are looking at, ready to send.
#[derive(Default)]
pub struct QuickMention {
	/// The mention waiting to be written, handed to the composer once.
	pending: std::cell::RefCell<Option<String>>,
}

impl crate::Plugin for QuickMention {
	fn meta(&self) -> Meta {
		Meta {
			id: "QuickMention",
			name: "QuickMention",
			description: "Puts a mention of the author in the composer.",
			authors: "Vencord",
			tags: &["Shortcuts", "Utility"],
			aliases: &["quickMention"],
			default_enabled: false,
		}
	}

	fn message_actions(&self) -> &'static [crate::MessageAction] {
		&[crate::MessageAction {
			id: "mention-author",
			label: "Mention this person",
		}]
	}

	fn run_action(&self, action: &str, message: &model::Message) -> Option<crate::ActionResult> {
		if action != "mention-author" {
			return None;
		}
		// A mention of yourself is a no-op, and the original leaves that one out too.
		if message.author.id == model::Id(0) {
			return None;
		}
		*self.pending.borrow_mut() = Some(format!("<@{}> ", message.author.id));
		Some(crate::ActionResult::Notice(
			"The mention is in the composer".to_string(),
		))
	}

	fn take_compose(&mut self) -> Option<String> {
		self.pending.borrow_mut().take()
	}
}

const REPLY_SETTINGS: &[Setting] = &[Setting {
	key: "preset",
	label: "The reply to write",
	kind: SettingKind::Text { multiline: true },
	default: Fallback::Text("On my way"),
}];

/// QuickReply: a reply you keep to hand, written into the composer.
#[derive(Default)]
pub struct QuickReply {
	preset: String,
	pending: std::cell::RefCell<Option<String>>,
}

impl crate::Plugin for QuickReply {
	fn meta(&self) -> Meta {
		Meta {
			id: "QuickReply",
			name: "QuickReply",
			description: "Keeps a reply to hand and writes it in the composer.",
			authors: "Vencord",
			tags: &["Shortcuts", "Utility"],
			aliases: &["quickReply"],
			default_enabled: false,
		}
	}

	fn settings(&self) -> &'static [Setting] {
		REPLY_SETTINGS
	}

	fn configure(&mut self, values: &Values) {
		// A draft is bounded like any other, and a reply is a line rather than an essay.
		self.preset = text_or(values, REPLY_SETTINGS, "preset")
			.trim()
			.chars()
			.filter(|character| *character != '\0')
			.take(512)
			.collect();
	}

	fn message_actions(&self) -> &'static [crate::MessageAction] {
		&[crate::MessageAction {
			id: "quick-reply",
			label: "Write my usual reply",
		}]
	}

	fn run_action(&self, action: &str, _message: &model::Message) -> Option<crate::ActionResult> {
		if action != "quick-reply" || self.preset.is_empty() {
			return None;
		}
		*self.pending.borrow_mut() = Some(self.preset.clone());
		Some(crate::ActionResult::Notice(
			"Your reply is in the composer".to_string(),
		))
	}

	fn take_compose(&mut self) -> Option<String> {
		self.pending.borrow_mut().take()
	}

	fn summary(&self) -> Option<String> {
		(!self.preset.is_empty()).then(|| format!("{:?}", self.preset))
	}
}

#[cfg(test)]
mod compose_tests {
	use super::*;
	use crate::{Plugin, Staged};

	fn message(author: u64) -> model::Message {
		let mut message = test_support::message(1, model::Id(7));
		message.author.id = model::Id(author);
		message
	}

	fn staged(name: &str) -> Vec<Staged> {
		vec![Staged {
			name: name.to_string(),
			bytes: 1,
		}]
	}

	#[test]
	fn a_mention_is_handed_over_once() {
		let mut plugin = QuickMention::default();
		assert!(plugin.run_action("mention-author", &message(42)).is_some());
		assert_eq!(plugin.take_compose().as_deref(), Some("<@42> "));
		assert!(plugin.take_compose().is_none(), "it is written once");
	}

	#[test]
	fn the_mention_is_about_the_author() {
		let mut plugin = QuickMention::default();
		plugin.run_action("mention-author", &message(99));
		assert_eq!(plugin.take_compose(), Some("<@99> ".to_string()));
	}

	#[test]
	fn the_keeps_its_reply_to_hand() {
		let mut plugin = QuickReply::default();
		plugin.configure(&Values(
			[("preset".to_string(), serde_json::json!("On my way"))]
				.into_iter()
				.collect(),
		));
		plugin.run_action("quick-reply", &message(1));
		assert_eq!(plugin.take_compose().as_deref(), Some("On my way"));
		assert_eq!(plugin.summary().as_deref(), Some("\"On my way\""));
	}

	#[test]
	fn an_empty_reply_writes_nothing() {
		let mut plugin = QuickReply::default();
		plugin.configure(&Values(
			[("preset".to_string(), serde_json::json!("   "))]
				.into_iter()
				.collect(),
		));
		assert!(plugin.run_action("quick-reply", &message(1)).is_none());
		assert!(plugin.take_compose().is_none());
	}

	#[test]
	fn the_registry_hands_the_first_one_over() {
		let mut registry = crate::Registry::new();
		registry.set_enabled("QuickMention", true);
		let message = message(7);
		registry.run_action("QuickMention", "mention-author", &message);
		assert_eq!(registry.take_compose().as_deref(), Some("<@7> "));
	}

	#[test]
	fn nothing_is_handed_over_when_nothing_asked() {
		let mut registry = crate::Registry::new();
		registry.set_enabled("QuickMention", true);
		assert!(registry.take_compose().is_none());
		let _ = staged("a.txt");
	}
}
