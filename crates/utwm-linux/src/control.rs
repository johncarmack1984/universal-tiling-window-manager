//! How the command line reaches the running app: `quit`, `reload`, `pause`
//! and `update` connect to a Unix socket and send one word, as on the Mac.

use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::Duration;

/// Requests the command line sends to the running instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Quit,
    Reload,
    TogglePause,
    /// Check for an update now, and install it.
    Update,
}

impl Control {
    fn word(self) -> &'static str {
        match self {
            Self::Quit => "quit",
            Self::Reload => "reload",
            Self::TogglePause => "pause",
            Self::Update => "update",
        }
    }

    fn from_word(word: &str) -> Option<Self> {
        [Self::Quit, Self::Reload, Self::TogglePause, Self::Update]
            .into_iter()
            .find(|control| control.word() == word)
    }
}

/// Receives requests, on the listener's thread.
pub type Reporter = Box<dyn Fn(Control) + Send + 'static>;

/// Socket paths are limited to 108 bytes on Linux, the last one a nul.
const MAX_SOCKET_PATH: usize = 107;

/// `$XDG_RUNTIME_DIR/universal-tiling-window-manager/control.sock`: the
/// user's own runtime folder, which nobody else can enter and which logging
/// out clears. Without one, `~/.local/state`.
fn socket_path() -> Option<PathBuf> {
    let folder = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::home_dir().map(|home| home.join(".local").join("state")))?;
    let path = folder.join(utwm_config::load::APP_DIR).join("control.sock");
    (path.as_os_str().len() <= MAX_SOCKET_PATH).then_some(path)
}

/// Whether an instance is running and listening.
pub fn is_running() -> bool {
    socket_path().is_some_and(|path| UnixStream::connect(path).is_ok())
}

/// Sends `control` to the running instance. Returns false if none is running.
pub fn send_control(control: Control) -> bool {
    let Some(mut stream) = socket_path().and_then(|path| UnixStream::connect(path).ok()) else {
        return false;
    };
    stream.write_all(control.word().as_bytes()).is_ok()
}

/// Listens for the command line on a thread of its own, until dropped.
pub struct Listener {
    path: PathBuf,
}

impl Listener {
    pub fn start(report: Reporter) -> std::io::Result<Self> {
        let path = socket_path()
            .ok_or_else(|| std::io::Error::other("no short enough path for the control socket"))?;
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        // Left by a copy that didn't exit cleanly. Only one copy runs at a
        // time (the single-instance check comes first), so it is stale.
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path)?;
        std::thread::Builder::new()
            .name("control".to_owned())
            .spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { continue };
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                    let mut word = String::new();
                    let _ = (&mut stream).take(16).read_to_string(&mut word);
                    match Control::from_word(word.trim()) {
                        Some(control) => report(control),
                        None => tracing::debug!(word, "unknown control request"),
                    }
                }
            })?;
        Ok(Self { path })
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Removes the socket, for an exit that ends the process without dropping
/// the listener.
pub fn remove_socket() {
    if let Some(path) = socket_path() {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_round_trip() {
        for control in [
            Control::Quit,
            Control::Reload,
            Control::TogglePause,
            Control::Update,
        ] {
            assert_eq!(Control::from_word(control.word()), Some(control));
        }
        assert_eq!(Control::from_word("frobnicate"), None);
    }

    #[test]
    fn the_socket_path_fits() {
        if let Some(path) = socket_path() {
            assert!(path.as_os_str().len() <= MAX_SOCKET_PATH);
            assert!(path.ends_with("universal-tiling-window-manager/control.sock"));
        }
    }
}
