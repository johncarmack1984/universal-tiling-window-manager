//! How the command line reaches the running app: `quit`, `reload` and `pause`
//! connect to a Unix socket and send one word. The Windows build finds its
//! hidden window by class name instead; this is the Mac's equivalent.

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
}

impl Control {
    fn word(self) -> &'static str {
        match self {
            Self::Quit => "quit",
            Self::Reload => "reload",
            Self::TogglePause => "pause",
        }
    }

    fn from_word(word: &str) -> Option<Self> {
        [Self::Quit, Self::Reload, Self::TogglePause]
            .into_iter()
            .find(|control| control.word() == word)
    }
}

/// Receives requests, on the listener's thread.
pub type Reporter = Box<dyn Fn(Control) + Send + 'static>;

/// Socket paths are limited to 104 bytes on macOS.
const MAX_SOCKET_PATH: usize = 103;

/// `~/Library/Application Support/universal-tiling-window-manager/control.sock`,
/// or the per-user temporary folder when a long home path would make that
/// too long for a socket. Not the temporary folder by default: macOS clears
/// files there that go unused for days, and the app runs for weeks.
fn socket_path() -> Option<PathBuf> {
    let name = "control.sock";
    let support = std::env::home_dir().map(|home| {
        home.join("Library")
            .join("Application Support")
            .join(utwm_config::load::APP_DIR)
            .join(name)
    });
    support
        .filter(|path| path.as_os_str().len() <= MAX_SOCKET_PATH)
        .or_else(|| {
            let path = std::env::temp_dir().join(format!("{}.sock", utwm_config::load::APP_DIR));
            (path.as_os_str().len() <= MAX_SOCKET_PATH).then_some(path)
        })
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
        for control in [Control::Quit, Control::Reload, Control::TogglePause] {
            assert_eq!(Control::from_word(control.word()), Some(control));
        }
        assert_eq!(Control::from_word("frobnicate"), None);
    }

    #[test]
    fn the_socket_path_fits() {
        let path = socket_path().expect("a path");
        assert!(path.as_os_str().len() <= MAX_SOCKET_PATH);
    }
}
