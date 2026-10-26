//! Desktop notifications, for what Windows and the Mac show as the tray's
//! tooltip: the tray GNOME shows on Ubuntu, the AppIndicator extension's,
//! never shows tooltips.

use std::collections::HashMap;

use zbus::zvariant::Value;

use crate::{APP_ID, shell};

const NOTIFICATIONS: &str = "org.freedesktop.Notifications";
const NOTIFICATIONS_PATH: &str = "/org/freedesktop/Notifications";

/// Shows `message` as a notification from `app`. Without a notification
/// server, nothing happens.
pub fn show(app: &str, message: &str) {
    let Some(connection) = shell::connection() else {
        return;
    };
    // The app's desktop entry, so GNOME shows its name and icon with it.
    let hints = HashMap::from([("desktop-entry", Value::from(APP_ID))]);
    let actions: &[&str] = &[];
    let shown: zbus::Result<u32> =
        shell::proxy(connection, NOTIFICATIONS, NOTIFICATIONS_PATH, NOTIFICATIONS).and_then(
            |notifications| {
                notifications.call(
                    "Notify",
                    &(app, 0_u32, "", app, message, actions, hints, -1_i32),
                )
            },
        );
    if let Err(error) = shown {
        tracing::debug!(%error, "could not show a notification");
    }
}
