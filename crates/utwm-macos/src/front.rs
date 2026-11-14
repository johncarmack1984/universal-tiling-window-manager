//! The app in front, watched for remote-control apps. RustDesk, Screen
//! Sharing and Windows App send every key to the machine they control, the
//! shortcuts included. RustDesk catches them with an event tap, which runs
//! before any hot key fires, but macOS hides every keystroke from event taps
//! while any app holds Secure Input (a focused password field, a terminal's
//! Secure Keyboard Entry, or an app that never let go of it) and goes on
//! firing hot keys, so this app's would win then: a shortcut meant for the
//! other machine would act on this one, and only its modifiers would reach
//! the other side. While a remote-control app is in front, the hot keys step
//! aside, and its keys reach it as any key does.

use std::ptr::NonNull;

use block2::RcBlock;
use objc2_app_kit::{
    NSRunningApplication, NSWorkspace, NSWorkspaceApplicationKey,
    NSWorkspaceDidActivateApplicationNotification,
};
use objc2_foundation::NSNotification;

/// Remote-control apps by bundle id: RustDesk, Screen Sharing, and Windows
/// App, which kept Microsoft Remote Desktop's.
const REMOTE_CONTROL: &[&str] = &[
    "com.carriez.rustdesk",
    "com.apple.ScreenSharing",
    "com.microsoft.rdc.macos",
];

/// Whether the app with this bundle id sends every key to another machine.
pub fn is_remote_control(bundle_id: &str) -> bool {
    REMOTE_CONTROL
        .iter()
        .any(|known| known.eq_ignore_ascii_case(bundle_id))
}

/// Receives the bundle id of each app that comes to the front, on the thread
/// that announces it, the main thread. It must be quick: send to a channel
/// and return.
pub type Reporter = Box<dyn Fn(Option<String>) + Send + 'static>;

/// Reports the app in front now, then each app that comes to the front, for
/// as long as this app runs.
pub fn watch(report: Reporter) {
    let workspace = NSWorkspace::sharedWorkspace();
    report(bundle_id(workspace.frontmostApplication().as_deref()));
    let activated = RcBlock::new(move |notification: NonNull<NSNotification>| {
        // SAFETY: the notification center passes a notification that lives
        // until the block returns.
        let notification = unsafe { notification.as_ref() };
        // SAFETY: reads a constant AppKit defines.
        let key = unsafe { NSWorkspaceApplicationKey };
        let app = notification
            .userInfo()
            .and_then(|info| info.objectForKey(key))
            .and_then(|app| app.downcast::<NSRunningApplication>().ok());
        report(bundle_id(app.as_deref()));
    });
    // SAFETY: the name is AppKit's own constant. With no object to filter on
    // and no queue, the block runs on the thread that posts, which for the
    // workspace's own notifications is the main thread.
    let observer = unsafe {
        workspace
            .notificationCenter()
            .addObserverForName_object_queue_usingBlock(
                Some(NSWorkspaceDidActivateApplicationNotification),
                None,
                None,
                &activated,
            )
    };
    // The app watches for as long as it runs.
    std::mem::forget(observer);
}

fn bundle_id(app: Option<&NSRunningApplication>) -> Option<String> {
    app?.bundleIdentifier().map(|id| id.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2_foundation::{NSDictionary, NSString};

    use super::*;

    #[test]
    fn remote_control_apps_are_known_by_bundle_id() {
        assert!(is_remote_control("com.carriez.rustdesk"));
        assert!(is_remote_control("com.apple.screensharing"));
        assert!(is_remote_control("com.microsoft.rdc.macos"));
        assert!(!is_remote_control("com.googlecode.iterm2"));
        assert!(!is_remote_control(""));
    }

    #[test]
    fn reports_the_app_each_activation_names() {
        // Any app with a bundle id stands in for the one coming to the front;
        // a Mac with no Dock running has no front app to report either.
        let Some(dock) = NSRunningApplication::runningApplicationsWithBundleIdentifier(
            &NSString::from_str("com.apple.dock"),
        )
        .firstObject() else {
            return;
        };
        let (sender, receiver) = mpsc::channel();
        watch(Box::new(move |bundle_id| {
            let _ = sender.send(bundle_id);
        }));
        // The app in front now, whichever it is.
        assert!(receiver.recv_timeout(Duration::from_secs(1)).is_ok());

        // SAFETY: reads a constant AppKit defines.
        let key = unsafe { NSWorkspaceApplicationKey };
        let app: &AnyObject = &dock;
        let info: Retained<NSDictionary<NSString, AnyObject>> =
            NSDictionary::from_slices(&[key], &[app]);
        // SAFETY: a dictionary of strings to objects is a dictionary of
        // objects to objects.
        let info: Retained<NSDictionary> = unsafe { Retained::cast_unchecked(info) };
        // SAFETY: the name is AppKit's own constant, and the user info holds
        // what AppKit puts there: the running application under its key.
        unsafe {
            NSWorkspace::sharedWorkspace()
                .notificationCenter()
                .postNotificationName_object_userInfo(
                    NSWorkspaceDidActivateApplicationNotification,
                    None,
                    Some(&info),
                );
        }
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(1)).ok(),
            Some(Some("com.apple.dock".to_owned()))
        );
    }
}
