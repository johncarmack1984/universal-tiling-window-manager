//! The platform crate this build drives. `utwm-windows`, `utwm-macos` and
//! `utwm-linux` expose the same names for what the tray app needs (a window
//! handle, the window functions, the displays, start at login, the shortcut
//! listener and the command line's requests), so the rest of the app is
//! written once.

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

#[cfg(target_os = "linux")]
pub use utwm_linux::control::{Control, send_control};
#[cfg(target_os = "linux")]
pub use utwm_linux::shortcuts::Shortcuts;
#[cfg(target_os = "linux")]
pub use utwm_linux::{Window, autostart, monitors, window};

/// The icon at each size the Linux install puts one, the same files
/// `just install` stages.
#[cfg(target_os = "linux")]
pub const ICONS: utwm_linux::install::Icons<'static> = &[
    ("32x32", include_bytes!("../icons/32x32.png")),
    ("128x128", include_bytes!("../icons/128x128.png")),
    ("256x256", include_bytes!("../icons/icon.png")),
];

/// Shows a folder in Explorer, the Finder, or the desktop's file manager.
pub fn open_folder(folder: &Path) -> std::io::Result<()> {
    let program = if cfg!(windows) {
        "explorer.exe"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program)
        .arg(folder)
        .spawn()
        .map(drop)
}
