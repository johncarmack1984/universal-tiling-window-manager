//! The tray icon (the menu bar icon on the Mac) and its menu, laid out like
//! Rectangle's menu: every action with a picture of where it puts the window
//! and its shortcut, grouped, then the app's own switches.
//!
//! On Linux the GNOME Shell extension draws the same menu in the top bar once
//! it runs, each group a row of its actions' pictures, and the tray icon,
//! which GNOME shows through its AppIndicator extension, stands in until
//! then.

use std::path::Path;
use std::sync::mpsc::Sender;

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, IconMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use utwm_config::actions::{self, ActionName};
use utwm_config::bindings::Resolved;
use utwm_config::{ActionId, Chord, MenuGroup, Platform, Target};

use crate::controller::Command;
use crate::icons::{self, Picture};
use crate::platform;

type R = tauri::DynRuntime;

const TRAY_ID: &str = "universal-tiling-window-manager";

/// The tray's icon. The Mac's menu bar wants a black template image, which
/// it tints to match the bar; the Windows tray and GNOME's top bar show the
/// icon in colour.
#[cfg(target_os = "macos")]
const TRAY_ICON: &[u8] = include_bytes!("../icons/tray-template.png");
#[cfg(not(target_os = "macos"))]
const TRAY_ICON: &[u8] = include_bytes!("../icons/tray.png");

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

/// `text` as a menu should show it. Menus take an `&` to mark the letter
/// after it as the item's access key, and the Mac drops it, so "Fill & Tile"
/// came out "Fill  Tile"; a doubled one shows as written.
fn literal(text: &str) -> String {
    text.replace('&', "&&")
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
    let lines = lines(
        config,
        status,
        paused,
        platform::autostart::is_enabled(),
        updates,
    );
    let menu = build_menu(app, &lines)?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(TRAY_ICON)?)
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

/// Brings the menu up to date. Returns whether it shows anywhere: on Linux
/// it may be in neither place yet, with no extension to draw it and no tray
/// icon made.
pub fn refresh(
    app: &tauri::AppHandle,
    config: &Resolved,
    status: &Status,
    paused: bool,
    updates: Option<&UpdatesLine>,
) -> tauri::Result<bool> {
    let lines = lines(
        config,
        status,
        paused,
        platform::autostart::is_enabled(),
        updates,
    );
    let tooltip = tooltip(app, config, paused);
    let tray = app.tray_by_id(TRAY_ID);
    // GNOME's top bar shows one or the other.
    #[cfg(target_os = "linux")]
    if utwm_linux::menu::available()
        && utwm_linux::menu::show(&tooltip, TRAY_ICON, &shell_items(&lines))
    {
        if let Some(tray) = &tray {
            tray.set_visible(false)?;
        }
        return Ok(true);
    }
    let Some(tray) = tray else {
        return Ok(false);
    };
    tray.set_menu(Some(build_menu(app, &lines)?))?;
    tray.set_tooltip(Some(tooltip))?;
    #[cfg(target_os = "linux")]
    tray.set_visible(true)?;
    Ok(true)
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

/// Forwards clicks on the menu the GNOME Shell extension draws into the
/// controller's channel, as the tray's own menu does.
#[cfg(target_os = "linux")]
pub fn shell_menu_reporter(commands: Sender<Command>) -> Box<dyn Fn(String) + Send + 'static> {
    Box::new(move |id| {
        if let Some(command) = command_for(&id) {
            let _ = commands.send(command);
        }
    })
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

/// A picture drawn the way this platform's menus show one.
fn image(picture: Option<Picture>) -> Option<Image<'static>> {
    let icon = icons::draw(picture?, icons::Style::MENU);
    Some(Image::new_owned(icon.rgba, icon.width, icon.height))
}

/// Appends a menu item for one action, with its picture, and its shortcut
/// shown the way the platform's menus do.
fn append_action(
    submenu: &Submenu<R>,
    app: &tauri::AppHandle,
    id: String,
    label: &str,
    picture: Option<Picture>,
    chord: Option<Chord>,
) -> tauri::Result<()> {
    let icon = image(picture);
    let label = literal(label);
    #[cfg(target_os = "macos")]
    if let Some(accelerator) = chord.and_then(accelerator) {
        // macOS draws the shortcut right-aligned in its own glyphs. The
        // native item is only made on append, so that is where a key the menu
        // can't draw would fail; the plain label below stands in.
        let item = IconMenuItem::with_id(
            app,
            id.clone(),
            &label,
            true,
            icon.clone(),
            Some(accelerator),
        )?;
        if submenu.append(&item).is_ok() {
            return Ok(());
        }
    }
    let text = match chord {
        // Windows right-aligns what follows a tab, as menus show accelerators.
        Some(chord) if cfg!(windows) => format!("{label}\t{}", chord_label(chord)),
        // GNOME's tray draws a tab as plain space and shows no accelerators.
        Some(chord) => format!("{label} ({})", chord_label(chord)),
        None => label,
    };
    submenu.append(&IconMenuItem::with_id(
        app,
        id,
        text,
        true,
        icon,
        None::<&str>,
    )?)
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

/// One line of the menu, before a platform draws it.
enum Line {
    /// Text, which a click acts on while it's enabled.
    Text {
        id: &'static str,
        text: String,
        enabled: bool,
    },
    /// A switch, ticked when it's on.
    Check {
        id: &'static str,
        text: String,
        checked: bool,
    },
    Separator,
    /// A group of actions or custom positions, with the picture that stands
    /// for them all.
    Group {
        label: String,
        picture: Option<Picture>,
        items: Vec<Item>,
    },
}

/// An action or a custom position in a group.
struct Item {
    /// What a click on it reports: `action:<name>` or `spec:<id>`.
    id: String,
    label: String,
    picture: Option<Picture>,
    chord: Option<Chord>,
}

/// What the menu holds, in order: where the shortcuts came from, every
/// action by group, then the app's own switches. Each platform draws these
/// lines its own way.
fn lines(
    config: &Resolved,
    status: &Status,
    paused: bool,
    autostart: bool,
    updates: Option<&UpdatesLine>,
) -> Vec<Line> {
    let mut lines = vec![
        Line::Text {
            id: "status",
            text: status.text.clone(),
            enabled: status.clickable,
        },
        Line::Separator,
    ];

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
    let settings = &config.settings;
    for group in groups {
        lines.push(Line::Group {
            label: group.label().to_owned(),
            picture: icons::for_group(group, settings),
            items: ActionId::all()
                .into_iter()
                .filter(|action| action.group() == group)
                .map(|action| Item {
                    id: format!("action:{}", action.name()),
                    label: action.label(),
                    picture: icons::for_action(action, settings),
                    chord: config.chord_for(Target::Action(action)),
                })
                .collect(),
        });
    }

    let specs: Vec<Item> = config
        .specs
        .iter()
        .filter(|spec| !config.hidden_specs.contains(&spec.id))
        .map(|spec| Item {
            id: format!("spec:{}", spec.id),
            label: spec.name.clone(),
            picture: icons::for_spec(spec, settings),
            chord: config.chord_for(Target::Spec(spec.id)),
        })
        .collect();
    if let Some(first) = specs.first() {
        lines.push(Line::Group {
            label: "Custom".to_owned(),
            // The first custom position stands for them all.
            picture: first.picture,
            items: specs,
        });
    }

    lines.extend([
        Line::Separator,
        Line::Check {
            id: "pause",
            text: "Pause shortcuts".to_owned(),
            checked: paused,
        },
        Line::Text {
            id: "reload",
            text: "Reload configuration".to_owned(),
            enabled: true,
        },
        Line::Text {
            id: "choose-config",
            text: "Choose configuration…".to_owned(),
            enabled: true,
        },
        Line::Text {
            id: "open-config",
            text: "Open configuration folder".to_owned(),
            enabled: true,
        },
        Line::Check {
            id: "autostart",
            text: "Start at login".to_owned(),
            checked: autostart,
        },
    ]);
    if let Some(updates) = updates {
        lines.push(Line::Text {
            id: "update",
            text: updates.text.clone(),
            enabled: updates.enabled,
        });
    }
    lines.extend([
        Line::Separator,
        Line::Text {
            id: "quit",
            text: "Quit".to_owned(),
            enabled: true,
        },
    ]);
    lines
}

/// The lines as the tray's menu: each group a submenu.
fn build_menu(app: &tauri::AppHandle, lines: &[Line]) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    for line in lines {
        match line {
            Line::Text { id, text, enabled } => menu.append(&MenuItem::with_id(
                app,
                *id,
                literal(text),
                *enabled,
                None::<&str>,
            )?)?,
            Line::Check { id, text, checked } => menu.append(&CheckMenuItem::with_id(
                app,
                *id,
                literal(text),
                true,
                *checked,
                None::<&str>,
            )?)?,
            Line::Separator => menu.append(&PredefinedMenuItem::separator(app)?)?,
            Line::Group {
                label,
                picture,
                items,
            } => {
                let submenu = Submenu::new_with_icon(app, literal(label), true, image(*picture))?;
                for item in items {
                    append_action(
                        &submenu,
                        app,
                        item.id.clone(),
                        &item.label,
                        item.picture,
                        item.chord,
                    )?;
                }
                menu.append(&submenu)?;
            }
        }
    }
    Ok(menu)
}

/// The lines as the menu the GNOME Shell extension draws: each group a row
/// of its items' pictures, each with its name and shortcut.
#[cfg(target_os = "linux")]
fn shell_items(lines: &[Line]) -> Vec<utwm_linux::menu::Item> {
    use utwm_linux::menu;

    let mut shell = Vec::new();
    for line in lines {
        match line {
            Line::Text { id, text, enabled } => {
                shell.push(menu::Item::text(*id, text.as_str(), *enabled));
            }
            Line::Check { id, text, checked } => {
                shell.push(menu::Item::check(*id, text.as_str(), *checked));
            }
            Line::Separator => shell.push(menu::Item::separator()),
            Line::Group { label, items, .. } => {
                let group = format!("group:{label}");
                shell.push(menu::Item::group(group.as_str(), label.as_str()));
                shell.extend(items.iter().map(|item| {
                    menu::Item::action(
                        item.id.as_str(),
                        group.as_str(),
                        item.label.as_str(),
                        item.chord.map(chord_label),
                        item.picture
                            .and_then(|picture| icons::draw(picture, icons::Style::SHELL).png()),
                    )
                }));
            }
        }
    }
    shell
}

#[cfg(test)]
mod content_tests {
    use utwm_config::Overrides;
    use utwm_config::specs::{Anchor, Dimension, ManualSpec, Placement};

    use super::*;

    fn config(specs: Vec<ManualSpec>) -> Resolved {
        Resolved {
            specs,
            ..utwm_config::load::builtin(&Overrides::default())
        }
    }

    fn custom(id: i64, name: &str) -> ManualSpec {
        ManualSpec {
            id,
            name: name.to_owned(),
            shortcut: None,
            placement: Placement {
                anchor: Anchor::BottomLeft,
                x: Dimension::Keep,
                y: Dimension::Keep,
                width: Dimension::Fraction(1.0 / 3.0),
                height: Dimension::Fraction(0.7),
                display: None,
            },
            repetitions: Vec::new(),
        }
    }

    fn menu(config: &Resolved, updates: Option<&UpdatesLine>) -> Vec<Line> {
        let status = Status::plain("22 default shortcuts".to_owned());
        lines(config, &status, true, false, updates)
    }

    fn group_items(lines: &[Line]) -> Vec<(&str, &Item)> {
        lines
            .iter()
            .filter_map(|line| match line {
                Line::Group { label, items, .. } => Some((label.as_str(), items)),
                _ => None,
            })
            .flat_map(|(label, items)| items.iter().map(move |item| (label, item)))
            .collect()
    }

    #[test]
    fn every_action_is_in_its_group_once() {
        let lines = menu(&config(Vec::new()), None);
        let items = group_items(&lines);
        let actions = ActionId::all();
        assert_eq!(items.len(), actions.len());
        for action in actions {
            let id = format!("action:{}", action.name());
            let found: Vec<_> = items.iter().filter(|(_, item)| item.id == id).collect();
            assert_eq!(found.len(), 1, "{id}");
            assert_eq!(found[0].0, action.group().label(), "{id}");
            assert!(
                matches!(command_for(&id), Some(Command::Menu(Target::Action(found))) if found == action),
                "{id}"
            );
        }
    }

    #[test]
    fn custom_positions_follow_the_actions() {
        let mut config = config(vec![custom(1, "Left Below"), custom(2, "Hidden")]);
        config.hidden_specs = vec![2];
        let lines = menu(&config, None);
        let (label, items) = lines
            .iter()
            .rev()
            .find_map(|line| match line {
                Line::Group { label, items, .. } => Some((label, items)),
                _ => None,
            })
            .expect("groups");
        assert_eq!(label, "Custom");
        let ids: Vec<&str> = items.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids, ["spec:1"]);
        assert!(matches!(
            command_for("spec:1"),
            Some(Command::Menu(Target::Spec(1)))
        ));
        let lines = menu(&self::config(Vec::new()), None);
        assert!(
            !lines
                .iter()
                .any(|line| matches!(line, Line::Group { label, .. } if label == "Custom"))
        );
    }

    #[test]
    fn the_status_comes_first_and_quit_last() {
        let updates = UpdatesLine {
            text: "Check for updates".to_owned(),
            enabled: true,
        };
        let lines = menu(&config(Vec::new()), Some(&updates));
        assert!(matches!(
            lines.first(),
            Some(Line::Text {
                id: "status",
                enabled: false,
                ..
            })
        ));
        assert!(matches!(lines.last(), Some(Line::Text { id: "quit", .. })));
        let switches: Vec<(&str, bool)> = lines
            .iter()
            .filter_map(|line| match line {
                Line::Check { id, checked, .. } => Some((*id, *checked)),
                _ => None,
            })
            .collect();
        assert_eq!(switches, [("pause", true), ("autostart", false)]);
        for line in &lines {
            if let Line::Text { id, .. } | Line::Check { id, .. } = line {
                // The Mac and Linux act on the first line; Windows has
                // nothing to fix there.
                if *id != "status" || cfg!(any(target_os = "macos", target_os = "linux")) {
                    assert!(command_for(id).is_some(), "{id}");
                }
            }
        }
        assert!(lines.iter().any(
            |line| matches!(line, Line::Text { id: "update", text, .. } if text == "Check for updates")
        ));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn gnome_shell_gets_each_group_then_its_pictures() {
        use utwm_linux::menu::{Kind, MAX_IMAGE_BYTES, MAX_ITEMS, PICTURE_SIZE};

        // A PNG's width, from its header; drawn at twice the size the
        // extension shows, a picture stays sharp on a HiDPI screen.
        let width = |png: &[u8]| {
            png.get(16..20)
                .and_then(|bytes| bytes.try_into().ok())
                .map(u32::from_be_bytes)
        };

        let items = shell_items(&menu(&config(vec![custom(7, "Left Below")]), None));
        assert!(items.len() <= MAX_ITEMS);
        let mut group = None;
        let mut actions = 0;
        for item in &items {
            match item.kind {
                Kind::Group => group = Some(item.id.clone()),
                Kind::Action => {
                    actions += 1;
                    assert!(group.is_some(), "{} comes before any group", item.id);
                    assert_eq!(item.group, group, "{}", item.id);
                    let picture = item.picture.as_deref().expect("every action has a picture");
                    assert!(picture.starts_with(b"\x89PNG\r\n\x1a\n"), "{}", item.id);
                    assert!(picture.len() <= MAX_IMAGE_BYTES, "{}", item.id);
                    assert_eq!(width(picture), Some(2 * PICTURE_SIZE), "{}", item.id);
                    assert!(command_for(&item.id).is_some(), "{}", item.id);
                }
                _ => {
                    group = None;
                    assert_eq!(item.group, None, "{}", item.id);
                }
            }
        }
        assert_eq!(actions, ActionId::all().len() + 1);
        // Labels go as written: GNOME's menus take no access keys.
        assert!(
            items
                .iter()
                .any(|item| item.kind == Kind::Group && item.label == "Fill & Tile")
        );
        let left_half = items
            .iter()
            .find(|item| item.id == "action:leftHalf")
            .expect("Left Half");
        assert_eq!(left_half.label, "Left Half");
        assert_eq!(left_half.shortcut.as_deref(), Some("Ctrl+Alt+Left"));
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    fn accelerator_for(text: &str) -> Option<String> {
        accelerator(Chord::parse(text).expect("parses"))
    }

    #[test]
    fn ampersands_show_as_written() {
        assert_eq!(literal("Fill & Tile"), "Fill && Tile");
        assert_eq!(literal("Left 1/3 Below Movie"), "Left 1/3 Below Movie");
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
