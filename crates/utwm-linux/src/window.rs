//! Finding, measuring and moving other apps' windows, through the bridge.
//!
//! A [`Window`] is plain data, Mutter's id for the window, which GNOME never
//! reuses within a session, so it crosses threads and keys the engine's
//! history the way an HWND does on Windows. Frames are what Mutter calls the
//! frame rect: what you see of the window, without the shadow a client-side
//! decorated app draws around it.

use std::fmt;

use utwm_layout::{PixelRect, Screen};

use crate::shell::{self, CallError, Facts, Frame};

/// A window of another app.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Window {
    /// Mutter's id for it (`Meta.Window.get_id`).
    pub id: u64,
}

/// What the engine keeps history under. Mutter never reuses a window id, so
/// the window itself tells windows apart.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub window: Window,
}

impl Key {
    pub fn of(window: Window) -> Self {
        Self { window }
    }
}

// The `Meta.WindowType`s this app acts on: ordinary windows, dialogs that
// stand on their own, and utility windows. Menus, tooltips, docks and the
// desktop are left alone.
const NORMAL: u32 = 0;
const DIALOG: u32 = 3;
const MODAL_DIALOG: u32 = 4;
const UTILITY: u32 = 7;

fn own_pid() -> i32 {
    i32::try_from(std::process::id()).unwrap_or(-1)
}

/// Whether a window with these facts is one this app may act on: a window
/// on the active workspace that isn't minimized, in full screen, attached to
/// another window, kept out of the taskbar, or this app's own.
fn manageable(facts: &Facts) -> bool {
    matches!(facts.window_type, NORMAL | DIALOG | MODAL_DIALOG | UTILITY)
        && !facts.attached_dialog
        && !facts.override_redirect
        && !facts.minimized
        && !facts.fullscreen
        && !facts.skip_taskbar
        && facts.on_active_workspace
        // A maximized window may say it can't move; placing it restores it first.
        && (facts.allows_move || facts.maximized != 0)
        // An X11 window that doesn't say whose it is (-1) is never ours.
        && (facts.pid < 0 || facts.pid != own_pid())
}

fn describe(window: Window) -> Option<Facts> {
    shell::describe(window.id).ok()
}

fn frame_rect((x, y, width, height): Frame) -> PixelRect {
    PixelRect::from_xywh(x, y, width, height)
}

/// The window keyboard actions apply to: the focused window (or the window a
/// focused dialog is attached to), if this app may move it.
pub fn foreground() -> Option<Window> {
    let id = shell::focused().ok().filter(|id| *id != 0)?;
    let window = Window { id };
    is_manageable(window).then_some(window)
}

/// The focused window, whatever this app would make of it. For the
/// end-to-end check, whose own window the app leaves alone.
pub fn focused() -> Option<Window> {
    shell::focused()
        .ok()
        .filter(|id| *id != 0)
        .map(|id| Window { id })
}

/// Whether this app may act on the window.
pub fn is_manageable(window: Window) -> bool {
    describe(window).is_some_and(|facts| manageable(&facts))
}

/// Whether the window's size can be changed.
pub fn is_resizable(window: Window) -> bool {
    describe(window).is_some_and(|facts| facts.allows_resize)
}

/// Whether the window is maximized, in either direction. GNOME's own
/// half-screen tiling counts: Mutter keeps such a window maximized
/// vertically, and reports it as not resizable until it is restored.
pub fn is_maximized(window: Window) -> bool {
    describe(window).is_some_and(|facts| facts.maximized != 0)
}

/// The window's frame, in GNOME's coordinates.
pub fn visible_frame(window: Window) -> Option<PixelRect> {
    describe(window).map(|facts| frame_rect(facts.frame))
}

#[derive(Debug, PartialEq, Eq)]
pub enum PlaceError {
    /// The GNOME Shell extension isn't running.
    Unavailable,
    /// GNOME Shell refused or failed; the text says why.
    Failed(String),
    Gone,
}

impl From<CallError> for PlaceError {
    fn from(error: CallError) -> Self {
        match error {
            CallError::Unavailable => Self::Unavailable,
            CallError::Gone => Self::Gone,
            CallError::Failed(why) => Self::Failed(why),
        }
    }
}

impl fmt::Display for PlaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => f.write_str("the GNOME Shell extension isn't running"),
            Self::Failed(why) => write!(f, "GNOME Shell refused: {why}"),
            Self::Gone => f.write_str("the window no longer exists"),
        }
    }
}

/// Gives the window the frame `target` and returns the frame it ended up
/// with, which can differ when the app enforces its own size limits. The
/// extension restores a maximized window first, and answers once the app has
/// redrawn at its new size. A window sent to another display is raised there.
pub fn place(
    window: Window,
    target: PixelRect,
    across_displays: bool,
    _screens: &[Screen],
) -> Result<PixelRect, PlaceError> {
    let frame = (target.left, target.top, target.width(), target.height());
    Ok(frame_rect(shell::place(window.id, frame, across_displays)?))
}

/// Moves the window so its frame starts at (`left`, `top`), keeping its size.
pub fn move_to(window: Window, left: i32, top: i32) -> Result<PixelRect, PlaceError> {
    Ok(frame_rect(shell::move_to(window.id, left, top)?))
}

/// Every manageable window on the active workspace, front to back.
pub fn manageable_windows() -> Vec<Window> {
    shell::windows()
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, facts)| manageable(facts))
        .map(|(id, _)| Window { id })
        .collect()
}

/// What `ignored_apps` names the window's app by: GNOME's id for the app,
/// which is its desktop file's name (`org.gnome.Terminal`), or, for a window
/// GNOME can't tie to an app, its sandbox's app id, its GTK application id or
/// its window class.
pub fn app_id(window: Window) -> Option<String> {
    describe(window).and_then(|facts| app_id_of(&facts))
}

fn app_id_of(facts: &Facts) -> Option<String> {
    // GNOME names an app it made up from a lone window `window:N`, with no
    // `.desktop` to strip.
    if let Some(desktop) = facts.app_id.strip_suffix(".desktop")
        && !desktop.is_empty()
    {
        return Some(desktop.to_owned());
    }
    [
        &facts.sandboxed_app_id,
        &facts.gtk_application_id,
        &facts.wm_class,
    ]
    .into_iter()
    .find(|id| !id.is_empty())
    .cloned()
}

/// The windows of process `pid` on the active workspace, front to back,
/// whatever this app would make of them. For the end-to-end check, which looks
/// for its own window.
pub fn windows_of(pid: u32) -> Vec<Window> {
    let pid = i32::try_from(pid).unwrap_or(-1);
    shell::windows()
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, facts)| facts.pid == pid)
        .map(|(id, _)| Window { id })
        .collect()
}

/// Gives the window the focus, as a click on it would. For the end-to-end
/// check: under Wayland an app can't focus its own window.
pub fn activate(window: Window) -> Result<(), PlaceError> {
    Ok(shell::activate(window.id)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terminal() -> Facts {
        Facts {
            frame: (0, 32, 800, 600),
            pid: 4242,
            window_type: NORMAL,
            allows_move: true,
            allows_resize: true,
            on_active_workspace: true,
            app_id: "org.gnome.Terminal.desktop".to_owned(),
            gtk_application_id: "org.gnome.Terminal".to_owned(),
            wm_class: "gnome-terminal-server".to_owned(),
            ..Facts::default()
        }
    }

    #[test]
    fn ordinary_windows_are_manageable() {
        assert!(manageable(&terminal()));
        let dialog = Facts {
            window_type: DIALOG,
            ..terminal()
        };
        assert!(manageable(&dialog));
        let maximized = Facts {
            allows_move: false,
            maximized: 3,
            ..terminal()
        };
        assert!(manageable(&maximized), "placing it restores it first");
        let x11_without_pid = Facts {
            pid: -1,
            ..terminal()
        };
        assert!(manageable(&x11_without_pid));
    }

    #[test]
    fn some_windows_are_left_alone() {
        for facts in [
            Facts {
                window_type: 1, // the desktop
                ..terminal()
            },
            Facts {
                attached_dialog: true,
                ..terminal()
            },
            Facts {
                minimized: true,
                ..terminal()
            },
            Facts {
                fullscreen: true,
                ..terminal()
            },
            Facts {
                skip_taskbar: true,
                ..terminal()
            },
            Facts {
                on_active_workspace: false,
                ..terminal()
            },
            Facts {
                allows_move: false,
                ..terminal()
            },
            Facts {
                pid: own_pid(),
                ..terminal()
            },
        ] {
            assert!(!manageable(&facts), "{facts:?}");
        }
    }

    #[test]
    fn apps_go_by_their_desktop_id() {
        assert_eq!(
            app_id_of(&terminal()).as_deref(),
            Some("org.gnome.Terminal")
        );
        let snap = Facts {
            app_id: "firefox_firefox.desktop".to_owned(),
            ..terminal()
        };
        assert_eq!(app_id_of(&snap).as_deref(), Some("firefox_firefox"));
        let lone = Facts {
            app_id: "window:12".to_owned(),
            sandboxed_app_id: String::new(),
            gtk_application_id: String::new(),
            wm_class: "xterm".to_owned(),
            ..terminal()
        };
        assert_eq!(app_id_of(&lone).as_deref(), Some("xterm"));
    }
}
