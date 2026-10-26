//! GNOME's own shortcuts on keys this app's bindings want, and freeing them.
//!
//! GNOME binds some of the keys Rectangle's default set uses: Ctrl+Alt with an
//! arrow switches workspaces, Ctrl+Alt+Shift+Up moves a window to the
//! workspace above, Ctrl+Alt+D shows the desktop and Ubuntu's Ctrl+Alt+T opens
//! a terminal. Mutter refuses a grab another binding holds, so those come back
//! refused. For each refused chord this finds the GNOME setting that names the
//! same key and, when asked, removes exactly that key from it, leaving the
//! setting's other keys alone (Super+Page Up still switches workspaces).
//!
//! GNOME names keys by symbol, and a symbol can move with the layout. A raw
//! keycode, or a key whose name never moves (an arrow, Enter, a function or
//! keypad key), matches exactly; a letter, digit or punctuation key matches at
//! its US position, where it sits on most layouts. Only chords Mutter refused
//! are looked for, and the app puts back whatever freeing didn't help.

use std::fmt;

use utwm_config::{Chord, Modifiers, PhysicalKey};

use crate::gsettings;

/// The settings whose shortcuts Mutter and gnome-settings-daemon grab.
const SCHEMAS: &[&str] = &[
    "org.gnome.desktop.wm.keybindings",
    "org.gnome.mutter.keybindings",
    "org.gnome.shell.keybindings",
    MEDIA_KEYS,
];
const MEDIA_KEYS: &str = "org.gnome.settings-daemon.plugins.media-keys";
/// A custom shortcut, one per path listed in the media keys' `custom-keybindings`.
const CUSTOM: &str = "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding";

/// A GNOME setting holding one of this app's keys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Holder {
    /// The schema, with `:path` for a custom shortcut.
    pub schema: String,
    pub key: String,
    /// The accelerator as GNOME has it, e.g. `<Primary><Alt>t`.
    pub accelerator: String,
    pub chord: Chord,
    /// Whether the setting holds this one accelerator rather than a list, as
    /// a custom shortcut's `binding` does.
    single: bool,
}

impl fmt::Display for Holder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", self.schema, self.key, self.accelerator)
    }
}

/// Logging out, and the keys gnome-settings-daemon keeps for hardware
/// buttons, are never edited.
fn never_edited(key: &str) -> bool {
    key == "logout" || key.ends_with("-static")
}

/// The GNOME settings that hold any of `chords`.
pub fn holders(chords: &[Chord]) -> Vec<Holder> {
    if chords.is_empty() {
        return Vec::new();
    }
    let mut holders: Vec<Holder> = SCHEMAS
        .iter()
        .flat_map(|schema| holders_in(schema, &gsettings::list(schema), chords))
        .collect();
    let paths = gsettings::get(MEDIA_KEYS, "custom-keybindings")
        .and_then(|text| gsettings::parse_strings(&text))
        .unwrap_or_default();
    for path in paths {
        let schema = format!("{CUSTOM}:{path}");
        let Some(accelerator) =
            gsettings::get(&schema, "binding").and_then(|text| gsettings::parse_string(&text))
        else {
            continue;
        };
        if let Some(chord) = parse(&accelerator).filter(|chord| chords.contains(chord)) {
            holders.push(Holder {
                schema,
                key: "binding".to_owned(),
                accelerator,
                chord,
                single: true,
            });
        }
    }
    holders
}

/// The holders of `chords` among one schema's keys, as `gsettings` lists them.
fn holders_in(schema: &str, keys: &[(String, String)], chords: &[Chord]) -> Vec<Holder> {
    let mut holders = Vec::new();
    for (key, value) in keys {
        if never_edited(key) {
            continue;
        }
        let Some(accelerators) = gsettings::parse_strings(value) else {
            continue;
        };
        for accelerator in accelerators {
            if let Some(chord) = parse(&accelerator).filter(|chord| chords.contains(chord)) {
                holders.push(Holder {
                    schema: schema.to_owned(),
                    key: key.clone(),
                    accelerator,
                    chord,
                    single: false,
                });
            }
        }
    }
    holders
}

/// Removes each holder's accelerator from its setting, leaving the setting's
/// other accelerators alone.
pub fn free(holders: &[Holder]) -> std::io::Result<()> {
    for holder in holders {
        if holder.single {
            gsettings::set(&holder.schema, &holder.key, "''")?;
        } else {
            let current = current_list(holder);
            let kept = without(&current, &holder.accelerator);
            if kept.len() != current.len() {
                gsettings::set(
                    &holder.schema,
                    &holder.key,
                    &gsettings::format_strings(&kept),
                )?;
            }
        }
        tracing::info!(%holder, "freed a GNOME shortcut for this app");
    }
    Ok(())
}

/// Puts each holder's accelerator back where it was.
pub fn restore(holders: &[Holder]) -> std::io::Result<()> {
    for holder in holders {
        if holder.single {
            gsettings::set(
                &holder.schema,
                &holder.key,
                &gsettings::quote(&holder.accelerator),
            )?;
        } else {
            let mut current = current_list(holder);
            if !current.contains(&holder.accelerator) {
                current.push(holder.accelerator.clone());
                gsettings::set(
                    &holder.schema,
                    &holder.key,
                    &gsettings::format_strings(&current),
                )?;
            }
        }
        tracing::info!(%holder, "gave a shortcut back to GNOME");
    }
    Ok(())
}

fn current_list(holder: &Holder) -> Vec<String> {
    gsettings::get(&holder.schema, &holder.key)
        .and_then(|text| gsettings::parse_strings(&text))
        .unwrap_or_default()
}

fn without(accelerators: &[String], removed: &str) -> Vec<String> {
    accelerators
        .iter()
        .filter(|accelerator| *accelerator != removed)
        .cloned()
        .collect()
}

/// The chord a GNOME accelerator names, if this app can tell its key.
fn parse(accelerator: &str) -> Option<Chord> {
    let mut modifiers = Modifiers::NONE;
    let mut rest = accelerator.trim();
    while let Some(tail) = rest.strip_prefix('<') {
        let (name, after) = tail.split_once('>')?;
        match name.to_ascii_lowercase().as_str() {
            "primary" | "control" | "ctrl" | "ctl" => modifiers.ctrl = true,
            "alt" | "mod1" | "meta" => modifiers.alt = true,
            "super" | "mod4" | "hyper" => modifiers.win = true,
            "shift" | "shft" => modifiers.shift = true,
            // `<Release>` fires as the key comes up, which no grab here takes.
            _ => return None,
        }
        rest = after;
    }
    if rest.is_empty() || rest.eq_ignore_ascii_case("disabled") {
        return None;
    }
    let key = match rest.strip_prefix("0x") {
        Some(hex) => PhysicalKey::from_evdev(u16::from_str_radix(hex, 16).ok()?.checked_sub(8)?)?,
        None => key_for(rest)?,
    };
    Some(Chord { modifiers, key })
}

/// The key a symbol is on: exactly, for keys whose names don't move with
/// the layout; at the US position for letters, digits and punctuation.
fn key_for(keysym: &str) -> Option<PhysicalKey> {
    let name = KEYSYMS
        .iter()
        .find(|(candidate, _)| candidate.eq_ignore_ascii_case(keysym))
        .map(|(_, name)| *name)
        .or_else(|| {
            let single = keysym.len() == 1 && keysym.chars().all(|c| c.is_ascii_alphanumeric());
            let function = keysym
                .strip_prefix(['F', 'f'])
                .is_some_and(|number| number.parse::<u8>().is_ok());
            (single || function).then_some(keysym)
        })?;
    PhysicalKey::from_windows_name(name)
}

/// GNOME's key names (X keysyms) and the PC key each is on.
const KEYSYMS: &[(&str, &str)] = &[
    ("Left", "Left"),
    ("Right", "Right"),
    ("Up", "Up"),
    ("Down", "Down"),
    ("Return", "Enter"),
    ("BackSpace", "Backspace"),
    ("Tab", "Tab"),
    ("Escape", "Esc"),
    ("space", "Space"),
    ("Home", "Home"),
    ("End", "End"),
    ("Page_Up", "PageUp"),
    ("Prior", "PageUp"),
    ("Page_Down", "PageDown"),
    ("Next", "PageDown"),
    ("Insert", "Insert"),
    ("Delete", "Delete"),
    ("Menu", "Menu"),
    ("KP_0", "Num0"),
    ("KP_1", "Num1"),
    ("KP_2", "Num2"),
    ("KP_3", "Num3"),
    ("KP_4", "Num4"),
    ("KP_5", "Num5"),
    ("KP_6", "Num6"),
    ("KP_7", "Num7"),
    ("KP_8", "Num8"),
    ("KP_9", "Num9"),
    ("KP_Insert", "Num0"),
    ("KP_End", "Num1"),
    ("KP_Down", "Num2"),
    ("KP_Next", "Num3"),
    ("KP_Page_Down", "Num3"),
    ("KP_Left", "Num4"),
    ("KP_Begin", "Num5"),
    ("KP_Right", "Num6"),
    ("KP_Home", "Num7"),
    ("KP_Up", "Num8"),
    ("KP_Prior", "Num9"),
    ("KP_Page_Up", "Num9"),
    ("KP_Delete", "Num."),
    ("KP_Decimal", "Num."),
    ("KP_Enter", "NumEnter"),
    ("KP_Add", "Num+"),
    ("KP_Subtract", "Num-"),
    ("KP_Multiply", "Num*"),
    ("KP_Divide", "Num/"),
    ("KP_Equal", "Num="),
    ("minus", "-"),
    ("equal", "="),
    ("bracketleft", "["),
    ("bracketright", "]"),
    ("backslash", "\\"),
    ("semicolon", ";"),
    ("apostrophe", "'"),
    ("grave", "`"),
    // Mutter's name for the key above Tab, whatever it carries.
    ("Above_Tab", "`"),
    ("comma", ","),
    ("period", "."),
    ("slash", "/"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(text: &str) -> Chord {
        Chord::parse(text).expect("parses")
    }

    #[test]
    fn gnomes_defaults_name_the_keys_they_hold() {
        for (accelerator, expected) in [
            ("<Control><Alt>Left", "ctrl+alt+left"),
            ("<Primary><Alt>d", "ctrl+alt+d"),
            ("<Control><Shift><Alt>Up", "ctrl+alt+shift+up"),
            ("<Primary><Alt>t", "ctrl+alt+t"),
            ("<Super>Page_Up", "super+pageup"),
            ("<Super>KP_Prior", "super+num9"),
            ("<Super>Above_Tab", "super+`"),
            ("<Control><Alt>0x71", "ctrl+alt+left"),
            ("<Shift><Super>F10", "shift+super+f10"),
        ] {
            assert_eq!(parse(accelerator), Some(chord(expected)), "{accelerator}");
        }
        for accelerator in [
            "",
            "disabled",
            "<Release>Super_L",
            "XF86Calculator",
            "<Super>",
        ] {
            assert_eq!(parse(accelerator), None, "{accelerator}");
        }
    }

    #[test]
    fn only_the_settings_naming_refused_chords_are_holders() {
        let keys = vec![
            (
                "switch-to-workspace-left".to_owned(),
                "['<Super>Page_Up', '<Super>KP_Prior', '<Super><Alt>Left', '<Control><Alt>Left']"
                    .to_owned(),
            ),
            (
                "show-desktop".to_owned(),
                "['<Primary><Super>d', '<Primary><Alt>d', '<Super>d']".to_owned(),
            ),
            (
                "logout".to_owned(),
                "['<Control><Alt>Delete', '<Control><Alt>Left']".to_owned(),
            ),
            ("volume-step".to_owned(), "6".to_owned()),
        ];
        let refused = [chord("ctrl+alt+left"), chord("ctrl+alt+d")];
        let found = holders_in("org.gnome.desktop.wm.keybindings", &keys, &refused);
        let found: Vec<(&str, &str)> = found
            .iter()
            .map(|holder| (holder.key.as_str(), holder.accelerator.as_str()))
            .collect();
        assert_eq!(
            found,
            [
                ("switch-to-workspace-left", "<Control><Alt>Left"),
                ("show-desktop", "<Primary><Alt>d")
            ]
        );
    }

    #[test]
    fn freeing_keeps_the_settings_other_keys() {
        let current: Vec<String> = ["<Super>Page_Up", "<Super><Alt>Left", "<Control><Alt>Left"]
            .map(str::to_owned)
            .to_vec();
        assert_eq!(
            without(&current, "<Control><Alt>Left"),
            ["<Super>Page_Up", "<Super><Alt>Left"]
        );
    }
}
