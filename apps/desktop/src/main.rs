//! universal-tiling-window-manager: Rectangle Pro's keyboard shortcuts on
//! Windows, configured by the Mac's own settings export.
//!
//! With no arguments it runs in the tray. The subcommands are for the command
//! line: `check` shows what an export binds, `probe` shows what the keyboard
//! delivers, and `quit`, `reload` and `pause` talk to the running instance.

// Release builds have no console window; `check` and `probe` attach to the
// terminal that launched them instead.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cli;

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod controller;
#[cfg(windows)]
mod logging;
#[cfg(windows)]
mod poll;
#[cfg(windows)]
mod tray;

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
