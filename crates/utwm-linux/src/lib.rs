//! The Linux side of the window manager, for GNOME on Wayland.
//!
//! Wayland lets no ordinary app move another app's windows or grab keys of its
//! own, so this crate drives a small GNOME Shell extension
//! (`gnome-extension/`) over the session bus. The extension reports the
//! displays and windows and carries out moves and key grabs; deciding which
//! windows to manage stays here.
//!
//! Like `utwm-windows` and `utwm-macos`, every function here is a thin, checked
//! wrapper; the other decisions live in `utwm-config` (which shortcut means
//! what) and `utwm-layout` (where a window goes). GNOME measures windows and
//! displays in one space, from the top left with y growing down, the
//! convention the layout engine already uses, so frames pass through as they
//! are.
#![cfg(target_os = "linux")]

pub mod autostart;
pub mod conflicts;
pub mod control;
pub mod extension;
mod gsettings;
pub mod monitors;
pub mod notify;
mod shell;
pub mod shortcuts;
pub mod tray_host;
pub mod window;

use std::path::PathBuf;

pub use window::Window;

/// The app's identifier, which names its desktop entry, its icon and its
/// autostart entry, as it names the Mac's bundle.
pub const APP_ID: &str = "com.johncarmack.universal-tiling-window-manager";

/// `$XDG_DATA_HOME`, or `~/.local/share`: where GNOME looks for this user's
/// extensions, desktop entries and icons.
pub fn data_home() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::home_dir().map(|home| home.join(".local").join("share")))
}
