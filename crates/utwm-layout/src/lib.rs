//! Where Rectangle's actions put a window, ported from Rectangle's Swift
//! (rxhanson/Rectangle@7c5c9c5, v2.0.4, MIT) to work in whole screen units:
//! physical pixels on Windows, points on the Mac.
//!
//! The crate is pure: it never touches a window. The platform layer asks
//! [`Engine::plan`] where an action goes, moves the window, and reports where it
//! landed with [`Engine::landed`], which is what makes the next press of the
//! same shortcut cycle instead of starting over.

pub mod align;
mod calc;
pub mod engine;
pub mod fill;
mod gaps;
pub mod geometry;
pub mod history;
pub mod screens;
pub mod spec;
pub mod sub_action;
pub mod tile;

pub use engine::{Engine, Move, Plan, Request};
pub use geometry::{Edges, PixelRect, Rect};
pub use history::{ActionKey, LastAction, WindowRecord};
pub use screens::{Screen, UsableScreens};
pub use sub_action::SubAction;
