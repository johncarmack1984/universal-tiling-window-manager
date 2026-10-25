//! The end-to-end check on Windows, on a 1920×1080 display with a bottom
//! taskbar (a 1920×1032 work area), using John's export. It opens a window of
//! its own, focuses it, presses shortcuts the way RustDesk delivers them (scan
//! codes, left Ctrl and Alt), and checks where the visible frame lands. It never
//! touches any other window, and puts NumLock back the way it found it.

use std::time::{Duration, Instant};

use utwm_config::PhysicalKey;
use utwm_layout::PixelRect;
use utwm_windows::{inject, monitors, window};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_NUMLOCK};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, MSG, PostQuitMessage,
    RegisterClassExW, SW_MAXIMIZE, SW_SHOW, SetForegroundWindow, ShowWindow, TranslateMessage,
    WM_DESTROY, WNDCLASSEXW, WS_OVERLAPPEDWINDOW,
};

const CTRL: PhysicalKey = PhysicalKey::new(0x1D);
const ALT: PhysicalKey = PhysicalKey::new(0x38);
const START: PixelRect = PixelRect::new(300, 200, 1100, 800);

fn key(name: &str) -> PhysicalKey {
    PhysicalKey::from_windows_name(name).unwrap_or(PhysicalKey::new(0))
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

fn focus(hwnd: window::Hwnd) {
    // SAFETY: plain call on our own window.
    unsafe { SetForegroundWindow(hwnd as HWND) };
    std::thread::sleep(Duration::from_millis(150));
}

/// Waits for the window to settle somewhere, then returns its frame.
fn settled(hwnd: window::Hwnd) -> Option<PixelRect> {
    let deadline = Instant::now() + Duration::from_millis(1500);
    let mut last = window::visible_frame(hwnd);
    loop {
        std::thread::sleep(Duration::from_millis(120));
        let now = window::visible_frame(hwnd);
        if now == last || Instant::now() > deadline {
            return now;
        }
        last = now;
    }
}

fn numlock_on() -> bool {
    // SAFETY: reads the toggle state of a virtual key.
    unsafe { GetKeyState(i32::from(VK_NUMLOCK)) & 1 == 1 }
}

fn set_numlock(on: bool) {
    if numlock_on() != on {
        inject::chord(&[], key("NumLock"));
        std::thread::sleep(Duration::from_millis(100));
    }
}

struct Report {
    passed: usize,
    failed: usize,
}

impl Report {
    fn check(&mut self, name: &str, actual: Option<PixelRect>, expected: PixelRect) {
        match actual {
            Some(rect) if rect == expected => {
                self.passed += 1;
                println!("ok    {name}: {rect}");
            }
            other => {
                self.failed += 1;
                println!("FAIL  {name}: expected {expected}, got {other:?}");
            }
        }
    }
}

fn press(hwnd: window::Hwnd, name: &str) -> Option<PixelRect> {
    focus(hwnd);
    inject::chord(&[CTRL, ALT], key(name));
    settled(hwnd)
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let screens = monitors::screens();
    let work = screens
        .first()
        .map(|screen| screen.work)
        .ok_or("no display")?;
    if work != PixelRect::new(0, 0, 1920, 1032) {
        return Err(format!("expected a 1920×1032 work area, found {work}").into());
    }
    if !utwm_windows::events::send_control(utwm_windows::events::Control::Reload) {
        return Err("start universal-tiling-window-manager first".into());
    }
    std::thread::sleep(Duration::from_millis(1500));
    let numlock_was_on = numlock_on();
    let hwnd = spawn_window();
    if hwnd == 0 {
        return Err("could not create the test window".into());
    }
    std::thread::sleep(Duration::from_millis(400));
    let start_visible = window::visible_frame(hwnd).ok_or("the test window has no frame")?;
    let mut report = Report {
        passed: 0,
        failed: 0,
    };

    report.check(
        "Ctrl+Alt+Left",
        press(hwnd, "Left"),
        PixelRect::new(0, 0, 960, 1032),
    );
    // Rectangle Pro's repeat moves a half on to the next display that way;
    // with one display there is none, so the window stays.
    report.check(
        "…again: no display to the left",
        press(hwnd, "Left"),
        PixelRect::new(0, 0, 960, 1032),
    );
    report.check(
        "Ctrl+Alt+Right",
        press(hwnd, "Right"),
        PixelRect::new(960, 0, 1920, 1032),
    );
    report.check(
        "Ctrl+Alt+Backspace restores",
        press(hwnd, "Backspace"),
        start_visible,
    );
    report.check(
        "Ctrl+Alt+Enter maximizes",
        press(hwnd, "Enter"),
        PixelRect::new(0, 0, 1920, 1032),
    );
    report.check(
        "Ctrl+Alt+C centers",
        press(hwnd, "C"),
        PixelRect::new(0, 0, 1920, 1032),
    );
    report.check(
        "Ctrl+Alt+F16 first fourth",
        press(hwnd, "F16"),
        PixelRect::new(0, 0, 480, 1032),
    );
    report.check(
        "Ctrl+Alt+U top left",
        press(hwnd, "U"),
        PixelRect::new(0, 0, 960, 516),
    );
    report.check(
        "Ctrl+Alt+Num= eighth",
        press(hwnd, "Num="),
        PixelRect::new(480, 0, 960, 516),
    );
    report.check(
        "Ctrl+Alt+Num/ eighth",
        press(hwnd, "Num/"),
        PixelRect::new(960, 0, 1440, 516),
    );
    set_numlock(true);
    report.check(
        "Ctrl+Alt+Num7, NumLock on",
        press(hwnd, "Num7"),
        PixelRect::new(0, 516, 480, 1032),
    );
    set_numlock(false);
    report.check(
        "Ctrl+Alt+Num5, NumLock off",
        press(hwnd, "Num5"),
        PixelRect::new(640, 0, 1280, 516),
    );
    report.check(
        "Ctrl+Alt+Num3, NumLock off",
        press(hwnd, "Num3"),
        PixelRect::new(1280, 516, 1920, 1032),
    );
    set_numlock(numlock_was_on);
    report.check(
        "Ctrl+Alt+X custom position",
        press(hwnd, "X"),
        PixelRect::new(0, 310, 639, 1032),
    );
    report.check(
        "Ctrl+Alt+E two thirds",
        press(hwnd, "E"),
        PixelRect::new(0, 0, 1280, 1032),
    );
    // Against the left, top and bottom edges: it grows 30 px to the right only.
    report.check(
        "Ctrl+Alt+= larger",
        press(hwnd, "="),
        PixelRect::new(0, 0, 1310, 1032),
    );

    // From a real (Windows) maximize, a half must come out restored.
    focus(hwnd);
    // SAFETY: maximizes our own window.
    unsafe { ShowWindow(hwnd as HWND, SW_MAXIMIZE) };
    std::thread::sleep(Duration::from_millis(400));
    report.check(
        "Ctrl+Alt+Left from maximized",
        press(hwnd, "Left"),
        PixelRect::new(0, 0, 960, 1032),
    );
    if window::is_maximized(hwnd) {
        report.failed += 1;
        println!("FAIL  the window is still in the maximized state");
    }

    // A lock key passes through and moves nothing.
    let before = window::visible_frame(hwnd);
    focus(hwnd);
    inject::chord(&[CTRL, ALT], key("NumLock"));
    std::thread::sleep(Duration::from_millis(300));
    set_numlock(numlock_was_on);
    if window::visible_frame(hwnd) == before {
        report.passed += 1;
        println!("ok    Ctrl+Alt+NumLock moves nothing");
    } else {
        report.failed += 1;
        println!("FAIL  Ctrl+Alt+NumLock moved the window");
    }

    println!("\n{} passed, {} failed", report.passed, report.failed);
    if report.failed > 0 {
        return Err("end-to-end check failed".into());
    }
    Ok(())
}
