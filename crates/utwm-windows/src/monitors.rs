//! The displays, in physical pixels.

use utwm_layout::Screen;
use windows_sys::Win32::Foundation::{LPARAM, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
};
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor, MDT_EFFECTIVE_DPI,
    SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::WindowsAndMessaging::MONITORINFOF_PRIMARY;
use windows_sys::core::BOOL;

use crate::to_pixel_rect;

/// Makes this process per-monitor DPI aware (v2), so every coordinate is a
/// physical pixel. Tauri's event loop does this too; whichever comes first
/// wins and the other call fails harmlessly.
pub fn ensure_per_monitor_dpi_awareness() {
    // SAFETY: takes a predefined awareness-context constant and touches no memory.
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

/// Every display with its work area and scale.
pub fn screens() -> Vec<Screen> {
    let mut screens: Vec<Screen> = Vec::new();
    // SAFETY: the callback only runs during this call, and the LPARAM points at
    // `screens`, which outlives it.
    unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            Some(collect),
            std::ptr::from_mut(&mut screens) as LPARAM,
        );
    }
    screens
}

unsafe extern "system" fn collect(monitor: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    // SAFETY: `data` is the `&mut Vec<Screen>` passed by `screens` above.
    let screens = unsafe { &mut *(data as *mut Vec<Screen>) };
    if let Some(screen) = describe(monitor) {
        screens.push(screen);
    }
    1
}

fn describe(monitor: HMONITOR) -> Option<Screen> {
    let mut info = MONITORINFO {
        cbSize: u32::try_from(std::mem::size_of::<MONITORINFO>()).ok()?,
        rcMonitor: RECT::default(),
        rcWork: RECT::default(),
        dwFlags: 0,
    };
    // SAFETY: `info` is a valid MONITORINFO with cbSize set.
    if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        return None;
    }
    let (mut dpi_x, mut dpi_y) = (96_u32, 96_u32);
    // SAFETY: both out-pointers are valid u32s.
    let dpi_ok =
        unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) } >= 0;
    let dpi = if dpi_ok && dpi_x > 0 { dpi_x } else { 96 };
    Some(Screen {
        frame: to_pixel_rect(info.rcMonitor),
        work: to_pixel_rect(info.rcWork),
        scale: f64::from(dpi) / 96.0,
        primary: info.dwFlags & MONITORINFOF_PRIMARY != 0,
    })
}
