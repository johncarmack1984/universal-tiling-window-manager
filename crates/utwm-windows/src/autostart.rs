//! Start at login, per user: a value under
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, which is what Task
//! Manager's Startup apps page lists and toggles. Only the tray menu turns this
//! on; nothing installs it behind the user's back.

use std::path::Path;

use crate::registry::{self, HKEY_CURRENT_USER};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// Task Manager records "disabled" here without touching the Run value.
const APPROVED_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
pub const VALUE_NAME: &str = "universal-tiling-window-manager";

/// Whether the app starts at login: listed under Run and not disabled in Task
/// Manager.
pub fn is_enabled() -> bool {
    let listed = command().is_some();
    // The first byte of the StartupApproved entry is odd when the user disabled it.
    let disabled = registry::read_binary(HKEY_CURRENT_USER, APPROVED_KEY, VALUE_NAME)
        .and_then(|bytes| bytes.first().copied())
        .is_some_and(|flag| flag & 1 == 1);
    listed && !disabled
}

/// What Start at login runs, whether or not Task Manager disabled it.
pub fn command() -> Option<String> {
    registry::read_string(HKEY_CURRENT_USER, RUN_KEY, VALUE_NAME)
}

/// Turns start at login on for `exe`, or off.
pub fn set_enabled(enabled: bool, exe: &Path) -> std::io::Result<()> {
    if enabled {
        registry::write_string(HKEY_CURRENT_USER, RUN_KEY, VALUE_NAME, &quoted(exe))?;
        // Clear a "disabled" left by Task Manager so the entry really runs.
        let _ = registry::delete_value(HKEY_CURRENT_USER, APPROVED_KEY, VALUE_NAME);
        Ok(())
    } else {
        registry::delete_value(HKEY_CURRENT_USER, RUN_KEY, VALUE_NAME)
    }
}

/// Points Start at login at `exe`, keeping it disabled if Task Manager
/// disabled it.
pub fn repoint(exe: &Path) -> std::io::Result<()> {
    registry::write_string(HKEY_CURRENT_USER, RUN_KEY, VALUE_NAME, &quoted(exe))
}

fn quoted(exe: &Path) -> String {
    format!("\"{}\"", exe.display())
}

/// The executable a Run command starts: the quoted path it begins with, or
/// its first word when it isn't quoted.
pub fn executable(command: &str) -> &str {
    let command = command.trim_start();
    if let Some(rest) = command.strip_prefix('"') {
        return rest.split('"').next().unwrap_or(rest);
    }
    command.split_whitespace().next().unwrap_or(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_command_names_its_executable() {
        assert_eq!(
            executable(r#""C:\Program Files\Window Manager\universal-tiling-window-manager.exe""#),
            r"C:\Program Files\Window Manager\universal-tiling-window-manager.exe"
        );
        assert_eq!(
            executable(r#""C:\a b\app.exe" --config "x y.json""#),
            r"C:\a b\app.exe"
        );
        assert_eq!(executable(r"C:\tools\app.exe --flag"), r"C:\tools\app.exe");
        assert_eq!(executable(""), "");
    }

    #[test]
    fn what_start_at_login_writes_reads_back_as_the_same_executable() {
        let exe = Path::new(r"C:\Program Files\Window Manager\universal-tiling-window-manager.exe");
        assert_eq!(Path::new(executable(&quoted(exe))), exe);
    }
}
