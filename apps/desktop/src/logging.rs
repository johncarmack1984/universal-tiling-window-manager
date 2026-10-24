//! Logs to `%LOCALAPPDATA%\universal-tiling-window-manager\log.txt`, keeping
//! one previous file. `UTWM_LOG` sets the filter (`info` by default; `debug`
//! shows every key binding that fires and where each window landed).

use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::sync::Mutex;

const MAX_LOG_BYTES: u64 = 1 << 20;

pub fn log_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join(utwm_config::load::APP_DIR))
}

/// Starts logging. Returns the log file's path, if one could be opened.
pub fn init() -> Option<PathBuf> {
    let filter = tracing_subscriber::EnvFilter::try_from_env("UTWM_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let path = log_dir().and_then(|dir| {
        fs::create_dir_all(&dir).ok()?;
        let path = dir.join("log.txt");
        if fs::metadata(&path).is_ok_and(|meta| meta.len() > MAX_LOG_BYTES) {
            let _ = fs::rename(&path, dir.join("log.old.txt"));
        }
        Some(path)
    });
    let file = path
        .as_ref()
        .and_then(|path| OpenOptions::new().create(true).append(true).open(path).ok());
    match file {
        Some(file) => {
            let _ = tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_ansi(false)
                .with_writer(Mutex::new(file))
                .try_init();
            path
        }
        None => {
            let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
            None
        }
    }
}
