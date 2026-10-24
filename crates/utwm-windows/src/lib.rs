//! The Win32 side of the window manager: a low-level keyboard hook that matches
//! physical keys, and the code that finds, measures and moves other apps'
//! windows the way PowerToys FancyZones does (MIT; DWM frame compensation,
//! applying placement twice across DPI changes, restoring maximized windows
//! through their placement).
//!
//! Every function here is a thin, checked wrapper over Win32; the decisions live
//! in `utwm-config` (which key means what) and `utwm-layout` (where a window
//! goes).
#![cfg(windows)]

pub mod autostart;
pub mod console;
pub mod events;
pub mod hook;
pub mod inject;
pub mod monitors;
pub mod window;

pub use window::Hwnd;

use utwm_layout::PixelRect;
use windows_sys::Win32::Foundation::RECT;

fn to_pixel_rect(rect: RECT) -> PixelRect {
    PixelRect::new(rect.left, rect.top, rect.right, rect.bottom)
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
