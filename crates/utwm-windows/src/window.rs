//! Finding, measuring and moving other apps' top-level windows.
//!
//! Positions are always *visible* frames: Windows 10 and 11 pad most windows
//! with invisible resize borders, so `GetWindowRect` is a few pixels bigger than
//! what you see. The visible frame comes from DWM, and the outer rect handed to
//! `SetWindowPos` is the target grown by whatever padding this window has
//! (FancyZones' `SizeWindowToRect` does the same).

use std::ffi::c_void;

use utwm_layout::{PixelRect, Screen, screens};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, FILETIME, GetLastError, HWND, LPARAM, RECT,
};
use windows_sys::Win32::Graphics::Dwm::{
    DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, GetProcessTimes, OpenProcess, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::Shell::{QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GA_ROOT, GUI_INMENUMODE, GUI_POPUPMENUMODE, GUITHREADINFO, GWL_EXSTYLE, GWL_STYLE,
    GetAncestor, GetClassNameW, GetForegroundWindow, GetGUIThreadInfo, GetWindowLongPtrW,
    GetWindowPlacement, GetWindowRect, GetWindowThreadProcessId, IsHungAppWindow, IsIconic,
    IsWindowVisible, IsZoomed, SW_RESTORE, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOSIZE,
    SWP_NOZORDER, SetForegroundWindow, SetWindowPlacement, SetWindowPos, WINDOWPLACEMENT,
    WPF_RESTORETOMAXIMIZED, WS_CAPTION, WS_CHILD, WS_EX_APPWINDOW, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_POPUP, WS_THICKFRAME,
};
use windows_sys::core::BOOL;

use crate::to_pixel_rect;

/// A window handle as an integer, so it can cross threads.
pub type Hwnd = isize;

fn raw(hwnd: Hwnd) -> HWND {
    hwnd as HWND
}

fn handle(hwnd: HWND) -> Hwnd {
    hwnd as Hwnd
}

/// A window, told apart from a later window that reuses its handle: what the
/// engine keeps history under.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub window: Hwnd,
    pid: u32,
    started: u64,
}

impl Key {
    pub fn of(hwnd: Hwnd) -> Self {
        let pid = process_id(hwnd);
        Self {
            window: hwnd,
            pid,
            started: process_start_time(pid).unwrap_or(0),
        }
    }
}

/// Shell surfaces that are top-level windows but never targets.
const SHELL_CLASSES: &[&str] = &[
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "TopLevelWindowForOverflowXamlIsland",
    "XamlExplorerHostIslandWindow",
    "NotifyIconOverflowWindow",
];

/// The window keyboard actions apply to: the root of the foreground window,
/// if it is one this app may move.
pub fn foreground() -> Option<Hwnd> {
    // SAFETY: no arguments; returns a handle or null.
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_null() {
        return None;
    }
    // SAFETY: `foreground` is a window handle the system just returned.
    let root = unsafe { GetAncestor(foreground, GA_ROOT) };
    let root = if root.is_null() { foreground } else { root };
    is_manageable(handle(root)).then_some(handle(root))
}

/// The root of the foreground window, manageable or not (for tracking which
/// window the tray menu should act on).
pub fn foreground_root() -> Option<Hwnd> {
    // SAFETY: no arguments; returns a handle or null.
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_null() {
        return None;
    }
    // SAFETY: `foreground` is a window handle the system just returned.
    let root = unsafe { GetAncestor(foreground, GA_ROOT) };
    Some(handle(if root.is_null() { foreground } else { root }))
}

/// Whether the focus sits on one of this app's own windows with no menu open,
/// where the tray leaves it once its menu closes. While it does, Windows keeps
/// the keyboard from this app's own hook, so no shortcut works until another
/// window has the focus.
pub fn focus_left_on_own_window() -> bool {
    let Some(root) = foreground_root() else {
        return false;
    };
    if !is_own(root) {
        return false;
    }
    let mut info = GUITHREADINFO {
        cbSize: u32::try_from(std::mem::size_of::<GUITHREADINFO>()).unwrap_or(72),
        ..Default::default()
    };
    // SAFETY: `info` is a GUITHREADINFO with its size set, for the thread that
    // owns a window the system just returned.
    let read = unsafe {
        GetGUIThreadInfo(
            GetWindowThreadProcessId(raw(root), std::ptr::null_mut()),
            &mut info,
        )
    };
    read != 0 && info.flags & (GUI_INMENUMODE | GUI_POPUPMENUMODE) == 0
}

/// Gives the window the focus. Windows allows it only while this app has the
/// focus, as it does once the tray menu closes.
pub fn activate(hwnd: Hwnd) -> bool {
    // SAFETY: plain call on a window handle.
    unsafe { SetForegroundWindow(raw(hwnd)) != 0 }
}

fn long(hwnd: Hwnd, index: i32) -> u32 {
    // SAFETY: reads a window long; an invalid handle yields 0.
    let value = unsafe { GetWindowLongPtrW(raw(hwnd), index) };
    // Styles live in the low 32 bits.
    u32::try_from(value & 0xFFFF_FFFF).unwrap_or(0)
}

/// The window class name.
pub fn class_name(hwnd: Hwnd) -> String {
    let mut buffer = [0_u16; 256];
    // SAFETY: the buffer is 256 u16s and we pass its length.
    let len = unsafe { GetClassNameW(raw(hwnd), buffer.as_mut_ptr(), 256) };
    let len = usize::try_from(len).unwrap_or(0).min(buffer.len());
    String::from_utf16_lossy(&buffer[..len])
}

fn is_cloaked(hwnd: Hwnd) -> bool {
    let mut cloaked: u32 = 0;
    // SAFETY: `cloaked` is a u32 out-parameter of the size we pass.
    let result = unsafe {
        DwmGetWindowAttribute(
            raw(hwnd),
            u32::try_from(DWMWA_CLOAKED).unwrap_or(14),
            std::ptr::from_mut(&mut cloaked).cast::<c_void>(),
            4,
        )
    };
    result >= 0 && cloaked != 0
}

pub fn process_id(hwnd: Hwnd) -> u32 {
    let mut pid = 0_u32;
    // SAFETY: `pid` is a valid out-pointer.
    unsafe { GetWindowThreadProcessId(raw(hwnd), &mut pid) };
    pid
}

/// Whether the window is one of this app's own.
fn is_own(hwnd: Hwnd) -> bool {
    // SAFETY: no arguments.
    process_id(hwnd) == unsafe { GetCurrentProcessId() }
}

/// Whether this app may act on the window: a visible, uncloaked, unminimized
/// application window that isn't part of the shell or of this app. Cloaking
/// also hides windows on other virtual desktops.
pub fn is_manageable(hwnd: Hwnd) -> bool {
    if hwnd == 0 {
        return false;
    }
    // SAFETY: plain queries on a window handle; invalid handles yield false.
    let (visible, minimized) = unsafe { (IsWindowVisible(raw(hwnd)), IsIconic(raw(hwnd))) };
    if visible == 0 || minimized != 0 || is_cloaked(hwnd) {
        return false;
    }
    let style = long(hwnd, GWL_STYLE);
    let ex_style = long(hwnd, GWL_EXSTYLE);
    if style & WS_CHILD != 0 || ex_style & WS_EX_NOACTIVATE != 0 {
        return false;
    }
    if ex_style & WS_EX_TOOLWINDOW != 0 && ex_style & WS_EX_APPWINDOW == 0 {
        return false;
    }
    // Popups without a caption or a resizable frame are menus, tooltips and
    // splash screens.
    if style & WS_POPUP != 0 && style & WS_THICKFRAME == 0 && style & WS_CAPTION != WS_CAPTION {
        return false;
    }
    if is_own(hwnd) {
        return false;
    }
    !SHELL_CLASSES.contains(&class_name(hwnd).as_str())
}

/// Whether the user can resize the window by its frame.
pub fn is_resizable(hwnd: Hwnd) -> bool {
    long(hwnd, GWL_STYLE) & WS_THICKFRAME != 0
}

pub fn is_maximized(hwnd: Hwnd) -> bool {
    // SAFETY: plain query on a window handle.
    unsafe { IsZoomed(raw(hwnd)) != 0 }
}

/// A full-screen Direct3D app (a game) is running; acting on it would fight it.
pub fn full_screen_game_running() -> bool {
    let mut state = 0;
    // SAFETY: `state` is a valid out-pointer.
    let ok = unsafe { SHQueryUserNotificationState(&mut state) } >= 0;
    ok && state == QUNS_RUNNING_D3D_FULL_SCREEN
}

/// The rect `GetWindowRect` reports, invisible borders included.
pub fn outer_rect(hwnd: Hwnd) -> Option<PixelRect> {
    let mut rect = RECT::default();
    // SAFETY: `rect` is a valid out-pointer.
    (unsafe { GetWindowRect(raw(hwnd), &mut rect) } != 0).then(|| to_pixel_rect(rect))
}

/// What the user sees of the window: DWM's extended frame bounds, or the outer
/// rect for windows DWM doesn't frame.
pub fn visible_frame(hwnd: Hwnd) -> Option<PixelRect> {
    let mut rect = RECT::default();
    // SAFETY: `rect` is a RECT out-parameter of the size we pass.
    let result = unsafe {
        DwmGetWindowAttribute(
            raw(hwnd),
            u32::try_from(DWMWA_EXTENDED_FRAME_BOUNDS).unwrap_or(9),
            std::ptr::from_mut(&mut rect).cast::<c_void>(),
            u32::try_from(std::mem::size_of::<RECT>()).unwrap_or(16),
        )
    };
    if result >= 0 && rect.right > rect.left {
        Some(to_pixel_rect(rect))
    } else {
        outer_rect(hwnd)
    }
}

/// The invisible padding: how far the outer rect extends past the visible
/// frame on each side.
fn padding(hwnd: Hwnd) -> Option<PixelRect> {
    let outer = outer_rect(hwnd)?;
    let visible = visible_frame(hwnd)?;
    Some(PixelRect::new(
        visible.left - outer.left,
        visible.top - outer.top,
        outer.right - visible.right,
        outer.bottom - visible.bottom,
    ))
}

fn grow(target: PixelRect, padding: PixelRect) -> PixelRect {
    PixelRect::new(
        target.left - padding.left,
        target.top - padding.top,
        target.right + padding.right,
        target.bottom + padding.bottom,
    )
}

#[derive(Debug, PartialEq, Eq)]
pub enum PlaceError {
    /// The app isn't responding; moving it would block this app too.
    Hung,
    /// Windows refused, which for a responsive window means it belongs to an
    /// elevated (administrator) process.
    Denied,
    Failed(u32),
    Gone,
}

impl std::fmt::Display for PlaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hung => f.write_str("the app is not responding"),
            Self::Denied => f.write_str(
                "Windows refused; the window probably belongs to an app running as administrator",
            ),
            Self::Failed(code) => write!(f, "SetWindowPos failed with error {code}"),
            Self::Gone => f.write_str("the window no longer exists"),
        }
    }
}

fn set_window_pos(hwnd: Hwnd, rect: PixelRect, extra_flags: u32) -> Result<(), PlaceError> {
    // SAFETY: plain call on a window handle; no pointers involved.
    let ok = unsafe {
        SetWindowPos(
            raw(hwnd),
            std::ptr::null_mut(),
            rect.left,
            rect.top,
            rect.width(),
            rect.height(),
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER | extra_flags,
        )
    };
    if ok != 0 {
        return Ok(());
    }
    // SAFETY: no arguments.
    match unsafe { GetLastError() } {
        ERROR_ACCESS_DENIED => Err(PlaceError::Denied),
        code => Err(PlaceError::Failed(code)),
    }
}

/// Gives the window the visible frame `target` and returns the frame it ended
/// up with, which can differ when the app enforces its own size limits.
///
/// A maximized window is first restored through its placement, at roughly the
/// target, so it doesn't flash back to its old restored size. When the move
/// crosses to a display with a different scale the app rescales itself on
/// arrival, so the frame is applied a second time with fresh padding.
pub fn place(
    hwnd: Hwnd,
    target: PixelRect,
    across_displays: bool,
    screens: &[Screen],
) -> Result<PixelRect, PlaceError> {
    // SAFETY: plain query on a window handle.
    if unsafe { IsHungAppWindow(raw(hwnd)) } != 0 {
        return Err(PlaceError::Hung);
    }
    if is_maximized(hwnd) {
        restore_from_maximized(hwnd, target, screens)?;
    }
    let passes = if across_displays { 2 } else { 1 };
    for _ in 0..passes {
        let padding = padding(hwnd).ok_or(PlaceError::Gone)?;
        set_window_pos(hwnd, grow(target, padding), 0)?;
    }
    visible_frame(hwnd).ok_or(PlaceError::Gone)
}

/// Moves the window so its visible frame starts at (`left`, `top`), keeping
/// its size.
pub fn move_to(hwnd: Hwnd, left: i32, top: i32) -> Result<PixelRect, PlaceError> {
    let padding = padding(hwnd).ok_or(PlaceError::Gone)?;
    let at = PixelRect::from_xywh(left - padding.left, top - padding.top, 1, 1);
    set_window_pos(hwnd, at, SWP_NOSIZE)?;
    visible_frame(hwnd).ok_or(PlaceError::Gone)
}

fn restore_from_maximized(
    hwnd: Hwnd,
    target: PixelRect,
    screens: &[Screen],
) -> Result<(), PlaceError> {
    let mut placement = WINDOWPLACEMENT {
        length: u32::try_from(std::mem::size_of::<WINDOWPLACEMENT>()).unwrap_or(44),
        ..Default::default()
    };
    // SAFETY: `placement` is a WINDOWPLACEMENT with its length set.
    if unsafe { GetWindowPlacement(raw(hwnd), &mut placement) } == 0 {
        return Err(PlaceError::Gone);
    }
    // The placement rect is in workspace coordinates, which are offset from
    // screen coordinates by a taskbar docked on the left or top.
    let (dx, dy) = screens::containing(screens, target)
        .and_then(|index| screens.get(index))
        .map_or((0, 0), |screen| {
            (
                screen.work.left - screen.frame.left,
                screen.work.top - screen.frame.top,
            )
        });
    placement.showCmd = u32::try_from(SW_RESTORE).unwrap_or(9);
    placement.flags &= !WPF_RESTORETOMAXIMIZED;
    placement.rcNormalPosition = RECT {
        left: target.left - dx,
        top: target.top - dy,
        right: target.right - dx,
        bottom: target.bottom - dy,
    };
    // SAFETY: `placement` was filled by GetWindowPlacement and edited in place.
    if unsafe { SetWindowPlacement(raw(hwnd), &placement) } == 0 {
        // SAFETY: no arguments.
        return Err(match unsafe { GetLastError() } {
            ERROR_ACCESS_DENIED => PlaceError::Denied,
            code => PlaceError::Failed(code),
        });
    }
    Ok(())
}

/// Every manageable top-level window, front to back.
pub fn manageable_windows() -> Vec<Hwnd> {
    let mut all: Vec<Hwnd> = Vec::new();
    // SAFETY: the callback only runs during this call, and the LPARAM points at
    // `all`, which outlives it.
    unsafe { EnumWindows(Some(collect), std::ptr::from_mut(&mut all) as LPARAM) };
    all.into_iter()
        .filter(|hwnd| is_manageable(*hwnd))
        .collect()
}

unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> BOOL {
    // SAFETY: `data` is the `&mut Vec<Hwnd>` passed by `manageable_windows`.
    let all = unsafe { &mut *(data as *mut Vec<Hwnd>) };
    all.push(handle(hwnd));
    1
}

/// What `ignored_apps` names the window's app by: its executable's file name.
pub fn app_id(hwnd: Hwnd) -> Option<String> {
    exe_name(process_id(hwnd))
}

/// The executable's file name, e.g. `notepad.exe`.
pub fn exe_name(pid: u32) -> Option<String> {
    // SAFETY: opening a process for limited queries; the handle is closed below.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return None;
    }
    let mut buffer = [0_u16; 1024];
    let mut len = 1024_u32;
    // SAFETY: the buffer holds `len` u16s and `len` is updated in place.
    let ok = unsafe {
        QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut len)
    };
    // SAFETY: `process` is a handle we opened.
    unsafe { CloseHandle(process) };
    if ok == 0 {
        return None;
    }
    let path =
        String::from_utf16_lossy(&buffer[..usize::try_from(len).unwrap_or(0).min(buffer.len())]);
    path.rsplit(['\\', '/']).next().map(str::to_owned)
}

/// When the process started, as a FILETIME tick count. A window handle can be
/// reused after its window is destroyed; the owning process's start time tells
/// the new window from the old one.
pub fn process_start_time(pid: u32) -> Option<u64> {
    // SAFETY: opening a process for limited queries; the handle is closed below.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return None;
    }
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: four valid FILETIME out-pointers.
    let ok = unsafe { GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user) };
    // SAFETY: `process` is a handle we opened.
    unsafe { CloseHandle(process) };
    (ok != 0).then(|| (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}
