//! Open an address the owner asked for, and only the schemes a desktop should hand to the OS.
//!
//! The address always comes from this user's own settings, never from a message body, and a
//! custom scheme is passed as one argument with no shell involved, so a setting cannot smuggle
//! a second command in.

/// Longest address handed to the platform; longer is a mistake, not a link.
pub const MAX_URL: usize = 2048;

/// Whether an address is one this app will hand to the desktop.
///
/// Web links and addresses are allowed. A scheme from Discord's own launcher set is allowed
/// because that is what a game shortcut needs, and nothing else is: a `file:` address would
/// open local files, and anything unknown is a setting typo at best.
pub fn allowed(url: &str) -> bool {
	let url = url.trim();
	if url.is_empty() || url.len() > MAX_URL || url.chars().any(char::is_control) {
		return false;
	}
	let Some((scheme, rest)) = url.split_once(':') else {
		return false;
	};
	if scheme.is_empty()
		|| !scheme.chars().all(|character| {
			character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
		}) {
		return false;
	}
	if rest.is_empty() {
		return false;
	}
	let scheme = scheme.to_ascii_lowercase();
	match scheme.as_str() {
		"http" | "https" | "mailto" => true,
		scheme if LAUNCHERS.contains(&scheme) => true,
		// Anything else must look like a reverse-domain scheme, which is how a launcher
		// identifies itself; a bare word is a typo rather than a handler.
		scheme => scheme.contains('.'),
	}
}

/// The launcher schemes TestCord's own game shortcuts use, each tied to a game it launches.
const LAUNCHERS: &[&str] = &[
	"com.epicgames.launcher",
	"com.valvesoftware.valvescopegames",
	"steam",
	"roblox",
	"minecraft",
	"com.blizzard.wtcg",
	"battlenet",
	"origin",
	"com.riotgames.league",
	"leagueoflegends",
	"spotify",
];

/// Open an address with the desktop's own handler.
///
/// Returns the address it refused, so a caller can show it rather than pretend it worked. This
/// is a local action on a local setting: nothing is fetched, and no shell parses the string.
pub fn open(url: &str) -> Result<(), String> {
	let url = url.trim();
	if !allowed(url) {
		return Err(url.to_owned());
	}
	native::open(url)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
mod native {
	pub fn open(url: &str) -> Result<(), String> {
		let mut command = std::process::Command::new("xdg-open");
		command.arg(url);
		match command.spawn() {
			Ok(_) => Ok(()),
			Err(_) => Err(url.to_owned()),
		}
	}
}

#[cfg(target_os = "windows")]
mod native {
	pub fn open(url: &str) -> Result<(), String> {
		// `ShellExecuteW` is reached through `cmd /c start` on the system's own path, so the
		// address is quoted rather than escaped and no second command can ride along.
		match std::process::Command::new("cmd")
			.args(["/c", "start", "", url])
			.spawn()
		{
			Ok(_) => Ok(()),
			Err(_) => Err(url.to_owned()),
		}
	}
}

#[cfg(target_os = "macos")]
mod native {
	pub fn open(url: &str) -> Result<(), String> {
		match std::process::Command::new("open").arg(url).spawn() {
			Ok(_) => Ok(()),
			Err(_) => Err(url.to_owned()),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_web_link_is_allowed() {
		assert!(allowed("https://example.com/x?y=1"));
		assert!(allowed("http://example.com"));
		assert!(allowed("mailto:someone@example.com"));
	}

	#[test]
	fn a_game_launcher_is_allowed() {
		assert!(allowed("com.epicgames.launcher://apps/anything"));
		assert!(allowed("steam://run/440"));
		assert!(
			allowed("STEAM://run/440"),
			"the scheme's case does not matter"
		);
	}

	#[test]
	fn local_files_and_unknown_schemes_are_refused() {
		assert!(!allowed("file:///etc/passwd"));
		assert!(!allowed("javascript:alert(1)"));
		assert!(!allowed("data:text/html,<script>"));
		assert!(!allowed("vbscript:msgbox"));
		assert!(!allowed("somethingelse://x"));
	}

	#[test]
	fn a_malformed_address_is_refused() {
		assert!(!allowed(""));
		assert!(!allowed("   "));
		assert!(!allowed("no-scheme"));
		assert!(!allowed("://missing"));
		assert!(!allowed("https:"));
		assert!(!allowed(&format!(
			"https://example.com/{}",
			"a".repeat(MAX_URL)
		)));
	}

	#[test]
	fn a_control_character_cannot_hide_in_an_address() {
		assert!(!allowed("https://example.com/\nrm -rf /"));
		assert!(!allowed("https://example.com/\0"));
	}

	#[test]
	fn a_refused_address_comes_back_so_it_can_be_shown() {
		assert_eq!(
			open("javascript:alert(1)"),
			Err("javascript:alert(1)".to_owned())
		);
	}
}
