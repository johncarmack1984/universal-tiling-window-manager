//! Window managers that answer the same shortcuts. While Rectangle Pro still
//! runs, a shortcut both apps registered may go to either of them, so the menu
//! says so until it is quit.

use objc2_app_kit::NSRunningApplication;
use objc2_foundation::NSString;

const RECTANGLES: &[(&str, &str)] = &[
    ("com.knollsoft.Hookshot", "Rectangle Pro"),
    ("com.knollsoft.Rectangle", "Rectangle"),
];

/// The name of a Rectangle running alongside this app, if one is.
pub fn rectangle_running() -> Option<&'static str> {
    RECTANGLES
        .iter()
        .find(|(bundle_id, _)| {
            NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(
                bundle_id,
            ))
            .count()
                > 0
        })
        .map(|(_, name)| *name)
}
