//! The worker that runs actions: it owns the per-window history and the
//! shortcut listener, and is the only thread that moves windows.
//!
//! Everything arrives as a [`Command`] on one channel — a binding from the
//! listener, a tray menu click, a system event, a configuration change — so the
//! order of events is the order they happened in and no state needs a lock.

#[cfg(target_os = "linux")]
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
#[cfg(target_os = "linux")]
use std::time::Duration;

use utwm_config::bindings::Resolved;
use utwm_config::{ActionId, Chord, Target};
use utwm_layout::{Engine, Move, PixelRect, Plan, Request, Screen, align};
#[cfg(windows)]
use utwm_windows::events::SystemEvent;

use crate::platform::{self, Control, Shortcuts, Window, monitors, window};
use crate::{tray, updates};

pub enum Command {
    /// The listener matched binding `index` of the current configuration.
    Binding(usize),
    /// A tray menu item for one action or custom position.
    Menu(Target),
    #[cfg(windows)]
    System(SystemEvent),
    /// The command line's `quit`, `reload` or `pause`.
    Control(Control),
    /// A configuration file changed, or a reload was asked for.
    Reload,
    TogglePause,
    ToggleAutostart,
    OpenConfigFolder,
    /// What the menu's first line reports changed: on the Mac, Accessibility
    /// access came or went, or a Rectangle started or quit; on Linux, the
    /// GNOME Shell extension came or went, or took a key it had refused.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    StatusChanged,
    /// The menu's first line was clicked; it takes the step it describes.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    FixStatus,
    /// GNOME can show a tray at last, so the tray is made.
    #[cfg(target_os = "linux")]
    TrayHostAppeared,
    /// A moment after GNOME's shortcuts were freed: whatever is still
    /// refused gets its GNOME shortcut back.
    #[cfg(target_os = "linux")]
    CheckFreed,
    /// The menu asked for an update check.
    CheckForUpdates,
    /// What the update thread is doing, for the menu.
    Updates(updates::State),
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
    /// The controller's own channel, for a tray made after startup and for
    /// checks it schedules for itself.
    #[cfg(target_os = "linux")]
    pub commands: Sender<Command>,
}

pub struct Controller {
    shared: Arc<Shared>,
    config: Arc<Resolved>,
    /// Why the last load failed, while running on a fallback.
    load_error: Option<String>,
    shortcuts: Shortcuts,
    /// Chords the system wouldn't register. The Mac refuses ones another app
    /// already holds; the Windows hook takes every chord.
    refused: Vec<Chord>,
    engine: Engine<window::Key>,
    /// Whether this run already asked macOS for Accessibility access, so a
    /// shortcut pressed without it asks once rather than on every press.
    #[cfg(target_os = "macos")]
    asked_for_access: bool,
    /// The GNOME settings holding refused chords, which the menu offers to
    /// free.
    #[cfg(target_os = "linux")]
    gnome_held: Vec<utwm_linux::conflicts::Holder>,
    /// GNOME shortcuts just freed, until the check a moment later.
    #[cfg(target_os = "linux")]
    freed: Vec<utwm_linux::conflicts::Holder>,
    /// Chords that freeing GNOME's shortcuts didn't free, which the menu no
    /// longer offers.
    #[cfg(target_os = "linux")]
    unfreeable: HashSet<Chord>,
    /// The update thread, when this copy was installed the way a release
    /// installs.
    updates: Option<updates::Updates>,
    update_state: updates::State,
    /// The last application window in the foreground, for tray menu actions:
    /// by the time a menu item is clicked on Windows the tray itself has the
    /// focus. Neither the Mac's menu bar nor GNOME's top bar takes it, so
    /// those keep none and act on the foreground window.
    last_target: Option<Window>,
}

impl Controller {
    pub fn new(
        shared: Arc<Shared>,
        config: Arc<Resolved>,
        load_error: Option<String>,
        shortcuts: Shortcuts,
        refused: Vec<Chord>,
        updates: Option<updates::Updates>,
    ) -> Self {
        Self {
            shared,
            config,
            load_error,
            shortcuts,
            refused,
            engine: Engine::new(),
            #[cfg(target_os = "macos")]
            asked_for_access: false,
            #[cfg(target_os = "linux")]
            gnome_held: Vec::new(),
            #[cfg(target_os = "linux")]
            freed: Vec::new(),
            #[cfg(target_os = "linux")]
            unfreeable: HashSet::new(),
            updates,
            update_state: updates::State::Idle,
            last_target: if cfg!(windows) {
                window::foreground()
            } else {
                None
            },
        }
    }

    pub fn status(&self) -> tray::Status {
        #[cfg(target_os = "macos")]
        {
            if !utwm_macos::permission::is_granted() {
                return tray::Status {
                    text: "Needs Accessibility access: open System Settings…".to_owned(),
                    clickable: true,
                };
            }
            if let Some(name) = utwm_macos::conflicts::rectangle_running() {
                return tray::Status::plain(format!(
                    "{name} is running too and answers the same shortcuts; quit it"
                ));
            }
        }
        #[cfg(target_os = "linux")]
        if let Some(status) = self.gnome_status() {
            return status;
        }
        tray::Status::plain(tray::status_line(
            &self.config,
            self.load_error.as_deref(),
            self.refused.len(),
        ))
    }

    pub fn config(&self) -> &Arc<Resolved> {
        &self.config
    }

    /// The menu's updates line, on a copy that updates itself.
    pub fn updates_line(&self) -> Option<tray::UpdatesLine> {
        self.updates.as_ref().map(|_| {
            let (text, enabled) = self.update_state.menu_line();
            tray::UpdatesLine { text, enabled }
        })
    }

    pub fn run(mut self, commands: Receiver<Command>) {
        while let Ok(command) = commands.recv() {
            match command {
                Command::Binding(index) => self.binding(index),
                Command::Menu(target) => {
                    let target_window = self
                        .last_target
                        .filter(|candidate| window::is_manageable(*candidate))
                        .or_else(window::foreground);
                    if let Some(target_window) = target_window {
                        self.run_target(target_window, target);
                    }
                }
                #[cfg(windows)]
                Command::System(event) => self.system(event),
                Command::Control(control) => self.control(control),
                Command::Reload => self.reload(),
                Command::TogglePause => self.toggle_pause(),
                Command::ToggleAutostart => self.toggle_autostart(),
                Command::OpenConfigFolder => self.open_config_folder(),
                #[cfg(target_os = "macos")]
                Command::StatusChanged => {
                    // A Rectangle that held some shortcuts may have quit.
                    if !self.refused.is_empty() {
                        self.refused = self.shortcuts.set_bindings(self.chords());
                    }
                    self.refresh_tray();
                }
                // The extension may be a new one holding nothing yet, or one
                // that just took a key it had refused.
                #[cfg(target_os = "linux")]
                Command::StatusChanged => {
                    let refused = self.shortcuts.set_bindings(self.chords());
                    self.set_refused(refused);
                    self.refresh_tray();
                }
                #[cfg(target_os = "macos")]
                Command::FixStatus => self.ask_for_access(),
                #[cfg(target_os = "linux")]
                Command::FixStatus => self.fix_status(),
                #[cfg(target_os = "linux")]
                Command::TrayHostAppeared => self.create_tray(),
                #[cfg(target_os = "linux")]
                Command::CheckFreed => self.check_freed(),
                Command::CheckForUpdates => self.check_for_updates(),
                Command::Updates(state) => {
                    self.update_state = state;
                    self.refresh_tray();
                }
                Command::Quit => {
                    self.quit();
                    break;
                }
            }
        }
    }

    fn chords(&self) -> Vec<Chord> {
        self.config
            .bindings
            .iter()
            .map(|binding| binding.chord)
            .collect()
    }

    fn quit(&self) {
        if let Some(app) = self.shared.app.get() {
            app.exit(0);
        }
    }

    /// Shows macOS's prompt, which also lists this app in System Settings,
    /// and opens the Accessibility pane where it is switched on.
    #[cfg(target_os = "macos")]
    fn ask_for_access(&mut self) {
        self.asked_for_access = true;
        if !utwm_macos::permission::request() {
            utwm_macos::permission::open_settings();
        }
    }

    fn control(&mut self, control: Control) {
        match control {
            Control::Quit => self.quit(),
            Control::Reload => self.reload(),
            Control::TogglePause => self.toggle_pause(),
            Control::Update => self.check_for_updates(),
        }
    }

    fn check_for_updates(&self) {
        match &self.updates {
            Some(updates) => updates.check_now(),
            None => {
                tracing::info!("this copy wasn't installed from a release, so it doesn't update")
            }
        }
    }

    fn binding(&mut self, index: usize) {
        let Some(binding) = self.config.bindings.get(index) else {
            return;
        };
        let targets: Vec<Target> = binding.targets().collect();
        let chord = tray::chord_label(binding.chord);
        #[cfg(windows)]
        if window::full_screen_game_running() {
            tracing::debug!(%chord, "a full-screen game has the display; ignoring");
            return;
        }
        #[cfg(target_os = "macos")]
        if !utwm_macos::permission::is_granted() {
            tracing::warn!(%chord, "no Accessibility access yet, so no window can move");
            if !self.asked_for_access {
                self.ask_for_access();
            }
            return;
        }
        let Some(target_window) = window::foreground() else {
            tracing::debug!(%chord, "no window to act on");
            return;
        };
        if let Some(app) = self.ignored(target_window) {
            tracing::debug!(%chord, app, "ignored app");
            return;
        }
        let target = if targets.len() > 1 {
            let Some(frame) = window::visible_frame(target_window) else {
                return;
            };
            let key = window::Key::of(target_window);
            self.engine.select(&key, &targets, frame)
        } else {
            targets.first().copied()
        };
        if let Some(target) = target {
            tracing::debug!(%chord, ?target, "shortcut");
            self.run_target(target_window, target);
        }
    }

    /// The app's name for `ignored_apps` (an executable name on Windows, a
    /// bundle id on the Mac), when it is on the ignore list.
    fn ignored(&self, target_window: Window) -> Option<String> {
        if self.config.ignored_apps.is_empty() {
            return None;
        }
        let app = window::app_id(target_window)?;
        self.config
            .ignored_apps
            .iter()
            .any(|ignored| ignored.eq_ignore_ascii_case(&app))
            .then_some(app)
    }

    fn run_target(&mut self, target_window: Window, target: Target) {
        let screens = monitors::screens();
        if let Target::Action(ActionId::Tile { rows, cols }) = target {
            self.tile(target_window, rows, cols, &screens);
            return;
        }
        let Some(frame) = window::visible_frame(target_window) else {
            return;
        };
        let key = window::Key::of(target_window);
        let others: Vec<PixelRect> = if matches!(
            target,
            Target::Action(ActionId::FillLeft | ActionId::FillRight)
        ) {
            window::manageable_windows()
                .into_iter()
                .filter(|other| *other != target_window)
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
                if let Some(achieved) = self.apply(target_window, &planned, &screens) {
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
    fn apply(
        &self,
        target_window: Window,
        planned: &Move,
        screens: &[Screen],
    ) -> Option<PixelRect> {
        let settings = &self.config.settings;
        let fixed_size = planned.resizes
            && !window::is_resizable(target_window)
            && !window::is_maximized(target_window);
        let result = if fixed_size {
            // A window that can't be resized keeps its size and moves only.
            window::visible_frame(target_window)
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
                    window::move_to(target_window, aligned.left, aligned.top)
                })
        } else {
            window::place(
                target_window,
                planned.rect,
                planned.across_displays,
                screens,
            )
            .and_then(|achieved| {
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
                    window::move_to(target_window, aligned.left, aligned.top)
                }
            })
        };
        match result {
            Ok(achieved) => Some(achieved),
            Err(error) => {
                let app = window::app_id(target_window).unwrap_or_else(|| "a window".to_owned());
                tracing::warn!(%app, %error, "could not move window");
                self.notify(&format!("Couldn't move {app}: {error}"));
                None
            }
        }
    }

    fn tile(&mut self, focused: Window, rows: u8, cols: u8, screens: &[Screen]) {
        let mut order = vec![focused];
        order.extend(
            window::manageable_windows()
                .into_iter()
                .filter(|candidate| *candidate != focused),
        );
        let windows: Vec<(window::Key, PixelRect)> = order
            .into_iter()
            .filter(|candidate| self.ignored(*candidate).is_none())
            .filter_map(|candidate| {
                window::visible_frame(candidate).map(|frame| (window::Key::of(candidate), frame))
            })
            .collect();
        let config = Arc::clone(&self.config);
        for (key, rect) in self
            .engine
            .plan_tile(rows, cols, &windows, screens, &config.settings)
        {
            if let Err(error) = window::place(key.window, rect, false, screens) {
                tracing::warn!(%error, "could not tile a window");
            }
        }
    }

    #[cfg(windows)]
    fn system(&mut self, event: SystemEvent) {
        match event {
            SystemEvent::WindowDestroyed(hwnd) => self.engine.retain(|key| key.window != hwnd),
            SystemEvent::Foreground(hwnd) => {
                if window::is_manageable(hwnd) {
                    self.last_target = Some(hwnd);
                }
            }
            SystemEvent::SessionUnlocked | SystemEvent::Resumed => {
                tracing::info!("re-installing the keyboard hook");
                self.shortcuts.reinstall();
            }
            SystemEvent::DisplaysChanged => {}
            SystemEvent::Control(control) => self.control(control),
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
                self.config = Arc::new(resolved);
                self.load_error = None;
                #[cfg(windows)]
                self.shortcuts.set_bindings(self.chords());
                #[cfg(target_os = "macos")]
                {
                    self.refused = self.shortcuts.set_bindings(self.chords());
                }
                #[cfg(target_os = "linux")]
                {
                    let refused = self.shortcuts.set_bindings(self.chords());
                    self.set_refused(refused);
                }
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
        #[cfg(windows)]
        self.shortcuts.set_paused(paused);
        #[cfg(target_os = "macos")]
        {
            self.refused = self.shortcuts.set_paused(paused);
        }
        #[cfg(target_os = "linux")]
        {
            let refused = self.shortcuts.set_paused(paused);
            self.set_refused(refused);
        }
        tracing::info!(paused, "shortcuts");
        self.refresh_tray();
    }

    fn toggle_autostart(&mut self) {
        let enable = !platform::autostart::is_enabled();
        let result =
            std::env::current_exe().and_then(|exe| platform::autostart::set_enabled(enable, &exe));
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
        if let Err(error) = platform::open_folder(&folder) {
            tracing::warn!(%error, "could not open the configuration folder");
        }
    }

    fn refresh_tray(&self) {
        if let Some(app) = self.shared.app.get() {
            let paused = self.shared.paused.load(Ordering::SeqCst);
            let updates = self.updates_line();
            if let Err(error) =
                tray::refresh(app, &self.config, &self.status(), paused, updates.as_ref())
            {
                tracing::warn!(%error, "could not update the tray menu");
            }
        }
    }

    /// Shows a short message: as the tray's tooltip, or on Linux, whose tray
    /// shows no tooltips, as a notification.
    fn notify(&self, message: &str) {
        if let Some(app) = self.shared.app.get() {
            #[cfg(not(target_os = "linux"))]
            tray::set_tooltip(app, message);
            #[cfg(target_os = "linux")]
            utwm_linux::notify::show(&app.package_info().name, message);
        }
    }

    /// The menu's first line on Linux when something needs doing: the next
    /// step toward a running GNOME Shell extension, or the offer to free the
    /// shortcuts GNOME holds.
    #[cfg(target_os = "linux")]
    fn gnome_status(&self) -> Option<tray::Status> {
        use utwm_linux::extension::State;

        let step = |text: &str| {
            Some(tray::Status {
                text: text.to_owned(),
                clickable: true,
            })
        };
        let state = utwm_linux::extension::state();
        let problem = match &state {
            State::NoGnomeShell => Some(tray::Status::plain(
                "Needs GNOME Shell; this desktop isn't supported yet".to_owned(),
            )),
            State::UserExtensionsOff => step("GNOME's extensions are switched off: switch them on"),
            State::NotInstalled => step("Needs its GNOME Shell extension: set it up"),
            State::NeedsLogIn => step("Log out and back in to start its GNOME Shell extension…"),
            State::Disabled => step("Its GNOME Shell extension is off: switch it on"),
            State::Failed(error) => Some(tray::Status::plain(format!(
                "Its GNOME Shell extension failed: {error}"
            ))),
            State::OutOfDate(version) => Some(tray::Status::plain(format!(
                "Its GNOME Shell extension doesn't support GNOME {version} yet"
            ))),
            State::Stale { compatible: false } => {
                step("Log out and back in to finish updating its GNOME Shell extension…")
            }
            State::Stale { compatible: true } | State::Active => None,
        };
        problem
            .or_else(|| {
                let held = self
                    .gnome_held
                    .iter()
                    .map(|holder| holder.chord)
                    .collect::<HashSet<_>>()
                    .len();
                (held > 0).then(|| tray::Status {
                    text: if held == 1 {
                        "1 shortcut is GNOME's own: use it here".to_owned()
                    } else {
                        format!("{held} shortcuts are GNOME's own: use them here")
                    },
                    clickable: true,
                })
            })
            .or_else(|| {
                matches!(state, State::Stale { compatible: true }).then(|| tray::Status {
                    text: "Log out and back in to finish updating its GNOME Shell extension…"
                        .to_owned(),
                    clickable: true,
                })
            })
    }

    /// Takes the step the menu's first line offers on Linux.
    #[cfg(target_os = "linux")]
    fn fix_status(&mut self) {
        use utwm_linux::extension::{self, State};

        let (result, what) = match extension::state() {
            State::Active | State::Stale { compatible: true } if !self.gnome_held.is_empty() => {
                (self.free_gnome_shortcuts(), "free GNOME's shortcuts")
            }
            State::Stale { compatible: true } => (extension::log_out(), "log out"),
            _ => (extension::set_up(), "set up the GNOME Shell extension"),
        };
        if let Err(error) = result {
            tracing::warn!(%error, what, "could not take the menu's step");
            self.notify(&format!("Couldn't {what}: {error}"));
        }
        self.refresh_tray();
    }

    /// Frees the keys GNOME's own shortcuts hold, leaving those shortcuts
    /// their other keys. The extension takes each key as GNOME lets go of it;
    /// a moment later, whatever it still couldn't take goes back to GNOME.
    #[cfg(target_os = "linux")]
    fn free_gnome_shortcuts(&mut self) -> std::io::Result<()> {
        self.freed = std::mem::take(&mut self.gnome_held);
        let freeing = utwm_linux::conflicts::free(&self.freed);
        let commands = self.shared.commands.clone();
        std::thread::Builder::new()
            .name("check-freed".to_owned())
            .spawn(move || {
                std::thread::sleep(Duration::from_secs(3));
                let _ = commands.send(Command::CheckFreed);
            })?;
        freeing
    }

    #[cfg(target_os = "linux")]
    fn check_freed(&mut self) {
        let refused = self.shortcuts.set_bindings(self.chords());
        let unhelped: Vec<utwm_linux::conflicts::Holder> = std::mem::take(&mut self.freed)
            .into_iter()
            .filter(|holder| refused.contains(&holder.chord))
            .collect();
        if !unhelped.is_empty() {
            tracing::warn!(
                count = unhelped.len(),
                "freeing GNOME's shortcuts didn't free these keys; giving them back"
            );
            if let Err(error) = utwm_linux::conflicts::restore(&unhelped) {
                tracing::warn!(%error, "could not give GNOME its shortcuts back");
            }
            self.unfreeable
                .extend(unhelped.iter().map(|holder| holder.chord));
        }
        self.set_refused(refused);
        self.refresh_tray();
    }

    /// Records the chords GNOME refused, and which of them GNOME's own
    /// shortcuts hold.
    #[cfg(target_os = "linux")]
    fn set_refused(&mut self, refused: Vec<Chord>) {
        let offered: Vec<Chord> = refused
            .iter()
            .filter(|chord| !self.unfreeable.contains(chord))
            .copied()
            .collect();
        self.gnome_held = utwm_linux::conflicts::holders(&offered);
        self.refused = refused;
    }

    /// Makes the tray, once GNOME can show one.
    #[cfg(target_os = "linux")]
    fn create_tray(&self) {
        let Some(app) = self.shared.app.get() else {
            return;
        };
        let paused = self.shared.paused.load(Ordering::SeqCst);
        let updates = self.updates_line();
        let made = tray::create(
            app,
            &self.config,
            &self.status(),
            paused,
            updates.as_ref(),
            self.shared.commands.clone(),
        );
        if let Err(error) = made {
            tracing::warn!(%error, "could not make the tray");
        }
    }
}

/// Forwards hook events into the controller's channel.
#[cfg(windows)]
pub fn hook_reporter(sender: Sender<Command>) -> utwm_windows::hook::Reporter {
    Box::new(move |event| {
        if let utwm_windows::hook::HookEvent::Triggered(index) = event {
            let _ = sender.send(Command::Binding(index));
        }
    })
}

/// Forwards system events into the controller's channel, the command line's
/// requests as the same [`Command::Control`] the Mac sends.
#[cfg(windows)]
pub fn events_reporter(sender: Sender<Command>) -> utwm_windows::events::Reporter {
    Box::new(move |event| {
        let command = match event {
            SystemEvent::Control(control) => Command::Control(control),
            other => Command::System(other),
        };
        let _ = sender.send(command);
    })
}

/// Forwards hot key presses into the controller's channel.
#[cfg(target_os = "macos")]
pub fn hotkeys_reporter(sender: Sender<Command>) -> utwm_macos::hotkeys::Reporter {
    Box::new(move |index| {
        let _ = sender.send(Command::Binding(index));
    })
}

/// Forwards the GNOME Shell extension's news into the controller's channel:
/// a shortcut pressed, or the extension coming, going or taking a key.
#[cfg(target_os = "linux")]
pub fn shortcuts_reporter(sender: Sender<Command>) -> utwm_linux::shortcuts::Reporter {
    use utwm_linux::shortcuts::ShortcutEvent;

    Box::new(move |event| {
        let command = match event {
            ShortcutEvent::Triggered(index) => Command::Binding(index),
            ShortcutEvent::ShellChanged => Command::StatusChanged,
        };
        let _ = sender.send(command);
    })
}

/// Forwards the command line's requests into the controller's channel.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn control_reporter(sender: Sender<Command>) -> Box<dyn Fn(Control) + Send + 'static> {
    Box::new(move |control| {
        let _ = sender.send(Command::Control(control));
    })
}
