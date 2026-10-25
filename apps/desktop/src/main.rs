//! universal-tiling-window-manager: Rectangle Pro's keyboard shortcuts on
//! Windows and the Mac, configured by Rectangle Pro's own settings export.
//!
//! With no arguments it runs in the tray (the menu bar on the Mac). The
//! subcommands are for the command line: `check` shows what an export binds,
//! `probe` shows what the keyboard delivers (Windows), and `quit`, `reload` and
//! `pause` talk to the running instance.

// Release builds have no console window; `check` and `probe` attach to the
// terminal that launched them instead.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cli;

#[cfg(any(windows, target_os = "macos"))]
mod app;
#[cfg(any(windows, target_os = "macos"))]
mod controller;
#[cfg(any(windows, target_os = "macos"))]
mod logging;
#[cfg(any(windows, target_os = "macos"))]
mod platform;
#[cfg(any(windows, target_os = "macos"))]
mod poll;
#[cfg(any(windows, target_os = "macos"))]
mod tray;
#[cfg(any(windows, target_os = "macos"))]
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
