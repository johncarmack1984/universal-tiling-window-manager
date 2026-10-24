//! The worker that runs actions: it owns the per-window history and the
//! keyboard hook, and is the only thread that moves windows.
//!
//! Everything arrives as a [`Command`] on one channel — a binding from the hook,
//! a tray menu click, a system event, a configuration change — so the order of
//! events is the order they happened in and no state needs a lock.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};

use utwm_config::bindings::Resolved;
use utwm_config::{ActionId, Target};
use utwm_layout::{Engine, Move, PixelRect, Plan, Request, Screen, align};
use utwm_windows::Hwnd;
use utwm_windows::events::{Control, SystemEvent};
use utwm_windows::hook::Hook;
use utwm_windows::{monitors, window};

use crate::tray;

pub enum Command {
    /// The hook matched binding `index` of the current configuration.
    Binding(usize),
    /// A tray menu item for one action or custom position.
    Menu(Target),
    System(SystemEvent),
    /// A configuration file changed, or a reload was asked for.
    Reload,
    TogglePause,
    ToggleAutostart,
    OpenConfigFolder,
    Quit,
}

/// State the tray and the controller both read.
pub struct Shared {
    pub app: OnceLock<tauri::AppHandle>,
    pub paused: AtomicBool,
    pub home: PathBuf,
    pub explicit_config: Option<PathBuf>,
    /// Files whose changes trigger a reload.
    pub watched: Arc<Mutex<Vec<PathBuf>>>,
}

/// A window, told apart from a later window that reuses its handle.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct WindowKey {
    hwnd: Hwnd,
    pid: u32,
    started: u64,
}

impl WindowKey {
    fn of(hwnd: Hwnd) -> Self {
        let pid = window::process_id(hwnd);
        Self {
            hwnd,
            pid,
            started: window::process_start_time(pid).unwrap_or(0),
        }
    }
}

pub struct Controller {
    shared: Arc<Shared>,
    config: Arc<Resolved>,
    /// Why the last load failed, while running on a fallback.
    load_error: Option<String>,
    hook: Hook,
    engine: Engine<WindowKey>,
    /// The last application window in the foreground, for tray menu actions:
    /// by the time a menu item is clicked the tray itself has the focus.
    last_target: Option<Hwnd>,
}

impl Controller {
    pub fn new(
        shared: Arc<Shared>,
        config: Arc<Resolved>,
        load_error: Option<String>,
        hook: Hook,
    ) -> Self {
        Self {
            shared,
            config,
            load_error,
            hook,
            engine: Engine::new(),
            last_target: window::foreground(),
        }
    }

    pub fn status(&self) -> String {
        tray::status_line(&self.config, self.load_error.as_deref())
    }

    pub fn config(&self) -> &Arc<Resolved> {
        &self.config
    }

    pub fn run(mut self, commands: Receiver<Command>) {
        while let Ok(command) = commands.recv() {
            match command {
                Command::Binding(index) => self.binding(index),
                Command::Menu(target) => {
                    let hwnd = self
                        .last_target
                        .filter(|hwnd| window::is_manageable(*hwnd))
                        .or_else(window::foreground);
                    if let Some(hwnd) = hwnd {
                        self.run_target(hwnd, target);
                    }
                }
                Command::System(event) => self.system(event),
                Command::Reload => self.reload(),
                Command::TogglePause => self.toggle_pause(),
                Command::ToggleAutostart => self.toggle_autostart(),
                Command::OpenConfigFolder => self.open_config_folder(),
                Command::Quit => {
                    if let Some(app) = self.shared.app.get() {
                        app.exit(0);
                    }
                    break;
                }
            }
        }
    }

    fn binding(&mut self, index: usize) {
        let Some(binding) = self.config.bindings.get(index) else {
            return;
        };
        let targets: Vec<Target> = binding.targets().collect();
        let chord = binding.chord;
        if window::full_screen_game_running() {
            tracing::debug!(%chord, "a full-screen game has the display; ignoring");
            return;
        }
        let Some(hwnd) = window::foreground() else {
            tracing::debug!(%chord, "no window to act on");
            return;
        };
        if let Some(exe) = self.ignored(hwnd) {
            tracing::debug!(%chord, exe, "ignored app");
            return;
        }
        let target = if targets.len() > 1 {
            let Some(frame) = window::visible_frame(hwnd) else {
                return;
            };
            let key = WindowKey::of(hwnd);
            self.engine.select(&key, &targets, frame)
        } else {
            targets.first().copied()
        };
        if let Some(target) = target {
            tracing::debug!(%chord, ?target, "shortcut");
            self.run_target(hwnd, target);
        }
    }

    /// The exe name, when the window's app is on the ignore list.
    fn ignored(&self, hwnd: Hwnd) -> Option<String> {
        if self.config.ignored_apps.is_empty() {
            return None;
        }
        let exe = window::exe_name(window::process_id(hwnd))?;
        self.config
            .ignored_apps
            .iter()
            .any(|ignored| ignored.eq_ignore_ascii_case(&exe))
            .then_some(exe)
    }

    fn run_target(&mut self, hwnd: Hwnd, target: Target) {
        let screens = monitors::screens();
        if let Target::Action(ActionId::Tile { rows, cols }) = target {
            self.tile(hwnd, rows, cols, &screens);
            return;
        }
        let Some(frame) = window::visible_frame(hwnd) else {
            return;
        };
        let key = WindowKey::of(hwnd);
        let others: Vec<PixelRect> = if matches!(
            target,
            Target::Action(ActionId::FillLeft | ActionId::FillRight)
        ) {
            window::manageable_windows()
                .into_iter()
                .filter(|other| *other != hwnd)
                .filter_map(window::visible_frame)
                .collect()
        } else {
            Vec::new()
        };
        let config = Arc::clone(&self.config);
        let request = Request {
            target,
            window: frame,
            screens: &screens,
            settings: &config.settings,
            specs: &config.specs,
            others: &others,
        };
        match self.engine.plan(&key, &request) {
            Plan::Move(planned) => {
                if let Some(achieved) = self.apply(hwnd, &planned, &screens) {
                    tracing::debug!(?target, from = %frame, to = %achieved, "moved");
                    self.engine.landed(&key, &planned, achieved);
                }
            }
            Plan::AlreadyThere => tracing::debug!(?target, "already there"),
            Plan::Nothing(why) => tracing::debug!(?target, why, "nothing to do"),
        }
    }

    /// Moves the window, then, if the app refused the size, places what it
    /// accepted against the edges the tile was meant to touch.
    fn apply(&self, hwnd: Hwnd, planned: &Move, screens: &[Screen]) -> Option<PixelRect> {
        let settings = &self.config.settings;
        let fixed_size =
            planned.resizes && !window::is_resizable(hwnd) && !window::is_maximized(hwnd);
        let result = if fixed_size {
            // A window that can't be resized keeps its size and moves only.
            window::visible_frame(hwnd)
                .ok_or(window::PlaceError::Gone)
                .and_then(|current| {
                    let aligned = align::realign(
                        planned.rect,
                        current.width(),
                        current.height(),
                        planned.initial,
                        planned.screen_visible,
                        settings.move_fixed_size_to_edge,
                        planned.gap,
                    );
                    window::move_to(hwnd, aligned.left, aligned.top)
                })
        } else {
            window::place(hwnd, planned.rect, planned.across_displays, screens).and_then(
                |achieved| {
                    let refused = (achieved.width() - planned.rect.width()).abs() > 1
                        || (achieved.height() - planned.rect.height()).abs() > 1;
                    if !refused {
                        return Ok(achieved);
                    }
                    let aligned = align::realign(
                        planned.rect,
                        achieved.width(),
                        achieved.height(),
                        planned.initial,
                        planned.screen_visible,
                        settings.move_fixed_size_to_edge,
                        planned.gap,
                    );
                    if (aligned.left, aligned.top) == (achieved.left, achieved.top) {
                        Ok(achieved)
                    } else {
                        window::move_to(hwnd, aligned.left, aligned.top)
                    }
                },
            )
        };
        match result {
            Ok(achieved) => Some(achieved),
            Err(error) => {
                let exe = window::exe_name(window::process_id(hwnd))
                    .unwrap_or_else(|| "a window".to_owned());
                tracing::warn!(%exe, %error, "could not move window");
                self.notify(&format!("Couldn't move {exe}: {error}"));
                None
            }
        }
    }

    fn tile(&mut self, focused: Hwnd, rows: u8, cols: u8, screens: &[Screen]) {
        let mut order = vec![focused];
        order.extend(
            window::manageable_windows()
                .into_iter()
                .filter(|hwnd| *hwnd != focused),
        );
        let windows: Vec<(WindowKey, PixelRect)> = order
            .into_iter()
            .filter(|hwnd| self.ignored(*hwnd).is_none())
            .filter_map(|hwnd| {
                window::visible_frame(hwnd).map(|frame| (WindowKey::of(hwnd), frame))
            })
            .collect();
        let config = Arc::clone(&self.config);
        for (key, rect) in self
            .engine
            .plan_tile(rows, cols, &windows, screens, &config.settings)
        {
            if let Err(error) = window::place(key.hwnd, rect, false, screens) {
                tracing::warn!(%error, "could not tile a window");
            }
        }
    }

    fn system(&mut self, event: SystemEvent) {
        match event {
            SystemEvent::WindowDestroyed(hwnd) => self.engine.retain(|key| key.hwnd != hwnd),
            SystemEvent::Foreground(hwnd) => {
                if window::is_manageable(hwnd) {
                    self.last_target = Some(hwnd);
                }
            }
            SystemEvent::SessionUnlocked | SystemEvent::Resumed => {
                tracing::info!("re-installing the keyboard hook");
                self.hook.reinstall();
            }
            SystemEvent::DisplaysChanged => {}
            SystemEvent::Control(Control::Quit) => {
                if let Some(app) = self.shared.app.get() {
                    app.exit(0);
                }
            }
            SystemEvent::Control(Control::Reload) => self.reload(),
            SystemEvent::Control(Control::TogglePause) => self.toggle_pause(),
        }
    }

    pub fn reload(&mut self) {
        let overrides = utwm_config::load::load_overrides(&self.shared.home);
        let loaded = overrides.and_then(|overrides| {
            if let Ok(mut watched) = self.shared.watched.lock() {
                *watched = utwm_config::load::watched_paths(
                    &self.shared.home,
                    self.shared.explicit_config.as_deref(),
                    &overrides,
                );
            }
            utwm_config::load::load_with(
                &self.shared.home,
                self.shared.explicit_config.as_deref(),
                &overrides,
            )
        });
        match loaded {
            Ok(resolved) => {
                tracing::info!(
                    source = %resolved.source.describe(),
                    bindings = resolved.bindings.len(),
                    skipped = resolved.skipped.len(),
                    "configuration loaded"
                );
                for warning in &resolved.warnings {
                    tracing::warn!("{warning}");
                }
                self.hook.set_bindings(
                    resolved
                        .bindings
                        .iter()
                        .map(|binding| binding.chord)
                        .collect(),
                );
                self.config = Arc::new(resolved);
                self.load_error = None;
                // History holds actions by name; a new export can't invalidate it.
            }
            Err(error) => {
                // Keep the last good bindings; say why the new ones didn't load.
                tracing::warn!(%error, "configuration not reloaded");
                self.load_error = Some(error.to_string());
            }
        }
        self.refresh_tray();
    }

    fn toggle_pause(&mut self) {
        let paused = !self.shared.paused.load(Ordering::SeqCst);
        self.shared.paused.store(paused, Ordering::SeqCst);
        self.hook.set_paused(paused);
        tracing::info!(paused, "shortcuts");
        self.refresh_tray();
    }

    fn toggle_autostart(&mut self) {
        let enable = !utwm_windows::autostart::is_enabled();
        let result = std::env::current_exe()
            .and_then(|exe| utwm_windows::autostart::set_enabled(enable, &exe));
        if let Err(error) = result {
            tracing::warn!(%error, "could not change start at login");
            self.notify(&format!("Couldn't change Start at login: {error}"));
        }
        self.refresh_tray();
    }

    fn open_config_folder(&self) {
        let folder = self
            .config
            .source
            .path
            .as_ref()
            .and_then(|path| path.parent().map(PathBuf::from))
            .unwrap_or_else(|| self.shared.home.join(".config").join("rectangle-pro"));
        let _ = std::fs::create_dir_all(&folder);
        if let Err(error) = std::process::Command::new("explorer.exe")
            .arg(&folder)
            .spawn()
        {
            tracing::warn!(%error, "could not open the configuration folder");
        }
    }

    fn refresh_tray(&self) {
        if let Some(app) = self.shared.app.get() {
            let paused = self.shared.paused.load(Ordering::SeqCst);
            if let Err(error) = tray::refresh(app, &self.config, &self.status(), paused) {
                tracing::warn!(%error, "could not update the tray menu");
            }
        }
    }

    /// Shows a short message as the tray tooltip.
    fn notify(&self, message: &str) {
        if let Some(app) = self.shared.app.get() {
            tray::set_tooltip(app, message);
        }
    }
}

/// Forwards hook events into the controller's channel.
pub fn hook_reporter(sender: Sender<Command>) -> utwm_windows::hook::Reporter {
    Box::new(move |event| {
        if let utwm_windows::hook::HookEvent::Triggered(index) = event {
            let _ = sender.send(Command::Binding(index));
        }
    })
}

/// Forwards system events into the controller's channel.
pub fn events_reporter(sender: Sender<Command>) -> utwm_windows::events::Reporter {
    Box::new(move |event| {
        let _ = sender.send(Command::System(event));
    })
}
