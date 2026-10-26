//! Physical keys: the macOS key codes an export stores, and the Windows scan
//! codes they land on.
//!
//! A macOS key code names a key *position* on an ANSI keyboard, not the letter
//! printed on it. Windows scan codes do the same, so translating one to the other
//! keeps a binding on the same physical key whatever layout either machine uses.
//! The table mirrors the one RustDesk applies in its default Map mode
//! (`rdev::macos_code_to_win_scancode`), which is what a Mac keyboard driving
//! this machine over RustDesk actually delivers.
//!
//! No two macOS key codes share a position, so the table also runs backwards:
//! the Mac registers a chord by the key code in that position. Linux names
//! positions by input event code, which for the main block is the set-1 make
//! code itself.

use std::fmt;

/// A key by position: its PS/2 set-1 make code and whether it carries the E0
/// prefix. Home (E0 47) and keypad 7 (47) share a code and differ only in
/// `extended`, which is exactly the distinction virtual-key codes lose when
/// NumLock is off.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PhysicalKey {
    pub scan: u16,
    pub extended: bool,
}

impl PhysicalKey {
    pub const fn new(scan: u16) -> Self {
        Self {
            scan,
            extended: false,
        }
    }

    pub const fn ext(scan: u16) -> Self {
        Self {
            scan,
            extended: true,
        }
    }

    /// NumLock, CapsLock and ScrollLock. Binding one would toggle the lock as a
    /// side effect, and RustDesk taps NumLock on its own to sync lock state, so
    /// these positions are never bound.
    pub fn is_lock_key(self) -> bool {
        !self.extended && matches!(self.scan, 0x45 | 0x3A | 0x46)
    }

    /// Ctrl, Shift, Alt and Win on either side.
    pub fn is_modifier(self) -> bool {
        matches!(
            (self.scan, self.extended),
            (0x1D | 0x2A | 0x36 | 0x38, false) | (0x1D | 0x38 | 0x5B | 0x5C, true)
        )
    }

    /// The name this key goes by on a PC keyboard.
    pub fn windows_name(self) -> &'static str {
        WIN_NAMES
            .iter()
            .find(|(key, _)| *key == self)
            .map_or("?", |(_, name)| name)
    }

    /// The Linux input event code in this position (`KEY_*` in
    /// `linux/input-event-codes.h`), if Linux has one. The main block's codes
    /// are the set-1 make codes; the rest differ. An XKB keycode, which
    /// GNOME's accelerators take, is this plus 8.
    pub fn evdev(self) -> Option<u16> {
        if self.extended {
            return EVDEV_EXTENDED
                .iter()
                .find(|(scan, _)| *scan == self.scan)
                .map(|(_, code)| *code);
        }
        match self.scan {
            0x01..=0x53 | 0x56..=0x58 => Some(self.scan),
            0x59 => Some(117),
            // F13 to F20.
            0x64..=0x6B => Some(self.scan - 0x64 + 183),
            0x73 => Some(89),
            0x79 => Some(92),
            0x7B => Some(94),
            0x7D => Some(124),
            0x7E => Some(121),
            _ => None,
        }
    }

    /// The key in the position Linux calls `code`, if this table names one.
    pub fn from_evdev(code: u16) -> Option<Self> {
        WIN_NAMES
            .iter()
            .map(|(key, _)| *key)
            .find(|key| key.evdev() == Some(code))
    }

    /// Looks a key up by the name `windows_name` gives it, ignoring case, plus a
    /// few common aliases. Used for the Windows-side override file.
    pub fn from_windows_name(name: &str) -> Option<Self> {
        let wanted = name.trim().to_ascii_lowercase();
        if let Some((key, _)) = WIN_NAMES
            .iter()
            .find(|(_, candidate)| candidate.eq_ignore_ascii_case(&wanted))
        {
            return Some(*key);
        }
        ALIASES
            .iter()
            .find(|(alias, _)| *alias == wanted)
            .and_then(|(_, canonical)| Self::from_windows_name(canonical))
    }
}

impl fmt::Display for PhysicalKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.windows_name())
    }
}

/// Mouse buttons are stored as key code 0xFFC0 + button (Rectangle's
/// `MouseButtonShortcut`). They have no keyboard position, so they never bind.
pub fn is_mouse_button_code(key_code: i64) -> bool {
    (0xFFC0..=0xFFDF).contains(&key_code)
}

/// The Windows key in the same position as a macOS key code, if it has one.
pub fn from_mac_key_code(key_code: i64) -> Option<PhysicalKey> {
    MAC_KEYS
        .iter()
        .find(|entry| i64::from(entry.0) == key_code)
        .map(|entry| entry.1)
}

/// The macOS key code in the same position as a Windows key, if a Mac
/// keyboard has one there.
pub fn to_mac_key_code(key: PhysicalKey) -> Option<u16> {
    MAC_KEYS
        .iter()
        .find(|entry| entry.1 == key)
        .map(|entry| entry.0)
}

/// How a macOS key code reads on the Mac: the glyph or label the key carries.
pub fn mac_name(key_code: i64) -> String {
    MAC_KEYS
        .iter()
        .find(|entry| i64::from(entry.0) == key_code)
        .map_or_else(|| format!("key {key_code}"), |entry| entry.2.to_owned())
}

const fn k(scan: u16) -> PhysicalKey {
    PhysicalKey::new(scan)
}

const fn e(scan: u16) -> PhysicalKey {
    PhysicalKey::ext(scan)
}

/// macOS virtual key code (Carbon `Events.h`), Windows key, Mac label.
const MAC_KEYS: &[(u16, PhysicalKey, &str)] = &[
    (0x00, k(0x1E), "A"),
    (0x01, k(0x1F), "S"),
    (0x02, k(0x20), "D"),
    (0x03, k(0x21), "F"),
    (0x04, k(0x23), "H"),
    (0x05, k(0x22), "G"),
    (0x06, k(0x2C), "Z"),
    (0x07, k(0x2D), "X"),
    (0x08, k(0x2E), "C"),
    (0x09, k(0x2F), "V"),
    (0x0A, k(0x56), "§"),
    (0x0B, k(0x30), "B"),
    (0x0C, k(0x10), "Q"),
    (0x0D, k(0x11), "W"),
    (0x0E, k(0x12), "E"),
    (0x0F, k(0x13), "R"),
    (0x10, k(0x15), "Y"),
    (0x11, k(0x14), "T"),
    (0x12, k(0x02), "1"),
    (0x13, k(0x03), "2"),
    (0x14, k(0x04), "3"),
    (0x15, k(0x05), "4"),
    (0x16, k(0x07), "6"),
    (0x17, k(0x06), "5"),
    (0x18, k(0x0D), "="),
    (0x19, k(0x0A), "9"),
    (0x1A, k(0x08), "7"),
    (0x1B, k(0x0C), "-"),
    (0x1C, k(0x09), "8"),
    (0x1D, k(0x0B), "0"),
    (0x1E, k(0x1B), "]"),
    (0x1F, k(0x18), "O"),
    (0x20, k(0x16), "U"),
    (0x21, k(0x1A), "["),
    (0x22, k(0x17), "I"),
    (0x23, k(0x19), "P"),
    (0x24, k(0x1C), "↩"),
    (0x25, k(0x26), "L"),
    (0x26, k(0x24), "J"),
    (0x27, k(0x28), "'"),
    (0x28, k(0x25), "K"),
    (0x29, k(0x27), ";"),
    (0x2A, k(0x2B), "\\"),
    (0x2B, k(0x33), ","),
    (0x2C, k(0x35), "/"),
    (0x2D, k(0x31), "N"),
    (0x2E, k(0x32), "M"),
    (0x2F, k(0x34), "."),
    (0x30, k(0x0F), "⇥"),
    (0x31, k(0x39), "Space"),
    (0x32, k(0x29), "`"),
    (0x33, k(0x0E), "⌫"),
    (0x35, k(0x01), "⎋"),
    (0x36, e(0x5C), "Right ⌘"),
    (0x37, e(0x5B), "⌘"),
    (0x38, k(0x2A), "⇧"),
    (0x39, k(0x3A), "⇪"),
    (0x3A, k(0x38), "⌥"),
    (0x3B, k(0x1D), "⌃"),
    (0x3C, k(0x36), "Right ⇧"),
    (0x3D, e(0x38), "Right ⌥"),
    (0x3E, e(0x1D), "Right ⌃"),
    (0x40, k(0x68), "F17"),
    (0x41, k(0x53), "Keypad ."),
    (0x43, k(0x37), "Keypad *"),
    (0x45, k(0x4E), "Keypad +"),
    (0x47, k(0x45), "Keypad Clear"),
    (0x48, e(0x30), "Volume Up"),
    (0x49, e(0x2E), "Volume Down"),
    (0x4A, e(0x20), "Mute"),
    (0x4B, e(0x35), "Keypad /"),
    (0x4C, e(0x1C), "Keypad Enter"),
    (0x4E, k(0x4A), "Keypad -"),
    (0x4F, k(0x69), "F18"),
    (0x50, k(0x6A), "F19"),
    (0x51, k(0x59), "Keypad ="),
    (0x52, k(0x52), "Keypad 0"),
    (0x53, k(0x4F), "Keypad 1"),
    (0x54, k(0x50), "Keypad 2"),
    (0x55, k(0x51), "Keypad 3"),
    (0x56, k(0x4B), "Keypad 4"),
    (0x57, k(0x4C), "Keypad 5"),
    (0x58, k(0x4D), "Keypad 6"),
    (0x59, k(0x47), "Keypad 7"),
    (0x5A, k(0x6B), "F20"),
    (0x5B, k(0x48), "Keypad 8"),
    (0x5C, k(0x49), "Keypad 9"),
    (0x5D, k(0x7D), "¥"),
    (0x5E, k(0x73), "_"),
    (0x5F, k(0x7E), "Keypad ,"),
    (0x60, k(0x3F), "F5"),
    (0x61, k(0x40), "F6"),
    (0x62, k(0x41), "F7"),
    (0x63, k(0x3D), "F3"),
    (0x64, k(0x42), "F8"),
    (0x65, k(0x43), "F9"),
    (0x66, k(0x7B), "英数"),
    (0x67, k(0x57), "F11"),
    (0x68, k(0x79), "かな"),
    (0x69, k(0x64), "F13"),
    (0x6A, k(0x67), "F16"),
    (0x6B, k(0x65), "F14"),
    (0x6D, k(0x44), "F10"),
    (0x6F, k(0x58), "F12"),
    (0x71, k(0x66), "F15"),
    (0x72, e(0x52), "Help"),
    (0x73, e(0x47), "↖"),
    (0x74, e(0x49), "⇞"),
    (0x75, e(0x53), "⌦"),
    (0x76, k(0x3E), "F4"),
    (0x77, e(0x4F), "↘"),
    (0x78, k(0x3C), "F2"),
    (0x79, e(0x51), "⇟"),
    (0x7A, k(0x3B), "F1"),
    (0x7B, e(0x4B), "←"),
    (0x7C, e(0x4D), "→"),
    (0x7D, e(0x50), "↓"),
    (0x7E, e(0x48), "↑"),
];

/// Extended (E0) scan code, Linux input event code.
const EVDEV_EXTENDED: &[(u16, u16)] = &[
    (0x1C, 96),  // KEY_KPENTER
    (0x1D, 97),  // KEY_RIGHTCTRL
    (0x20, 113), // KEY_MUTE
    (0x2E, 114), // KEY_VOLUMEDOWN
    (0x30, 115), // KEY_VOLUMEUP
    (0x35, 98),  // KEY_KPSLASH
    (0x38, 100), // KEY_RIGHTALT
    (0x47, 102), // KEY_HOME
    (0x48, 103), // KEY_UP
    (0x49, 104), // KEY_PAGEUP
    (0x4B, 105), // KEY_LEFT
    (0x4D, 106), // KEY_RIGHT
    (0x4F, 107), // KEY_END
    (0x50, 108), // KEY_DOWN
    (0x51, 109), // KEY_PAGEDOWN
    (0x52, 110), // KEY_INSERT
    (0x53, 111), // KEY_DELETE
    (0x5B, 125), // KEY_LEFTMETA
    (0x5C, 126), // KEY_RIGHTMETA
    (0x5D, 127), // KEY_COMPOSE, the Menu key
];

/// Every key this table can name on the Windows side.
const WIN_NAMES: &[(PhysicalKey, &str)] = &[
    (k(0x01), "Esc"),
    (k(0x02), "1"),
    (k(0x03), "2"),
    (k(0x04), "3"),
    (k(0x05), "4"),
    (k(0x06), "5"),
    (k(0x07), "6"),
    (k(0x08), "7"),
    (k(0x09), "8"),
    (k(0x0A), "9"),
    (k(0x0B), "0"),
    (k(0x0C), "-"),
    (k(0x0D), "="),
    (k(0x0E), "Backspace"),
    (k(0x0F), "Tab"),
    (k(0x10), "Q"),
    (k(0x11), "W"),
    (k(0x12), "E"),
    (k(0x13), "R"),
    (k(0x14), "T"),
    (k(0x15), "Y"),
    (k(0x16), "U"),
    (k(0x17), "I"),
    (k(0x18), "O"),
    (k(0x19), "P"),
    (k(0x1A), "["),
    (k(0x1B), "]"),
    (k(0x1C), "Enter"),
    (k(0x1D), "LCtrl"),
    (k(0x1E), "A"),
    (k(0x1F), "S"),
    (k(0x20), "D"),
    (k(0x21), "F"),
    (k(0x22), "G"),
    (k(0x23), "H"),
    (k(0x24), "J"),
    (k(0x25), "K"),
    (k(0x26), "L"),
    (k(0x27), ";"),
    (k(0x28), "'"),
    (k(0x29), "`"),
    (k(0x2A), "LShift"),
    (k(0x2B), "\\"),
    (k(0x2C), "Z"),
    (k(0x2D), "X"),
    (k(0x2E), "C"),
    (k(0x2F), "V"),
    (k(0x30), "B"),
    (k(0x31), "N"),
    (k(0x32), "M"),
    (k(0x33), ","),
    (k(0x34), "."),
    (k(0x35), "/"),
    (k(0x36), "RShift"),
    (k(0x37), "Num*"),
    (k(0x38), "LAlt"),
    (k(0x39), "Space"),
    (k(0x3A), "CapsLock"),
    (k(0x3B), "F1"),
    (k(0x3C), "F2"),
    (k(0x3D), "F3"),
    (k(0x3E), "F4"),
    (k(0x3F), "F5"),
    (k(0x40), "F6"),
    (k(0x41), "F7"),
    (k(0x42), "F8"),
    (k(0x43), "F9"),
    (k(0x44), "F10"),
    (k(0x45), "NumLock"),
    (k(0x46), "ScrollLock"),
    (k(0x47), "Num7"),
    (k(0x48), "Num8"),
    (k(0x49), "Num9"),
    (k(0x4A), "Num-"),
    (k(0x4B), "Num4"),
    (k(0x4C), "Num5"),
    (k(0x4D), "Num6"),
    (k(0x4E), "Num+"),
    (k(0x4F), "Num1"),
    (k(0x50), "Num2"),
    (k(0x51), "Num3"),
    (k(0x52), "Num0"),
    (k(0x53), "Num."),
    (k(0x56), "IntlBackslash"),
    (k(0x57), "F11"),
    (k(0x58), "F12"),
    (k(0x59), "Num="),
    (k(0x64), "F13"),
    (k(0x65), "F14"),
    (k(0x66), "F15"),
    (k(0x67), "F16"),
    (k(0x68), "F17"),
    (k(0x69), "F18"),
    (k(0x6A), "F19"),
    (k(0x6B), "F20"),
    (k(0x73), "IntlRo"),
    (k(0x79), "Convert"),
    (k(0x7B), "NonConvert"),
    (k(0x7D), "IntlYen"),
    (k(0x7E), "Num,"),
    (e(0x1C), "NumEnter"),
    (e(0x1D), "RCtrl"),
    (e(0x20), "VolumeMute"),
    (e(0x2E), "VolumeDown"),
    (e(0x30), "VolumeUp"),
    (e(0x35), "Num/"),
    (e(0x38), "RAlt"),
    (e(0x47), "Home"),
    (e(0x48), "Up"),
    (e(0x49), "PageUp"),
    (e(0x4B), "Left"),
    (e(0x4D), "Right"),
    (e(0x4F), "End"),
    (e(0x50), "Down"),
    (e(0x51), "PageDown"),
    (e(0x52), "Insert"),
    (e(0x53), "Delete"),
    (e(0x5B), "LWin"),
    (e(0x5C), "RWin"),
    (e(0x5D), "Menu"),
];

/// Extra spellings the override file accepts, mapped onto `WIN_NAMES`.
const ALIASES: &[(&str, &str)] = &[
    ("escape", "Esc"),
    ("return", "Enter"),
    ("del", "Delete"),
    ("ins", "Insert"),
    ("pgup", "PageUp"),
    ("pgdn", "PageDown"),
    ("minus", "-"),
    ("equal", "="),
    ("plus", "="),
    ("numpad0", "Num0"),
    ("numpad1", "Num1"),
    ("numpad2", "Num2"),
    ("numpad3", "Num3"),
    ("numpad4", "Num4"),
    ("numpad5", "Num5"),
    ("numpad6", "Num6"),
    ("numpad7", "Num7"),
    ("numpad8", "Num8"),
    ("numpad9", "Num9"),
    ("numpadenter", "NumEnter"),
    // The Mac keypad's Clear key sits where NumLock is.
    ("clear", "NumLock"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mac_key_has_a_windows_name() {
        for (code, key, _) in MAC_KEYS {
            assert_ne!(
                key.windows_name(),
                "?",
                "macOS key code {code:#x} has no Windows name"
            );
        }
    }

    #[test]
    fn mac_codes_are_unique() {
        let mut codes: Vec<u16> = MAC_KEYS.iter().map(|entry| entry.0).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), MAC_KEYS.len());
    }

    #[test]
    fn positions_map_back_to_the_mac_key_code() {
        for (code, key, _) in MAC_KEYS {
            assert_eq!(
                to_mac_key_code(*key),
                Some(*code),
                "{} is claimed by more than one macOS key code",
                key.windows_name()
            );
        }
        assert_eq!(to_mac_key_code(e(0x5D)), None, "a Mac has no Menu key");
    }

    #[test]
    fn home_and_keypad_7_are_different_keys() {
        let home = from_mac_key_code(0x73).expect("Home maps");
        let keypad7 = from_mac_key_code(0x59).expect("keypad 7 maps");
        assert_eq!(home.scan, keypad7.scan);
        assert_ne!(home, keypad7);
    }

    #[test]
    fn keypad_clear_is_the_numlock_position() {
        let clear = from_mac_key_code(0x47).expect("Clear maps");
        assert!(clear.is_lock_key());
        assert_eq!(clear.windows_name(), "NumLock");
    }

    #[test]
    fn override_names_round_trip() {
        for (key, name) in WIN_NAMES {
            assert_eq!(PhysicalKey::from_windows_name(name), Some(*key), "{name}");
        }
        assert_eq!(PhysicalKey::from_windows_name("numpad7"), Some(k(0x47)));
        assert_eq!(PhysicalKey::from_windows_name("LEFT"), Some(e(0x4B)));
    }

    #[test]
    fn every_named_key_has_its_own_linux_code() {
        let mut codes = Vec::new();
        for (key, name) in WIN_NAMES {
            let code = key.evdev();
            assert!(code.is_some(), "{name} has no Linux code");
            codes.extend(code);
        }
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), WIN_NAMES.len());
    }

    #[test]
    fn linux_codes_match_input_event_codes() {
        for (name, code) in [
            ("Esc", 1),
            ("A", 30),
            ("Num7", 71),
            ("Num.", 83),
            ("IntlBackslash", 86),
            ("F12", 88),
            ("Num=", 117),
            ("F13", 183),
            ("F20", 190),
            ("NumEnter", 96),
            ("Home", 102),
            ("Left", 105),
            ("Delete", 111),
            ("LWin", 125),
            ("Menu", 127),
        ] {
            let key = PhysicalKey::from_windows_name(name).expect(name);
            assert_eq!(key.evdev(), Some(code), "{name}");
            assert_eq!(PhysicalKey::from_evdev(code), Some(key), "{name}");
        }
        assert_eq!(PhysicalKey::from_evdev(84), None);
    }

    #[test]
    fn mouse_buttons_are_recognised() {
        assert!(is_mouse_button_code(0xFFC2));
        assert!(!is_mouse_button_code(0x7B));
    }
}
