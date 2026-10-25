//! Finding, measuring and moving other apps' windows through the Accessibility
//! API, the way Rectangle's `AccessibilityElement` does.
//!
//! A [`Window`] is plain data, a process id and the window server's number for
//! the window, so it crosses threads and keys the engine's history the way an
//! HWND does on Windows. The Accessibility element behind it is looked up once
//! and remembered by the thread that moves windows.

use std::cell::RefCell;
use std::collections::HashMap;

use objc2_app_kit::{NSRunningApplication, NSWorkspace};
use objc2_application_services::AXError;
use objc2_core_foundation::{CFDictionary, CFNumber, CFString, CFType, CGPoint, CGSize};
use objc2_core_graphics::{
    CGWindowListCopyWindowInfo, CGWindowListOption, kCGWindowAlpha, kCGWindowLayer,
    kCGWindowNumber, kCGWindowOwnerPID,
};
use utwm_layout::{PixelRect, Rect, Screen};

use crate::ax::Element;

/// A window of another app.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Window {
    pub pid: i32,
    /// The window server's number for it (`CGWindowID`).
    pub id: u32,
}

/// What the engine keeps history under. The window server never reuses a
/// window number, so the window itself tells windows apart.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub window: Window,
}

impl Key {
    pub fn of(window: Window) -> Self {
        Self { window }
    }
}

const POSITION: &str = "AXPosition";
const SIZE: &str = "AXSize";
/// VoiceOver's switch, which some apps also turn on for any assistive client.
/// With it on an app animates frame changes and drops the ones that arrive
/// mid-animation, so Rectangle turns it off for the length of a move.
const ENHANCED_UI: &str = "AXEnhancedUserInterface";

/// Elements remembered before starting over; a forgotten window is simply
/// looked up again.
const REMEMBERED: usize = 256;

thread_local! {
    static ELEMENTS: RefCell<HashMap<Window, Element>> = RefCell::new(HashMap::new());
}

fn remember(window: Window, element: &Element) {
    ELEMENTS.with(|elements| {
        let mut elements = elements.borrow_mut();
        if elements.len() >= REMEMBERED {
            elements.clear();
        }
        elements.insert(window, element.clone());
    });
}

fn forget(window: Window) {
    ELEMENTS.with(|elements| {
        elements.borrow_mut().remove(&window);
    });
}

/// The window's element: remembered, or found among its app's windows.
fn element(window: Window) -> Option<Element> {
    if let Some(element) = ELEMENTS.with(|elements| elements.borrow().get(&window).cloned()) {
        return Some(element);
    }
    let element = Element::application(window.pid)
        .elements("AXWindows")
        .into_iter()
        .find(|element| element.window_id() == Some(window.id))?;
    remember(window, &element);
    Some(element)
}

fn own_pid() -> i32 {
    i32::try_from(std::process::id()).unwrap_or(-1)
}

/// Caps how long any Accessibility call waits for an app, in place of the
/// system's six seconds, so a hung app holds up the next shortcut for a
/// second rather than for half a minute.
pub fn limit_waits() {
    Element::system_wide().set_timeout(1.0);
}

/// The window keyboard actions apply to: the focused window of the frontmost
/// app, or its first window, if this app may move it (Rectangle's
/// `getFrontWindowElement`).
pub fn foreground() -> Option<Window> {
    let pid = NSWorkspace::sharedWorkspace()
        .frontmostApplication()?
        .processIdentifier();
    if pid == own_pid() {
        return None;
    }
    let app = Element::application(pid);
    let element = app
        .element("AXFocusedWindow")
        .or_else(|| app.elements("AXWindows").into_iter().next())?;
    let window = Window {
        pid,
        id: element.window_id()?,
    };
    remember(window, &element);
    is_manageable(window).then_some(window)
}

/// Whether this app may act on the window: a window (not a sheet) that isn't
/// minimized, in full screen, or this app's own.
pub fn is_manageable(window: Window) -> bool {
    if window.pid == own_pid() {
        return false;
    }
    let Some(element) = element(window) else {
        return false;
    };
    element.string("AXRole").as_deref() == Some("AXWindow")
        && element.bool("AXMinimized") != Some(true)
        && element.bool("AXFullScreen") != Some(true)
}

/// Whether the window's size can be changed.
pub fn is_resizable(window: Window) -> bool {
    element(window).is_some_and(|element| element.settable(SIZE))
}

/// A Mac window has no maximized state to come back from: a zoomed window is
/// just a frame.
pub fn is_maximized(_window: Window) -> bool {
    false
}

/// The window's frame, in points from the top left of the main display.
pub fn visible_frame(window: Window) -> Option<PixelRect> {
    frame_of(&element(window)?)
}

fn frame_of(element: &Element) -> Option<PixelRect> {
    let origin = element.point(POSITION)?;
    let size = element.size(SIZE)?;
    Some(
        Rect {
            x: origin.x,
            y: origin.y,
            w: size.width,
            h: size.height,
        }
        .to_pixels(),
    )
}

#[derive(Debug, PartialEq, Eq)]
pub enum PlaceError {
    /// The app didn't answer in time; it is busy or not responding.
    Hung,
    /// macOS refused, which means this app has no Accessibility access.
    Denied,
    Failed(i32),
    Gone,
}

impl From<AXError> for PlaceError {
    fn from(error: AXError) -> Self {
        match error {
            AXError::CannotComplete => Self::Hung,
            AXError::APIDisabled => Self::Denied,
            AXError::InvalidUIElement => Self::Gone,
            other => Self::Failed(other.0),
        }
    }
}

impl std::fmt::Display for PlaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hung => f.write_str("the app is not responding"),
            Self::Denied => f.write_str(
                "macOS refused; give universal-tiling-window-manager Accessibility access in System Settings",
            ),
            Self::Failed(code) => write!(f, "the Accessibility API failed with error {code}"),
            Self::Gone => f.write_str("the window no longer exists"),
        }
    }
}

/// Whether a failed step should stop the move. Anything else (an app turning
/// down one size) leaves the frame the app accepted, which the caller reads
/// back and realigns, as Rectangle's best-effort mover does.
fn stops(error: &PlaceError) -> bool {
    !matches!(error, PlaceError::Failed(_))
}

/// Runs `change` with the app's enhanced user interface off.
fn without_animation<T>(pid: i32, change: impl FnOnce() -> T) -> T {
    let app = Element::application(pid);
    let enhanced = app.bool(ENHANCED_UI) == Some(true);
    if enhanced {
        let _ = app.set_bool(ENHANCED_UI, false);
    }
    let result = change();
    if enhanced {
        let _ = app.set_bool(ENHANCED_UI, true);
    }
    result
}

/// Gives the window the frame `target` and returns the frame it ended up with,
/// which can differ when the app enforces its own size limits.
///
/// Like Rectangle's `setFrame`, it sets the size, then the position, then the
/// size again: the first makes the window fit where it is going, the second
/// takes effect once it is on a display big enough for it. A window sent to
/// another display is raised there too, as Rectangle does.
pub fn place(
    window: Window,
    target: PixelRect,
    across_displays: bool,
    _screens: &[Screen],
) -> Result<PixelRect, PlaceError> {
    let element = element(window).ok_or(PlaceError::Gone)?;
    let size = CGSize {
        width: f64::from(target.width()),
        height: f64::from(target.height()),
    };
    let origin = CGPoint {
        x: f64::from(target.left),
        y: f64::from(target.top),
    };
    let steps: [&dyn Fn() -> Result<(), AXError>; 3] = [
        &|| element.set_size(SIZE, size),
        &|| element.set_point(POSITION, origin),
        &|| element.set_size(SIZE, size),
    ];
    let stopped = without_animation(window.pid, || {
        steps
            .iter()
            .map(|step| step().map_err(PlaceError::from))
            .find_map(|result| result.err().filter(stops))
    });
    if let Some(error) = stopped {
        if error == PlaceError::Gone {
            forget(window);
        }
        return Err(error);
    }
    if across_displays {
        let _ = element.perform("AXRaise");
    }
    frame_of(&element).ok_or(PlaceError::Gone)
}

/// Moves the window so its frame starts at (`left`, `top`), keeping its size.
pub fn move_to(window: Window, left: i32, top: i32) -> Result<PixelRect, PlaceError> {
    let element = element(window).ok_or(PlaceError::Gone)?;
    let origin = CGPoint {
        x: f64::from(left),
        y: f64::from(top),
    };
    without_animation(window.pid, || element.set_point(POSITION, origin)).map_err(|error| {
        let error = PlaceError::from(error);
        if error == PlaceError::Gone {
            forget(window);
        }
        error
    })?;
    frame_of(&element).ok_or(PlaceError::Gone)
}

/// Every manageable standard window on screen in the current Space, front to
/// back. The order comes from the window server, which the Accessibility API
/// can't give across apps.
pub fn manageable_windows() -> Vec<Window> {
    let mut apps: HashMap<i32, Vec<(u32, Element)>> = HashMap::new();
    let mut windows = Vec::new();
    for (pid, id) in on_screen_windows() {
        let elements = apps.entry(pid).or_insert_with(|| {
            Element::application(pid)
                .elements("AXWindows")
                .into_iter()
                .filter_map(|element| element.window_id().map(|id| (id, element)))
                .collect()
        });
        let Some((_, element)) = elements.iter().find(|(candidate, _)| *candidate == id) else {
            continue;
        };
        let window = Window { pid, id };
        remember(window, element);
        if element.string("AXSubrole").as_deref() == Some("AXStandardWindow")
            && is_manageable(window)
        {
            windows.push(window);
        }
    }
    windows
}

/// The process and number of every ordinary window on screen, front to back.
fn on_screen_windows() -> Vec<(i32, u32)> {
    let options =
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements;
    let Some(list) = CGWindowListCopyWindowInfo(options, 0) else {
        return Vec::new();
    };
    // SAFETY: the window list is an array of dictionaries keyed by strings.
    let list = unsafe { list.cast_unchecked::<CFDictionary<CFString, CFType>>() };
    // SAFETY: reads the window list's documented key constants.
    let (number_key, owner_key, layer_key, alpha_key) = unsafe {
        (
            kCGWindowNumber,
            kCGWindowOwnerPID,
            kCGWindowLayer,
            kCGWindowAlpha,
        )
    };
    let number = |info: &CFDictionary<CFString, CFType>, key: &CFString| {
        info.get(key)?.downcast::<CFNumber>().ok()
    };
    let own = own_pid();
    list.iter()
        .filter_map(|info| {
            let id = u32::try_from(number(&info, number_key)?.as_i64()?).ok()?;
            let pid = number(&info, owner_key)?.as_i32()?;
            let layer = number(&info, layer_key)?.as_i32()?;
            let visible = number(&info, alpha_key)
                .and_then(|alpha| alpha.as_f64())
                .is_none_or(|alpha| alpha > 0.0);
            // Layer 0 holds ordinary app windows; menus, the Dock and
            // overlays sit above it.
            (layer == 0 && visible && pid != own).then_some((pid, id))
        })
        .collect()
}

/// What `ignored_apps` and Rectangle's `disabledApps` name the window's app
/// by: its bundle id.
pub fn app_id(window: Window) -> Option<String> {
    let app = NSRunningApplication::runningApplicationWithProcessIdentifier(window.pid)?;
    Some(app.bundleIdentifier()?.to_string())
}
