//! Which machine the bindings are for.
//!
//! The same configuration serves both. On Windows each Mac shortcut becomes
//! the Windows key in the same position; on the Mac it stays exactly as the
//! configuration has it.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Mac,
}

impl Platform {
    /// The platform this build runs on. Anything that isn't a Mac reads
    /// exports the Windows way, which is also what `check` shows on Linux.
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
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
        })
    }
}
