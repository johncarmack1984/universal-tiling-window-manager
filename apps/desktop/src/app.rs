//! The tray app: Tauri 3 with no windows, the platform's shortcut listener, and
//! the worker that moves windows.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, OnceLock};

use tauri::{Manager, RunEvent};
use utwm_config::{Chord, load};

use crate::controller::{self, Command, Controller, Shared};
use crate::platform::Shortcuts;
use crate::{logging, poll, tray, updates};

pub fn run(explicit_config: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    // Every coordinate this app reads or writes is a physical pixel. Tauri's
    // event loop asks for the same awareness; whichever call comes first wins.
    #[cfg(windows)]
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
            tracing::warn!(%error, "configuration not loaded; using the default shortcuts");
            (load::builtin(&overrides), Some(error.to_string()))
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
        // A second launch exits here, before it registers a second set of
        // shortcuts.
        .plugin(tauri_plugin_single_instance::init(|_app, argv, _cwd| {
            tracing::info!(?argv, "already running; ignoring the second launch");
        }))
        // Reads the endpoint and the public key from tauri.conf.json.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(move |app| {
            // A menu bar app: no Dock icon and no menu bar of its own.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let handle = app.handle().clone();
            let _ = shared.app.set(handle.clone());

            let chords = config
                .bindings
                .iter()
                .map(|binding| binding.chord)
                .collect();
            let (shortcuts, refused) = listen(app, chords, &commands)?;
            let reload = commands.clone();
            poll::spawn(Arc::clone(&shared.watched), move || {
                let _ = reload.send(Command::Reload);
            })?;

            let updates = updates::start(handle.clone(), commands.clone(), Arc::clone(&shared));
            let controller = Controller::new(
                Arc::clone(&shared),
                Arc::new(config),
                load_error,
                shortcuts,
                refused,
                updates,
            );
            tray::create(
                &handle,
                controller.config(),
                &controller.status(),
                false,
                controller.updates_line().as_ref(),
                commands.clone(),
            )?;
            std::thread::Builder::new()
                .name("controller".to_owned())
                .spawn(move || controller.run(receiver))?;
            Ok(())
        })
        .build(tauri::generate_context!())?;

    app.run(|_app, event| match event {
        // There are no windows to close; only Quit ends the app.
        RunEvent::ExitRequested {
            code: None, api, ..
        } => api.prevent_exit(),
        #[cfg(target_os = "macos")]
        RunEvent::Exit => utwm_macos::control::remove_socket(),
        _ => {}
    });
    Ok(())
}

/// Keeps the system-events thread alive for as long as the app runs.
#[cfg(windows)]
struct EventsGuard(#[allow(dead_code)] utwm_windows::events::Events);

/// Installs the keyboard hook and starts the thread that watches for system
/// events and the command line's requests. The hook takes every chord.
#[cfg(windows)]
fn listen(
    app: &mut tauri::App,
    chords: Vec<Chord>,
    commands: &Sender<Command>,
) -> Result<(Shortcuts, Vec<Chord>), Box<dyn std::error::Error>> {
    use utwm_windows::events::Events;
    use utwm_windows::hook::{Hook, Mode};

    let hook = Hook::start(
        Mode::Active,
        chords,
        controller::hook_reporter(commands.clone()),
    )?;
    let events = Events::start(controller::events_reporter(commands.clone()))?;
    app.manage(EventsGuard(events));
    Ok((hook, Vec::new()))
}

/// Keeps the command line's socket open for as long as the app runs.
#[cfg(target_os = "macos")]
struct ControlGuard(#[allow(dead_code)] utwm_macos::control::Listener);

/// Registers the hot keys and opens the command line's socket. Returns the
/// chords macOS refused because another app holds them.
///
/// Moving windows needs Accessibility access, but nothing asks for it here: a
/// prompt at every login until it is granted would be a nag. The menu says
/// what is missing, and asking happens when the user clicks that line or
/// first presses a shortcut.
#[cfg(target_os = "macos")]
fn listen(
    app: &mut tauri::App,
    chords: Vec<Chord>,
    commands: &Sender<Command>,
) -> Result<(Shortcuts, Vec<Chord>), Box<dyn std::error::Error>> {
    use utwm_macos::{conflicts, control, permission, window};

    window::limit_waits();
    if !permission::is_granted() {
        tracing::warn!("no Accessibility access yet; windows move once it is granted");
    }
    let (hotkeys, refused) =
        Shortcuts::start(chords, controller::hotkeys_reporter(commands.clone()))?;
    let listener = control::Listener::start(controller::control_reporter(commands.clone()))?;
    app.manage(ControlGuard(listener));

    // The menu's first line follows Accessibility access and any Rectangle
    // still running; neither announces itself, so look every couple of seconds.
    let status = commands.clone();
    std::thread::Builder::new()
        .name("status".to_owned())
        .spawn(move || {
            let observe = || (permission::is_granted(), conflicts::rectangle_running());
            let mut last = observe();
            loop {
                std::thread::sleep(std::time::Duration::from_secs(2));
                let now = observe();
                if now != last {
                    last = now;
                    if status.send(Command::StatusChanged).is_err() {
                        break;
                    }
                }
            }
        })?;
    Ok((hotkeys, refused))
}
