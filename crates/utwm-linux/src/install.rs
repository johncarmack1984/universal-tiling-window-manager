//! Installing for this user, as a release's `install` command and `just
//! install` both do: the executable in `~/.local/bin`, which is the copy that
//! updates itself, and a desktop entry and icons in GNOME's data folder
//! (`$XDG_DATA_HOME`, `~/.local/share` by default), so GNOME lists the app
//! and starts it like any other. An update replaces only the executable; the
//! installed copy puts back its desktop entry and icons wherever they differ
//! from its own, as it does the GNOME Shell extension.

use std::error::Error;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::control::{self, Control};
use crate::extension::{self, State};
use crate::{APP_ID, APP_NAME, autostart};

/// The executable's name, built or installed.
pub const EXECUTABLE: &str = "universal-tiling-window-manager";

/// The app's icon at each size GNOME files one under (`32x32` and so on),
/// as PNG files.
pub type Icons<'a> = &'a [(&'a str, &'a [u8])];

/// Where the installed copy runs from: the folder for a user's own
/// executables.
pub fn executable() -> Option<PathBuf> {
    std::env::home_dir().map(|home| home.join(".local").join("bin").join(EXECUTABLE))
}

/// Whether `running` is the installed copy, however the path to it is spelled.
pub fn is_installed_copy(running: &Path) -> bool {
    let (Some(installed), Ok(running)) = (executable(), running.canonicalize()) else {
        return false;
    };
    installed
        .canonicalize()
        .is_ok_and(|installed| installed == running)
}

/// Installs `source`, a downloaded or freshly built executable, for this user:
/// quits a running copy, lays out the executable, desktop entry and icons,
/// installs the GNOME Shell extension, and starts the installed copy. Returns
/// where that is.
pub fn install(source: &Path, icons: Icons<'_>) -> Result<PathBuf, Box<dyn Error>> {
    let installed = executable().ok_or("no home directory")?;
    let bin = installed.parent().ok_or("no folder above the executable")?;
    let share = crate::data_home().ok_or("no home directory")?;
    quit_running()?;
    lay_out(source, icons, bin, &share, &installed)?;
    extension::install()?;
    start(&installed)?;
    Ok(installed)
}

/// What is left to do once installed, if anything.
pub fn advice() -> Option<&'static str> {
    match extension::state() {
        State::NeedsLogIn | State::Stale { .. } => Some(
            "log out and back in once, so GNOME Shell starts the extension the app works through",
        ),
        State::UserExtensionsOff => {
            Some("GNOME's extensions are switched off; the app's menu offers to switch them on")
        }
        State::NoGnomeShell => {
            Some("this isn't a GNOME session; the app needs GNOME Shell to move windows")
        }
        _ => None,
    }
}

/// Lays out an install: `source` in `bin`, and in `share` the desktop entry
/// (`applications`) and the icons (`icons/hicolor`), the entry running
/// `installed`. Each file goes through a temporary name and a rename, so
/// nothing ever sees half a file.
pub fn lay_out(
    source: &Path,
    icons: Icons<'_>,
    bin: &Path,
    share: &Path,
    installed: &Path,
) -> std::io::Result<()> {
    put(&bin.join(EXECUTABLE), &std::fs::read(source)?, 0o755)?;
    for (path, contents) in data_files(icons, installed) {
        put(&share.join(path), &contents, 0o644)?;
    }
    Ok(())
}

/// At startup, in the installed copy: puts back the desktop entry and icons
/// wherever they differ from this copy's, unless the desktop entry is gone,
/// which means someone removed it. Returns whether anything changed.
pub fn refresh(icons: Icons<'_>) -> bool {
    let Ok(running) = std::env::current_exe() else {
        return false;
    };
    if !is_installed_copy(&running) {
        return false;
    }
    let (Some(installed), Some(share)) = (executable(), crate::data_home()) else {
        return false;
    };
    let files = data_files(icons, &installed);
    if files
        .first()
        .is_none_or(|(entry, _)| !share.join(entry).is_file())
    {
        return false;
    }
    let mut changed = false;
    for (path, contents) in files {
        let path = share.join(path);
        if std::fs::read(&path).is_ok_and(|current| current == contents) {
            continue;
        }
        match put(&path, &contents, 0o644) {
            Ok(()) => changed = true,
            Err(error) => tracing::warn!(%error, path = %path.display(), "could not refresh"),
        }
    }
    if changed {
        tracing::info!("put back the desktop entry and icons this copy carries");
    }
    changed
}

/// What GNOME reads about the app. It is a tray app with no window, so it
/// doesn't ask GNOME to wait for one when it starts.
pub fn desktop_entry(executable: &Path) -> String {
    format!(
        "[Desktop Entry]
Type=Application
Name={APP_NAME}
GenericName=Tiling Window Manager
Comment=Tile windows from the keyboard, with one configuration for Windows, the Mac and Linux
Exec={}
Icon={APP_ID}
Terminal=false
Categories=Utility;
Keywords=tile;tiling;window;rectangle;snap;
StartupNotify=false
X-GNOME-UsesNotifications=true
",
        autostart::exec_argument(executable)
    )
}

/// The files that go in the data folder, by their path within it: the
/// desktop entry first, then the icon at each size.
fn data_files(icons: Icons<'_>, installed: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = vec![(
        Path::new("applications").join(format!("{APP_ID}.desktop")),
        desktop_entry(installed).into_bytes(),
    )];
    for (size, png) in icons {
        files.push((
            Path::new("icons")
                .join("hicolor")
                .join(size)
                .join("apps")
                .join(format!("{APP_ID}.png")),
            png.to_vec(),
        ));
    }
    files
}

/// Writes `contents` to `path` with `mode`, through a temporary name beside
/// it and a rename.
fn put(path: &Path, contents: &[u8], mode: u32) -> std::io::Result<()> {
    let (Some(folder), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(std::io::Error::other(format!(
            "{} names no file in a folder",
            path.display()
        )));
    };
    std::fs::create_dir_all(folder)?;
    let temporary = folder.join(format!(".{}.new", name.to_string_lossy()));
    std::fs::write(&temporary, contents)?;
    std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(mode))?;
    std::fs::rename(&temporary, path)
}

/// Asks a running copy to quit, and waits up to ten seconds for it to go.
fn quit_running() -> Result<(), Box<dyn Error>> {
    if control::send_control(Control::Quit) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while control::is_running() {
            if Instant::now() > deadline {
                return Err("the running copy didn't quit".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    Ok(())
}

/// Starts `executable` on its own, out of the launching terminal's reach.
fn start(executable: &Path) -> std::io::Result<()> {
    Command::new(executable)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of the test's own, removed when it ends.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("utwm-install-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("creates a scratch folder");
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_desktop_entry_runs_the_installed_executable() {
        let entry = desktop_entry(Path::new(
            "/home/me/.local/bin/universal-tiling-window-manager",
        ));
        assert!(entry.starts_with("[Desktop Entry]\n"));
        assert!(entry.contains("\nName=Window Manager\n"));
        assert!(entry.contains("\nExec=\"/home/me/.local/bin/universal-tiling-window-manager\"\n"));
        assert!(entry.contains("\nIcon=com.johncarmack.universal-tiling-window-manager\n"));
    }

    #[test]
    fn laying_out_puts_every_file_where_gnome_looks() {
        let scratch = Scratch::new("lay-out");
        let source = scratch.0.join("download");
        std::fs::write(&source, b"\x7fELF the app").expect("writes the source");
        let (bin, share) = (scratch.0.join("bin"), scratch.0.join("share"));
        let installed = bin.join(EXECUTABLE);
        let icons: Icons<'_> = &[("32x32", b"small"), ("256x256", b"larger")];
        lay_out(&source, icons, &bin, &share, &installed).expect("lays out");

        assert_eq!(
            std::fs::read(&installed).expect("installed"),
            b"\x7fELF the app"
        );
        let mode = std::fs::metadata(&installed)
            .expect("installed")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o755);
        let entry =
            std::fs::read_to_string(share.join("applications").join(format!("{APP_ID}.desktop")))
                .expect("the desktop entry");
        assert_eq!(entry, desktop_entry(&installed));
        let large = share
            .join("icons/hicolor/256x256/apps")
            .join(format!("{APP_ID}.png"));
        assert_eq!(std::fs::read(large).expect("the large icon"), b"larger");
        // Nothing half-written stays behind.
        assert!(std::fs::read_dir(&bin).expect("bin").all(|entry| {
            !entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .ends_with(".new")
        }));
    }

    #[test]
    fn laying_out_again_replaces_what_is_there() {
        let scratch = Scratch::new("again");
        let (bin, share) = (scratch.0.join("bin"), scratch.0.join("share"));
        let installed = bin.join(EXECUTABLE);
        for build in [b"\x7fELF old".as_slice(), b"\x7fELF new".as_slice()] {
            let source = scratch.0.join("build");
            std::fs::write(&source, build).expect("writes the build");
            lay_out(&source, &[], &bin, &share, &installed).expect("lays out");
        }
        assert_eq!(
            std::fs::read(&installed).expect("installed"),
            b"\x7fELF new"
        );
    }
}
