//! universal-tiling-window-manager: a tiling window manager you drive from the
//! keyboard, on Windows, the Mac and Linux, with one configuration for all
//! three.
//!
//! With no arguments it runs in the tray (the menu bar on the Mac). The
//! subcommands are for the command line: `check` shows what the configuration
//! binds, `probe` shows what the keyboard delivers (Windows), and `quit`,
//! `reload`, `pause` and `update` talk to the running instance.

// Release builds have no console window; `check` and `probe` attach to the
// terminal that launched them instead.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cli;

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
mod app;
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
mod controller;
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

fn main() -> ExitCode {
    let invocation = match cli::parse(std::env::args().skip(1)) {
        Ok(invocation) => invocation,
        Err(message) => {
            cli::print_to_terminal(&format!("{message}\n\n{}", cli::USAGE));
            return ExitCode::from(2);
        }
    };
    cli::run(invocation)
}
