//! The tray icon (the menu bar icon on the Mac) and its menu, laid out like
//! Rectangle's menu: every action with its shortcut, grouped, then the app's
//! own switches.

use std::path::Path;
use std::sync::mpsc::Sender;

use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use utwm_config::actions::{self, ActionName};
use utwm_config::bindings::Resolved;
use utwm_config::{ActionId, Chord, MenuGroup, Platform, Target};

use crate::controller::Command;
use crate::platform;

type R = tauri::DynRuntime;

const TRAY_ID: &str = "universal-tiling-window-manager";

/// The menu's first line.
pub struct Status {
    pub text: String,
    /// Whether clicking it takes the step it describes: asking for
    /// Accessibility access on the Mac, or on Linux setting up the GNOME Shell
    /// extension or freeing GNOME's shortcuts.
    pub clickable: bool,
}

impl Status {
    pub fn plain(text: String) -> Self {
        Self {
            text,
            clickable: false,
        }
    }
}

/// The updates line, on a copy that updates itself.
pub struct UpdatesLine {
    pub text: String,
    /// Whether clicking it checks now; off while a check or an install runs.
    pub enabled: bool,
}

/// One line saying where the shortcuts came from and how many bound.
pub fn status_line(config: &Resolved, error: Option<&str>, refused: usize, home: &Path) -> String {
    if let Some(error) = error {
        return format!("Config error, keeping previous shortcuts: {error}");
    }
    let count = config.bindings.len();
    let shortcuts = match (&config.source.kind, &config.source.path) {
        (Some(kind), _) => format!("{count} shortcuts imported from {}", kind.label()),
        // Nothing was where the config goes, so the line says where that is.
        (None, Some(path)) => format!(
            "{count} default shortcuts: nothing at {}",
            tilde(path, home)
        ),
        (None, None) => format!("{count} default shortcuts"),
    };
    let skipped = if config.skipped.is_empty() {
        String::new()
    } else {
        format!(", {} not bound", config.skipped.len())
    };
    let refused = if refused == 0 {
        String::new()
    } else {
        format!(", {refused} taken by other apps")
    };
    format!("{shortcuts}{skipped}{refused}")
}

/// `path` with the home folder written as `~`.
fn tilde(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) => Path::new("~").join(rest).display().to_string(),
        Err(_) => path.display().to_string(),
    }
}

/// A chord the way this platform's keyboard labels it: `Ctrl+Alt+Left` on
/// Windows and Linux, `⌃⌥←` on the Mac.
pub fn chord_label(chord: Chord) -> String {
    chord.label(Platform::current())
}

/// The app's name (`productName`) and what it is doing.
fn tooltip(app: &tauri::AppHandle, config: &Resolved, paused: bool) -> String {
    let name = &app.package_info().name;
    if paused {
        format!("{name}: paused")
    } else {
        format!("{name}: {} shortcuts", config.bindings.len())
    }
}

pub fn create(
    app: &tauri::AppHandle,
    config: &Resolved,
    status: &Status,
    paused: bool,
    updates: Option<&UpdatesLine>,
    commands: Sender<Command>,
) -> tauri::Result<()> {
    let menu = build_menu(app, config, status, paused, updates)?;
    // The Mac's menu bar wants a black template image, which it tints to
    // match the bar; the Windows tray shows the icon in colour.
    #[cfg(target_os = "macos")]
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-template.png"))?;
    #[cfg(not(target_os = "macos"))]
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip(tooltip(app, config, paused))
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
    status: &Status,
    paused: bool,
    updates: Option<&UpdatesLine>,
) -> tauri::Result<()> {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return Ok(());
    };
    tray.set_menu(Some(build_menu(app, config, status, paused, updates)?))?;
    tray.set_tooltip(Some(tooltip(app, config, paused)))?;
    Ok(())
}

/// GNOME's tray never shows tooltips, so Linux notifies instead.
#[cfg(not(target_os = "linux"))]
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
        "choose-config" => Some(Command::ChooseConfig),
        "open-config" => Some(Command::OpenConfigFolder),
        "autostart" => Some(Command::ToggleAutostart),
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        "status" => Some(Command::FixStatus),
        "update" => Some(Command::CheckForUpdates),
        "quit" => Some(Command::Quit),
        _ => None,
    }
}

/// Appends a menu item for one action, showing its shortcut the way the
/// platform's menus do.
fn append_action(
    submenu: &Submenu<R>,
    app: &tauri::AppHandle,
    id: String,
    label: &str,
    chord: Option<Chord>,
) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    if let Some(accelerator) = chord.and_then(accelerator) {
        // macOS draws the shortcut right-aligned in its own glyphs. The
        // native item is only made on append, so that is where a key the menu
        // can't draw would fail; the plain label below stands in.
        let item = MenuItem::with_id(app, id.clone(), label, true, Some(accelerator))?;
        if submenu.append(&item).is_ok() {
            return Ok(());
        }
    }
    let text = match chord {
        // Windows right-aligns what follows a tab, as menus show accelerators.
        Some(chord) if cfg!(windows) => format!("{label}\t{}", chord_label(chord)),
        // GNOME's tray draws a tab as plain space and shows no accelerators.
        Some(chord) => format!("{label} ({})", chord_label(chord)),
        None => label.to_owned(),
    };
    submenu.append(&MenuItem::with_id(app, id, text, true, None::<&str>)?)
}

/// The chord as a menu accelerator. `None` for keys a macOS menu can't draw
/// (keypad Clear, the JIS keys, the volume keys) and for forward delete,
/// which menus draw with the delete key's glyph; those show their shortcut in
/// the label instead.
#[cfg(target_os = "macos")]
fn accelerator(chord: Chord) -> Option<String> {
    let name = chord.key.windows_name();
    let key = match name {
        "Up" => "ArrowUp",
        "Down" => "ArrowDown",
        "Left" => "ArrowLeft",
        "Right" => "ArrowRight",
        "Esc" => "Escape",
        // The Mac's Help key sits where Insert is.
        "Insert" => "Help",
        "Num*" => "NumMultiply",
        "Num+" => "NumAdd",
        "Num-" => "NumSubtract",
        "Num." => "NumDecimal",
        "Num/" => "NumDivide",
        "Num=" => "NumEqual",
        "Num," => "NumpadComma",
        "Backspace" | "Tab" | "Enter" | "Space" | "Home" | "End" | "PageUp" | "PageDown"
        | "NumEnter" | "IntlYen" => name,
        _ if name.len() == 1 => name,
        _ if name
            .strip_prefix("Num")
            .is_some_and(|digit| digit.len() == 1) =>
        {
            name
        }
        _ if name
            .strip_prefix('F')
            .and_then(|number| number.parse::<u8>().ok())
            .is_some_and(|number| (1..=20).contains(&number)) =>
        {
            name
        }
        _ => return None,
    };
    let modifiers = chord.modifiers;
    let mut accelerator = String::new();
    for (on, name) in [
        (modifiers.ctrl, "Ctrl+"),
        (modifiers.alt, "Alt+"),
        (modifiers.shift, "Shift+"),
        (modifiers.win, "Cmd+"),
    ] {
        if on {
            accelerator.push_str(name);
        }
    }
    accelerator.push_str(key);
    Some(accelerator)
}

fn build_menu(
    app: &tauri::AppHandle,
    config: &Resolved,
    status: &Status,
    paused: bool,
    updates: Option<&UpdatesLine>,
) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(
        app,
        "status",
        &status.text,
        status.clickable,
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
            append_action(
                &submenu,
                app,
                format!("action:{}", action.name()),
                &action.label(),
                chord,
            )?;
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
            append_action(&custom, app, format!("spec:{}", spec.id), &spec.name, chord)?;
        }
        menu.append(&custom)?;
    }

    let separator = PredefinedMenuItem::separator(app)?;
    let pause =
        CheckMenuItem::with_id(app, "pause", "Pause shortcuts", true, paused, None::<&str>)?;
    let reload = MenuItem::with_id(app, "reload", "Reload configuration", true, None::<&str>)?;
    let choose = MenuItem::with_id(
        app,
        "choose-config",
        "Choose configuration…",
        true,
        None::<&str>,
    )?;
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
        platform::autostart::is_enabled(),
        None::<&str>,
    )?;
    let separator_2 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let items: [&dyn IsMenuItem<R>; 6] = [&separator, &pause, &reload, &choose, &open, &autostart];
    menu.append_items(&items)?;
    if let Some(updates) = updates {
        menu.append(&MenuItem::with_id(
            app,
            "update",
            &updates.text,
            updates.enabled,
            None::<&str>,
        )?)?;
    }
    let items: [&dyn IsMenuItem<R>; 2] = [&separator_2, &quit];
    menu.append_items(&items)?;
    Ok(menu)
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    fn accelerator_for(text: &str) -> Option<String> {
        accelerator(Chord::parse(text).expect("parses"))
    }

    #[test]
    fn mac_menus_draw_most_shortcuts_themselves() {
        assert_eq!(
            accelerator_for("ctrl+alt+left").as_deref(),
            Some("Ctrl+Alt+ArrowLeft")
        );
        assert_eq!(
            accelerator_for("ctrl+alt+win+right").as_deref(),
            Some("Ctrl+Alt+Cmd+ArrowRight")
        );
        assert_eq!(
            accelerator_for("ctrl+alt+num7").as_deref(),
            Some("Ctrl+Alt+Num7")
        );
        assert_eq!(
            accelerator_for("ctrl+alt+num=").as_deref(),
            Some("Ctrl+Alt+NumEqual")
        );
        assert_eq!(
            accelerator_for("ctrl+alt+f16").as_deref(),
            Some("Ctrl+Alt+F16")
        );
        assert_eq!(accelerator_for("ctrl+alt+e").as_deref(), Some("Ctrl+Alt+E"));
        assert_eq!(accelerator_for("ctrl+alt+clear"), None);
        assert_eq!(accelerator_for("ctrl+alt+delete"), None);
    }

    #[test]
    fn every_accelerator_parses_as_tauri_will() {
        for text in [
            "ctrl+alt+left",
            "ctrl+alt+enter",
            "ctrl+alt+backspace",
            "ctrl+alt+shift+up",
            "ctrl+alt+=",
            "ctrl+alt+-",
            "ctrl+alt+home",
            "ctrl+alt+num*",
            "ctrl+alt+num+",
            "ctrl+alt+num-",
            "ctrl+alt+num.",
            "ctrl+alt+num/",
            "ctrl+alt+numenter",
            "ctrl+alt+insert",
            "ctrl+alt+f1",
        ] {
            let accelerator = accelerator_for(text).expect(text);
            assert!(
                accelerator
                    .parse::<muda::accelerator::Accelerator>()
                    .is_ok(),
                "{text} → {accelerator}"
            );
        }
    }
}
