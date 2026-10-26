//! The end-to-end check on Windows, on whatever displays it has. It opens a
//! window of its own, focuses it, presses the configured shortcuts the way
//! RustDesk delivers them (scan codes, left-hand modifiers), and checks that
//! the window lands exactly where the layout engine says it should, given the
//! same configuration. It never touches any other window, and puts NumLock
//! back the way it found it.
//!
//! It presses what the Mac and Linux checks press, then every other shortcut
//! the configuration binds, since on Windows each key reaches the app as a
//! scan code of its own. The keypad goes twice, NumLock on and then off: with
//! it off, Windows reports most keypad keys as the navigation keys that share
//! their scan codes (keypad 7 as Home), which the app must still tell apart.
//! Last, a half must bring the window out of a real Windows maximize, and a
//! lock key must pass through.
//!
//! Start the app first (`just run`).

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use utwm_config::bindings::Resolved;
use utwm_config::{ActionId, Chord, Modifiers, PhysicalKey, Platform, Target};
use utwm_layout::{Engine, PixelRect, Plan, Request, Screen};
use utwm_windows::events::{Control, send_control};
use utwm_windows::{inject, monitors, window};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_NUMLOCK};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, MSG, PostQuitMessage,
    RegisterClassExW, SW_MAXIMIZE, SW_SHOW, SetForegroundWindow, ShowWindow, TranslateMessage,
    WM_DESTROY, WNDCLASSEXW, WS_OVERLAPPEDWINDOW,
};

/// The left-hand modifiers, as RustDesk sends them.
const CTRL: PhysicalKey = PhysicalKey::new(0x1D);
const ALT: PhysicalKey = PhysicalKey::new(0x38);
const SHIFT: PhysicalKey = PhysicalKey::new(0x2A);
const WIN: PhysicalKey = PhysicalKey::ext(0x5B);
const NUM_LOCK: PhysicalKey = PhysicalKey::new(0x45);

/// Where the window starts, in physical pixels from the top left of the main
/// display; Restore brings it back here.
const START: PixelRect = PixelRect::new(300, 200, 1100, 800);

/// What gets pressed first, in order, as on the Mac and Linux. With
/// Rectangle's default cycle sizes the three left halves are ½, ⅔ and ⅓. The
/// display moves need a second display; with one, the engine has nothing to
/// plan for them and they are skipped.
const PRESSES: &[ActionId] = &[
    ActionId::LeftHalf,
    ActionId::LeftHalf,
    ActionId::LeftHalf,
    ActionId::RightHalf,
    ActionId::NextDisplay,
    ActionId::PreviousDisplay,
    ActionId::TopLeft,
    ActionId::NextDisplay,
    ActionId::BottomRight,
    ActionId::PreviousDisplay,
    ActionId::FirstThird,
    ActionId::CenterTwoThirds,
    ActionId::Maximize,
    ActionId::NextDisplay,
    ActionId::PreviousDisplay,
    ActionId::Smaller,
    ActionId::Center,
    ActionId::Restore,
];

pub fn run() -> ExitCode {
    // Physical pixels throughout, as the app sees the displays.
    monitors::ensure_per_monitor_dpi_awareness();
    let config = match load() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("could not load the configuration: {error}");
            return ExitCode::FAILURE;
        }
    };
    // The app reads its configuration again, so both start from the same one.
    if !send_control(Control::Reload) {
        eprintln!("start universal-tiling-window-manager first");
        return ExitCode::FAILURE;
    }
    std::thread::sleep(Duration::from_millis(1500));
    let numlock_was_on = numlock_on();
    let window = spawn_window();
    if window == 0 {
        eprintln!("could not open the test window");
        return ExitCode::FAILURE;
    }
    let passed = checks(&config, window);
    set_numlock(numlock_was_on);
    println!("{}", if passed { "passed" } else { "failed" });
    if passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// The app's own configuration, found the way the app finds it.
fn load() -> Result<Resolved, Box<dyn std::error::Error>> {
    let home = std::env::home_dir().ok_or("no home directory")?;
    let explicit = std::env::var_os("UTWM_CONFIG").map(PathBuf::from);
    Ok(utwm_config::load(&home, explicit.as_deref())?)
}

fn checks(config: &Resolved, window: window::Hwnd) -> bool {
    if !wait_for(Duration::from_secs(10), || {
        window::foreground_root() == Some(window)
    }) {
        println!("FAIL the test window never got the focus; click it and run again");
        return false;
    }
    std::thread::sleep(Duration::from_millis(300));
    let mut check = Check {
        config,
        screens: monitors::screens(),
        window,
        engine: Engine::new(),
        passed: true,
    };

    for action in PRESSES {
        if check.action(*action, "") == Outcome::Stop {
            return false;
        }
    }

    // Every other shortcut, with the keypad reading as numbers.
    set_numlock(true);
    let pressed: Vec<Chord> = PRESSES
        .iter()
        .filter_map(|action| config.chord_for(Target::Action(*action)))
        .collect();
    for binding in &config.bindings {
        if pressed.contains(&binding.chord) {
            continue;
        }
        if check.binding(binding.chord, "") == Outcome::Stop {
            return false;
        }
    }

    // The keypad again, now reading as navigation keys.
    set_numlock(false);
    for binding in &config.bindings {
        let chord = binding.chord;
        if !turns_with_numlock(chord.key) {
            continue;
        }
        // Were the app to take the key for the navigation key it now reads
        // as, it would run that key's action instead.
        let twin = Chord {
            modifiers: chord.modifiers,
            key: PhysicalKey::ext(chord.key.scan),
        };
        if config
            .binding_for(&twin)
            .is_some_and(|binding| binding.targets().any(moves_other_windows))
        {
            println!(
                "skip {} with NumLock off: a mix-up would run {}, which moves every window",
                chord.label(Platform::Windows),
                twin.label(Platform::Windows)
            );
            continue;
        }
        if check.binding(chord, " (NumLock off)") == Outcome::Stop {
            return false;
        }
    }

    // From a real Windows maximize, a half must come out restored.
    // SAFETY: maximizes our own window.
    unsafe { ShowWindow(window as HWND, SW_MAXIMIZE) };
    std::thread::sleep(Duration::from_millis(400));
    match check.action(ActionId::LeftHalf, " (from maximized)") {
        Outcome::Stop => return false,
        Outcome::Pressed if window::is_maximized(window) => {
            println!("FAIL the window is still in the maximized state");
            check.passed = false;
        }
        Outcome::Pressed | Outcome::Skipped => {}
    }

    // A lock key passes through and moves nothing.
    let lock = Chord {
        modifiers: Modifiers {
            ctrl: true,
            alt: true,
            ..Modifiers::NONE
        },
        key: NUM_LOCK,
    };
    let shortcut = lock.label(Platform::Windows);
    let before = window::visible_frame(window);
    if window::foreground_root() != Some(window) {
        println!("FAIL the test window lost the focus; stopping before {shortcut}");
        return false;
    }
    inject::chord(&modifier_keys(lock), lock.key);
    std::thread::sleep(Duration::from_millis(300));
    if window::visible_frame(window) == before {
        println!("ok   {shortcut:<22} moves nothing");
    } else {
        println!("FAIL {shortcut:<22} moved the window");
        check.passed = false;
    }
    check.passed
}

/// What came of one shortcut.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Outcome {
    /// It went in, and where the window landed was checked.
    Pressed,
    /// There was nothing to check, so it wasn't pressed.
    Skipped,
    /// The check can't go on: its window lost the focus or is gone.
    Stop,
}

/// The check under way: the configuration, and the engine planning each
/// landing as the app's engine does.
struct Check<'a> {
    config: &'a Resolved,
    screens: Vec<Screen>,
    window: window::Hwnd,
    engine: Engine<()>,
    passed: bool,
}

impl Check<'_> {
    /// Presses the shortcut that runs `action`, if the configuration has one.
    fn action(&mut self, action: ActionId, note: &str) -> Outcome {
        let Some(chord) = self.config.chord_for(Target::Action(action)) else {
            println!("skip {action}{note}: not bound in this configuration");
            return Outcome::Skipped;
        };
        self.binding(chord, note)
    }

    /// Presses `chord`, unless the engine can't say where its action puts the
    /// window from the window alone.
    fn binding(&mut self, chord: Chord, note: &str) -> Outcome {
        let config = self.config;
        let shortcut = chord.label(Platform::Windows);
        let Some(binding) = config.binding_for(&chord) else {
            println!("skip {shortcut}{note}: not bound in this configuration");
            return Outcome::Skipped;
        };
        // Which action a cycling shortcut runs depends on the window's
        // history in the app, which this check can't see.
        if binding.is_cycle() {
            println!("skip {shortcut}{note}: it cycles through several actions");
            return Outcome::Skipped;
        }
        let Some(entry) = binding.entries.first() else {
            return Outcome::Skipped;
        };
        if moves_other_windows(entry.target) {
            println!("skip {shortcut}{note}: {} moves every window", entry.name);
            return Outcome::Skipped;
        }
        // Fill takes the room the other windows leave, which this check
        // doesn't look at.
        if matches!(
            entry.target,
            Target::Action(ActionId::FillLeft | ActionId::FillRight)
        ) {
            println!(
                "skip {shortcut}{note}: where {} lands depends on the other windows",
                entry.name
            );
            return Outcome::Skipped;
        }
        self.press(chord, entry.target, &entry.name, note)
    }

    /// Presses `chord`, which runs `target`, and checks that the window lands
    /// where the engine plans.
    fn press(&mut self, chord: Chord, target: Target, name: &str, note: &str) -> Outcome {
        let shortcut = chord.label(Platform::Windows);
        let Some(before) = window::visible_frame(self.window) else {
            println!("FAIL the test window is gone");
            self.passed = false;
            return Outcome::Stop;
        };
        let request = Request {
            target,
            window: before,
            screens: &self.screens,
            settings: &self.config.settings,
            specs: &self.config.specs,
            others: &[],
        };
        let planned = match self.engine.plan(&(), &request) {
            Plan::Move(planned) => planned,
            Plan::AlreadyThere => {
                println!("skip {shortcut} {name}{note}: the window is already there");
                return Outcome::Skipped;
            }
            Plan::Nothing(why) => {
                println!("skip {shortcut} {name}{note}: {why}");
                return Outcome::Skipped;
            }
        };
        // A shortcut moves whichever window has the focus, so each press
        // waits on the test window still having it.
        if window::foreground_root() != Some(self.window) {
            println!(
                "FAIL the test window lost the focus; stopping before {shortcut} moves another window"
            );
            self.passed = false;
            return Outcome::Stop;
        }
        inject::chord(&modifier_keys(chord), chord.key);
        let landed = settle(self.window, before, planned.rect);
        self.engine.landed(&(), &planned, landed);
        if landed == planned.rect {
            println!("ok   {shortcut:<22} {name:<24} {landed}{note}");
        } else {
            println!(
                "FAIL {shortcut:<22} {name:<24} {landed}, expected {}{note}",
                planned.rect
            );
            self.passed = false;
        }
        Outcome::Pressed
    }
}

/// Tiling lays out every window on the display, so the check never runs it.
fn moves_other_windows(target: Target) -> bool {
    matches!(target, Target::Action(ActionId::Tile { .. }))
}

/// The keypad keys NumLock switches between numbers and navigation: all but
/// the operators, Enter and keypad =.
fn turns_with_numlock(key: PhysicalKey) -> bool {
    !key.extended && matches!(key.scan, 0x47..=0x49 | 0x4B..=0x4D | 0x4F..=0x53)
}

/// The chord's modifiers as RustDesk presses them.
fn modifier_keys(chord: Chord) -> Vec<PhysicalKey> {
    [
        (chord.modifiers.ctrl, CTRL),
        (chord.modifiers.alt, ALT),
        (chord.modifiers.shift, SHIFT),
        (chord.modifiers.win, WIN),
    ]
    .into_iter()
    .filter_map(|(on, key)| on.then_some(key))
    .collect()
}

/// Polls `done` every 100 ms for up to `limit`.
fn wait_for(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + limit;
    loop {
        if done() {
            return true;
        }
        if Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Waits for the window to reach `expected`, or to have moved and then held
/// still for 300 ms, up to two seconds.
fn settle(window: window::Hwnd, before: PixelRect, expected: PixelRect) -> PixelRect {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut last = before;
    let mut still_since = Instant::now();
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(25));
        let Some(now) = window::visible_frame(window) else {
            return last;
        };
        if now == expected {
            return now;
        }
        if now != last {
            last = now;
            still_since = Instant::now();
        } else if now != before && still_since.elapsed() >= Duration::from_millis(300) {
            return now;
        }
    }
    last
}

unsafe extern "system" fn proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_DESTROY {
        // SAFETY: ends this thread's message loop.
        unsafe { PostQuitMessage(0) };
        return 0;
    }
    // SAFETY: default handling for our own window.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

/// Creates a plain resizable window on a thread that pumps its messages,
/// so the app can move it like any other app's window.
fn spawn_window() -> window::Hwnd {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let name: Vec<u16> = "utwm-e2e\0".encode_utf16().collect();
        // SAFETY: standard window class registration and creation; the
        // strings outlive the calls.
        let hwnd = unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let class = WNDCLASSEXW {
                cbSize: u32::try_from(std::mem::size_of::<WNDCLASSEXW>()).unwrap_or(80),
                lpfnWndProc: Some(proc),
                hInstance: instance,
                lpszClassName: name.as_ptr(),
                ..Default::default()
            };
            RegisterClassExW(&class);
            CreateWindowExW(
                0,
                name.as_ptr(),
                name.as_ptr(),
                WS_OVERLAPPEDWINDOW,
                START.left,
                START.top,
                START.width(),
                START.height(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            )
        };
        // SAFETY: shows and focuses the window just created.
        unsafe {
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
        }
        let _ = tx.send(hwnd as window::Hwnd);
        let mut msg = MSG::default();
        // SAFETY: a standard message loop for this thread's window.
        unsafe {
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    });
    rx.recv().unwrap_or(0)
}

fn numlock_on() -> bool {
    // SAFETY: reads the toggle state of a virtual key.
    unsafe { GetKeyState(i32::from(VK_NUMLOCK)) & 1 == 1 }
}

fn set_numlock(on: bool) {
    if numlock_on() != on {
        inject::chord(&[], NUM_LOCK);
        std::thread::sleep(Duration::from_millis(100));
    }
}
