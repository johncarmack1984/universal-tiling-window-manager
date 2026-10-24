//! Notices when a configuration file changes.
//!
//! Polling a handful of files once a second costs nothing and copes with what a
//! file watcher makes awkward: directories that don't exist yet, and chezmoi
//! rewriting the export in place rather than atomically on Windows. A change
//! only counts once the file has stopped changing, so a reload never reads a
//! half-written export.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

type Signature = Option<(SystemTime, u64)>;

fn signature(path: &PathBuf) -> Signature {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// Polls `paths` every second and calls `changed` once a change has settled.
/// The list can be swapped while running, e.g. after the overrides file names
/// a different export.
pub fn spawn(
    paths: Arc<Mutex<Vec<PathBuf>>>,
    changed: impl Fn() + Send + 'static,
) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("config-poller".to_owned())
        .spawn(move || {
            let snapshot = |paths: &Arc<Mutex<Vec<PathBuf>>>| -> Vec<(PathBuf, Signature)> {
                let paths = paths.lock().map(|paths| paths.clone()).unwrap_or_default();
                paths
                    .into_iter()
                    .map(|path| {
                        let signature = signature(&path);
                        (path, signature)
                    })
                    .collect()
            };
            let mut settled = snapshot(&paths);
            let mut pending: Option<Vec<(PathBuf, Signature)>> = None;
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let now = snapshot(&paths);
                match &pending {
                    // Unchanged for a full second since the change was seen.
                    Some(previous) if *previous == now => {
                        pending = None;
                        settled = now;
                        changed();
                    }
                    Some(_) => pending = Some(now),
                    None if now != settled => pending = Some(now),
                    None => {}
                }
            }
        })?;
    Ok(())
}
