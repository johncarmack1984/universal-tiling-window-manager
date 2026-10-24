//! The tray app: Tauri 3 with no windows, the keyboard hook, and the worker
//! that moves windows.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock};

use tauri::{Manager, RunEvent};
use utwm_config::bindings::{self, Source};
use utwm_config::load;
use utwm_windows::events::Events;
use utwm_windows::hook::{Hook, Mode};

use crate::controller::{self, Command, Controller, Shared};
use crate::{logging, poll, tray};

/// Keeps the system-events thread alive for as long as the app runs.
struct EventsGuard(#[allow(dead_code)] Events);

pub fn run(explicit_config: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    // Every coordinate this app reads or writes is a physical pixel. Tauri's
    // event loop asks for the same awareness; whichever call comes first wins.
    utwm_windows::monitors::ensure_per_monitor_dpi_awareness();
    let log_file = logging::init();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), log = ?log_file, "starting");
    let home = std::env::home_dir().ok_or("no home directory")?;

    // A bad export or overrides file shouldn't stop the app: it starts on
    // Rectangle's defaults and says why in the tray.
    let overrides = load::load_overrides(&home).unwrap_or_default();
    let watched = load::watched_paths(&home, explicit_config.as_deref(), &overrides);
    let (config, load_error) = match load::load(&home, explicit_config.as_deref()) {
        Ok(config) => (config, None),
        Err(error) => {
            tracing::warn!(%error, "configuration not loaded; using Rectangle's defaults");
            let fallback =
                bindings::resolve(&load::builtin_export(), &overrides, Source::builtin());
            (fallback, Some(error.to_string()))
        }
    };
    tracing::info!(
        source = %config.source.describe(),
        bindings = config.bindings.len(),
        skipped = config.skipped.len(),
        "configuration loaded"
    );

    let shared = Arc::new(Shared {
        app: OnceLock::new(),
        paused: AtomicBool::new(false),
        home,
        explicit_config,
        watched: Arc::new(Mutex::new(watched)),
    });
    let (commands, receiver) = std::sync::mpsc::channel::<Command>();

    let app = tauri::Builder::default()
        .runtime(tauri_runtime_wry::Wry::default())
        // A second launch exits here, before it installs a second keyboard hook.
        .plugin(tauri_plugin_single_instance::init(|_app, argv, _cwd| {
            tracing::info!(?argv, "already running; ignoring the second launch");
        }))
        .setup(move |app| {
            let handle = app.handle().clone();
            let _ = shared.app.set(handle.clone());

            let chords = config
                .bindings
                .iter()
                .map(|binding| binding.chord)
                .collect();
            let hook = Hook::start(
                Mode::Active,
                chords,
                controller::hook_reporter(commands.clone()),
            )?;
            let events = Events::start(controller::events_reporter(commands.clone()))?;
            app.manage(EventsGuard(events));
            let reload = commands.clone();
            poll::spawn(Arc::clone(&shared.watched), move || {
                let _ = reload.send(Command::Reload);
            })?;

            let controller =
                Controller::new(Arc::clone(&shared), Arc::new(config), load_error, hook);
            tray::create(
                &handle,
                controller.config(),
                &controller.status(),
                false,
                commands.clone(),
            )?;
            std::thread::Builder::new()
                .name("controller".to_owned())
                .spawn(move || controller.run(receiver))?;
            Ok(())
        })
        .build(tauri::generate_context!())?;

    app.run(|_app, event| {
        // There are no windows to close; only Quit ends the app.
        if let RunEvent::ExitRequested {
            code: None, api, ..
        } = event
        {
            api.prevent_exit();
        }
    });
    Ok(())
}
