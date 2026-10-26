//! The Linux install: the release executable in `~/.local/bin`, and a desktop
//! entry and icons in GNOME's data folder (`$XDG_DATA_HOME`, `~/.local/share`
//! by default), so GNOME lists the app and starts it like any other, plus the
//! GNOME Shell extension the app works through. Everything is staged first in
//! `target/release/bundle/linux`, `bin` and `share` laid out as installed.
//! Installing is the built app's own `install` command, the one a release's
//! download runs.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;

use utwm_linux::install::{self, EXECUTABLE};

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
    let executable = release.join(EXECUTABLE);
    let staged = release.join("bundle").join("linux");
    let installed = install::executable().ok_or("no home directory")?;
    let icons = ICONS
        .iter()
        .map(|(size, path)| Ok((*size, std::fs::read(root.join("apps/desktop").join(path))?)))
        .collect::<std::io::Result<Vec<_>>>()?;
    let icons: Vec<(&str, &[u8])> = icons
        .iter()
        .map(|(size, png)| (*size, png.as_slice()))
        .collect();
    if staged.exists() {
        std::fs::remove_dir_all(&staged)?;
    }
    install::lay_out(
        &executable,
        &icons,
        &staged.join("bin"),
        &staged.join("share"),
        &installed,
    )?;
    println!("built {}", staged.display());

    if install {
        run_tool(Command::new(&executable).arg("install"))?;
    }
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
    use serde::Deserialize;

    /// The app's name and identifier as the Tauri configuration sets them.
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Names {
        product_name: String,
        identifier: String,
    }

    #[test]
    fn the_names_come_from_the_tauri_configuration() {
        let config = include_str!("../../../apps/desktop/tauri.conf.json");
        let names: Names = serde_json::from_str(config).expect("tauri.conf.json parses");
        assert_eq!(
            names.identifier,
            utwm_linux::APP_ID,
            "the desktop and autostart entries and notifications name the app by this identifier"
        );
        assert_eq!(
            names.product_name,
            utwm_linux::APP_NAME,
            "the desktop and autostart entries show this name"
        );
    }

    #[test]
    fn the_icons_are_the_apps() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/desktop");
        for (_, path) in super::ICONS {
            assert!(root.join(path).is_file(), "{path}");
        }
    }
}
