//! universal-tiling-window-manager: a tiling window manager you drive from the
//! keyboard, on Windows, the Mac and Linux, with one configuration for all
//! three.
//!
//! With no arguments it runs in the tray (the menu bar on the Mac). The
//! subcommands are for the command line: `check` shows what the configuration
//! binds, `probe` shows what the keyboard delivers (Windows), `install` puts a
//! download in place (Linux), and `quit`, `reload`, `pause` and `update` talk
//! to the running instance.
//!
//! Two executables start it. `universal-tiling-window-manager` is the command
//! line on every platform. A Windows release adds `Window Manager.exe`, the
//! tray app with UI Access, and there the command line run with no command
//! starts that copy instead of running the tray itself.

mod cli;

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
mod app;
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
mod controller;
#[cfg(windows)]
mod installed;
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
mod logging;
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
mod platform;
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
mod poll;
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
mod tray;
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
mod updates;

use std::process::ExitCode;

/// Which executable is running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Executable {
    /// `universal-tiling-window-manager`, the command line.
    CommandLine,
    /// `Window Manager.exe`, a Windows release's tray app.
    Tray,
}

/// Does what the arguments ask for.
pub fn main(executable: Executable) -> ExitCode {
    let invocation = match cli::parse(std::env::args().skip(1)) {
        Ok(invocation) => invocation,
        Err(message) => {
            cli::print_to_terminal(&format!("{message}\n\n{}", cli::USAGE));
            return ExitCode::from(2);
        }
    };
    cli::run(invocation, executable)
}
