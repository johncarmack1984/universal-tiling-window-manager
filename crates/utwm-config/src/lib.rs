//! The configuration, resolved into key bindings for Windows, the Mac or
//! Linux: the default shortcuts or an imported Rectangle Pro config, with
//! `config.toml` on top.
//!
//! The same files serve every machine, and the dotfiles carry them around. On
//! Windows and Linux each macOS key code becomes the PC key in the same
//! physical position; on the Mac the shortcuts stay as they are. The result
//! goes to the platform's shortcut listener and the window engine.

pub mod actions;
pub mod bindings;
pub mod chord;
pub mod export;
pub mod keys;
pub mod load;
pub mod matcher;
pub mod overrides;
pub mod platform;
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
pub use platform::Platform;
pub use settings::Settings;
pub use specs::{Anchor, Dimension, ManualSpec, Placement};
