//! The bundle: `Contents/MacOS` holds the release executable,
//! `Contents/Resources` the icon, and `Contents/Info.plist` describes the app
//! to macOS.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use serde::Deserialize;
use utwm_macos::control::{self, Control};

/// The package, and the executable the bundle wraps.
const EXECUTABLE: &str = "universal-tiling-window-manager";
/// Where `--install` puts the app.
const APPLICATIONS: &str = "/Applications";
/// The login item uses SMAppService, which arrived in macOS 13.
const MINIMUM_SYSTEM: &str = "13.0";

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// The app's name and bundle identifier, read from the Tauri configuration so
/// they are set in one place.
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
                    format!("unknown argument `{other}`; the only one is --install").into(),
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

    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    run_tool(
        Command::new(cargo)
            .args(["build", "--release", "--locked", "-p", EXECUTABLE])
            .current_dir(root),
    )?;
    let release = std::env::var_os("CARGO_TARGET_DIR")
        .map_or_else(|| root.join("target"), PathBuf::from)
        .join("release");
    let app = release
        .join("bundle")
        .join("macos")
        .join(format!("{}.app", names.product_name));
    assemble(
        &app,
        &release.join(EXECUTABLE),
        &root.join("apps/desktop/icons/icon.icns"),
        &names,
    )?;
    let identity = std::env::var("UTWM_SIGN_IDENTITY")
        .ok()
        .filter(|identity| !identity.is_empty());
    sign(&app, &names.identifier, identity.as_deref())?;
    println!("built {}", app.display());

    if install {
        let installed = install_app(&app, &names, identity.is_none())?;
        println!("installed {} and started it", installed.display());
        if identity.is_none() {
            println!(
                "signed ad hoc, so macOS asks for Accessibility access again after each install; set UTWM_SIGN_IDENTITY to a code-signing certificate to keep it"
            );
        }
        println!(
            "the command line is {}",
            installed.join("Contents/MacOS").join(EXECUTABLE).display()
        );
    }
    Ok(())
}

fn assemble(app: &Path, executable: &Path, icon: &Path, names: &Names) -> Result<()> {
    if app.exists() {
        std::fs::remove_dir_all(app)?;
    }
    let contents = app.join("Contents");
    let executables = contents.join("MacOS");
    let resources = contents.join("Resources");
    std::fs::create_dir_all(&executables)?;
    std::fs::create_dir_all(&resources)?;
    std::fs::copy(executable, executables.join(EXECUTABLE))?;
    std::fs::copy(icon, resources.join("icon.icns"))?;
    std::fs::write(contents.join("Info.plist"), info_plist(names))?;
    Ok(())
}

/// Signs the bundle as `identifier`, with the named certificate or ad hoc,
/// then checks the signature.
fn sign(app: &Path, identifier: &str, identity: Option<&str>) -> Result<()> {
    run_tool(
        Command::new("codesign")
            .args(["--force", "--sign", identity.unwrap_or("-")])
            .args(["--identifier", identifier])
            .arg(app),
    )?;
    run_tool(
        Command::new("codesign")
            .args(["--verify", "--strict"])
            .arg(app),
    )
}

/// Replaces the copy in /Applications with `app`, quitting a running copy
/// first, and starts it.
fn install_app(app: &Path, names: &Names, ad_hoc: bool) -> Result<PathBuf> {
    if control::send_control(Control::Quit) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while control::is_running() {
            if Instant::now() > deadline {
                return Err("the running copy didn't quit".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    let installed = Path::new(APPLICATIONS).join(format!("{}.app", names.product_name));
    if installed.exists() {
        std::fs::remove_dir_all(&installed)?;
    }
    // ditto copies a bundle the way Finder does, signature and all.
    run_tool(Command::new("ditto").arg(app).arg(&installed))?;
    if ad_hoc {
        // The old build's grant names a signature this build doesn't have, so
        // its switch in System Settings would stay on and do nothing. Clearing
        // it lets the app ask afresh.
        let _ = Command::new("tccutil")
            .args(["reset", "Accessibility", &names.identifier])
            .output();
    }
    run_tool(Command::new("open").arg(&installed))?;
    Ok(installed)
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

/// What macOS reads about the app. `LSUIElement` makes it a menu bar app: no
/// Dock icon and no menu bar of its own, from the moment it launches.
fn info_plist(names: &Names) -> String {
    let name = escape(&names.product_name);
    let identifier = escape(&names.identifier);
    let version = env!("CARGO_PKG_VERSION");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key>
	<string>en</string>
	<key>CFBundleDisplayName</key>
	<string>{name}</string>
	<key>CFBundleExecutable</key>
	<string>{EXECUTABLE}</string>
	<key>CFBundleIconFile</key>
	<string>icon</string>
	<key>CFBundleIdentifier</key>
	<string>{identifier}</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleName</key>
	<string>{name}</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>{version}</string>
	<key>CFBundleVersion</key>
	<string>{version}</string>
	<key>LSApplicationCategoryType</key>
	<string>public.app-category.productivity</string>
	<key>LSMinimumSystemVersion</key>
	<string>{MINIMUM_SYSTEM}</string>
	<key>LSUIElement</key>
	<true/>
	<key>NSHighResolutionCapable</key>
	<true/>
</dict>
</plist>
"#
    )
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
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
    fn the_plist_describes_a_menu_bar_app() {
        let plist = info_plist(&names());
        assert!(plist.contains(
            "<key>CFBundleIdentifier</key>\n\t<string>com.johncarmack.universal-tiling-window-manager</string>"
        ));
        assert!(plist.contains(
            "<key>CFBundleExecutable</key>\n\t<string>universal-tiling-window-manager</string>"
        ));
        assert!(plist.contains("<key>CFBundleName</key>\n\t<string>Window Manager</string>"));
        assert!(plist.contains("<key>LSUIElement</key>\n\t<true/>"));
        assert!(plist.contains(&format!(
            "<key>CFBundleShortVersionString</key>\n\t<string>{}</string>",
            env!("CARGO_PKG_VERSION")
        )));
    }

    #[test]
    fn the_names_come_from_the_tauri_configuration() {
        let config = include_str!("../../../apps/desktop/tauri.conf.json");
        let names: Names = serde_json::from_str(config).expect("tauri.conf.json parses");
        assert_eq!(
            names.identifier,
            utwm_macos::autostart::LABEL,
            "the login item looks for this bundle identifier"
        );
    }
}
