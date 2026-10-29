//! The macOS side of the window manager: global shortcuts registered the way
//! Rectangle registers them (Carbon hot keys), and the code that finds,
//! measures and moves other apps' windows through the Accessibility API, as
//! Rectangle does.
//!
//! Like `utwm-windows`, every function here is a thin, checked wrapper; the
//! decisions live in `utwm-config` (which shortcut means what) and
//! `utwm-layout` (where a window goes). The Accessibility API measures in
//! points from the top left of the main display with y growing down, the
//! convention the layout engine already uses, so frames pass through as they
//! are.
#![cfg(target_os = "macos")]

pub mod app;
pub mod autostart;
mod ax;
pub mod conflicts;
pub mod control;
pub mod hotkeys;
pub mod monitors;
pub mod permission;
pub mod window;

pub use window::Window;
