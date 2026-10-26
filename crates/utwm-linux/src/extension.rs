//! The GNOME Shell extension that does what GNOME keeps from apps: its files,
//! which this app carries and installs, and where it stands.
//!
//! GNOME Shell looks for new extensions only when it starts, so a first
//! install takes one log-out and log-in, and so does a newer copy, since the
//! shell keeps running the code it loaded. The menu's first line says which
//! step is next, and clicking it takes that step.

use std::collections::HashMap;
use std::path::PathBuf;

use zbus::zvariant::OwnedValue;

use crate::{gsettings, shell};

/// The extension's id, after the app's identifier.
pub const UUID: &str = "universal-tiling-window-manager@johncarmack.com";

const EXTENSION_JS: &str = include_str!("../gnome-extension/extension.js");
const METADATA_JSON: &str = include_str!("../gnome-extension/metadata.json");

const SHELL_PATH: &str = "/org/gnome/Shell";
const EXTENSIONS: &str = "org.gnome.Shell.Extensions";
const SESSION_MANAGER: &str = "org.gnome.SessionManager";
const SESSION_MANAGER_PATH: &str = "/org/gnome/SessionManager";
const SHELL_SETTINGS: &str = "org.gnome.shell";

// `state` in `GetExtensionInfo`'s answer: `ExtensionState` in GNOME Shell's
// misc/extensionUtils.js.
const ACTIVE: u8 = 1;
const ERROR: u8 = 3;
const OUT_OF_DATE: u8 = 4;
const ACTIVATING: u8 = 8;

/// Where the extension stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// Not a GNOME session: no GNOME Shell on the bus.
    NoGnomeShell,
    /// GNOME's switch for every user extension is off.
    UserExtensionsOff,
    NotInstalled,
    /// Installed and switched on; GNOME Shell starts it at the next log-in.
    NeedsLogIn,
    /// Installed, but switched off (in GNOME's Extensions app, say).
    Disabled,
    /// GNOME Shell couldn't start it; the text is GNOME's error.
    Failed(String),
    /// It doesn't support this version of GNOME, which is the text.
    OutOfDate(String),
    /// Running, but not the copy this app carries. A log-out and log-in loads
    /// the new one; until then it works if it still speaks this app's
    /// interface (`compatible`).
    Stale {
        compatible: bool,
    },
    /// Running, and the copy this app carries.
    Active,
}

impl State {
    /// Whether shortcuts and moves work.
    pub fn works(&self) -> bool {
        matches!(self, Self::Active | Self::Stale { compatible: true })
    }
}

/// Where the extension stands now.
pub fn state() -> State {
    let Some(connection) = shell::connection() else {
        return State::NoGnomeShell;
    };
    if shell::owner(connection, shell::SHELL).is_none() {
        return State::NoGnomeShell;
    }
    let Ok(extensions) = shell::proxy(connection, shell::SHELL, SHELL_PATH, EXTENSIONS) else {
        return State::NoGnomeShell;
    };
    if extensions
        .get_property::<bool>("UserExtensionsEnabled")
        .is_ok_and(|enabled| !enabled)
    {
        return State::UserExtensionsOff;
    }
    let info: HashMap<String, OwnedValue> = extensions
        .call("GetExtensionInfo", &(UUID,))
        .unwrap_or_default();
    let text = |key: &str| {
        info.get(key)
            .and_then(|value| value.downcast_ref::<String>().ok())
            .unwrap_or_default()
    };
    let state = info
        .get("state")
        .and_then(|value| value.downcast_ref::<f64>().ok())
        .and_then(state_code);
    let enabled = info
        .get("enabled")
        .and_then(|value| value.downcast_ref::<bool>().ok())
        .unwrap_or(false);
    match state {
        // Nothing known: never installed, or installed since GNOME Shell
        // started.
        None | Some(99) => {
            if installed() {
                State::NeedsLogIn
            } else {
                State::NotInstalled
            }
        }
        Some(ACTIVE | ACTIVATING) => running(),
        Some(ERROR) => State::Failed(text("error")),
        Some(OUT_OF_DATE) => State::OutOfDate(
            extensions
                .get_property::<String>("ShellVersion")
                .unwrap_or_default(),
        ),
        Some(_) if enabled => State::NeedsLogIn,
        Some(_) => State::Disabled,
    }
}

/// GNOME's state number, which arrives as a double.
fn state_code(value: f64) -> Option<u8> {
    [1, 2, 3, 4, 5, 6, 7, 8, 99]
        .into_iter()
        .find(|code| (f64::from(*code) - value).abs() < 0.5)
}

/// The state of a running extension, from what its bridge says.
fn running() -> State {
    match (shell::api_version(), shell::revision()) {
        (Ok(version), Ok(revision))
            if version == shell::API_VERSION && revision == self::revision() =>
        {
            State::Active
        }
        (Ok(version), _) => State::Stale {
            compatible: version == shell::API_VERSION,
        },
        (Err(error), _) => State::Failed(format!("its bridge isn't answering ({error:?})")),
    }
}

/// Takes the next step toward a running extension, the one the menu's first
/// line offers: installs it and switches it on, switches it or GNOME's user
/// extensions back on, or brings up GNOME's log-out dialog when logging in
/// again is what's left.
pub fn set_up() -> std::io::Result<()> {
    match state() {
        State::NotInstalled => install(),
        State::UserExtensionsOff => {
            let connection = shell::connection().ok_or_else(no_bus)?;
            shell::proxy(connection, shell::SHELL, SHELL_PATH, EXTENSIONS)
                .map_err(std::io::Error::other)?
                .set_property("UserExtensionsEnabled", true)
                .map_err(std::io::Error::other)
        }
        State::Disabled => {
            let connection = shell::connection().ok_or_else(no_bus)?;
            let enabled: bool = shell::proxy(connection, shell::SHELL, SHELL_PATH, EXTENSIONS)
                .and_then(|extensions| extensions.call("EnableExtension", &(UUID,)))
                .map_err(std::io::Error::other)?;
            if enabled {
                Ok(())
            } else {
                switch_on_at_login()
            }
        }
        State::NeedsLogIn | State::Stale { .. } => log_out(),
        State::NoGnomeShell | State::Failed(_) | State::OutOfDate(_) | State::Active => Ok(()),
    }
}

fn no_bus() -> std::io::Error {
    std::io::Error::other("no session bus")
}

/// Installs the copy this app carries and switches it on, for GNOME Shell to
/// start at the next log-in.
pub fn install() -> std::io::Result<()> {
    install_files()?;
    switch_on_at_login()
}

/// At startup: throws away an update GNOME staged for the next log-in, which
/// would come from extensions.gnome.org rather than from this app, and if the
/// extension is installed, puts back the copy this app carries wherever the
/// files differ. Returns whether the files changed, which takes a log-in to
/// load.
pub fn refresh() -> bool {
    remove_staged_update();
    if !installed() {
        return false;
    }
    match install_files() {
        Ok(changed) => {
            if changed {
                tracing::info!("updated the GNOME Shell extension; it loads at the next log-in");
            }
            changed
        }
        Err(error) => {
            tracing::warn!(%error, "could not update the GNOME Shell extension");
            false
        }
    }
}

/// GNOME's own log-out dialog, which asks before doing anything.
pub fn log_out() -> std::io::Result<()> {
    let connection = shell::connection().ok_or_else(no_bus)?;
    shell::proxy(
        connection,
        SESSION_MANAGER,
        SESSION_MANAGER_PATH,
        SESSION_MANAGER,
    )
    .and_then(|session| session.call_noreply("Logout", &(0_u32,)))
    .map_err(std::io::Error::other)
}

/// `$XDG_DATA_HOME`, or `~/.local/share`.
fn data_home() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::home_dir().map(|home| home.join(".local").join("share")))
}

/// Where GNOME Shell looks for this user's copy.
pub fn directory() -> Option<PathBuf> {
    data_home().map(|data| data.join("gnome-shell").join("extensions").join(UUID))
}

fn installed() -> bool {
    directory().is_some_and(|directory| directory.join("metadata.json").is_file())
}

/// Writes the extension's files wherever they differ from the copy this app
/// carries, each through a temporary file and a rename, `metadata.json`
/// last. Returns whether anything changed.
fn install_files() -> std::io::Result<bool> {
    let directory = directory().ok_or_else(|| std::io::Error::other("no home directory"))?;
    std::fs::create_dir_all(&directory)?;
    let mut changed = false;
    for (name, contents) in [
        ("extension.js", EXTENSION_JS.to_owned()),
        ("metadata.json", metadata()?),
    ] {
        let path = directory.join(name);
        if std::fs::read_to_string(&path).is_ok_and(|current| current == contents) {
            continue;
        }
        let temporary = directory.join(format!(".{name}.new"));
        std::fs::write(&temporary, contents)?;
        std::fs::rename(&temporary, &path)?;
        changed = true;
    }
    Ok(changed)
}

/// Throws away a copy GNOME downloaded from extensions.gnome.org to install
/// at the next log-in. This app installs its extension itself, and another
/// extension published under the same id must never replace it.
fn remove_staged_update() {
    let Some(staged) = data_home().map(|data| {
        data.join("gnome-shell")
            .join("extension-updates")
            .join(UUID)
    }) else {
        return;
    };
    if staged.exists() {
        match std::fs::remove_dir_all(&staged) {
            Ok(()) => tracing::warn!(
                path = %staged.display(),
                "removed an update GNOME downloaded for this app's extension; only this app updates it"
            ),
            Err(error) => tracing::warn!(%error, "could not remove an update GNOME staged"),
        }
    }
}

/// Puts the extension on GNOME's list to start, and off its list of
/// extensions switched off, which wins over the first.
fn switch_on_at_login() -> std::io::Result<()> {
    edit_list("enabled-extensions", |uuids| {
        if !uuids.iter().any(|uuid| uuid == UUID) {
            uuids.push(UUID.to_owned());
        }
    })?;
    edit_list("disabled-extensions", |uuids| {
        uuids.retain(|uuid| uuid != UUID)
    })
}

fn edit_list(key: &str, edit: impl FnOnce(&mut Vec<String>)) -> std::io::Result<()> {
    let current = gsettings::get(SHELL_SETTINGS, key)
        .and_then(|text| gsettings::parse_strings(&text))
        .unwrap_or_default();
    let mut edited = current.clone();
    edit(&mut edited);
    if edited == current {
        return Ok(());
    }
    gsettings::set(SHELL_SETTINGS, key, &gsettings::format_strings(&edited))
}

/// The copy's revision: a hash of its files, which the installed
/// `metadata.json` carries and the running extension reports, so the app can
/// tell a stale copy from its own.
pub fn revision() -> String {
    // FNV-1a: stable across builds and Rust versions, unlike the standard
    // library's hasher.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in METADATA_JSON.bytes().chain([0]).chain(EXTENSION_JS.bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// `metadata.json` as installed: the copy in the repository, plus the
/// revision.
fn metadata() -> std::io::Result<String> {
    let mut metadata: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(METADATA_JSON).map_err(std::io::Error::other)?;
    metadata.insert("revision".to_owned(), serde_json::Value::String(revision()));
    let mut text = serde_json::to_string_pretty(&metadata).map_err(std::io::Error::other)?;
    text.push('\n');
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extension_and_the_app_speak_the_same_interface() {
        assert!(
            EXTENSION_JS.contains(&format!("const API_VERSION = {};", shell::API_VERSION)),
            "API_VERSION differs between extension.js and shell.rs"
        );
        for name in [shell::BUS_NAME, shell::OBJECT_PATH, shell::INTERFACE] {
            assert!(
                EXTENSION_JS.contains(name),
                "extension.js doesn't name {name}"
            );
        }
    }

    #[test]
    fn installed_metadata_carries_the_uuid_and_the_revision() {
        let metadata: serde_json::Value =
            serde_json::from_str(&metadata().expect("builds")).expect("parses");
        assert_eq!(metadata["uuid"], UUID);
        assert_eq!(metadata["revision"], revision());
        assert_eq!(revision().len(), 16);
        assert_eq!(revision(), revision());
    }

    #[test]
    fn gnome_states_arrive_as_doubles() {
        assert_eq!(state_code(1.0), Some(ACTIVE));
        assert_eq!(state_code(4.0), Some(OUT_OF_DATE));
        assert_eq!(state_code(99.0), Some(99));
        assert_eq!(state_code(42.0), None);
    }
}
