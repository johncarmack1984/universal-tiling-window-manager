//! The app's menu in GNOME's top bar, drawn by the extension.
//!
//! GNOME has no tray of its own. The AppIndicator extension that stands in
//! for one draws a submenu as a collapsed line with no picture, so in its
//! menu the pictures of where each action puts the window show only inside a
//! group, a click away. The extension draws the menu itself instead, as this
//! app describes it: each group of actions as a row of their pictures, the
//! name and shortcut of each showing when it has the pointer or the focus,
//! and the app's own lines around them. Until the extension runs, or while a
//! copy from before it drew menus does, the tray icon shows the menu.

use zbus::zvariant::{SerializeDict, Type};

use crate::shell::{self, CallError};

/// The first version of the extension's interface with the menu.
const MENU_API_VERSION: u32 = 2;

/// The most items the extension takes in one menu; it refuses a larger menu
/// whole.
pub const MAX_ITEMS: usize = 500;

/// The most bytes the extension takes in a picture or the icon; it drops a
/// larger one.
pub const MAX_IMAGE_BYTES: usize = 65_536;

/// How many pixels across the extension shows a picture, on a screen with
/// one pixel to a point; twice that on a HiDPI screen.
pub const PICTURE_SIZE: u32 = 32;

/// What a menu item is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A line of text, which a click acts on while it is enabled.
    Text,
    /// A switch, on or off.
    Check,
    Separator,
    /// A group of actions, drawn as a row of their pictures.
    Group,
    /// One action in a group: its picture, whose name and shortcut show when
    /// it has the pointer or the focus.
    Action,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Check => "check",
            Self::Separator => "separator",
            Self::Group => "group",
            Self::Action => "action",
        }
    }
}

/// One item of the menu. Items come in order, each action after its group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// What a click on it reports.
    pub id: String,
    pub kind: Kind,
    pub label: String,
    /// The group an action belongs to, by the group's id.
    pub group: Option<String>,
    /// An action's shortcut, as GNOME's keyboards label it.
    pub shortcut: Option<String>,
    pub enabled: bool,
    pub checked: bool,
    /// An action's picture, a PNG.
    pub picture: Option<Vec<u8>>,
}

impl Item {
    fn new(id: impl Into<String>, kind: Kind, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind,
            label: label.into(),
            group: None,
            shortcut: None,
            enabled: true,
            checked: false,
            picture: None,
        }
    }

    pub fn text(id: impl Into<String>, label: impl Into<String>, enabled: bool) -> Self {
        Self {
            enabled,
            ..Self::new(id, Kind::Text, label)
        }
    }

    pub fn check(id: impl Into<String>, label: impl Into<String>, checked: bool) -> Self {
        Self {
            checked,
            ..Self::new(id, Kind::Check, label)
        }
    }

    pub fn separator() -> Self {
        Self::new("", Kind::Separator, "")
    }

    pub fn group(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self::new(id, Kind::Group, label)
    }

    pub fn action(
        id: impl Into<String>,
        group: impl Into<String>,
        label: impl Into<String>,
        shortcut: Option<String>,
        picture: Option<Vec<u8>>,
    ) -> Self {
        Self {
            group: Some(group.into()),
            shortcut,
            picture,
            ..Self::new(id, Kind::Action, label)
        }
    }

    fn wire(&self) -> Wire<'_> {
        Wire {
            id: &self.id,
            kind: self.kind.name(),
            label: &self.label,
            group: self.group.as_deref(),
            shortcut: self.shortcut.as_deref(),
            enabled: self.enabled,
            checked: self.checked,
            picture: self.picture.as_deref(),
        }
    }
}

/// An item as the bridge takes it: a dictionary, so a newer app can say more
/// than an older extension reads.
#[derive(Debug, SerializeDict, Type)]
#[zvariant(signature = "a{sv}", crate = "zbus::zvariant")]
struct Wire<'a> {
    id: &'a str,
    kind: &'a str,
    label: &'a str,
    group: Option<&'a str>,
    shortcut: Option<&'a str>,
    enabled: bool,
    checked: bool,
    picture: Option<&'a [u8]>,
}

/// Whether the extension can draw the menu: it's running, and it isn't a
/// copy from before it drew menus. When it can't, the tray icon shows the
/// menu.
pub fn available() -> bool {
    shell::api_version().is_ok_and(|version| version >= MENU_API_VERSION)
}

/// Puts the menu in GNOME's top bar, or brings the one there up to date,
/// under `name`, which screen readers read for it, and with `icon`, a PNG.
/// Returns whether the extension drew it, which it does once [`available`].
pub fn show(name: &str, icon: &[u8], items: &[Item]) -> bool {
    if items.len() > MAX_ITEMS {
        tracing::warn!(
            items = items.len(),
            "too many items for GNOME Shell's menu; the tray shows them"
        );
        return false;
    }
    let wire: Vec<Wire<'_>> = items.iter().map(Item::wire).collect();
    match shell::set_menu(name, icon, &wire) {
        Ok(()) => true,
        Err(CallError::Unavailable) => false,
        Err(error) => {
            tracing::warn!(
                ?error,
                "GNOME Shell didn't take the menu; the tray shows it"
            );
            false
        }
    }
}

/// Takes the menu out of the top bar, for an app about to exit. The
/// extension takes it out anyway once the app leaves the bus; this makes it
/// immediate.
pub fn remove() {
    let _ = shell::remove_menu();
}

/// Starts reporting clicks on the menu, by the id of the item clicked, on a
/// thread of its own; `report` should be quick: send to a channel and return.
pub fn listen(report: Box<dyn Fn(String) + Send + 'static>) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("gnome-menu".to_owned())
        .spawn(move || receive(&*report))?;
    Ok(())
}

fn receive(report: &dyn Fn(String)) {
    let Some(connection) = shell::connection() else {
        return;
    };
    let clicks = shell::proxy(
        connection,
        shell::BUS_NAME,
        shell::OBJECT_PATH,
        shell::INTERFACE,
    )
    .and_then(|proxy| proxy.receive_signal("MenuActivated"));
    let clicks = match clicks {
        Ok(clicks) => clicks,
        Err(error) => {
            tracing::warn!(%error, "can't listen to GNOME Shell; its menu won't answer clicks");
            return;
        }
    };
    for message in clicks {
        // Only GNOME Shell speaks for the extension.
        let from_shell = message.header().sender().is_some_and(|sender| {
            shell::owner(connection, shell::SHELL).as_deref() == Some(sender.as_str())
        });
        if !from_shell {
            continue;
        }
        if let Ok(id) = message.body().deserialize::<String>() {
            report(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use zbus::zvariant::serialized::Context;
    use zbus::zvariant::{LE, OwnedValue, to_bytes};

    use super::*;

    fn wire_dict(item: &Item) -> HashMap<String, OwnedValue> {
        let context = Context::new_dbus(LE, 0);
        let bytes = to_bytes(context, &item.wire()).expect("serializes");
        let (dict, _): (HashMap<String, OwnedValue>, _) =
            bytes.deserialize().expect("deserializes");
        dict
    }

    #[test]
    fn items_go_to_the_bridge_as_dictionaries() {
        assert_eq!(<Wire<'_> as Type>::SIGNATURE.to_string(), "a{sv}");
        let action = Item::action(
            "action:leftHalf",
            "group:Halves",
            "Left Half",
            Some("Ctrl+Alt+Left".to_owned()),
            Some(vec![0x89, b'P', b'N', b'G']),
        );
        let dict = wire_dict(&action);
        let text = |key: &str| {
            dict.get(key)
                .and_then(|value| value.downcast_ref::<String>().ok())
        };
        assert_eq!(text("id").as_deref(), Some("action:leftHalf"));
        assert_eq!(text("kind").as_deref(), Some("action"));
        assert_eq!(text("label").as_deref(), Some("Left Half"));
        assert_eq!(text("group").as_deref(), Some("group:Halves"));
        assert_eq!(text("shortcut").as_deref(), Some("Ctrl+Alt+Left"));
        assert_eq!(
            dict.get("enabled")
                .and_then(|value| value.downcast_ref::<bool>().ok()),
            Some(true)
        );
        let picture = dict
            .get("picture")
            .and_then(|value| Vec::<u8>::try_from(value.try_clone().ok()?).ok());
        assert_eq!(picture, Some(vec![0x89, b'P', b'N', b'G']));
    }

    #[test]
    fn what_an_item_lacks_stays_out_of_its_dictionary() {
        let dict = wire_dict(&Item::text("quit", "Quit", true));
        for key in ["group", "shortcut", "picture"] {
            assert!(!dict.contains_key(key), "{key}");
        }
        assert_eq!(
            dict.get("kind")
                .and_then(|value| value.downcast_ref::<String>().ok())
                .as_deref(),
            Some("text")
        );
        let dict = wire_dict(&Item::check("pause", "Pause shortcuts", true));
        assert_eq!(
            dict.get("checked")
                .and_then(|value| value.downcast_ref::<bool>().ok()),
            Some(true)
        );
    }
}
