//! Which machine the bindings are for.
//!
//! The same configuration serves all three. On Windows and Linux each Mac
//! shortcut becomes the PC key in the same position; on the Mac it stays
//! exactly as the configuration has it.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Mac,
    Linux,
}

impl Platform {
    /// The platform this build runs on. Anything that is neither a Mac nor
    /// Linux reads exports the Windows way.
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Windows
        }
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Windows => "Windows",
            Self::Mac => "the Mac",
            Self::Linux => "Linux",
        })
    }
}
