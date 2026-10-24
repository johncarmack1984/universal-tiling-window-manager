//! The tray icon and its menu, laid out like Rectangle's menu bar menu: every
//! action with its shortcut, grouped, then the app's own switches.

use std::sync::mpsc::Sender;

use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use utwm_config::actions::{self, ActionName};
use utwm_config::bindings::Resolved;
use utwm_config::{ActionId, MenuGroup, Target};

use crate::controller::Command;

type R = tauri::DynRuntime;

const TRAY_ID: &str = "universal-tiling-window-manager";
const APP_NAME: &str = "universal-tiling-window-manager";

/// One line saying where the shortcuts came from and how many bound.
pub fn status_line(config: &Resolved, error: Option<&str>) -> String {
    if let Some(error) = error {
        return format!("Config error, keeping previous shortcuts: {error}");
    }
    let source = match (&config.source.kind, &config.source.version) {
        (Some(kind), Some(version)) if !version.is_empty() => {
            format!("{} build {version}", kind.label())
        }
        (Some(kind), _) => kind.label().to_owned(),
        (None, _) => "Rectangle defaults".to_owned(),
    };
    let skipped = if config.skipped.is_empty() {
        String::new()
    } else {
        format!(", {} not bound", config.skipped.len())
    };
    format!("{source}: {} shortcuts{skipped}", config.bindings.len())
}

fn tooltip(config: &Resolved, paused: bool) -> String {
    if paused {
        format!("{APP_NAME}: paused")
    } else {
        format!("{APP_NAME}: {} shortcuts", config.bindings.len())
    }
}

pub fn create(
    app: &tauri::AppHandle,
    config: &Resolved,
    status: &str,
    paused: bool,
    commands: Sender<Command>,
) -> tauri::Result<()> {
    let menu = build_menu(app, config, status, paused)?;
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip(tooltip(config, paused))
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |_app, event| {
            if let Some(command) = command_for(event.id().as_ref()) {
                let _ = commands.send(command);
            }
        })
        .build(app)?;
    Ok(())
}

pub fn refresh(
    app: &tauri::AppHandle,
    config: &Resolved,
    status: &str,
    paused: bool,
) -> tauri::Result<()> {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return Ok(());
    };
    tray.set_menu(Some(build_menu(app, config, status, paused)?))?;
    tray.set_tooltip(Some(tooltip(config, paused)))?;
    Ok(())
}

pub fn set_tooltip(app: &tauri::AppHandle, message: &str) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        // Tooltips are capped at 127 characters by the shell.
        let text: String = message.chars().take(120).collect();
        let _ = tray.set_tooltip(Some(text));
    }
}

fn command_for(id: &str) -> Option<Command> {
    if let Some(name) = id.strip_prefix("action:") {
        return match actions::resolve(name) {
            ActionName::Supported(action) => Some(Command::Menu(Target::Action(action))),
            _ => None,
        };
    }
    if let Some(spec) = id.strip_prefix("spec:") {
        return spec.parse().ok().map(|id| Command::Menu(Target::Spec(id)));
    }
    match id {
        "pause" => Some(Command::TogglePause),
        "reload" => Some(Command::Reload),
        "open-config" => Some(Command::OpenConfigFolder),
        "autostart" => Some(Command::ToggleAutostart),
        "quit" => Some(Command::Quit),
        _ => None,
    }
}

/// "Left Half" with its shortcut right-aligned, the way Windows menus show
/// accelerators.
fn labelled(label: &str, chord: Option<utwm_config::Chord>) -> String {
    match chord {
        Some(chord) => format!("{label}\t{chord}"),
        None => label.to_owned(),
    }
}

fn build_menu(
    app: &tauri::AppHandle,
    config: &Resolved,
    status: &str,
    paused: bool,
) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(
        app,
        "status",
        status,
        false,
        None::<&str>,
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    let groups = [
        MenuGroup::Halves,
        MenuGroup::Corners,
        MenuGroup::Thirds,
        MenuGroup::Size,
        MenuGroup::Displays,
        MenuGroup::Fourths,
        MenuGroup::Sixths,
        MenuGroup::Eighths,
        MenuGroup::Pro,
    ];
    for group in groups {
        let submenu = Submenu::new(app, group.label(), true)?;
        for action in ActionId::all()
            .into_iter()
            .filter(|action| action.group() == group)
        {
            let chord = config.chord_for(Target::Action(action));
            let item = MenuItem::with_id(
                app,
                format!("action:{}", action.name()),
                labelled(&action.label(), chord),
                true,
                None::<&str>,
            )?;
            submenu.append(&item)?;
        }
        menu.append(&submenu)?;
    }

    let visible_specs: Vec<_> = config
        .specs
        .iter()
        .filter(|spec| !config.hidden_specs.contains(&spec.id))
        .collect();
    if !visible_specs.is_empty() {
        let custom = Submenu::new(app, "Custom", true)?;
        for spec in visible_specs {
            let chord = config.chord_for(Target::Spec(spec.id));
            let item = MenuItem::with_id(
                app,
                format!("spec:{}", spec.id),
                labelled(&spec.name, chord),
                true,
                None::<&str>,
            )?;
            custom.append(&item)?;
        }
        menu.append(&custom)?;
    }

    let separator = PredefinedMenuItem::separator(app)?;
    let pause =
        CheckMenuItem::with_id(app, "pause", "Pause shortcuts", true, paused, None::<&str>)?;
    let reload = MenuItem::with_id(app, "reload", "Reload configuration", true, None::<&str>)?;
    let open = MenuItem::with_id(
        app,
        "open-config",
        "Open configuration folder",
        true,
        None::<&str>,
    )?;
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "Start at login",
        true,
        utwm_windows::autostart::is_enabled(),
        None::<&str>,
    )?;
    let separator_2 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let items: [&dyn IsMenuItem<R>; 7] = [
        &separator,
        &pause,
        &reload,
        &open,
        &autostart,
        &separator_2,
        &quit,
    ];
    menu.append_items(&items)?;
    Ok(menu)
}
