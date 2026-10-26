//! Modifier sets and key chords on both sides of the translation.

use std::fmt;

use serde::Deserialize;

use crate::keys::{self, PhysicalKey};
use crate::platform::Platform;

/// NSEvent.ModifierFlags raw values. Rectangle keeps only these four and drops
/// every other bit when it loads a shortcut (MASShortcut's device-independent
/// mask), so hand-written exports with extra bits still mean the same chord.
const NS_SHIFT: u64 = 1 << 17;
const NS_CONTROL: u64 = 1 << 18;
const NS_OPTION: u64 = 1 << 19;
const NS_COMMAND: u64 = 1 << 20;

/// The modifiers of a Mac shortcut.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MacModifiers {
    pub control: bool,
    pub option: bool,
    pub shift: bool,
    pub command: bool,
}

impl MacModifiers {
    pub fn from_flags(flags: u64) -> Self {
        Self {
            control: flags & NS_CONTROL != 0,
            option: flags & NS_OPTION != 0,
            shift: flags & NS_SHIFT != 0,
            command: flags & NS_COMMAND != 0,
        }
    }
}

impl fmt::Display for MacModifiers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The order macOS menus print them in.
        for (on, glyph) in [
            (self.control, "⌃"),
            (self.option, "⌥"),
            (self.shift, "⇧"),
            (self.command, "⌘"),
        ] {
            if on {
                f.write_str(glyph)?;
            }
        }
        Ok(())
    }
}

/// The modifiers of a Windows chord. Left and right count the same.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
}

impl Modifiers {
    pub const NONE: Self = Self {
        ctrl: false,
        alt: false,
        shift: false,
        win: false,
    };

    pub fn is_empty(self) -> bool {
        self == Self::NONE
    }

    fn add(&mut self, modifier: WinModifier) {
        match modifier {
            WinModifier::Ctrl => self.ctrl = true,
            WinModifier::Alt => self.alt = true,
            WinModifier::Shift => self.shift = true,
            WinModifier::Win => self.win = true,
            WinModifier::None => {}
        }
    }
}

impl fmt::Display for Modifiers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, name) in [
            (self.ctrl, "Ctrl+"),
            (self.alt, "Alt+"),
            (self.shift, "Shift+"),
            (self.win, "Win+"),
        ] {
            if on {
                f.write_str(name)?;
            }
        }
        Ok(())
    }
}

/// One Windows modifier key, as the override file names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WinModifier {
    Ctrl,
    Alt,
    Shift,
    Win,
    /// Drop this Mac modifier entirely.
    None,
}

impl fmt::Display for WinModifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ctrl => "Ctrl",
            Self::Alt => "Alt",
            Self::Shift => "Shift",
            Self::Win => "Win",
            Self::None => "nothing",
        })
    }
}

/// Which Windows modifier each Mac modifier becomes.
///
/// The default is what a Mac keyboard sends to Windows, and what RustDesk's Map
/// mode delivers: Control is Ctrl, Option is Alt, Command is Win.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModifierMap {
    pub control: WinModifier,
    pub option: WinModifier,
    pub shift: WinModifier,
    pub command: WinModifier,
}

impl Default for ModifierMap {
    fn default() -> Self {
        Self {
            control: WinModifier::Ctrl,
            option: WinModifier::Alt,
            shift: WinModifier::Shift,
            command: WinModifier::Win,
        }
    }
}

impl ModifierMap {
    pub fn translate(&self, mac: MacModifiers) -> Modifiers {
        let mut modifiers = Modifiers::NONE;
        if mac.control {
            modifiers.add(self.control);
        }
        if mac.option {
            modifiers.add(self.option);
        }
        if mac.shift {
            modifiers.add(self.shift);
        }
        if mac.command {
            modifiers.add(self.command);
        }
        modifiers
    }
}

impl fmt::Display for ModifierMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "⌃→{} ⌥→{} ⇧→{} ⌘→{}",
            self.control, self.option, self.shift, self.command
        )
    }
}

/// A shortcut as the export stores it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MacChord {
    pub key_code: i64,
    pub modifiers: MacModifiers,
}

impl MacChord {
    pub fn new(key_code: i64, modifier_flags: u64) -> Self {
        Self {
            key_code,
            modifiers: MacModifiers::from_flags(modifier_flags),
        }
    }
}

impl fmt::Display for MacChord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let key = keys::mac_name(self.key_code);
        // Glyph keys sit flush against the modifiers, word keys get a space.
        if key.chars().count() == 1 {
            write!(f, "{}{key}", self.modifiers)
        } else if self.modifiers == MacModifiers::default() {
            f.write_str(&key)
        } else {
            write!(f, "{} {key}", self.modifiers)
        }
    }
}

/// A Windows key chord: exact modifiers plus one physical key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Chord {
    pub modifiers: Modifiers,
    pub key: PhysicalKey,
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.modifiers, self.key)
    }
}

impl Chord {
    /// The chord the way `platform`'s keyboard labels it: `Ctrl+Alt+Win+Right`
    /// on Windows, `Ctrl+Alt+Super+Right` on Linux, and `⌃⌥⌘→` on the Mac,
    /// where a key a Mac keyboard doesn't have keeps its PC name.
    pub fn label(self, platform: Platform) -> String {
        match platform {
            Platform::Windows => self.to_string(),
            Platform::Mac => self
                .to_mac()
                .map_or_else(|| self.to_string(), |mac| mac.to_string()),
            Platform::Linux => {
                let mut label = String::new();
                for (on, name) in [
                    (self.modifiers.ctrl, "Ctrl+"),
                    (self.modifiers.alt, "Alt+"),
                    (self.modifiers.shift, "Shift+"),
                    (self.modifiers.win, "Super+"),
                ] {
                    if on {
                        label.push_str(name);
                    }
                }
                label.push_str(self.key.windows_name());
                label
            }
        }
    }

    /// The same chord on a Mac keyboard: the macOS key code in this position,
    /// with Control, Option, Shift and Command for Ctrl, Alt, Shift and Win.
    /// `None` for keys a Mac keyboard doesn't have.
    pub fn to_mac(self) -> Option<MacChord> {
        let key_code = keys::to_mac_key_code(self.key)?;
        Some(MacChord {
            key_code: i64::from(key_code),
            modifiers: MacModifiers {
                control: self.modifiers.ctrl,
                option: self.modifiers.alt,
                shift: self.modifiers.shift,
                command: self.modifiers.win,
            },
        })
    }
}

/// Why a Windows chord in the override file didn't parse.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ChordParseError {
    #[error("`{0}` names no key")]
    NoKey(String),
    #[error("`{chord}` names more than one key (`{first}` and `{second}`)")]
    TwoKeys {
        chord: String,
        first: String,
        second: String,
    },
    #[error("`{0}` is not a key this app knows")]
    UnknownKey(String),
}

impl Chord {
    /// Parses `ctrl+alt+left`, `Win+Shift+Num7` and the like.
    pub fn parse(text: &str) -> Result<Self, ChordParseError> {
        let mut modifiers = Modifiers::NONE;
        let mut key: Option<(PhysicalKey, &str)> = None;
        // `+` is both the separator and a key name, so split on `+` that sits
        // between two other characters, and treat a trailing `+` as the key.
        let tokens = split_chord(text);
        for token in tokens {
            match token.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => modifiers.ctrl = true,
                "alt" | "option" => modifiers.alt = true,
                "shift" => modifiers.shift = true,
                "win" | "super" | "meta" | "cmd" | "command" => modifiers.win = true,
                _ => {
                    let parsed = PhysicalKey::from_windows_name(token)
                        .ok_or_else(|| ChordParseError::UnknownKey(token.to_owned()))?;
                    if let Some((_, first)) = key {
                        return Err(ChordParseError::TwoKeys {
                            chord: text.to_owned(),
                            first: first.to_owned(),
                            second: token.to_owned(),
                        });
                    }
                    key = Some((parsed, token));
                }
            }
        }
        let (key, _) = key.ok_or_else(|| ChordParseError::NoKey(text.to_owned()))?;
        Ok(Self { modifiers, key })
    }
}

fn split_chord(text: &str) -> Vec<&str> {
    let text = text.trim();
    let mut tokens = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        // A `+` separates only when a token precedes it and something follows;
        // the last `+` of `num+` belongs to the key name.
        if bytes[index] == b'+' && index > start && index + 1 < bytes.len() {
            tokens.push(text[start..index].trim());
            start = index + 1;
        }
        index += 1;
    }
    if start < text.len() {
        tokens.push(text[start..].trim());
    }
    tokens.retain(|token| !token.is_empty());
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_option_is_786432() {
        let mods = MacModifiers::from_flags(786_432);
        assert!(mods.control && mods.option && !mods.shift && !mods.command);
        assert_eq!(mods.to_string(), "⌃⌥");
    }

    #[test]
    fn extra_bits_are_ignored() {
        // TerminalCommands.md writes values like this by hand.
        let mods = MacModifiers::from_flags(1_835_305);
        assert!(mods.control && mods.option && mods.command && !mods.shift);
    }

    #[test]
    fn default_map_matches_a_mac_keyboard_on_windows() {
        let map = ModifierMap::default();
        let chord = map.translate(MacModifiers::from_flags(1_835_008));
        assert_eq!(chord.to_string(), "Ctrl+Alt+Win+");
    }

    #[test]
    fn mac_chords_read_like_the_mac_menu() {
        assert_eq!(MacChord::new(123, 786_432).to_string(), "⌃⌥←");
        assert_eq!(MacChord::new(71, 786_432).to_string(), "⌃⌥ Keypad Clear");
        assert_eq!(MacChord::new(126, 917_504).to_string(), "⌃⌥⇧↑");
    }

    #[test]
    fn the_default_map_round_trips_to_the_mac() {
        let map = ModifierMap::default();
        for mac in [
            MacChord::new(123, 786_432),
            MacChord::new(71, 786_432),
            MacChord::new(126, 917_504),
            MacChord::new(124, 1_835_008),
        ] {
            let key = keys::from_mac_key_code(mac.key_code).expect("maps");
            let chord = Chord {
                modifiers: map.translate(mac.modifiers),
                key,
            };
            assert_eq!(chord.to_mac(), Some(mac), "{mac}");
        }
        let menu = Chord::parse("ctrl+menu").expect("parses");
        assert_eq!(menu.to_mac(), None);
    }

    #[test]
    fn each_platform_labels_chords_its_own_way() {
        let next = Chord::parse("ctrl+alt+win+right").expect("parses");
        assert_eq!(next.label(Platform::Windows), "Ctrl+Alt+Win+Right");
        assert_eq!(next.label(Platform::Linux), "Ctrl+Alt+Super+Right");
        assert_eq!(next.label(Platform::Mac), "⌃⌥⌘→");
        let menu = Chord::parse("ctrl+menu").expect("parses");
        assert_eq!(menu.label(Platform::Mac), "Ctrl+Menu");
    }

    #[test]
    fn windows_chords_parse() {
        let chord = Chord::parse("ctrl+alt+left").expect("parses");
        assert_eq!(chord.to_string(), "Ctrl+Alt+Left");
        let chord = Chord::parse("Win + Shift + numpad7").expect("parses");
        assert_eq!(chord.to_string(), "Shift+Win+Num7");
        let chord = Chord::parse("ctrl+alt+num+").expect("trailing plus is the key");
        assert_eq!(chord.to_string(), "Ctrl+Alt+Num+");
        assert!(matches!(
            Chord::parse("ctrl+alt"),
            Err(ChordParseError::NoKey(_))
        ));
        assert!(matches!(
            Chord::parse("ctrl+a+b"),
            Err(ChordParseError::TwoKeys { .. })
        ));
        assert!(matches!(
            Chord::parse("ctrl+banana"),
            Err(ChordParseError::UnknownKey(_))
        ));
    }
}
