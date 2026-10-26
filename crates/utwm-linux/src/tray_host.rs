//! The StatusNotifierWatcher a tray icon registers with. GNOME has none of its
//! own: Ubuntu's AppIndicator extension provides one, and on other GNOMEs the
//! AppIndicator extension does once installed. The tray can only be made
//! while one is there.

use zbus::blocking::fdo::DBusProxy;

use crate::shell;

const WATCHER: &str = "org.kde.StatusNotifierWatcher";

/// Blocks until a StatusNotifierWatcher is on the bus. Returns false when
/// there is no session bus to wait on.
pub fn wait() -> bool {
    let Some(connection) = shell::connection() else {
        return false;
    };
    let Ok(bus) = DBusProxy::new(connection) else {
        return false;
    };
    // Subscribed before the first look, so a watcher that appears in between
    // isn't missed.
    let Ok(changes) = bus.receive_name_owner_changed_with_args(&[(0, WATCHER)]) else {
        return false;
    };
    if shell::owner(connection, WATCHER).is_some() {
        return true;
    }
    for change in changes {
        if change.args().is_ok_and(|args| args.new_owner().is_some()) {
            return true;
        }
    }
    false
}
