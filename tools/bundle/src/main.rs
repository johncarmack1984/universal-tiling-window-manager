//! `cargo run -p utwm-bundle -- [--install] [--universal]`, or `just bundle`
//! and `just install`
//!
//! Builds the release executable and wraps it in a Mac app bundle, so macOS
//! deals with an app rather than a bare executable: Accessibility access and
//! the login item go to it by name, Finder and Spotlight open it, and it lives
//! in the menu bar with no Dock icon. With `--install` it replaces the copy in
//! /Applications, quitting it first, and starts the new one. With
//! `--universal` the executable holds both Apple silicon and Intel code, as a
//! release ships it.
//!
//! It signs with the code-signing certificate `UTWM_SIGN_IDENTITY` names, or
//! ad hoc without one, and always with the hardened runtime that notarization
//! asks for. macOS ties Accessibility access to the signature, and an ad-hoc
//! signature changes with every build, so an ad-hoc install clears the old
//! grant and the app asks again; any certificate, a self-signed one included,
//! keeps the grant across installs.

#[cfg(target_os = "macos")]
mod mac;

use std::process::ExitCode;

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    match mac::run(std::env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("utwm-bundle: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    eprintln!("utwm-bundle builds the Mac app; on Windows, `just install` installs the executable");
    ExitCode::FAILURE
}
