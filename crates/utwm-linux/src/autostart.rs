//! Start at login, per user: an XDG autostart entry in `~/.config/autostart`,
//! which GNOME starts at every log-in. Only the tray menu turns this on;
//! nothing installs it behind the user's back.

use std::path::{Path, PathBuf};

use crate::APP_ID;

/// `$XDG_CONFIG_HOME/autostart`, or `~/.config/autostart`, and the entry named
/// by the app's identifier.
fn path() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::home_dir().map(|home| home.join(".config")))
        .map(|config| config.join("autostart").join(format!("{APP_ID}.desktop")))
}

/// Whether the app starts at login: the entry exists, and nothing (GNOME
/// Tweaks, say) switched it off.
pub fn is_enabled() -> bool {
    let Some(text) = path().and_then(|path| std::fs::read_to_string(path).ok()) else {
        return false;
    };
    !text.lines().any(|line| {
        matches!(
            line.trim(),
            "Hidden=true" | "X-GNOME-Autostart-enabled=false"
        )
    })
}

/// Turns start at login on for `exe`, or off. Takes effect at the next login;
/// the running copy carries on either way.
pub fn set_enabled(enabled: bool, exe: &Path) -> std::io::Result<()> {
    let path = path().ok_or_else(|| std::io::Error::other("no home directory"))?;
    if enabled {
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        std::fs::write(&path, entry(exe))
    } else {
        match std::fs::remove_file(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

/// The entry: run once at login, out of sight in the app grid, with the app's
/// name and icon. Not restarted if it quits, so Quit in the menu stays quit.
fn entry(exe: &Path) -> String {
    format!(
        "[Desktop Entry]
Type=Application
Name=Window Manager
Comment=A tiling window manager you drive from the keyboard
Exec={}
Icon={APP_ID}
Terminal=false
NoDisplay=true
X-GNOME-Autostart-enabled=true
",
        exec_argument(exe)
    )
}

/// `exe` as one quoted argument of a desktop entry's `Exec`. Inside the
/// quotes `"`, `` ` ``, `$` and `\` take a backslash, which the entry's own
/// escaping doubles again, and `%` is doubled so it isn't a field code.
pub fn exec_argument(exe: &Path) -> String {
    let mut quoted = String::from("\"");
    for c in exe.display().to_string().chars() {
        match c {
            '"' | '`' | '$' => {
                quoted.push_str("\\\\");
                quoted.push(c);
            }
            '\\' => quoted.push_str("\\\\\\\\"),
            '%' => quoted.push_str("%%"),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_entry_runs_the_executable_at_login() {
        let entry = entry(Path::new(
            "/home/me/.local/bin/universal-tiling-window-manager",
        ));
        assert!(entry.starts_with("[Desktop Entry]\n"));
        assert!(entry.contains("\nExec=\"/home/me/.local/bin/universal-tiling-window-manager\"\n"));
        assert!(entry.contains("\nX-GNOME-Autostart-enabled=true\n"));
    }

    #[test]
    fn exec_arguments_are_quoted_and_escaped() {
        assert_eq!(
            exec_argument(Path::new("/home/me/My Apps/utwm")),
            "\"/home/me/My Apps/utwm\""
        );
        assert_eq!(
            exec_argument(Path::new("/home/me/100%/$HOME/a\"b")),
            r#""/home/me/100%%/\\$HOME/a\\"b""#
        );
        assert_eq!(
            exec_argument(Path::new(r"/home/me/back\slash")),
            r#""/home/me/back\\\\slash""#
        );
    }
}
