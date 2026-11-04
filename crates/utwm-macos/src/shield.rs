//! The lock screen's shield. While macOS says it is up, every app keeps its
//! windows from assistive apps like this one, so no window can be found, let
//! alone moved. macOS has been seen to leave it up after the screen was
//! unlocked, which hides every window of every app until the next login;
//! [`lower_if_stuck`] puts it down again.
//!
//! Apps learn about the shield from the session agent's notifications: AppKit
//! reads the state of `com.apple.sessionagent.shieldWindowIsShowing` when it
//! starts and again whenever a shield notification is posted, and while that
//! state is set it vends no element of a window below the shield.

use std::ffi::{CStr, c_char, c_int};

use objc2_core_foundation::{CFBoolean, CFNumber, CFRetained, CFString, CFType};
use objc2_core_graphics::CGSessionCopyCurrentDictionary;

const SHIELD_SHOWING: &CStr = c"com.apple.sessionagent.shieldWindowIsShowing";
const SHIELD_LOWERED: &CStr = c"com.apple.sessionagent.shieldWindowLowered";
const LOCK_UI_SHOWING: &CStr = c"com.apple.sessionagent.screenLockUIIsShowing";

const NOTIFY_STATUS_OK: u32 = 0;

unsafe extern "C" {
    fn notify_register_check(name: *const c_char, out_token: *mut c_int) -> u32;
    fn notify_get_state(token: c_int, state: *mut u64) -> u32;
    fn notify_set_state(token: c_int, state: u64) -> u32;
    fn notify_post(name: *const c_char) -> u32;
    fn notify_cancel(token: c_int) -> u32;
}

/// What macOS says about this login session.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Session {
    /// The session agent's word that the shield is up.
    shield_showing: bool,
    /// The lock screen's own interface is showing.
    lock_ui_showing: bool,
    /// The screen is locked.
    locked: bool,
    /// This session has the display and keyboard, not another user's.
    on_console: bool,
}

impl Session {
    /// The shield is stuck when macOS says it is up but there is nothing for
    /// it to shield: the session has the console, the screen isn't locked,
    /// and the lock screen isn't showing.
    fn shield_stuck(self) -> bool {
        self.shield_showing && self.on_console && !self.locked && !self.lock_ui_showing
    }

    fn current() -> Self {
        let session = CGSessionCopyCurrentDictionary();
        let flag = |key: &str| {
            session.as_ref().is_some_and(|session| {
                // SAFETY: the session dictionary maps strings to CF values.
                let session = unsafe { session.cast_unchecked::<CFString, CFType>() };
                session.get(&CFString::from_str(key)).is_some_and(truthy)
            })
        };
        Self {
            shield_showing: state(SHIELD_SHOWING).is_some_and(|state| state != 0),
            lock_ui_showing: state(LOCK_UI_SHOWING).is_some_and(|state| state != 0),
            // With no session dictionary to say otherwise, assume the worst.
            locked: session.is_none() || flag("CGSSessionScreenIsLocked"),
            on_console: flag("kCGSSessionOnConsoleKey"),
        }
    }
}

/// A session flag, which macOS gives as a boolean or a number.
fn truthy(value: CFRetained<CFType>) -> bool {
    match value.downcast::<CFBoolean>() {
        Ok(boolean) => boolean.as_bool(),
        Err(value) => value
            .downcast::<CFNumber>()
            .ok()
            .and_then(|number| number.as_i64())
            .is_some_and(|number| number != 0),
    }
}

/// A registration for one notification's state, cancelled when dropped.
struct Token(c_int);

impl Token {
    fn register(name: &CStr) -> Option<Self> {
        let mut token = 0;
        // SAFETY: `name` is NUL-terminated and `token` is a valid out-pointer.
        let status = unsafe { notify_register_check(name.as_ptr(), &mut token) };
        (status == NOTIFY_STATUS_OK).then_some(Self(token))
    }
}

impl Drop for Token {
    fn drop(&mut self) {
        // SAFETY: the token came from notify_register_check and is cancelled
        // exactly once.
        unsafe { notify_cancel(self.0) };
    }
}

/// A notification's state, as whoever posts it last set it.
fn state(name: &CStr) -> Option<u64> {
    let token = Token::register(name)?;
    let mut state = 0;
    // SAFETY: the token is registered and `state` is a valid out-pointer.
    let status = unsafe { notify_get_state(token.0, &mut state) };
    (status == NOTIFY_STATUS_OK).then_some(state)
}

/// Puts the shield down when macOS left it up with the screen unlocked: sets
/// the state every app reads back to 0, then posts the notifications that
/// make them read it again. Returns whether it did. Each app hears of it on a
/// queue of its own, so its windows can take a moment to come back.
pub fn lower_if_stuck() -> bool {
    if !Session::current().shield_stuck() {
        return false;
    }
    let Some(token) = Token::register(SHIELD_SHOWING) else {
        return false;
    };
    // SAFETY: the token is registered.
    if unsafe { notify_set_state(token.0, 0) } != NOTIFY_STATUS_OK {
        return false;
    }
    for name in [SHIELD_LOWERED, SHIELD_SHOWING] {
        // SAFETY: `name` is NUL-terminated.
        unsafe { notify_post(name.as_ptr()) };
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_shield_with_nothing_to_shield_is_stuck() {
        let stuck = Session {
            shield_showing: true,
            on_console: true,
            ..Session::default()
        };
        assert!(stuck.shield_stuck());
        assert!(
            !Session {
                locked: true,
                ..stuck
            }
            .shield_stuck()
        );
        assert!(
            !Session {
                lock_ui_showing: true,
                ..stuck
            }
            .shield_stuck()
        );
        assert!(
            !Session {
                on_console: false,
                ..stuck
            }
            .shield_stuck()
        );
        assert!(
            !Session {
                shield_showing: false,
                ..stuck
            }
            .shield_stuck()
        );
    }

    #[test]
    fn reads_the_session_agents_state() {
        assert!(state(SHIELD_SHOWING).is_some());
    }
}
