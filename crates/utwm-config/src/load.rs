//! Finding and loading configuration from disk.
//!
//! The export is looked for in this order, first hit wins:
//!
//! 1. an explicit path (`--config` or `UTWM_CONFIG`);
//! 2. `rectangle_config` in the overrides file;
//! 3. `~/.config/rectangle-pro/RectangleProConfig.json`, where the dotfiles put
//!    the Mac's Rectangle Pro export;
//! 4. `~/.config/rectangle/RectangleConfig.json`, for free Rectangle;
//! 5. Rectangle's own recommended shortcuts, built in.
//!
//! The same paths serve both machines, and what is found resolves for the
//! platform this build runs on.

use std::path::{Path, PathBuf};

use crate::bindings::{self, Resolved, Source};
use crate::export::{Export, ExportError};
use crate::overrides::{Overrides, OverridesError};
use crate::platform::Platform;

pub const APP_DIR: &str = "universal-tiling-window-manager";

/// The overrides file.
pub fn overrides_path(home: &Path) -> PathBuf {
    home.join(".config").join(APP_DIR).join("config.toml")
}

/// The default export locations, in search order.
pub fn default_export_paths(home: &Path) -> [PathBuf; 2] {
    [
        home.join(".config")
            .join("rectangle-pro")
            .join("RectangleProConfig.json"),
        home.join(".config")
            .join("rectangle")
            .join("RectangleConfig.json"),
    ]
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{}: {source}", path.display())]
    Export {
        path: PathBuf,
        #[source]
        source: ExportError,
    },
    #[error(transparent)]
    Overrides(#[from] OverridesError),
}

/// Every file that, when it changes, should trigger a reload.
pub fn watched_paths(home: &Path, explicit: Option<&Path>, overrides: &Overrides) -> Vec<PathBuf> {
    let mut paths = vec![overrides_path(home)];
    if let Some(path) = explicit {
        paths.push(path.to_path_buf());
    }
    if let Some(path) = overrides.rectangle_config_path(home) {
        paths.push(path);
    }
    paths.extend(default_export_paths(home));
    paths
}

/// Reads the overrides file; a missing file is the same as an empty one.
pub fn load_overrides(home: &Path) -> Result<Overrides, LoadError> {
    let path = overrides_path(home);
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(Overrides::parse(&text, &path)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Overrides::default()),
        Err(source) => Err(LoadError::Io { path, source }),
    }
}

/// Finds, reads and resolves everything.
pub fn load(home: &Path, explicit: Option<&Path>) -> Result<Resolved, LoadError> {
    let overrides = load_overrides(home)?;
    load_with(home, explicit, &overrides)
}

/// Like [`load`], with overrides already read.
pub fn load_with(
    home: &Path,
    explicit: Option<&Path>,
    overrides: &Overrides,
) -> Result<Resolved, LoadError> {
    let chosen = explicit
        .map(Path::to_path_buf)
        .or_else(|| overrides.rectangle_config_path(home));
    let path = match chosen {
        // A path someone named must exist; silently falling back would hide a typo.
        Some(path) => Some(path),
        None => default_export_paths(home)
            .into_iter()
            .find(|path| path.is_file()),
    };
    let Some(path) = path else {
        return Ok(builtin(overrides));
    };
    let text = std::fs::read_to_string(&path).map_err(|source| LoadError::Io {
        path: path.clone(),
        source,
    })?;
    let export = Export::parse(&text).map_err(|source| LoadError::Export {
        path: path.clone(),
        source,
    })?;
    let source = Source {
        kind: Some(export.kind()),
        version: Some(export.version.clone()),
        path: Some(path),
    };
    Ok(bindings::resolve(
        &export,
        overrides,
        source,
        Platform::current(),
    ))
}

/// Rectangle's recommended shortcuts for this platform, for when there is no
/// export or it can't be read.
pub fn builtin(overrides: &Overrides) -> Resolved {
    bindings::resolve(
        &builtin_export(),
        overrides,
        Source::builtin(),
        Platform::current(),
    )
}

/// Rectangle's "Recommended" shortcuts (`alternateDefault` in
/// `WindowAction.swift`), the set Rectangle Pro also ships with. Used when no
/// export is found, so the app is useful before any dotfiles are applied.
pub fn builtin_export() -> Export {
    const CTRL_OPT: u64 = 786_432;
    const CTRL_OPT_SHIFT: u64 = 917_504;
    const CTRL_OPT_CMD: u64 = 1_835_008;
    let shortcuts = [
        ("leftHalf", 123, CTRL_OPT),
        ("rightHalf", 124, CTRL_OPT),
        ("bottomHalf", 125, CTRL_OPT),
        ("topHalf", 126, CTRL_OPT),
        ("bottomLeft", 38, CTRL_OPT),
        ("bottomRight", 40, CTRL_OPT),
        ("topLeft", 32, CTRL_OPT),
        ("topRight", 34, CTRL_OPT),
        ("maximize", 36, CTRL_OPT),
        ("maximizeHeight", 126, CTRL_OPT_SHIFT),
        ("previousDisplay", 123, CTRL_OPT_CMD),
        ("nextDisplay", 124, CTRL_OPT_CMD),
        ("larger", 24, CTRL_OPT),
        ("smaller", 27, CTRL_OPT),
        ("center", 8, CTRL_OPT),
        ("restore", 51, CTRL_OPT),
        ("firstThird", 2, CTRL_OPT),
        ("firstTwoThirds", 14, CTRL_OPT),
        ("centerThird", 3, CTRL_OPT),
        ("lastTwoThirds", 17, CTRL_OPT),
        ("lastThird", 5, CTRL_OPT),
        ("centerTwoThirds", 15, CTRL_OPT),
    ];
    Export {
        bundle_id: "builtin".to_owned(),
        version: String::new(),
        timestamp: None,
        shortcuts: shortcuts
            .into_iter()
            .map(|(name, key_code, modifier_flags)| {
                (
                    name.to_owned(),
                    crate::export::Shortcut {
                        key_code,
                        modifier_flags,
                    },
                )
            })
            .collect(),
        defaults: std::collections::BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_builtin_set_binds_everything() {
        for platform in [Platform::Windows, Platform::Mac] {
            let resolved = bindings::resolve(
                &builtin_export(),
                &Overrides::default(),
                Source::builtin(),
                platform,
            );
            assert_eq!(resolved.bindings.len(), 22, "{platform}");
            assert!(resolved.skipped.is_empty(), "{platform}");
        }
    }

    #[test]
    fn with_no_files_the_builtin_set_is_used() {
        let home = std::env::temp_dir().join("utwm-load-test-empty-home");
        let resolved = load(&home, None).expect("loads");
        assert_eq!(resolved.source, Source::builtin());
    }

    #[test]
    fn a_named_but_missing_export_is_an_error() {
        let home = std::env::temp_dir().join("utwm-load-test-empty-home");
        let missing = home.join("nope.json");
        assert!(matches!(
            load(&home, Some(&missing)),
            Err(LoadError::Io { .. })
        ));
    }
}
