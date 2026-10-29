//! This app's place among the running apps. A menu bar app is never the
//! active one by itself, so a panel it shows comes forward only when it asks
//! to, and the app that was active gets the focus back only when this one
//! steps aside.

use objc2_app_kit::NSApplication;

/// Makes this app the active one, so the panel it is about to show opens in
/// front of the app that was active.
// `activate` takes over from this in macOS 14; the app also runs on 13.
#[allow(deprecated)]
pub fn come_forward() {
    dispatch2::run_on_main(|mtm| {
        NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
    });
}

/// Hands the focus back to the app that was active before [`come_forward`].
/// Hiding affects only this app's windows, and it has none once the panel
/// closes; its menu bar icon stays.
pub fn step_back() {
    dispatch2::run_on_main(|mtm| NSApplication::sharedApplication(mtm).hide(None));
}
