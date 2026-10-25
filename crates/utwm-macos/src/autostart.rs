//! Start at login, per user. Run from its app bundle, the app registers itself
//! as a login item (`SMAppService`), which System Settings → General → Login
//! Items lists by name under Open at Login. Run without a bundle, from `cargo
//! run` say, it falls back to a LaunchAgent in `~/Library/LaunchAgents`, listed
//! under Allow in the Background. Only the menu turns either on; nothing
//! installs them behind the user's back.

use std::path::{Path, PathBuf};

use objc2::rc::Retained;
use objc2_foundation::NSBundle;
use objc2_service_management::{SMAppService, SMAppServiceStatus};

/// The bundle identifier, which also labels the LaunchAgent.
pub const LABEL: &str = "com.johncarmack.universal-tiling-window-manager";

/// This app's own login item, when it runs from its bundle.
fn login_item() -> Option<Retained<SMAppService>> {
    let bundled = NSBundle::mainBundle()
        .bundleIdentifier()
        .is_some_and(|id| id.to_string() == LABEL);
    // SAFETY: asks for the main app's service object; no arguments.
    bundled.then(|| unsafe { SMAppService::mainAppService() })
}

fn plist_path() -> Option<PathBuf> {
    std::env::home_dir().map(|home| {
        home.join("Library")
            .join("LaunchAgents")
            .join(format!("{LABEL}.plist"))
    })
}

/// Whether the app starts at login.
pub fn is_enabled() -> bool {
    match login_item() {
        Some(item) => {
            // SAFETY: reads the service's status; no arguments.
            let status = unsafe { item.status() };
            status == SMAppServiceStatus::Enabled
        }
        None => plist_path().is_some_and(|path| path.is_file()),
    }
}

/// Turns start at login on for `exe`, or off. Takes effect at the next login;
/// the running copy carries on either way.
pub fn set_enabled(enabled: bool, exe: &Path) -> std::io::Result<()> {
    if let Some(item) = login_item() {
        // An agent left from running unbundled would start a second copy.
        set_launch_agent(false, exe)?;
        // SAFETY: plain calls on the main app's service object.
        let result = unsafe {
            if enabled {
                item.registerAndReturnError()
            } else {
                item.unregisterAndReturnError()
            }
        };
        return result.map_err(|error| {
            // Switched off in System Settings, it can only be switched back on there.
            // SAFETY: reads the status and opens System Settings; no arguments.
            unsafe {
                if enabled && item.status() == SMAppServiceStatus::RequiresApproval {
                    SMAppService::openSystemSettingsLoginItems();
                }
            }
            std::io::Error::other(error.localizedDescription().to_string())
        });
    }
    set_launch_agent(enabled, exe)
}

fn set_launch_agent(enabled: bool, exe: &Path) -> std::io::Result<()> {
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
