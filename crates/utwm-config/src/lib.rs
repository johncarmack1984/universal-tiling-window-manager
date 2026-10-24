//! Rectangle and Rectangle Pro settings exports, resolved into Windows key
//! bindings.
//!
//! The Mac stays the place shortcuts are edited: Rectangle Pro writes
//! `RectangleProConfig.json`, the dotfiles carry it to Windows, and this crate
//! reads it, translates each macOS key code to the Windows key in the same
//! physical position, and hands the result to the keyboard hook and the window
//! engine.

pub mod actions;
pub mod bindings;
pub mod chord;
pub mod export;
pub mod keys;
pub mod load;
pub mod matcher;
pub mod overrides;
pub mod report;
pub mod settings;
pub mod specs;

pub use actions::{ActionId, MenuGroup};
pub use bindings::{Binding, Resolved, SkipReason, Skipped, Source, Target};
pub use chord::{Chord, MacChord, ModifierMap, Modifiers};
pub use export::{Export, ExportKind};
pub use keys::PhysicalKey;
pub use load::{LoadError, load};
pub use overrides::Overrides;
pub use settings::Settings;
pub use specs::{Anchor, Dimension, ManualSpec, Placement};
