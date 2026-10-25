//! `~/.config/universal-tiling-window-manager/config.toml`: the few choices that
//! belong to one machine rather than to the export. Everything else comes from
//! the export, so this file is optional and usually absent. The top-level keys
//! are Windows'; the `[mac]` table is the Mac's, so one file carried by the
//! dotfiles can serve both machines.
//!
//! ```toml
//! # Where the export lives, if not ~/.config/rectangle-pro/RectangleProConfig.json
//! rectangle_config = "~/Dropbox/RectangleProConfig.json"
//! # Windows apps whose windows are left alone (Rectangle's disabledApps)
//! ignored_apps = ["mstsc.exe"]
//!
//! [modifiers]          # what each Mac modifier becomes on Windows
//! command = "win"
//!
//! [shortcuts]          # Windows-only rebinds; "" unbinds
//! topLeftEighth = "ctrl+alt+num0"
//!
//! [mac]                # bundle ids, on top of the export's disabledApps
//! ignored_apps = ["com.apple.Terminal"]
//!
//! [mac.shortcuts]      # Mac-only rebinds, keys named as above; "" unbinds
//! leftHalf = "ctrl+alt+h"
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::chord::ModifierMap;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Overrides {
    pub rectangle_config: Option<String>,
    pub ignored_apps: Vec<String>,
    pub modifiers: ModifierMap,
    pub shortcuts: BTreeMap<String, String>,
    pub mac: MacOverrides,
}

/// The `[mac]` table: the Mac's counterparts of `ignored_apps` and
/// `[shortcuts]`. Modifiers need no table there; the export's are the Mac's.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MacOverrides {
    /// Bundle ids, e.g. `com.apple.Terminal`.
    pub ignored_apps: Vec<String>,
    /// Chords in the same syntax as `[shortcuts]`: keys by their position,
    /// named as on a PC keyboard, with `ctrl`, `alt` (or `option`), `shift`
    /// and `cmd` (or `win`).
    pub shortcuts: BTreeMap<String, String>,
}

#[derive(Debug, thiserror::Error)]
pub enum OverridesError {
    #[error("{path}: {source}")]
    Toml {
        path: String,
        #[source]
        source: Box<toml::de::Error>,
    },
}

impl Overrides {
    pub fn parse(text: &str, path: &Path) -> Result<Self, OverridesError> {
        toml::from_str(text).map_err(|source| OverridesError::Toml {
            path: path.display().to_string(),
            source: Box::new(source),
        })
    }

    /// `rectangle_config` with a leading `~` expanded against `home`.
    pub fn rectangle_config_path(&self, home: &Path) -> Option<PathBuf> {
        self.rectangle_config
            .as_deref()
            .map(|path| expand_home(path, home))
    }

    pub fn ignores_app(&self, exe_name: &str) -> bool {
        self.ignored_apps
            .iter()
            .any(|ignored| ignored.eq_ignore_ascii_case(exe_name))
    }
}

/// Expands `~`, `~/…` and `~\…`.
pub fn expand_home(path: &str, home: &Path) -> PathBuf {
    if path == "~" {
        return home.to_path_buf();
    }
    match path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        Some(rest) => home.join(rest),
        None => PathBuf::from(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chord::WinModifier;

    #[test]
    fn a_full_file_parses() {
        let overrides = Overrides::parse(
            r#"
            rectangle_config = "~/Dropbox/RectangleProConfig.json"
            ignored_apps = ["mstsc.exe"]
            [modifiers]
            command = "ctrl"
            [shortcuts]
            topLeftEighth = "ctrl+alt+num0"
            restore = ""
            "#,
            Path::new("config.toml"),
        )
        .expect("parses");
        assert_eq!(overrides.modifiers.command, WinModifier::Ctrl);
        assert_eq!(overrides.modifiers.option, WinModifier::Alt);
        assert!(overrides.ignores_app("MSTSC.EXE"));
        assert_eq!(
            overrides.rectangle_config_path(Path::new("/home/john")),
            Some(PathBuf::from("/home/john").join("Dropbox/RectangleProConfig.json"))
        );
        assert_eq!(
            overrides.shortcuts.get("restore").map(String::as_str),
            Some("")
        );
    }

    #[test]
    fn the_mac_table_parses_beside_the_windows_keys() {
        let overrides = Overrides::parse(
            r#"
            ignored_apps = ["mstsc.exe"]
            [shortcuts]
            topLeftEighth = "ctrl+alt+num0"
            [mac]
            ignored_apps = ["com.apple.Terminal"]
            [mac.shortcuts]
            leftHalf = "ctrl+option+h"
            "#,
            Path::new("config.toml"),
        )
        .expect("parses");
        assert_eq!(overrides.ignored_apps, ["mstsc.exe"]);
        assert_eq!(overrides.mac.ignored_apps, ["com.apple.Terminal"]);
        assert_eq!(
            overrides.mac.shortcuts.get("leftHalf").map(String::as_str),
            Some("ctrl+option+h")
        );
        assert!(Overrides::parse("[mac]\nmodifiers = {}", Path::new("config.toml")).is_err());
    }

    #[test]
    fn an_empty_file_is_all_defaults() {
        let overrides = Overrides::parse("", Path::new("config.toml")).expect("parses");
        assert_eq!(overrides, Overrides::default());
    }

    #[test]
    fn typos_are_errors_not_silently_ignored() {
        assert!(Overrides::parse("ignored_app = []", Path::new("config.toml")).is_err());
        assert!(
            Overrides::parse("[modifiers]\noption = \"hyper\"", Path::new("config.toml")).is_err()
        );
    }
}
