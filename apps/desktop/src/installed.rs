//! A Windows release: the installer puts two executables in one folder under
//! Program Files, for every user. `universal-tiling-window-manager.exe` is the
//! command line. `Window Manager.exe` is the tray app, with UI Access in its
//! manifest, so that started by an administrator it runs at High integrity
//! and moves the administrator's elevated windows without being elevated
//! itself.
//!
//! Windows starts a UI Access executable only through the shell, never
//! through CreateProcess, which is how terminals start programs, and the
//! Start menu, Start at login, the installer's last step and an update's
//! restart all run the command line, which starts the tray app that way.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use utwm_windows::{autostart, install, launch, token};

use crate::Executable;
use crate::cli::print_to_terminal;

/// The tray app's executable, as the installer names it.
pub const TRAY: &str = "Window Manager.exe";
/// The command line's executable.
pub const COMMAND_LINE: &str = "universal-tiling-window-manager.exe";

/// A release's tray app beside this executable, the command line.
pub fn tray_beside_this() -> Option<PathBuf> {
    let tray = std::env::current_exe().ok()?.with_file_name(TRAY);
    tray.is_file().then_some(tray)
}

/// Starts a release's tray app, `tray`, as the desktop's user, and leaves it
/// running; `--config` goes along when one was named.
pub fn start_tray(tray: &Path, config: Option<&Path>) -> ExitCode {
    let parameters = config
        .map(|config| format!("--config \"{}\"", config.display()))
        .unwrap_or_default();
    if as_shell_user(token::runs_as_administrator(), launch::shell_is_elevated()) {
        return start_as_shell_user(&parameters);
    }
    match launch::shell_open(tray, &parameters) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            print_to_terminal(&format!("could not start {}: {error}", tray.display()));
            ExitCode::FAILURE
        }
    }
}

/// Whether the tray app starts by way of the desktop's user. The shell
/// starts a UI Access executable with the token of whoever asks, so from a
/// process with an administrator's rights, an elevated terminal, it would run
/// with them, and so would every installer its updates run, without asking;
/// there this command line runs again as the user the desktop shell runs as,
/// unless that shell is elevated too, as with UAC off, where there is no one
/// else to ask as.
fn as_shell_user(administrator: bool, shell_elevated: Option<bool>) -> bool {
    administrator && shell_elevated == Some(false)
}

/// Runs this command line again as the desktop's user, with `parameters` and
/// no command, which starts the tray app from there.
fn start_as_shell_user(parameters: &str) -> ExitCode {
    let started = std::env::current_exe()
        .and_then(|exe| launch::run_as_shell_user(&exe, parameters, Duration::from_secs(15)));
    match started {
        Ok(Some(0) | None) => ExitCode::SUCCESS,
        Ok(Some(code)) => {
            print_to_terminal(&format!(
                "could not start Window Manager (exit code {code})"
            ));
            ExitCode::FAILURE
        }
        Err(error) => {
            print_to_terminal(&format!(
                "could not start Window Manager as the desktop's user: {error}; start it from the Start menu"
            ));
            ExitCode::FAILURE
        }
    }
}

/// What Start at login runs for this copy: for a release's tray app, the
/// command line beside it, which starts the tray app through the shell;
/// otherwise this executable itself.
pub fn start_at_login_target(exe: &Path, executable: Executable) -> PathBuf {
    let command_line = exe.with_file_name(COMMAND_LINE);
    if executable == Executable::Tray && command_line.is_file() {
        command_line
    } else {
        exe.to_path_buf()
    }
}

/// Moves Start at login over from a copy an earlier release installed for
/// this user alone, which a release for every user leaves behind when
/// someone else's administrator account approved its installer: left alone,
/// that copy would start at login and install this release again. The
/// installer does this itself, and the rest of the move, for the account
/// that approved it.
pub fn adopt_start_at_login(exe: &Path) {
    let Some(command) = autostart::command() else {
        return;
    };
    let per_user = install::for_this_user().or_else(|| {
        std::env::var_os("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join("Window Manager"))
    });
    let Some(per_user) = per_user else {
        return;
    };
    if !starts_the_copy_in(&command, &per_user) {
        return;
    }
    let target = exe.with_file_name(COMMAND_LINE);
    match autostart::repoint(&target) {
        Ok(()) => tracing::info!(
            from = %command,
            to = %target.display(),
            "Start at login now starts this copy instead of the one installed for this user alone"
        ),
        Err(error) => tracing::warn!(%error, "could not move Start at login over to this copy"),
    }
}

/// Whether a Start at login command runs the command line of the copy in
/// `folder`.
fn starts_the_copy_in(command: &str, folder: &Path) -> bool {
    install::same_path(
        Path::new(autostart::executable(command)),
        &folder.join(COMMAND_LINE),
    )
}

/// Runs a release's installer, `bytes`, whose signature the updater has
/// checked. It asks for an administrator, as installing for every user
/// needs, and once it has replaced this copy it starts the command line,
/// which starts the new tray app. Returns once the installer has started, or
/// the prompt was declined, which [`declined`] tells apart.
pub fn run_installer(bytes: &[u8], version: &str) -> std::io::Result<()> {
    let temp = std::env::temp_dir();
    remove_earlier_downloads(&temp);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let folder = temp.join(format!(
        "{DOWNLOADS}{version}-{}-{nanos}",
        std::process::id()
    ));
    // Passive: a progress bar and nothing to click; it starts the app again
    // when it is done.
    launch::write_and_open(
        &folder,
        &format!("Window-Manager_{version}_x64-setup.exe"),
        bytes,
        "/P /UPDATE /R",
    )
}

/// Whether running an installer failed because its administrator prompt was
/// declined.
pub fn declined(error: &std::io::Error) -> bool {
    /// winerror.h's ERROR_CANCELLED.
    const CANCELLED: i32 = 1223;
    error.raw_os_error() == Some(CANCELLED)
}

/// What every update's download folder is called, before its version.
const DOWNLOADS: &str = "window-manager-update-";

/// Removes the folders earlier updates downloaded into: the installer can't
/// delete itself.
fn remove_earlier_downloads(temp: &Path) {
    let Ok(entries) = std::fs::read_dir(temp) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with(DOWNLOADS) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_elevated_terminal_under_an_ordinary_shell_starts_the_tray_as_the_shell_user() {
        assert!(as_shell_user(true, Some(false)));
        assert!(!as_shell_user(false, Some(false)));
        // UAC off: the shell is elevated too, and there is no one else.
        assert!(!as_shell_user(true, Some(true)));
        // No desktop, as over SSH.
        assert!(!as_shell_user(true, None));
    }

    #[test]
    fn start_at_login_runs_the_command_line_of_the_per_user_copy() {
        let per_user = Path::new(r"C:\Users\me\AppData\Local\Window Manager");
        assert!(starts_the_copy_in(
            r#""C:\Users\me\AppData\Local\Window Manager\universal-tiling-window-manager.exe""#,
            per_user
        ));
        assert!(starts_the_copy_in(
            r#""c:\users\me\appdata\local\window manager\UNIVERSAL-TILING-WINDOW-MANAGER.EXE""#,
            per_user
        ));
        assert!(!starts_the_copy_in(
            r#""C:\Program Files\Window Manager\universal-tiling-window-manager.exe""#,
            per_user
        ));
        assert!(!starts_the_copy_in(
            r#""C:\Users\me\.cargo\bin\universal-tiling-window-manager.exe""#,
            per_user
        ));
    }

    #[test]
    fn a_tray_app_starts_at_login_through_its_command_line() {
        let dir = std::env::temp_dir().join(format!("utwm-installed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("creates a folder");
        let tray = dir.join(TRAY);
        let command_line = dir.join(COMMAND_LINE);
        let without = start_at_login_target(&tray, Executable::Tray);
        std::fs::write(&command_line, b"").expect("writes a stand-in");
        let with = start_at_login_target(&tray, Executable::Tray);
        let from_command_line = start_at_login_target(&command_line, Executable::CommandLine);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(without, tray);
        assert_eq!(with, command_line);
        assert_eq!(from_command_line, command_line);
    }

    #[test]
    fn a_declined_prompt_is_told_apart() {
        assert!(declined(&std::io::Error::from_raw_os_error(1223)));
        assert!(!declined(&std::io::Error::from_raw_os_error(5)));
        assert!(!declined(&std::io::Error::other("no")));
    }
}
