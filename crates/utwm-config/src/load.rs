//! Finding and loading configuration from disk.
//!
//! The config to import, a Rectangle or Rectangle Pro export, is looked for in
//! this order, first hit wins:
//!
//! 1. a path named for this run (`--config` or `UTWM_CONFIG`);
//! 2. the one chosen on this machine, from the menu's Choose configuration…
//!    or the `config` command, remembered outside the config ([`chosen`]);
//! 3. `import` in config.toml;
//! 4. `~/.config/window-manager/config.json`;
//! 5. `~/.config/rectangle-pro/RectangleProConfig.json`, where the dotfiles
//!    put the Mac's Rectangle Pro config before the app had a folder of its
//!    own;
//! 6. `~/.config/rectangle/RectangleConfig.json`, for free Rectangle's;
//! 7. none: the default shortcuts, built in.
//!
//! [`named`] gives the first two; [`load`] takes it from there. config.toml,
//! the app's own settings, is `~/.config/window-manager/config.toml`, or the
//! one in `~/.config/universal-tiling-window-manager` while only that one
//! exists. The same paths serve every machine, and what is found resolves for
//! the platform this build runs on.

use std::path::{Path, PathBuf};

use crate::bindings::{self, Resolved, Source};
use crate::export::{Export, ExportError};
use crate::overrides::{Overrides, OverridesError};
use crate::platform::Platform;

/// The app's own folder, under `~/.config` and wherever each platform keeps
/// logs and state: Window Manager, its functional name.
pub const FOLDER: &str = "window-manager";

/// The name the app is published under. On the Mac and Linux the command line
/// reaches the running app through a socket in a folder of this name, and
/// config.toml lived in `~/.config/universal-tiling-window-manager` before
/// the app had a folder named for itself.
pub const APP_DIR: &str = "universal-tiling-window-manager";

/// `~/.config/window-manager`, home of config.json and config.toml.
pub fn config_dir(home: &Path) -> PathBuf {
    home.join(".config").join(FOLDER)
}

/// `~/.config/window-manager/config.json`, where the config to import goes
/// unless it is named elsewhere.
pub fn default_config_path(home: &Path) -> PathBuf {
    config_dir(home).join("config.json")
}

/// The overrides file: `~/.config/window-manager/config.toml`, or the one
/// earlier versions read while only that one exists.
pub fn overrides_path(home: &Path) -> PathBuf {
    let current = config_dir(home).join("config.toml");
    let earlier = earlier_overrides_path(home);
    if !current.exists() && earlier.exists() {
        earlier
    } else {
        current
    }
}

fn earlier_overrides_path(home: &Path) -> PathBuf {
    home.join(".config").join(APP_DIR).join("config.toml")
}

/// The default export locations, in search order: the app's own, then where
/// the dotfiles put Rectangle Pro's and free Rectangle's.
pub fn default_export_paths(home: &Path) -> [PathBuf; 3] {
    [
        default_config_path(home),
        home.join(".config")
            .join("rectangle-pro")
            .join("RectangleProConfig.json"),
        home.join(".config")
            .join("rectangle")
            .join("RectangleConfig.json"),
    ]
}

/// Where this machine keeps what the app remembers outside the config:
/// `%LOCALAPPDATA%\window-manager` on Windows, `~/Library/Application
/// Support/window-manager` on the Mac and `~/.local/state/window-manager` on
/// Linux, or under `$XDG_STATE_HOME`.
pub fn state_dir(home: &Path) -> PathBuf {
    let base = match Platform::current() {
        Platform::Windows => std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData").join("Local")),
        Platform::Mac => home.join("Library").join("Application Support"),
        Platform::Linux => std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .filter(|dir| dir.is_absolute())
            .unwrap_or_else(|| home.join(".local").join("state")),
    };
    base.join(FOLDER)
}

/// The file that remembers the config chosen on this machine.
pub fn chosen_file(home: &Path) -> PathBuf {
    state_dir(home).join("config-path")
}

/// The config chosen on this machine with the menu's Choose configuration…
/// or the `config` command, if any.
pub fn chosen(home: &Path) -> Option<PathBuf> {
    read_chosen(&chosen_file(home))
}

/// The path `file` remembers: its one line, trimmed.
pub fn read_chosen(file: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(file).ok()?;
    let path = text.trim();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

/// Remembers `path` in `file`, or with `None` forgets the choice.
pub fn write_chosen(file: &Path, path: Option<&Path>) -> std::io::Result<()> {
    let Some(path) = path else {
        return match std::fs::remove_file(file) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        };
    };
    let text = path.to_str().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{} isn't valid Unicode", path.display()),
        )
    })?;
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(file, format!("{text}\n"))
}

/// The config named for this run (`--config` or `UTWM_CONFIG`), or else the
/// one chosen on this machine.
pub fn named(home: &Path, explicit: Option<&Path>) -> Option<PathBuf> {
    explicit.map(Path::to_path_buf).or_else(|| chosen(home))
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

/// Every file that, when it changes, should trigger a reload: both places
/// config.toml can be, the remembered choice, and every config that could
/// be imported. `explicit` is what [`named`] gives.
pub fn watched_paths(home: &Path, explicit: Option<&Path>, overrides: &Overrides) -> Vec<PathBuf> {
    let mut paths = vec![
        config_dir(home).join("config.toml"),
        earlier_overrides_path(home),
        chosen_file(home),
    ];
    if let Some(path) = explicit {
        paths.push(path.to_path_buf());
    }
    if let Some(path) = overrides.import_path(home) {
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

/// Finds, reads and resolves everything. `explicit` is what [`named`] gives.
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
        .or_else(|| overrides.import_path(home));
    let path = match chosen {
        // A path someone named must exist; silently falling back would hide a typo.
        Some(path) => Some(path),
        None => default_export_paths(home)
            .into_iter()
            .find(|path| path.is_file()),
    };
    // With nothing to import, the source says where the config was expected.
    let Some(path) = path else {
        return Ok(defaults(
            overrides,
            Source::nothing_at(default_config_path(home)),
        ));
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

/// The default shortcuts for this platform, for when the config to import
/// can't be read: Rectangle's recommended set.
pub fn builtin(overrides: &Overrides) -> Resolved {
    defaults(overrides, Source::builtin())
}

fn defaults(overrides: &Overrides, source: Source) -> Resolved {
    bindings::resolve(&builtin_export(), overrides, source, Platform::current())
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

    const EXPORT: &str = r#"{ "bundleId": "com.knollsoft.Rectangle", "version": "111",
        "shortcuts": { "leftHalf": { "keyCode": 123, "modifierFlags": 786432 } },
        "defaults": {} }"#;

    /// An empty home of this test's own; the guard removes it.
    struct Home(PathBuf);

    impl Home {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("utwm-load-test-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("creates the test home");
            Self(path)
        }

        /// Writes `text` at `relative`, a path with `/` between its parts.
        fn write(&self, relative: &str, text: &str) -> PathBuf {
            let path = relative
                .split('/')
                .fold(self.0.clone(), |path, part| path.join(part));
            if let Some(folder) = path.parent() {
                std::fs::create_dir_all(folder).expect("creates the folder");
            }
            std::fs::write(&path, text).expect("writes the file");
            path
        }
    }

    impl Drop for Home {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_builtin_set_binds_everything() {
        for platform in [Platform::Windows, Platform::Mac, Platform::Linux] {
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
    fn with_no_files_the_builtin_set_is_used_and_says_where_it_looked() {
        let home = Home::new("empty");
        let resolved = load(&home.0, None).expect("loads");
        let expected = default_config_path(&home.0);
        assert!(expected.ends_with(".config/window-manager/config.json"));
        assert_eq!(resolved.source, Source::nothing_at(expected.clone()));
        assert_eq!(resolved.bindings.len(), 22);
        assert!(
            resolved
                .source
                .describe()
                .contains(&expected.display().to_string())
        );
    }

    #[test]
    fn a_named_but_missing_export_is_an_error() {
        let home = Home::new("missing");
        let missing = home.0.join("nope.json");
        assert!(matches!(
            load(&home.0, Some(&missing)),
            Err(LoadError::Io { .. })
        ));
    }

    #[test]
    fn the_apps_own_folder_comes_before_rectangle_pros() {
        let home = Home::new("both");
        let own = home.write(".config/window-manager/config.json", EXPORT);
        home.write(".config/rectangle-pro/RectangleProConfig.json", EXPORT);
        let resolved = load(&home.0, None).expect("loads");
        assert_eq!(resolved.source.path, Some(own));
    }

    #[test]
    fn rectangle_pros_folder_still_works() {
        let home = Home::new("earlier");
        let earlier = home.write(".config/rectangle-pro/RectangleProConfig.json", EXPORT);
        let resolved = load(&home.0, None).expect("loads");
        assert_eq!(resolved.source.path, Some(earlier));
    }

    #[test]
    fn config_toml_moves_and_the_earlier_one_reads_until_it_does() {
        let home = Home::new("overrides");
        home.write(
            ".config/universal-tiling-window-manager/config.toml",
            r#"ignored_apps = ["earlier.exe"]"#,
        );
        let overrides = load_overrides(&home.0).expect("loads");
        assert_eq!(overrides.ignored_apps, ["earlier.exe"]);
        home.write(
            ".config/window-manager/config.toml",
            r#"ignored_apps = ["current.exe"]"#,
        );
        let overrides = load_overrides(&home.0).expect("loads");
        assert_eq!(overrides.ignored_apps, ["current.exe"]);
    }

    #[test]
    fn a_choice_is_remembered_and_forgotten() {
        let home = Home::new("chosen");
        let file = home.0.join("state").join("config-path");
        assert_eq!(read_chosen(&file), None);
        let config = home.write("Dropbox/config.json", EXPORT);
        write_chosen(&file, Some(&config)).expect("remembers");
        assert_eq!(read_chosen(&file), Some(config.clone()));
        let resolved = load(&home.0, read_chosen(&file).as_deref()).expect("loads");
        assert_eq!(resolved.source.path, Some(config));
        write_chosen(&file, None).expect("forgets");
        assert_eq!(read_chosen(&file), None);
        write_chosen(&file, None).expect("forgetting twice is fine");
    }

    #[test]
    fn a_choice_is_watched() {
        let home = Home::new("watched");
        let paths = watched_paths(&home.0, None, &Overrides::default());
        assert!(paths.contains(&chosen_file(&home.0)));
        assert!(paths.contains(&config_dir(&home.0).join("config.toml")));
        assert!(paths.contains(&default_config_path(&home.0)));
    }
}
