//! Updates. Every GitHub release carries the signed app for each platform and
//! a `latest.json` describing them; this reads that a minute after launch,
//! every six hours after, and whenever the menu or the command line (`update`)
//! asks. A newer release is downloaded and its signature checked against the
//! public key in tauri.conf.json. On the Mac it then replaces the bundle this
//! copy runs from, and on Linux the executable, and the new copy starts once
//! this one has quit. On Windows the release's installer runs in its passive
//! mode, a progress bar with nothing to click, installing for the same users
//! as before, and starts the app again itself. For a copy installed for
//! everyone that takes an administrator's prompt first; when it is declined,
//! the app goes on as it was, and doesn't ask about that release again until
//! the menu or `update` asks.
//!
//! Only a copy installed the way a release installs updates itself. On the
//! Mac that is a copy running from an app bundle: the updater replaces
//! whatever holds `Contents/MacOS`, which for `cargo run` would be the build
//! directory. On Windows it is a copy in the folder the installer recorded:
//! one from `cargo install` would get a second copy beside it, with Start at
//! login still naming the first. On Linux it is the copy in `~/.local/bin`,
//! where a release's `install` and `just install` put it; the updater
//! replaces whatever file this copy runs from, which for `cargo run` or
//! `cargo install` would be a build of someone's own.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use tauri_plugin_updater::UpdaterExt;

use crate::controller::{Command, Shared};

/// What installing can fail with; the update thread only logs it.
type BoxError = Box<dyn std::error::Error>;

/// The first check waits out the crowd of launches at login.
const FIRST_CHECK: Duration = Duration::from_secs(60);
/// Between automatic checks.
const INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

/// What the updater is doing, for the menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    Checking,
    /// A check someone asked for found nothing newer.
    UpToDate,
    Installing(String),
    /// The administrator prompt for this release's installer was declined.
    #[cfg(windows)]
    Declined(String),
    /// A check someone asked for failed; the log says why.
    Failed,
}

impl State {
    /// The menu line, and whether clicking it checks now.
    pub fn menu_line(&self) -> (String, bool) {
        match self {
            Self::Idle => ("Check for updates".to_owned(), true),
            Self::Checking => ("Checking for updates…".to_owned(), false),
            Self::UpToDate => (format!("Up to date ({})", env!("CARGO_PKG_VERSION")), true),
            Self::Installing(version) => (format!("Updating to {version}…"), false),
            #[cfg(windows)]
            Self::Declined(version) => (format!("Update to {version}"), true),
            Self::Failed => ("Couldn't check for updates; try again".to_owned(), true),
        }
    }
}

/// What an update check came to.
enum Outcome {
    UpToDate,
    /// This release is installed, or on Windows its installer is running.
    Installed(String),
    /// The administrator prompt for this release's installer was declined.
    #[cfg(windows)]
    Declined(String),
}

/// The handle the controller asks for a check through.
pub struct Updates(Sender<()>);

impl Updates {
    pub fn check_now(&self) {
        let _ = self.0.send(());
    }
}

/// Starts the thread that checks for updates, or says why there is none.
/// The updater plugin must already be registered.
pub fn start(
    app: tauri::AppHandle,
    report: Sender<Command>,
    shared: Arc<Shared>,
) -> Option<Updates> {
    let executable = match std::env::current_exe() {
        Ok(executable) => executable,
        Err(error) => {
            tracing::warn!(%error, "can't tell where this copy runs from, so it won't update itself");
            return None;
        }
    };
    let installed = match installed_copy(&executable) {
        Ok(installed) => installed,
        Err(why) => {
            tracing::info!(executable = %executable.display(), "{why}; updates are off");
            return None;
        }
    };
    tracing::info!(installed = %installed.display(), "updates are on");
    let (requests, received) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("updates".to_owned())
        .spawn(move || run(&app, &executable, &installed, &received, &report, &shared));
    match spawned {
        Ok(_) => Some(Updates(requests)),
        Err(error) => {
            tracing::warn!(%error, "could not start the update thread");
            None
        }
    }
}

/// What a release replaces, when this copy updates itself: on the Mac, the
/// bundle it runs from.
#[cfg(target_os = "macos")]
fn installed_copy(executable: &Path) -> Result<PathBuf, &'static str> {
    let bundle = bundle_of(executable).ok_or("not running from an app bundle")?;
    // A quarantined app opened where it was downloaded runs from a read-only
    // copy macOS makes, which can't be replaced.
    if bundle
        .components()
        .any(|part| part.as_os_str() == "AppTranslocation")
    {
        return Err(
            "running from macOS's read-only copy of a downloaded app; move it to /Applications",
        );
    }
    Ok(bundle.to_path_buf())
}

/// What a release replaces, when this copy updates itself: on Windows, the
/// folder the installer put it in, for everyone or for this user, which the
/// installer records for Settings → Apps.
#[cfg(windows)]
fn installed_copy(executable: &Path) -> Result<PathBuf, &'static str> {
    let folder = executable
        .parent()
        .ok_or("no folder above the executable")?;
    crate::installed::scope(folder)
        .map(|_| folder.to_path_buf())
        .ok_or("not a copy the installer put in place")
}

/// What a release replaces, when this copy updates itself: on Linux, the
/// executable in `~/.local/bin`.
#[cfg(target_os = "linux")]
fn installed_copy(executable: &Path) -> Result<PathBuf, &'static str> {
    if utwm_linux::install::is_installed_copy(executable) {
        Ok(executable.to_path_buf())
    } else {
        Err("not the copy installed in ~/.local/bin")
    }
}

/// The bundle holding `executable`, which is `X.app/Contents/MacOS/<name>`.
#[cfg(target_os = "macos")]
fn bundle_of(executable: &Path) -> Option<&Path> {
    let macos = executable.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    (macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && bundle.extension()? == "app")
        .then_some(bundle)
}

fn run(
    app: &tauri::AppHandle,
    executable: &Path,
    installed: &Path,
    requests: &Receiver<()>,
    report: &Sender<Command>,
    shared: &Shared,
) {
    let mut wait = FIRST_CHECK;
    // The release whose administrator prompt was declined, which only a check
    // someone asks for installs.
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut declined: Option<String> = None;
    loop {
        let asked = match requests.recv_timeout(wait) {
            Ok(()) => true,
            Err(RecvTimeoutError::Timeout) => false,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        wait = INTERVAL;
        // Shortcuts come back on when the app restarts, so a paused copy
        // waits until someone asks.
        if !asked && shared.paused.load(Ordering::SeqCst) {
            tracing::info!("shortcuts are paused; skipping this update check");
            continue;
        }
        let _ = report.send(Command::Updates(State::Checking));
        let skip = if asked { None } else { declined.as_deref() };
        let outcome = tauri::async_runtime::block_on(install_newer(app, executable, report, skip));
        let state = match outcome {
            Ok(Outcome::Installed(version)) => {
                tracing::info!(version, "update installed; restarting");
                if restart_after_exit(installed) {
                    let _ = report.send(Command::Quit);
                    return;
                }
                State::Idle
            }
            Ok(Outcome::UpToDate) => {
                tracing::info!(version = env!("CARGO_PKG_VERSION"), "up to date");
                if asked { State::UpToDate } else { State::Idle }
            }
            #[cfg(windows)]
            Ok(Outcome::Declined(version)) => {
                tracing::info!(
                    version,
                    "the administrator prompt for this update was declined; it waits until asked"
                );
                declined = Some(version.clone());
                State::Declined(version)
            }
            Err(error) => {
                tracing::warn!(%error, "update check failed");
                if asked { State::Failed } else { State::Idle }
            }
        };
        let _ = report.send(Command::Updates(state));
    }
}

/// Installs the latest release in place of this copy when it is newer,
/// unless it is `skip`, a release whose administrator prompt was declined.
async fn install_newer(
    app: &tauri::AppHandle,
    executable: &Path,
    report: &Sender<Command>,
    skip: Option<&str>,
) -> Result<Outcome, BoxError> {
    let updater = app.updater_builder().executable_path(executable).build()?;
    let Some(update) = updater.check().await? else {
        return Ok(Outcome::UpToDate);
    };
    #[cfg(windows)]
    if skip == Some(update.version.as_str()) {
        return Ok(Outcome::Declined(update.version));
    }
    #[cfg(not(windows))]
    let _ = skip;
    tracing::info!(from = %update.current_version, to = %update.version, "installing an update");
    let _ = report.send(Command::Updates(State::Installing(update.version.clone())));
    // On Windows the installer runs from here rather than from the updater
    // plugin, which writes it somewhere any program the user runs could
    // change it before the administrator prompt runs it, and takes the tray
    // icon down before the prompt is answered, leaving none when it is
    // declined.
    #[cfg(windows)]
    {
        let scope = executable
            .parent()
            .and_then(crate::installed::scope)
            .ok_or("no longer a copy the installer put in place")?;
        let bytes = update.download(|_, _| {}, || {}).await?;
        match crate::installed::run_installer(&bytes, &update.version, scope) {
            Ok(()) => Ok(Outcome::Installed(update.version)),
            Err(error) if crate::installed::declined(&error) => {
                Ok(Outcome::Declined(update.version))
            }
            Err(error) => Err(error.into()),
        }
    }
    #[cfg(not(windows))]
    {
        update.download_and_install(|_, _| {}, || {}).await?;
        Ok(Outcome::Installed(update.version))
    }
}

/// Opens `bundle` again once this process has exited: a new copy can't start
/// while this one holds the single-instance lock and the control socket, and
/// `open` starts it the way the Finder does. Returns whether that is arranged.
#[cfg(target_os = "macos")]
fn restart_after_exit(bundle: &Path) -> bool {
    let script = r#"while kill -0 "$1" 2>/dev/null; do sleep 0.2; done; exec /usr/bin/open "$2""#;
    let spawned = std::process::Command::new("/bin/sh")
        .args(["-c", script, "relaunch"])
        .arg(std::process::id().to_string())
        .arg(bundle)
        .spawn();
    if let Err(error) = &spawned {
        tracing::warn!(%error, "could not arrange the restart; the update applies at the next launch");
    }
    spawned.is_ok()
}

/// The installer starts the new copy itself once it is done; this one only
/// has to quit, which the installer would otherwise make it do.
#[cfg(windows)]
fn restart_after_exit(_installed: &Path) -> bool {
    true
}

/// Starts `executable` again once this process has exited: a new copy can't
/// start while this one holds the single-instance lock and the control
/// socket. It runs in a process group of its own, as `install` starts one.
#[cfg(target_os = "linux")]
fn restart_after_exit(executable: &Path) -> bool {
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;

    let script = r#"while kill -0 "$1" 2>/dev/null; do sleep 0.2; done; exec "$2""#;
    let spawned = std::process::Command::new("/bin/sh")
        .args(["-c", script, "relaunch"])
        .arg(std::process::id().to_string())
        .arg(executable)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn();
    if let Err(error) = &spawned {
        tracing::warn!(%error, "could not arrange the restart; the update applies at the next launch");
    }
    spawned.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn only_a_bundled_executable_has_a_bundle() {
        let bundled = Path::new(
            "/Applications/Window Manager.app/Contents/MacOS/universal-tiling-window-manager",
        );
        assert_eq!(
            bundle_of(bundled),
            Some(Path::new("/Applications/Window Manager.app"))
        );
        for unbundled in [
            "/Users/me/code/utwm/target/debug/universal-tiling-window-manager",
            "/Users/me/code/utwm/target/release/universal-tiling-window-manager",
            "/opt/Contents/MacOS/universal-tiling-window-manager",
            "/Applications/Window Manager.app/Contents/Resources/universal-tiling-window-manager",
            "universal-tiling-window-manager",
        ] {
            assert_eq!(bundle_of(Path::new(unbundled)), None, "{unbundled}");
        }
    }

    #[test]
    fn the_menu_only_offers_a_check_when_none_is_running() {
        assert!(State::Idle.menu_line().1);
        assert!(!State::Checking.menu_line().1);
        assert!(!State::Installing("9.9.9".to_owned()).menu_line().1);
        assert_eq!(
            State::Installing("9.9.9".to_owned()).menu_line().0,
            "Updating to 9.9.9…"
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_declined_update_is_offered_from_the_menu() {
        assert_eq!(
            State::Declined("9.9.9".to_owned()).menu_line(),
            ("Update to 9.9.9".to_owned(), true)
        );
    }
}
