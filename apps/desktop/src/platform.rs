//! The platform crate this build drives. `utwm-windows` and `utwm-macos`
//! expose the same names for what the tray app needs (a window handle, the
//! window functions, the displays, start at login, the shortcut listener and
//! the command line's requests), so the rest of the app is written once.

use std::path::Path;

#[cfg(windows)]
pub use utwm_windows::events::{Control, send_control};
#[cfg(windows)]
pub use utwm_windows::hook::Hook as Shortcuts;
#[cfg(windows)]
pub use utwm_windows::{Hwnd as Window, autostart, monitors, window};

#[cfg(target_os = "macos")]
pub use utwm_macos::control::{Control, send_control};
#[cfg(target_os = "macos")]
pub use utwm_macos::hotkeys::Hotkeys as Shortcuts;
#[cfg(target_os = "macos")]
pub use utwm_macos::{Window, autostart, monitors, window};

/// Shows a folder in Explorer, or in the Finder.
pub fn open_folder(folder: &Path) -> std::io::Result<()> {
    let program = if cfg!(windows) {
        "explorer.exe"
    } else {
        "open"
    };
    std::process::Command::new(program)
        .arg(folder)
        .spawn()
        .map(drop)
}
