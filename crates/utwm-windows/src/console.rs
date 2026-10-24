//! Console output for a GUI-subsystem executable.
//!
//! The release build has no console of its own, so `check` and `probe` print
//! by attaching to the terminal that launched them. When output is already
//! redirected (to a file or a pipe) the inherited handle is used as is.

use windows_sys::Win32::System::Console::{
    ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_OUTPUT_HANDLE,
};

/// Makes `println!` reach the launching terminal. Returns false when there is
/// nowhere to print.
pub fn attach() -> bool {
    // SAFETY: queries a standard handle; no memory involved.
    let current = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    if !current.is_null() && current as isize != -1 {
        return true;
    }
    // SAFETY: attaches to the parent's console if it has one.
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) != 0 }
}
