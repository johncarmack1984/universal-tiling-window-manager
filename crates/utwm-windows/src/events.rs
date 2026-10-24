//! System notifications the app reacts to, on a thread of their own:
//! windows being destroyed (to drop their history), the foreground changing (so
//! the tray menu knows which window it is for), and the session unlocking or the
//! machine resuming (when the keyboard hook gets re-installed).

use std::cell::RefCell;
use std::thread::JoinHandle;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::RemoteDesktop::{
    NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification, WTSUnRegisterSessionNotification,
};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CHILDID_SELF, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    EVENT_OBJECT_DESTROY, EVENT_SYSTEM_FOREGROUND, FindWindowW, GetMessageW, MSG, OBJID_WINDOW,
    PBT_APMRESUMEAUTOMATIC, PostMessageW, PostThreadMessageW, RegisterClassExW, TranslateMessage,
    WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_APP, WM_DISPLAYCHANGE, WM_POWERBROADCAST,
    WM_QUIT, WM_WTSSESSION_CHANGE, WNDCLASSEXW, WS_OVERLAPPED, WTS_SESSION_UNLOCK,
};

use crate::{Hwnd, wide};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemEvent {
    WindowDestroyed(Hwnd),
    Foreground(Hwnd),
    SessionUnlocked,
    Resumed,
    DisplaysChanged,
    /// Another process (the `quit`, `reload` and `pause` commands) asked for this.
    Control(Control),
}

/// Requests the command line sends to the running instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Quit,
    Reload,
    TogglePause,
}

impl Control {
    fn message(self) -> u32 {
        match self {
            Self::Quit => WM_APP + 10,
            Self::Reload => WM_APP + 11,
            Self::TogglePause => WM_APP + 12,
        }
    }

    fn from_message(message: u32) -> Option<Self> {
        [Self::Quit, Self::Reload, Self::TogglePause]
            .into_iter()
            .find(|control| control.message() == message)
    }
}

/// The class of the hidden window that receives notifications; the command
/// line finds the running instance by it.
const CLASS_NAME: &str = "UniversalTilingWindowManagerEvents";

/// Receives system events, on the events thread.
pub type Reporter = Box<dyn Fn(SystemEvent) + Send + 'static>;

/// Sends `control` to the running instance. Returns false if none is running.
pub fn send_control(control: Control) -> bool {
    let class = wide(CLASS_NAME);
    // SAFETY: the class name outlives the call; a null title matches any.
    let window = unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) };
    if window.is_null() {
        return false;
    }
    // SAFETY: posts a plain message with no pointers to a window we found.
    unsafe { PostMessageW(window, control.message(), 0, 0) != 0 }
}

thread_local! {
    static REPORTER: RefCell<Option<Reporter>> = const { RefCell::new(None) };
}

fn emit(event: SystemEvent) {
    REPORTER.with(|reporter| {
        if let Some(report) = reporter.borrow().as_ref() {
            report(event);
        }
    });
}

pub struct Events {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl Events {
    pub fn start(report: Reporter) -> std::io::Result<Self> {
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<u32>();
        let thread = std::thread::Builder::new()
            .name("system-events".to_owned())
            .spawn(move || run(report, ready_tx))?;
        let thread_id = ready_rx
            .recv()
            .map_err(|_| std::io::Error::other("the system events thread exited"))?;
        Ok(Self {
            thread_id,
            thread: Some(thread),
        })
    }
}

impl Drop for Events {
    fn drop(&mut self) {
        // SAFETY: posts to a thread we created; no pointers involved.
        unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0) };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(report: Reporter, ready: std::sync::mpsc::Sender<u32>) {
    REPORTER.with(|slot| *slot.borrow_mut() = Some(report));
    // SAFETY: no arguments.
    let thread_id = unsafe { GetCurrentThreadId() };

    // WinEvent hooks deliver to this thread's message loop.
    // SAFETY: `win_event` has the WINEVENTPROC signature and lives forever.
    let destroy_hook = unsafe {
        SetWinEventHook(
            EVENT_OBJECT_DESTROY,
            EVENT_OBJECT_DESTROY,
            std::ptr::null_mut(),
            Some(win_event),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        )
    };
    // SAFETY: as above.
    let foreground_hook = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            std::ptr::null_mut(),
            Some(win_event),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        )
    };

    // Session and power notifications need a real top-level window; it is
    // never shown.
    let window = create_hidden_window();
    if !window.is_null() {
        // SAFETY: `window` is a window this thread owns.
        unsafe { WTSRegisterSessionNotification(window, NOTIFY_FOR_THIS_SESSION) };
    }
    let _ = ready.send(thread_id);

    let mut msg = MSG::default();
    loop {
        // SAFETY: `msg` is a valid MSG for this thread.
        let got = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
        if got <= 0 {
            break;
        }
        // SAFETY: standard dispatch of a message just retrieved.
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    // SAFETY: tearing down what this thread created.
    unsafe {
        if !window.is_null() {
            WTSUnRegisterSessionNotification(window);
            DestroyWindow(window);
        }
        UnhookWinEvent(destroy_hook);
        UnhookWinEvent(foreground_hook);
    }
}

fn create_hidden_window() -> HWND {
    let class_name = wide(CLASS_NAME);
    // SAFETY: a null module name returns this executable's handle.
    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
    let class = WNDCLASSEXW {
        cbSize: u32::try_from(std::mem::size_of::<WNDCLASSEXW>()).unwrap_or(80),
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        lpszClassName: class_name.as_ptr(),
        ..Default::default()
    };
    // SAFETY: `class` is fully initialised and its name outlives the call.
    unsafe { RegisterClassExW(&class) };
    let title = wide("universal-tiling-window-manager events");
    // SAFETY: the class was registered above; the strings outlive the call.
    unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_WTSSESSION_CHANGE if wparam == usize::try_from(WTS_SESSION_UNLOCK).unwrap_or(0) => {
            emit(SystemEvent::SessionUnlocked);
        }
        WM_POWERBROADCAST if wparam == usize::try_from(PBT_APMRESUMEAUTOMATIC).unwrap_or(0) => {
            emit(SystemEvent::Resumed);
        }
        WM_DISPLAYCHANGE => emit(SystemEvent::DisplaysChanged),
        other => {
            if let Some(control) = Control::from_message(other) {
                emit(SystemEvent::Control(control));
                return 0;
            }
        }
    }
    // SAFETY: default handling for a window this thread owns.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

unsafe extern "system" fn win_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    object: i32,
    child: i32,
    _thread: u32,
    _time: u32,
) {
    if hwnd.is_null() || object != OBJID_WINDOW || child != i32::try_from(CHILDID_SELF).unwrap_or(0)
    {
        return;
    }
    match event {
        EVENT_OBJECT_DESTROY => emit(SystemEvent::WindowDestroyed(hwnd as Hwnd)),
        EVENT_SYSTEM_FOREGROUND => emit(SystemEvent::Foreground(hwnd as Hwnd)),
        _ => {}
    }
}
