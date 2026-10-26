//! The Linux install: the release executable in `~/.local/bin`, and a desktop
//! entry and icons in GNOME's data folder (`$XDG_DATA_HOME`, `~/.local/share`
//! by default), so GNOME lists the app and starts it like any other, plus the
//! GNOME Shell extension the app works through. Everything is staged first in
//! `target/release/bundle/linux`, `bin` and `share` laid out as installed.

use std::error::Error;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Deserialize;
use utwm_linux::control::{self, Control};
use utwm_linux::extension::{self, State};

/// The package, and the executable installed.
const EXECUTABLE: &str = "universal-tiling-window-manager";
/// The icons, by the size each is.
const ICONS: [(&str, &str); 3] = [
    ("32x32", "icons/32x32.png"),
    ("128x128", "icons/128x128.png"),
    ("256x256", "icons/icon.png"),
];
/// The system's pkg-config. Homebrew's, often first on a Linux path, knows
/// only Homebrew's libraries, and GTK and WebKitGTK are the system's.
const SYSTEM_PKG_CONFIG: &str = "/usr/bin/pkg-config";

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// The app's name and identifier, read from the Tauri configuration so they
/// are set in one place.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Names {
    product_name: String,
    identifier: String,
}

pub fn run(args: impl Iterator<Item = String>) -> Result<()> {
    let mut install = false;
    for arg in args {
        match arg.as_str() {
            "--install" => install = true,
            other => {
                return Err(
                    format!("unknown argument `{other}`; the argument is --install").into(),
                );
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .ok_or("no workspace above tools/bundle")?;
    let config = std::fs::read_to_string(root.join("apps/desktop/tauri.conf.json"))?;
    let names: Names = serde_json::from_str(&config)?;
    let home = std::env::home_dir().ok_or("no home directory")?;

    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut build = Command::new(&cargo);
    build
        .args(["build", "--release", "--locked", "-p", EXECUTABLE])
        .current_dir(root);
    if std::env::var_os("PKG_CONFIG").is_none() && Path::new(SYSTEM_PKG_CONFIG).is_file() {
        build.env("PKG_CONFIG", SYSTEM_PKG_CONFIG);
    }
    run_tool(&mut build)?;
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    let release = target.join("release");
    let staged = release.join("bundle").join("linux");
    let bin = home.join(".local").join("bin");
    let installed_executable = bin.join(EXECUTABLE);
    stage(
        &staged,
        &release.join(EXECUTABLE),
        &root.join("apps/desktop"),
        &names,
        &installed_executable,
    )?;
    println!("built {}", staged.display());

    if install {
        let share = utwm_linux::data_home().ok_or("no home directory")?;
        install_files(&staged, &bin, &share)?;
        extension::install()?;
        start(&installed_executable)?;
        println!(
            "installed {} and started it",
            installed_executable.display()
        );
        match extension::state() {
            State::NeedsLogIn | State::Stale { .. } => println!(
                "log out and back in once, so GNOME Shell starts the extension the app works through"
            ),
            State::UserExtensionsOff => println!(
                "GNOME's extensions are switched off; the app's menu offers to switch them on"
            ),
            State::NoGnomeShell => {
                println!("this isn't a GNOME session; the app needs GNOME Shell to move windows");
            }
            _ => {}
        }
    }
    Ok(())
}

/// Lays out what installing puts in place: the executable in `bin`, and in
/// `share` the desktop entry (`applications`) and the icons
/// (`icons/hicolor`).
fn stage(
    staged: &Path,
    executable: &Path,
    desktop: &Path,
    names: &Names,
    installed_executable: &Path,
) -> Result<()> {
    if staged.exists() {
        std::fs::remove_dir_all(staged)?;
    }
    let bin = staged.join("bin");
    let applications = staged.join("share").join("applications");
    std::fs::create_dir_all(&bin)?;
    std::fs::create_dir_all(&applications)?;
    std::fs::copy(executable, bin.join(EXECUTABLE))?;
    std::fs::write(
        applications.join(format!("{}.desktop", names.identifier)),
        desktop_entry(names, installed_executable),
    )?;
    for (size, icon) in ICONS {
        let apps = staged
            .join("share")
            .join("icons")
            .join("hicolor")
            .join(size)
            .join("apps");
        std::fs::create_dir_all(&apps)?;
        std::fs::copy(
            desktop.join(icon),
            apps.join(format!("{}.png", names.identifier)),
        )?;
    }
    Ok(())
}

/// What GNOME reads about the app. It is a tray app with no window, so it
/// doesn't ask GNOME to wait for one when it starts.
fn desktop_entry(names: &Names, executable: &Path) -> String {
    format!(
        "[Desktop Entry]
Type=Application
Name={}
GenericName=Tiling Window Manager
Comment=Tile windows from the keyboard, with one configuration for Windows, the Mac and Linux
Exec={}
Icon={}
Terminal=false
Categories=Utility;
Keywords=tile;tiling;window;rectangle;snap;
StartupNotify=false
X-GNOME-UsesNotifications=true
",
        names.product_name,
        utwm_linux::autostart::exec_argument(executable),
        names.identifier
    )
}

/// Copies the staged `bin` into `bin` and the staged `share` into `share`,
/// quitting a running copy first. Each file goes through a temporary name and
/// a rename, so nothing ever sees half a file.
fn install_files(staged: &Path, bin: &Path, share: &Path) -> Result<()> {
    if control::send_control(Control::Quit) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while control::is_running() {
            if Instant::now() > deadline {
                return Err("the running copy didn't quit".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    for (from, to) in [(staged.join("bin"), bin), (staged.join("share"), share)] {
        for file in files(&from)? {
            let relative = file.strip_prefix(&from)?;
            let destination = to.join(relative);
            let folder = destination
                .parent()
                .ok_or("an installed file has no folder")?;
            std::fs::create_dir_all(folder)?;
            let temporary = folder.join(format!(
                ".{}.new",
                relative.file_name().unwrap_or_default().to_string_lossy()
            ));
            std::fs::copy(&file, &temporary)?;
            std::fs::rename(&temporary, &destination)?;
        }
    }
    Ok(())
}

/// Every file under `folder`, however deep.
fn files(folder: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(folder)? {
        let path = entry?.path();
        if path.is_dir() {
            found.extend(files(&path)?);
        } else {
            found.push(path);
        }
    }
    Ok(found)
}

/// Starts the installed app on its own, out of this terminal's reach.
fn start(executable: &Path) -> Result<()> {
    Command::new(executable)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()?;
    Ok(())
}

fn run_tool(command: &mut Command) -> Result<()> {
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "{} failed ({status})",
            command.get_program().to_string_lossy()
        )
        .into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Names {
        Names {
            product_name: "Window Manager".to_owned(),
            identifier: "com.johncarmack.universal-tiling-window-manager".to_owned(),
        }
    }

    #[test]
    fn the_desktop_entry_runs_the_installed_executable() {
        let entry = desktop_entry(
            &names(),
            Path::new("/home/me/.local/bin/universal-tiling-window-manager"),
        );
        assert!(entry.starts_with("[Desktop Entry]\n"));
        assert!(entry.contains("\nName=Window Manager\n"));
        assert!(entry.contains("\nExec=\"/home/me/.local/bin/universal-tiling-window-manager\"\n"));
        assert!(entry.contains("\nIcon=com.johncarmack.universal-tiling-window-manager\n"));
    }

    #[test]
    fn the_names_come_from_the_tauri_configuration() {
        let config = include_str!("../../../apps/desktop/tauri.conf.json");
        let names: Names = serde_json::from_str(config).expect("tauri.conf.json parses");
        assert_eq!(
            names.identifier,
            utwm_linux::APP_ID,
            "the autostart entry and notifications name the app by this identifier"
        );
    }
}
