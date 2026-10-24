//! The keyboard hook's decisions, kept free of Win32 so they can be tested.
//!
//! The low-level hook hands every key event here and does what it is told: let
//! it through, swallow it, or swallow it and run a binding. Matching is on the
//! physical key (scan code plus the extended flag) and the exact set of
//! modifiers held, so keypad 7 and Home, or keypad = and keypad 5, never get
//! confused the way their virtual-key codes do when NumLock is off.

use std::collections::{HashMap, HashSet};

use crate::chord::{Chord, Modifiers};
use crate::keys::PhysicalKey;

/// `dwExtraInfo` on every event this app injects ("UTWM"), so the hook can
/// recognise and ignore its own keystrokes. Other injected input, such as
/// RustDesk's, is matched like a real keyboard.
pub const INJECTED_TAG: usize = 0x5554_574D;

/// Virtual-key code Windows uses for Unicode injection (`VK_PACKET`).
const VK_PACKET: u16 = 0xE7;

/// One event as the hook sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: PhysicalKey,
    /// The virtual-key code Windows derived; only used to spot `VK_PACKET`.
    pub vk: u16,
    pub down: bool,
    pub extra_info: usize,
}

/// What the hook should do with an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Pass,
    Swallow,
    /// Swallow and run binding `index`. `mask` asks for a dummy keystroke before
    /// Alt or Win comes up, or releasing it would open the menu bar or Start.
    Trigger {
        index: usize,
        mask: bool,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Held {
    lctrl: bool,
    rctrl: bool,
    lalt: bool,
    ralt: bool,
    lshift: bool,
    rshift: bool,
    lwin: bool,
    rwin: bool,
}

impl Held {
    fn modifiers(self) -> Modifiers {
        Modifiers {
            ctrl: self.lctrl || self.rctrl,
            alt: self.lalt || self.ralt,
            shift: self.lshift || self.rshift,
            win: self.lwin || self.rwin,
        }
    }

    /// Records a modifier transition. Returns false for keys that aren't
    /// modifiers.
    fn update(&mut self, key: PhysicalKey, down: bool) -> bool {
        let slot = match (key.scan, key.extended) {
            (0x1D, false) => &mut self.lctrl,
            (0x1D, true) => &mut self.rctrl,
            (0x38, false) => &mut self.lalt,
            (0x38, true) => &mut self.ralt,
            (0x2A, false) => &mut self.lshift,
            (0x36, false) => &mut self.rshift,
            (0x5B, true) => &mut self.lwin,
            (0x5C, true) => &mut self.rwin,
            _ => return false,
        };
        *slot = down;
        true
    }
}

pub struct Matcher {
    table: HashMap<Chord, usize>,
    held: Held,
    /// Non-modifier keys currently down, to tell auto-repeat from a new press.
    down: HashSet<PhysicalKey>,
    /// Keys whose press was swallowed, so their release is swallowed too.
    swallowed: HashSet<PhysicalKey>,
    paused: bool,
}

impl Matcher {
    /// `chords[i]` triggers binding `i`.
    pub fn new(chords: impl IntoIterator<Item = Chord>) -> Self {
        Self {
            table: chords
                .into_iter()
                .enumerate()
                .map(|(index, chord)| (chord, index))
                .collect(),
            held: Held::default(),
            down: HashSet::new(),
            swallowed: HashSet::new(),
            paused: false,
        }
    }

    /// Swaps in new bindings without forgetting which keys are held.
    pub fn replace_bindings(&mut self, chords: impl IntoIterator<Item = Chord>) {
        self.table = chords
            .into_iter()
            .enumerate()
            .map(|(index, chord)| (chord, index))
            .collect();
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    /// Forgets every held key, e.g. after the session was locked and the hook
    /// missed the releases.
    pub fn reset(&mut self) {
        self.held = Held::default();
        self.down.clear();
        self.swallowed.clear();
    }

    /// The modifiers this matcher believes are held.
    pub fn modifiers(&self) -> Modifiers {
        self.held.modifiers()
    }

    /// Decides one event. `actual` is the system's own view of the modifiers
    /// (`GetAsyncKeyState`), when available: a release the hook never saw (a
    /// session switch, the secure desktop) would otherwise leave a modifier stuck
    /// and fire bindings that weren't pressed.
    pub fn on_event(&mut self, event: KeyEvent, actual: Option<Modifiers>) -> Decision {
        if event.extra_info == INJECTED_TAG || event.vk == VK_PACKET {
            return Decision::Pass;
        }
        // AltGr arrives as a fake LCtrl with scan code 0x21D followed by RAlt.
        // The fake Ctrl isn't a key the user pressed.
        if event.key.scan == 0x21D {
            return Decision::Pass;
        }
        if self.held.update(event.key, event.down) {
            return Decision::Pass;
        }

        if !event.down {
            self.down.remove(&event.key);
            return if self.swallowed.remove(&event.key) {
                Decision::Swallow
            } else {
                Decision::Pass
            };
        }

        if !self.down.insert(event.key) {
            // Auto-repeat: a binding fires once per press.
            return if self.swallowed.contains(&event.key) {
                Decision::Swallow
            } else {
                Decision::Pass
            };
        }

        if self.paused || event.key.is_lock_key() {
            return Decision::Pass;
        }

        if let Some(actual) = actual {
            self.resync(actual);
        }
        let modifiers = self.held.modifiers();
        match self.table.get(&Chord {
            modifiers,
            key: event.key,
        }) {
            Some(&index) => {
                self.swallowed.insert(event.key);
                Decision::Trigger {
                    index,
                    mask: modifiers.alt || modifiers.win,
                }
            }
            None => Decision::Pass,
        }
    }

    /// Drops modifiers the system says are up. A modifier the system reports
    /// down but the hook never saw stays unset: guessing a side would be wrong
    /// as often as right, and the next press fixes it.
    fn resync(&mut self, actual: Modifiers) {
        if !actual.ctrl {
            self.held.lctrl = false;
            self.held.rctrl = false;
        }
        if !actual.alt {
            self.held.lalt = false;
            self.held.ralt = false;
        }
        if !actual.shift {
            self.held.lshift = false;
            self.held.rshift = false;
        }
        if !actual.win {
            self.held.lwin = false;
            self.held.rwin = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LCTRL: PhysicalKey = PhysicalKey::new(0x1D);
    const LALT: PhysicalKey = PhysicalKey::new(0x38);
    const LWIN: PhysicalKey = PhysicalKey::ext(0x5B);
    const LEFT: PhysicalKey = PhysicalKey::ext(0x4B);
    const HOME: PhysicalKey = PhysicalKey::ext(0x47);
    const KP7: PhysicalKey = PhysicalKey::new(0x47);
    const KP5: PhysicalKey = PhysicalKey::new(0x4C);
    const KP_EQUAL: PhysicalKey = PhysicalKey::new(0x59);
    const NUMLOCK: PhysicalKey = PhysicalKey::new(0x45);

    fn ctrl_alt(key: PhysicalKey) -> Chord {
        Chord {
            modifiers: Modifiers {
                ctrl: true,
                alt: true,
                ..Modifiers::NONE
            },
            key,
        }
    }

    fn press(key: PhysicalKey) -> KeyEvent {
        KeyEvent {
            key,
            vk: 0,
            down: true,
            extra_info: 0,
        }
    }

    fn release(key: PhysicalKey) -> KeyEvent {
        KeyEvent {
            key,
            vk: 0,
            down: false,
            extra_info: 0,
        }
    }

    fn matcher() -> Matcher {
        Matcher::new([
            ctrl_alt(LEFT),
            ctrl_alt(HOME),
            ctrl_alt(KP7),
            ctrl_alt(KP_EQUAL),
            ctrl_alt(KP5),
        ])
    }

    #[test]
    fn ctrl_alt_left_triggers_and_its_release_is_swallowed() {
        let mut m = matcher();
        assert_eq!(m.on_event(press(LCTRL), None), Decision::Pass);
        assert_eq!(m.on_event(press(LALT), None), Decision::Pass);
        assert_eq!(
            m.on_event(press(LEFT), None),
            Decision::Trigger {
                index: 0,
                mask: true
            }
        );
        assert_eq!(
            m.on_event(press(LEFT), None),
            Decision::Swallow,
            "auto-repeat"
        );
        assert_eq!(m.on_event(release(LEFT), None), Decision::Swallow);
        assert_eq!(m.on_event(release(LALT), None), Decision::Pass);
        assert_eq!(m.on_event(release(LCTRL), None), Decision::Pass);
        assert_eq!(
            m.on_event(press(LEFT), None),
            Decision::Pass,
            "plain Left types normally"
        );
    }

    #[test]
    fn home_and_keypad_keys_stay_distinct() {
        let mut m = matcher();
        m.on_event(press(LCTRL), None);
        m.on_event(press(LALT), None);
        assert_eq!(
            m.on_event(press(HOME), None),
            Decision::Trigger {
                index: 1,
                mask: true
            }
        );
        m.on_event(release(HOME), None);
        assert_eq!(
            m.on_event(press(KP7), None),
            Decision::Trigger {
                index: 2,
                mask: true
            }
        );
        m.on_event(release(KP7), None);
        assert_eq!(
            m.on_event(press(KP_EQUAL), None),
            Decision::Trigger {
                index: 3,
                mask: true
            }
        );
        m.on_event(release(KP_EQUAL), None);
        assert_eq!(
            m.on_event(press(KP5), None),
            Decision::Trigger {
                index: 4,
                mask: true
            }
        );
    }

    #[test]
    fn rustdesk_numlock_sync_taps_pass_through() {
        let mut m = matcher();
        m.on_event(press(LCTRL), None);
        m.on_event(press(LALT), None);
        // RustDesk's host taps NumLock around a keypad key to match the client's
        // lock state; the tap must neither trigger nor be swallowed.
        assert_eq!(m.on_event(press(NUMLOCK), None), Decision::Pass);
        assert_eq!(m.on_event(release(NUMLOCK), None), Decision::Pass);
        assert_eq!(
            m.on_event(press(KP7), None),
            Decision::Trigger {
                index: 2,
                mask: true
            }
        );
        assert_eq!(m.on_event(press(NUMLOCK), None), Decision::Pass);
        assert_eq!(m.on_event(release(NUMLOCK), None), Decision::Pass);
        assert_eq!(m.on_event(release(KP7), None), Decision::Swallow);
    }

    #[test]
    fn extra_modifiers_do_not_match() {
        let mut m = matcher();
        m.on_event(press(LCTRL), None);
        m.on_event(press(LALT), None);
        m.on_event(press(LWIN), None);
        assert_eq!(m.on_event(press(LEFT), None), Decision::Pass);
    }

    #[test]
    fn own_injected_keys_are_ignored() {
        let mut m = matcher();
        let tagged = |event: KeyEvent| KeyEvent {
            extra_info: INJECTED_TAG,
            ..event
        };
        assert_eq!(m.on_event(tagged(press(LCTRL)), None), Decision::Pass);
        assert_eq!(m.on_event(tagged(press(LALT)), None), Decision::Pass);
        assert_eq!(
            m.on_event(press(LEFT), None),
            Decision::Pass,
            "tagged modifiers were not tracked"
        );
    }

    #[test]
    fn a_missed_release_is_corrected_by_the_system_state() {
        let mut m = matcher();
        m.on_event(press(LCTRL), None);
        m.on_event(press(LALT), None);
        // The hook never saw Ctrl come up; the system says it is up.
        let actual = Modifiers {
            alt: true,
            ..Modifiers::NONE
        };
        assert_eq!(m.on_event(press(LEFT), Some(actual)), Decision::Pass);
    }

    #[test]
    fn pausing_lets_everything_through() {
        let mut m = matcher();
        m.set_paused(true);
        m.on_event(press(LCTRL), None);
        m.on_event(press(LALT), None);
        assert_eq!(m.on_event(press(LEFT), None), Decision::Pass);
    }

    #[test]
    fn altgr_fake_ctrl_is_not_a_modifier() {
        let mut m = matcher();
        m.on_event(press(PhysicalKey::new(0x21D)), None);
        m.on_event(press(PhysicalKey::ext(0x38)), None);
        // RAlt alone with Left is Alt+Left, not Ctrl+Alt+Left.
        assert_eq!(m.on_event(press(LEFT), None), Decision::Pass);
    }
}
