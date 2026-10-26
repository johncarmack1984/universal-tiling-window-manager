//! Global shortcuts through Carbon's `RegisterEventHotKey`, which is how
//! Rectangle registers its own (by way of MASShortcut).
//!
//! Unlike an event tap, a hot key needs no permission, never sees any other
//! keystroke, and fires once per press however long the key is held. The
//! system delivers each press to the main thread's event loop, so
//! registration and the handler live on the main thread, and calls from
//! elsewhere hop over to it.

use std::cell::RefCell;
use std::ffi::c_void;

use utwm_config::Chord;

type OSStatus = i32;
type EventTargetRef = *mut c_void;
type EventHandlerRef = *mut c_void;
type EventHandlerCallRef = *mut c_void;
type EventRef = *mut c_void;
type EventHotKeyRef = *mut c_void;
type EventHandlerUPP =
    Option<unsafe extern "C" fn(EventHandlerCallRef, EventRef, *mut c_void) -> OSStatus>;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct EventHotKeyID {
    signature: u32,
    id: u32,
}

#[repr(C)]
struct EventTypeSpec {
    event_class: u32,
    event_kind: u32,
}

const fn four_cc(code: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*code)
}

const NO_ERR: OSStatus = 0;
const EVENT_NOT_HANDLED: OSStatus = -9874;
const EVENT_CLASS_KEYBOARD: u32 = four_cc(b"keyb");
const EVENT_HOT_KEY_PRESSED: u32 = 5;
const PARAM_DIRECT_OBJECT: u32 = four_cc(b"----");
const TYPE_HOT_KEY_ID: u32 = four_cc(b"hkid");
/// Tags this app's hot keys ("UTWM"), as the Windows hook tags its keystrokes.
const SIGNATURE: u32 = four_cc(b"UTWM");

// Carbon's modifier bits (`Events.h`).
const CMD_KEY: u32 = 1 << 8;
const SHIFT_KEY: u32 = 1 << 9;
const OPTION_KEY: u32 = 1 << 11;
const CONTROL_KEY: u32 = 1 << 12;

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn GetApplicationEventTarget() -> EventTargetRef;
    fn InstallEventHandler(
        target: EventTargetRef,
        handler: EventHandlerUPP,
        type_count: usize,
        types: *const EventTypeSpec,
        user_data: *mut c_void,
        out_handler: *mut EventHandlerRef,
    ) -> OSStatus;
    fn RegisterEventHotKey(
        key_code: u32,
        modifiers: u32,
        id: EventHotKeyID,
        target: EventTargetRef,
        options: u32,
        out_hot_key: *mut EventHotKeyRef,
    ) -> OSStatus;
    fn UnregisterEventHotKey(hot_key: EventHotKeyRef) -> OSStatus;
    fn GetEventParameter(
        event: EventRef,
        name: u32,
        desired_type: u32,
        actual_type: *mut u32,
        buffer_size: usize,
        actual_size: *mut usize,
        data: *mut c_void,
    ) -> OSStatus;
}

/// Receives the index of each binding pressed. Called on the main thread, so
/// it must be quick: send to a channel and return.
pub type Reporter = Box<dyn Fn(usize) + Send + 'static>;

/// One chord as Carbon takes it: a macOS key code and modifier bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HotKey {
    pub key_code: u32,
    pub modifiers: u32,
}

impl HotKey {
    /// `None` for a chord whose key a Mac keyboard doesn't have.
    pub fn of(chord: Chord) -> Option<Self> {
        let mac = chord.to_mac()?;
        let modifiers = mac.modifiers;
        let bits = [
            (modifiers.command, CMD_KEY),
            (modifiers.shift, SHIFT_KEY),
            (modifiers.option, OPTION_KEY),
            (modifiers.control, CONTROL_KEY),
        ]
        .into_iter()
        .filter(|(on, _)| *on)
        .fold(0, |bits, (_, bit)| bits | bit);
        Some(Self {
            key_code: u32::try_from(mac.key_code).ok()?,
            modifiers: bits,
        })
    }
}

struct State {
    report: Reporter,
    chords: Vec<Chord>,
    registered: Vec<EventHotKeyRef>,
    paused: bool,
    /// Bumped on every re-registration, and carried in each hot key's id, so
    /// a press queued before new bindings arrived can't run the wrong one.
    generation: u32,
}

thread_local! {
    /// Only ever touched on the main thread.
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// The registered shortcuts. Its methods may be called from any thread.
pub struct Hotkeys {
    _private: (),
}

impl Hotkeys {
    /// Registers `chords[i]` to report `i`. Returns the chords macOS refused,
    /// usually because another app holds them.
    pub fn start(chords: Vec<Chord>, report: Reporter) -> std::io::Result<(Self, Vec<Chord>)> {
        dispatch2::run_on_main(|_| {
            let types = [EventTypeSpec {
                event_class: EVENT_CLASS_KEYBOARD,
                event_kind: EVENT_HOT_KEY_PRESSED,
            }];
            let mut handler: EventHandlerRef = std::ptr::null_mut();
            // SAFETY: `on_hot_key` has the handler signature and lives for the
            // program; `types` outlives the call, which copies it.
            let status = unsafe {
                InstallEventHandler(
                    GetApplicationEventTarget(),
                    Some(on_hot_key),
                    types.len(),
                    types.as_ptr(),
                    std::ptr::null_mut(),
                    &mut handler,
                )
            };
            if status != NO_ERR {
                return Err(std::io::Error::other(format!(
                    "InstallEventHandler failed with {status}"
                )));
            }
            let refused = STATE.with(|state| {
                let mut state = state.borrow_mut();
                let state = state.insert(State {
                    report,
                    chords,
                    registered: Vec::new(),
                    paused: false,
                    generation: 0,
                });
                state.register()
            });
            Ok((Self { _private: () }, refused))
        })
    }

    /// Replaces the bindings. Returns the chords macOS refused.
    pub fn set_bindings(&self, chords: Vec<Chord>) -> Vec<Chord> {
        dispatch2::run_on_main(|_| {
            with_state(|state| {
                state.unregister();
                state.chords = chords;
                state.register()
            })
        })
    }

    /// Paused, no shortcut is registered, so every key reaches the apps.
    /// Returns the chords macOS refused once resumed.
    pub fn set_paused(&self, paused: bool) -> Vec<Chord> {
        dispatch2::run_on_main(|_| {
            with_state(|state| {
                state.paused = paused;
                state.unregister();
                state.register()
            })
        })
    }
}

fn with_state(f: impl FnOnce(&mut State) -> Vec<Chord>) -> Vec<Chord> {
    STATE.with(|state| state.borrow_mut().as_mut().map(f).unwrap_or_default())
}

impl State {
    fn unregister(&mut self) {
        for hot_key in self.registered.drain(..) {
            // SAFETY: each ref came from RegisterEventHotKey and is released
            // exactly once.
            unsafe { UnregisterEventHotKey(hot_key) };
        }
    }

    /// Registers every chord unless paused; returns those macOS refused.
    fn register(&mut self) -> Vec<Chord> {
        self.generation = self.generation.wrapping_add(1) & 0xFFFF;
        if self.paused {
            return Vec::new();
        }
        let mut refused = Vec::new();
        for (index, chord) in self.chords.iter().enumerate() {
            let Some(hot_key) = HotKey::of(*chord) else {
                refused.push(*chord);
                continue;
            };
            let Ok(index) = u32::try_from(index).map(|index| index & 0xFFFF) else {
                continue;
            };
            let id = EventHotKeyID {
                signature: SIGNATURE,
                id: (self.generation << 16) | index,
            };
            let mut registered: EventHotKeyRef = std::ptr::null_mut();
            // SAFETY: plain values and a valid out-pointer; the application
            // target lives for the program.
            let status = unsafe {
                RegisterEventHotKey(
                    hot_key.key_code,
                    hot_key.modifiers,
                    id,
                    GetApplicationEventTarget(),
                    0,
                    &mut registered,
                )
            };
            if status == NO_ERR && !registered.is_null() {
                self.registered.push(registered);
            } else {
                tracing::warn!(%chord, status, "macOS refused a shortcut");
                refused.push(*chord);
            }
        }
        refused
    }
}

unsafe extern "C" fn on_hot_key(
    _call: EventHandlerCallRef,
    event: EventRef,
    _user_data: *mut c_void,
) -> OSStatus {
    let mut id = EventHotKeyID::default();
    // SAFETY: asks for the event's hot key id into a buffer of exactly that
    // size; the event is valid for the duration of the handler.
    let status = unsafe {
        GetEventParameter(
            event,
            PARAM_DIRECT_OBJECT,
            TYPE_HOT_KEY_ID,
            std::ptr::null_mut(),
            std::mem::size_of::<EventHotKeyID>(),
            std::ptr::null_mut(),
            std::ptr::from_mut(&mut id).cast(),
        )
    };
    if status != NO_ERR || id.signature != SIGNATURE {
        return EVENT_NOT_HANDLED;
    }
    STATE.with(|state| {
        if let Ok(state) = state.try_borrow()
            && let Some(state) = state.as_ref()
            && id.id >> 16 == state.generation
            && let Ok(index) = usize::try_from(id.id & 0xFFFF)
            && index < state.chords.len()
        {
            (state.report)(index);
        }
    });
    NO_ERR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_become_carbon_key_codes_and_modifiers() {
        let left = Chord::parse("ctrl+alt+left").expect("parses");
        assert_eq!(
            HotKey::of(left),
            Some(HotKey {
                key_code: 0x7B,
                modifiers: CONTROL_KEY | OPTION_KEY
            })
        );
        let next = Chord::parse("ctrl+alt+win+right").expect("parses");
        assert_eq!(
            HotKey::of(next).map(|hot_key| hot_key.modifiers),
            Some(CONTROL_KEY | OPTION_KEY | CMD_KEY)
        );
        let clear = Chord::parse("ctrl+alt+clear").expect("parses");
        assert_eq!(
            HotKey::of(clear).map(|hot_key| hot_key.key_code),
            Some(0x47)
        );
        assert_eq!(HotKey::of(Chord::parse("ctrl+menu").expect("parses")), None);
    }

    #[test]
    fn four_character_codes_read_big_endian() {
        assert_eq!(EVENT_CLASS_KEYBOARD, 0x6B65_7962);
        assert_eq!(SIGNATURE, 0x5554_574D);
    }
}
