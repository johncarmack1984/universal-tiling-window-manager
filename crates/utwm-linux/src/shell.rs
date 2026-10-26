//! The session bus, and the bridge on it: the interface the GNOME Shell
//! extension (`gnome-extension/extension.js`) exports. Every call is a short,
//! blocking round trip, made from the thread that moves windows or from the
//! shortcut listener, never from the GTK main thread.

use std::sync::OnceLock;
use std::time::Duration;

use zbus::blocking::fdo::DBusProxy;
use zbus::blocking::{Connection, Proxy};
use zbus::names::BusName;
use zbus::proxy::CacheProperties;
use zbus::zvariant::{DeserializeDict, Type};

/// The interface this build speaks: the extension's `API_VERSION`, bumped
/// together whenever the interface changes.
pub const API_VERSION: u32 = 1;

pub(crate) const BUS_NAME: &str = "com.johncarmack.UniversalTilingWindowManager.Shell";
pub(crate) const OBJECT_PATH: &str = "/com/johncarmack/UniversalTilingWindowManager/Shell";
pub(crate) const INTERFACE: &str = "com.johncarmack.UniversalTilingWindowManager.Shell1";

/// GNOME Shell's own name. The bridge's name must have the same owner, since
/// any process may claim a name on the session bus.
pub(crate) const SHELL: &str = "org.gnome.Shell";

/// How long a call may take. A move answers once the app has redrawn, within
/// a second; every other call answers at once.
const TIMEOUT: Duration = Duration::from_secs(2);

/// A frame as the bridge sends one: x, y, width, height.
pub(crate) type Frame = (i32, i32, i32, i32);

/// A display as the bridge sends one: its frame, its work area, its scale,
/// and whether it is the primary display.
pub(crate) type RawScreen = (i32, i32, i32, i32, i32, i32, i32, i32, f64, bool);

/// What the bridge says about one window.
#[derive(Clone, Debug, Default, PartialEq, Eq, DeserializeDict, Type)]
#[zvariant(
    signature = "a{sv}",
    rename_all = "kebab-case",
    crate = "zbus::zvariant"
)]
pub(crate) struct Facts {
    pub frame: Frame,
    pub pid: i32,
    /// Mutter's `Meta.WindowType`.
    pub window_type: u32,
    pub attached_dialog: bool,
    pub override_redirect: bool,
    pub minimized: bool,
    pub fullscreen: bool,
    /// Mutter's `Meta.MaximizeFlags`: horizontally 1, vertically 2.
    pub maximized: u32,
    pub allows_move: bool,
    pub allows_resize: bool,
    pub skip_taskbar: bool,
    pub on_active_workspace: bool,
    /// GNOME Shell's id for the app: `org.gnome.Terminal.desktop`, or for a
    /// window it can't tie to an app, `window:N`.
    pub app_id: String,
    pub sandboxed_app_id: String,
    pub gtk_application_id: String,
    pub wm_class: String,
}

/// Why a call to the bridge failed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CallError {
    /// No session bus, no extension, or something other than GNOME Shell
    /// holding the extension's name.
    Unavailable,
    /// The window no longer exists.
    Gone,
    /// GNOME Shell refused or failed; the text says why.
    Failed(String),
}

impl From<zbus::Error> for CallError {
    fn from(error: zbus::Error) -> Self {
        match error {
            zbus::Error::MethodError(name, detail, _) => match name.as_str() {
                "org.freedesktop.DBus.Error.UnknownObject" => Self::Gone,
                "org.freedesktop.DBus.Error.ServiceUnknown"
                | "org.freedesktop.DBus.Error.NameHasNoOwner" => Self::Unavailable,
                _ => Self::Failed(detail.unwrap_or_else(|| name.to_string())),
            },
            zbus::Error::FDO(error) => match *error {
                zbus::fdo::Error::UnknownObject(_) => Self::Gone,
                zbus::fdo::Error::ServiceUnknown(_) | zbus::fdo::Error::NameHasNoOwner(_) => {
                    Self::Unavailable
                }
                other => Self::Failed(other.to_string()),
            },
            other => Self::Failed(other.to_string()),
        }
    }
}

pub(crate) type Result<T> = std::result::Result<T, CallError>;

/// The session bus, opened on first use; `None` without one.
pub(crate) fn connection() -> Option<&'static Connection> {
    static CONNECTION: OnceLock<Option<Connection>> = OnceLock::new();
    CONNECTION
        .get_or_init(|| {
            zbus::blocking::connection::Builder::session()
                .and_then(|builder| builder.method_timeout(TIMEOUT).build())
                .inspect_err(|error| tracing::warn!(%error, "no session bus"))
                .ok()
        })
        .as_ref()
}

/// The unique name that owns `name` on the bus, if anything does.
pub(crate) fn owner(connection: &Connection, name: &str) -> Option<String> {
    let name = BusName::try_from(name).ok()?;
    let bus = DBusProxy::new(connection).ok()?;
    bus.get_name_owner(name).ok().map(|owner| owner.to_string())
}

/// A proxy for `interface` at `path` on `destination`, reading properties
/// fresh every time.
pub(crate) fn proxy<D>(
    connection: &Connection,
    destination: D,
    path: &'static str,
    interface: &'static str,
) -> zbus::Result<Proxy<'static>>
where
    D: TryInto<BusName<'static>>,
    D::Error: Into<zbus::Error>,
{
    zbus::blocking::proxy::Builder::<Proxy<'static>>::new(connection)
        .destination(destination)?
        .path(path)?
        .interface(interface)?
        .cache_properties(CacheProperties::No)
        .build()
}

/// The bridge, if the extension is running inside GNOME Shell. Calls go to
/// GNOME Shell's own connection rather than to the extension's name, which
/// could change hands between this check and the call.
pub(crate) fn bridge() -> Result<Proxy<'static>> {
    let connection = connection().ok_or(CallError::Unavailable)?;
    let held_by = owner(connection, BUS_NAME).ok_or(CallError::Unavailable)?;
    if owner(connection, SHELL).as_deref() != Some(held_by.as_str()) {
        tracing::warn!(
            owner = held_by,
            "something other than GNOME Shell holds the extension's name; ignoring it"
        );
        return Err(CallError::Unavailable);
    }
    Ok(proxy(connection, held_by, OBJECT_PATH, INTERFACE)?)
}

pub(crate) fn api_version() -> Result<u32> {
    Ok(bridge()?.get_property("ApiVersion")?)
}

pub(crate) fn revision() -> Result<String> {
    Ok(bridge()?.get_property("Revision")?)
}

pub(crate) fn screens() -> Result<Vec<RawScreen>> {
    Ok(bridge()?.call("Screens", &())?)
}

pub(crate) fn focused() -> Result<u64> {
    Ok(bridge()?.call("Focused", &())?)
}

pub(crate) fn windows() -> Result<Vec<(u64, Facts)>> {
    Ok(bridge()?.call("Windows", &())?)
}

pub(crate) fn describe(window: u64) -> Result<Facts> {
    Ok(bridge()?.call("Describe", &(window,))?)
}

pub(crate) fn place(window: u64, frame: Frame, raise: bool) -> Result<Frame> {
    Ok(bridge()?.call("Place", &(window, frame, raise))?)
}

pub(crate) fn move_to(window: u64, x: i32, y: i32) -> Result<Frame> {
    Ok(bridge()?.call("MoveTo", &(window, x, y))?)
}

pub(crate) fn activate(window: u64) -> Result<()> {
    Ok(bridge()?.call("Activate", &(window,))?)
}

pub(crate) fn set_bindings(generation: u32, accelerators: &[String]) -> Result<Vec<bool>> {
    Ok(bridge()?.call("SetBindings", &(generation, accelerators))?)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use zbus::zvariant::serialized::Context;
    use zbus::zvariant::{LE, Value, to_bytes};

    use super::*;

    #[test]
    fn facts_read_from_the_bridges_dictionary() {
        let mut dict: HashMap<&str, Value> = HashMap::new();
        dict.insert("frame", Value::from((10, 20, 300, 400)));
        dict.insert("pid", Value::from(42));
        dict.insert("window-type", Value::from(0_u32));
        dict.insert("attached-dialog", Value::from(false));
        dict.insert("override-redirect", Value::from(false));
        dict.insert("minimized", Value::from(false));
        dict.insert("fullscreen", Value::from(false));
        dict.insert("maximized", Value::from(2_u32));
        dict.insert("allows-move", Value::from(true));
        dict.insert("allows-resize", Value::from(true));
        dict.insert("skip-taskbar", Value::from(false));
        dict.insert("on-active-workspace", Value::from(true));
        dict.insert("app-id", Value::from("org.gnome.Terminal.desktop"));
        dict.insert("sandboxed-app-id", Value::from(""));
        dict.insert("gtk-application-id", Value::from("org.gnome.Terminal"));
        dict.insert("wm-class", Value::from("gnome-terminal-server"));
        // A newer extension may say more; an older app ignores it.
        dict.insert("something-new", Value::from(7_u32));
        let context = Context::new_dbus(LE, 0);
        let bytes = to_bytes(context, &dict).expect("serializes");
        let (facts, _): (Facts, _) = bytes.deserialize().expect("deserializes");
        assert_eq!(facts.frame, (10, 20, 300, 400));
        assert_eq!(facts.maximized, 2);
        assert_eq!(facts.app_id, "org.gnome.Terminal.desktop");
        assert!(facts.on_active_workspace);
    }

    #[test]
    fn unknown_object_means_the_window_is_gone() {
        let error = zbus::Error::FDO(Box::new(zbus::fdo::Error::UnknownObject(
            "window 7 no longer exists".to_owned(),
        )));
        assert_eq!(CallError::from(error), CallError::Gone);
        let error = zbus::Error::FDO(Box::new(zbus::fdo::Error::AccessDenied(
            "the screen is locked".to_owned(),
        )));
        assert!(matches!(CallError::from(error), CallError::Failed(_)));
    }
}
