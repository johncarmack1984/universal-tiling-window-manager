//! `~/.config/universal-tiling-window-manager/config.toml`: the few choices that
//! only make sense on Windows. Everything else comes from the Mac's export, so
//! this file is optional and usually absent.
//!
//! ```toml
//! # Where the export lives, if not ~/.config/rectangle-pro/RectangleProConfig.json
//! rectangle_config = "~/Dropbox/RectangleProConfig.json"
//! # Windows apps whose windows are left alone (Rectangle's disabledApps)
//! ignored_apps = ["mstsc.exe"]
//!
//! [modifiers]          # what each Mac modifier becomes
//! command = "win"
//!
//! [shortcuts]          # Windows-only rebinds; "" unbinds
//! topLeftEighth = "ctrl+alt+num0"
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
