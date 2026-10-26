//! Global shortcuts, grabbed for this app by the extension inside GNOME Shell,
//! the way GNOME Shell grabs gnome-settings-daemon's media keys.
//!
//! Under Wayland an app can't grab keys, so the extension grabs them (Mutter's
//! `grab_accelerator`) and tells this app when one is pressed. As Carbon does
//! on the Mac, Mutter refuses a key another binding already holds; those come
//! back refused. Each binding names its key by position (its keycode), so it
//! stays on the same key whatever the keyboard layout, as on Windows.
//!
//! The extension lives in GNOME Shell's process and comes and goes with it:
//! it starts after a log-in, and GNOME turns it off and on again when the
//! shell restarts. Whenever it appears, leaves, or takes a key it had to
//! refuse, this reports `ShellChanged` and the bindings go to it again.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use utwm_config::Chord;
use zbus::blocking::Connection;
use zbus::blocking::fdo::DBusProxy;

use crate::shell::{self, CallError};

/// What the listener reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShortcutEvent {
    /// Binding `index` was pressed.
    Triggered(usize),
    /// The extension appeared, left, or took a key it had refused: the
    /// bindings should go to it again.
    ShellChanged,
}

/// Receives what the listener reports, on the listener's threads, so it must
/// be quick: send to a channel and return.
pub type Reporter = Box<dyn Fn(ShortcutEvent) + Send + Sync + 'static>;

/// gnome-settings-daemon's media keys, among them Ubuntu's Ctrl+Alt+T for a
/// terminal, which GNOME Shell grabs for it once both are up.
const MEDIA_KEYS: &str = "org.gnome.SettingsDaemon.MediaKeys";
/// gnome-session, which starts gnome-settings-daemon; without it there are no
/// media keys to wait for.
const SESSION_MANAGER: &str = "org.gnome.SessionManager";
/// How long the first grab waits for them, and then for their grabs, so the
/// keys GNOME binds for itself go to GNOME at every log-in rather than to
/// whichever grabs first.
const MEDIA_KEYS_WAIT: Duration = Duration::from_secs(5);
const MEDIA_KEYS_GRACE: Duration = Duration::from_secs(1);

/// The most bindings the extension takes in one set. It refuses a larger set
/// whole, so the rest are refused here instead.
pub(crate) const MAX_BINDINGS: usize = 200;

struct State {
    chords: Vec<Chord>,
    paused: bool,
    /// Bumped with every set of bindings sent and carried back with each
    /// press, so a press queued before new bindings arrived can't run the
    /// wrong one.
    generation: u32,
    /// For each accelerator sent, in order, its index into `chords`.
    sent: Vec<usize>,
    /// The chords the extension said it took, from the last set sent.
    held: Vec<Chord>,
}

/// The shortcuts. Its methods may be called from any thread.
pub struct Shortcuts {
    state: Arc<Mutex<State>>,
}

impl Shortcuts {
    /// Starts listening for the extension and the keys it reports, with
    /// `chords[i]` reporting `Triggered(i)`. It grabs nothing yet, so it never
    /// waits on the bus: the first `ShellChanged`, sent once the extension is
    /// there, is the cue to send the bindings.
    pub fn start(chords: Vec<Chord>, report: Reporter) -> std::io::Result<(Self, Vec<Chord>)> {
        let state = Arc::new(Mutex::new(State {
            chords,
            paused: false,
            generation: 0,
            sent: Vec::new(),
            held: Vec::new(),
        }));
        let report = Arc::new(report);
        let listener_state = Arc::clone(&state);
        let listener_report = Arc::clone(&report);
        std::thread::Builder::new()
            .name("shortcuts".to_owned())
            .spawn(move || listen(&listener_state, &listener_report))?;
        std::thread::Builder::new()
            .name("gnome-shell".to_owned())
            .spawn(move || watch(&report))?;
        Ok((Self { state }, Vec::new()))
    }

    /// Replaces the bindings. Returns the chords GNOME refused.
    pub fn set_bindings(&self, chords: Vec<Chord>) -> Vec<Chord> {
        if let Ok(mut state) = self.state.lock() {
            state.chords = chords;
        }
        self.register()
    }

    /// Paused, nothing is grabbed, so every key reaches the apps. Returns the
    /// chords GNOME refused once resumed.
    pub fn set_paused(&self, paused: bool) -> Vec<Chord> {
        if let Ok(mut state) = self.state.lock() {
            state.paused = paused;
        }
        self.register()
    }

    /// The chords the extension took for this app, as of the last bindings
    /// sent: none while paused, or while the extension is away.
    pub fn held(&self) -> Vec<Chord> {
        self.state
            .lock()
            .map(|state| state.held.clone())
            .unwrap_or_default()
    }

    /// Sends the bindings to the extension. Returns the chords refused.
    fn register(&self) -> Vec<Chord> {
        let Ok(mut state) = self.state.lock() else {
            return Vec::new();
        };
        state.generation = state.generation.wrapping_add(1);
        let chords = if state.paused {
            Vec::new()
        } else {
            state.chords.clone()
        };
        let mut refused = Vec::new();
        let mut sent = Vec::new();
        let mut accelerators = Vec::new();
        for (index, chord) in chords.iter().enumerate() {
            match accelerator(*chord) {
                Some(accelerator) if accelerators.len() < MAX_BINDINGS => {
                    sent.push(index);
                    accelerators.push(accelerator);
                }
                _ => refused.push(*chord),
            }
        }
        state.sent.clone_from(&sent);
        state.held.clear();
        let generation = state.generation;
        drop(state);

        let sent_chords = sent.iter().filter_map(|index| chords.get(*index).copied());
        match shell::set_bindings(generation, &accelerators) {
            Ok(grabbed) => {
                let mut held = Vec::new();
                for (chord, grabbed) in sent_chords.zip(grabbed) {
                    if grabbed {
                        held.push(chord);
                    } else {
                        refused.push(chord);
                    }
                }
                if let Ok(mut state) = self.state.lock() {
                    state.held = held;
                }
            }
            // Nothing grabs until the extension is there; the menu says why.
            Err(CallError::Unavailable) => {}
            Err(error) => {
                tracing::warn!(?error, "GNOME Shell didn't take the shortcuts");
                refused.extend(sent_chords);
            }
        }
        refused
    }
}

/// Drops every grab, for an app about to exit. The extension drops them
/// anyway once this process leaves the bus; this makes it immediate.
pub fn release() {
    let _ = shell::set_bindings(0, &[]);
}

/// What the extension grabs for `chord`: its modifiers as Mutter names them,
/// then its key's XKB keycode (Linux's code plus 8), so the binding stays on
/// that key whatever the layout. `None` for a key Linux has no code for, and
/// for typing, which the extension won't grab: one such chord in a set would
/// have it refuse the whole set.
fn accelerator(chord: Chord) -> Option<String> {
    if chord.is_typing() {
        return None;
    }
    let keycode = chord.key.evdev()? + 8;
    let modifiers = chord.modifiers;
    let mut accelerator = String::new();
    for (on, name) in [
        (modifiers.ctrl, "<Control>"),
        (modifiers.alt, "<Alt>"),
        (modifiers.shift, "<Shift>"),
        (modifiers.win, "<Super>"),
    ] {
        if on {
            accelerator.push_str(name);
        }
    }
    Some(format!("{accelerator}0x{keycode:02x}"))
}

/// Reports the presses and newly taken keys the extension signals.
fn listen(state: &Mutex<State>, report: &Reporter) {
    let Some(connection) = shell::connection() else {
        return;
    };
    let signals = shell::proxy(
        connection,
        shell::BUS_NAME,
        shell::OBJECT_PATH,
        shell::INTERFACE,
    )
    .and_then(|proxy| proxy.receive_all_signals());
    let signals = match signals {
        Ok(signals) => signals,
        Err(error) => {
            tracing::warn!(%error, "can't listen to GNOME Shell; shortcuts won't work");
            return;
        }
    };
    for message in signals {
        // Only GNOME Shell speaks for the extension.
        let header = message.header();
        let from_shell = header.sender().is_some_and(|sender| {
            shell::owner(connection, shell::SHELL).as_deref() == Some(sender.as_str())
        });
        if !from_shell {
            continue;
        }
        match header.member().map(|member| member.as_str()) {
            Some("Activated") => {
                let Ok((generation, index)) = message.body().deserialize::<(u32, u32)>() else {
                    continue;
                };
                let binding = state.lock().ok().and_then(|state| {
                    let index = usize::try_from(index).ok()?;
                    (state.generation == generation)
                        .then(|| state.sent.get(index).copied())
                        .flatten()
                });
                if let Some(binding) = binding {
                    report(ShortcutEvent::Triggered(binding));
                }
            }
            Some("GrabsChanged") => report(ShortcutEvent::ShellChanged),
            _ => {}
        }
    }
}

/// Reports the extension appearing and leaving. It subscribes before its
/// first look, so an extension that appears in between isn't missed.
fn watch(report: &Reporter) {
    let Some(connection) = shell::connection() else {
        return;
    };
    let bus = match DBusProxy::new(connection) {
        Ok(bus) => bus,
        Err(error) => {
            tracing::warn!(%error, "can't watch for GNOME Shell's extension");
            return;
        }
    };
    let changes = match bus.receive_name_owner_changed_with_args(&[(0, shell::BUS_NAME)]) {
        Ok(changes) => changes,
        Err(error) => {
            tracing::warn!(%error, "can't watch for GNOME Shell's extension");
            return;
        }
    };
    wait_for_media_keys(connection);
    report(ShortcutEvent::ShellChanged);
    for _ in changes {
        report(ShortcutEvent::ShellChanged);
    }
}

/// Waits, a few seconds at most, for gnome-settings-daemon's media keys and
/// then for their grabs.
fn wait_for_media_keys(connection: &Connection) {
    if shell::owner(connection, MEDIA_KEYS).is_some()
        || shell::owner(connection, SESSION_MANAGER).is_none()
    {
        return;
    }
    let deadline = Instant::now() + MEDIA_KEYS_WAIT;
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(250));
        if shell::owner(connection, MEDIA_KEYS).is_some() {
            std::thread::sleep(MEDIA_KEYS_GRACE);
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accelerator_for(text: &str) -> Option<String> {
        accelerator(Chord::parse(text).expect("parses"))
    }

    #[test]
    fn chords_become_mutter_accelerators_by_keycode() {
        assert_eq!(
            accelerator_for("ctrl+alt+left").as_deref(),
            Some("<Control><Alt>0x71")
        );
        assert_eq!(
            accelerator_for("ctrl+alt+shift+up").as_deref(),
            Some("<Control><Alt><Shift>0x6f")
        );
        assert_eq!(
            accelerator_for("ctrl+alt+win+right").as_deref(),
            Some("<Control><Alt><Super>0x72")
        );
        assert_eq!(
            accelerator_for("ctrl+alt+esc").as_deref(),
            Some("<Control><Alt>0x09"),
            "Mutter reads a keycode only with two hex digits"
        );
        assert_eq!(
            accelerator_for("ctrl+alt+num7").as_deref(),
            Some("<Control><Alt>0x4f")
        );
        assert_eq!(
            accelerator_for("ctrl+alt+home").as_deref(),
            Some("<Control><Alt>0x6e")
        );
    }

    #[test]
    fn typing_is_never_sent() {
        for typing in ["a", "shift+a", "left", "shift+enter"] {
            assert_eq!(accelerator_for(typing), None, "{typing}");
        }
        assert_eq!(accelerator_for("f5").as_deref(), Some("0x47"));
        assert_eq!(accelerator_for("shift+f13").as_deref(), Some("<Shift>0xbf"));
    }
}
