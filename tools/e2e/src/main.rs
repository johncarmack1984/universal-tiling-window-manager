//! `cargo run -p utwm-e2e`, or `just e2e`: the end-to-end check against the
//! running app on this platform. It opens a window of its own, presses
//! shortcuts the way the keyboard does, and checks where the window lands. It
//! never touches any other window.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

use std::process::ExitCode;

#[cfg(windows)]
fn main() -> ExitCode {
    match windows::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    macos::run()
}

#[cfg(not(any(windows, target_os = "macos")))]
fn main() -> ExitCode {
    eprintln!("the end-to-end check runs on Windows and the Mac");
    ExitCode::FAILURE
}
