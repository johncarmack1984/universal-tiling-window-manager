//! Updates. Every GitHub release carries the signed app for each platform and
//! a `latest.json` describing them; this reads that a minute after launch,
//! every six hours after, and whenever the menu or the command line (`update`)
//! asks. A newer release is downloaded and its signature checked against the
//! public key in tauri.conf.json. On the Mac it then replaces the bundle this
//! copy runs from, and on Linux the executable, and the new copy starts once
//! this one has quit; on Windows the release's installer runs in its passive
//! mode, a progress bar with nothing to click, and starts the app again
//! itself.
//!
//! Only a copy installed the way a release installs updates itself. On the
//! Mac that is a copy running from an app bundle: the updater replaces
//! whatever holds `Contents/MacOS`, which for `cargo run` would be the build
//! directory. On Windows it is a copy the installer put in place: one from
//! `cargo install` would get a second copy beside it, with Start at login
//! still naming the first. On Linux it is the copy in `~/.local/bin`, where
//! a release's `install` and `just install` put it; the updater replaces
//! whatever file this copy runs from, which for `cargo run` or `cargo
//! install` would be a build of someone's own.

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
            Self::Failed => ("Couldn't check for updates; try again".to_owned(), true),
        }
    }
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
/// folder the installer put it in. The installer marks the executable it
/// installs, which a `cargo install` build doesn't carry.
#[cfg(windows)]
fn installed_copy(executable: &Path) -> Result<PathBuf, &'static str> {
    use tauri::utils::{config::BundleType, platform::bundle_type};

    if bundle_type() != Some(BundleType::Nsis) {
        return Err("not installed by the installer");
    }
    executable
        .parent()
        .map(Path::to_path_buf)
        .ok_or("no folder above the executable")
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
        let outcome = tauri::async_runtime::block_on(install_newer(app, executable, report));
        let state = match outcome {
            Ok(Some(version)) => {
                tracing::info!(version, "update installed; restarting");
                if restart_after_exit(installed) {
                    let _ = report.send(Command::Quit);
                    return;
                }
                State::Idle
            }
            Ok(None) => {
                tracing::info!(version = env!("CARGO_PKG_VERSION"), "up to date");
                if asked { State::UpToDate } else { State::Idle }
            }
            Err(error) => {
                tracing::warn!(%error, "update check failed");
                if asked { State::Failed } else { State::Idle }
            }
        };
        let _ = report.send(Command::Updates(state));
    }
}

/// Installs the latest release in place of this copy when it is newer.
/// Returns the version installed.
async fn install_newer(
    app: &tauri::AppHandle,
    executable: &Path,
    report: &Sender<Command>,
) -> Result<Option<String>, BoxError> {
    let builder = app.updater_builder().executable_path(executable);
    // On Windows the plugin ends this process as soon as the installer
    // starts. The tray icon goes first, or Windows leaves it in the tray
    // until the pointer passes over it.
    #[cfg(windows)]
    let builder = {
        let app = app.clone();
        builder.on_before_exit(move || app.cleanup_before_exit())
    };
    let updater = builder.build()?;
    let Some(update) = updater.check().await? else {
        return Ok(None);
    };
    tracing::info!(from = %update.current_version, to = %update.version, "installing an update");
    let _ = report.send(Command::Updates(State::Installing(update.version.clone())));
    update.download_and_install(|_, _| {}, || {}).await?;
    Ok(Some(update.version))
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

/// The installer starts the new copy itself; installing ended this process
/// before it could get here.
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
}
