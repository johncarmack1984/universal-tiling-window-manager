//! Start at login, per user: a LaunchAgent in `~/Library/LaunchAgents` that
//! runs this executable when the user logs in. System Settings → General →
//! Login Items lists it under "Allow in the Background", where it can be
//! switched off too. Only the menu turns this on; nothing installs it behind
//! the user's back.

use std::path::{Path, PathBuf};

pub const LABEL: &str = "com.johncarmack.universal-tiling-window-manager";

fn plist_path() -> Option<PathBuf> {
    std::env::home_dir().map(|home| {
        home.join("Library")
            .join("LaunchAgents")
            .join(format!("{LABEL}.plist"))
    })
}

/// Whether the app starts at login.
pub fn is_enabled() -> bool {
    plist_path().is_some_and(|path| path.is_file())
}

/// Turns start at login on for `exe`, or off. Takes effect at the next login;
/// the running copy carries on either way.
pub fn set_enabled(enabled: bool, exe: &Path) -> std::io::Result<()> {
    let path = plist_path().ok_or_else(|| std::io::Error::other("no home directory"))?;
    if enabled {
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        std::fs::write(&path, plist(exe))
    } else {
        match std::fs::remove_file(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

/// The agent: run once at login, in the user's graphical session, with the
/// priority of an app the user is working with. Not kept alive, so Quit in
/// the menu stays quit.
fn plist(exe: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{LABEL}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{}</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>LimitLoadToSessionType</key>
	<string>Aqua</string>
	<key>ProcessType</key>
	<string>Interactive</string>
</dict>
</plist>
"#,
        escape(&exe.display().to_string())
    )
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agent_runs_the_executable_at_login() {
        let plist = plist(Path::new(
            "/Users/me/.cargo/bin/universal-tiling-window-manager",
        ));
        assert!(
            plist.contains("<string>/Users/me/.cargo/bin/universal-tiling-window-manager</string>")
        );
        assert!(plist.contains("<key>RunAtLoad</key>\n\t<true/>"));
        assert!(!plist.contains("KeepAlive"));
    }

    #[test]
    fn paths_are_escaped() {
        let plist = plist(Path::new("/Users/me/R&D <tools>/utwm"));
        assert!(plist.contains("<string>/Users/me/R&amp;D &lt;tools&gt;/utwm</string>"));
    }
}
