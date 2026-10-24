//! The low-level keyboard hook.
//!
//! It runs on a thread of its own that does nothing but pump messages, as
//! Microsoft recommends: Windows silently removes a low-level hook whose
//! callback takes longer than `LowLevelHooksTimeout`. The callback only asks the
//! [`Matcher`] what to do and, on a match, sends the binding's index down a
//! reporter; moving windows happens elsewhere.
//!
//! Bindings, pause and re-installation arrive as thread messages, so the
//! matcher is only ever touched by the hook thread and needs no lock.

use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use utwm_config::matcher::{Decision, KeyEvent, Matcher};
use utwm_config::{Chord, Modifiers, PhysicalKey};
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_INJECTED,
    MSG, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_APP,
    WM_KEYDOWN, WM_QUIT, WM_SYSKEYDOWN,
};

use crate::inject;

const MSG_MASK: u32 = WM_APP + 1;
const MSG_REBIND: u32 = WM_APP + 2;
const MSG_PAUSE: u32 = WM_APP + 3;
const MSG_REINSTALL: u32 = WM_APP + 4;

/// Receives what the hook reports. Called on the hook thread, so it must be
/// quick: send to a channel and return.
pub type Reporter = Box<dyn Fn(HookEvent) + Send + 'static>;

/// What the hook reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookEvent {
    /// Binding `index` was pressed.
    Triggered(usize),
    /// Probe mode: one raw event and what the matcher would have done.
    Probed(Probe),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Probe {
    pub key: PhysicalKey,
    pub vk: u16,
    pub down: bool,
    pub injected: bool,
    pub extra_info: usize,
    pub modifiers: Modifiers,
    pub decision: Decision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Match, swallow and report bindings.
    Active,
    /// Report every event and what would have matched; swallow nothing.
    Probe,
}

struct State {
    matcher: Matcher,
    report: Reporter,
    mode: Mode,
    thread_id: u32,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

#[derive(Default)]
struct Shared {
    pending: Mutex<Option<Vec<Chord>>>,
    paused: Mutex<bool>,
}

/// Controls the running hook thread.
pub struct Hook {
    thread_id: u32,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Hook {
    /// Starts the hook thread with `chords[i]` reporting `Triggered(i)`.
    pub fn start(mode: Mode, chords: Vec<Chord>, report: Reporter) -> std::io::Result<Self> {
        let shared = Arc::new(Shared::default());
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<u32, String>>();
        let thread_shared = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("keyboard-hook".to_owned())
            .spawn(move || run(mode, chords, report, thread_shared, ready_tx))?;
        match ready_rx.recv() {
            Ok(Ok(thread_id)) => Ok(Self {
                thread_id,
                shared,
                thread: Some(thread),
            }),
            Ok(Err(message)) => Err(std::io::Error::other(message)),
            Err(_) => Err(std::io::Error::other("the keyboard hook thread exited")),
        }
    }

    fn post(&self, message: u32) {
        // SAFETY: posts a message to a thread we created; no pointers involved.
        unsafe { PostThreadMessageW(self.thread_id, message, 0, 0) };
    }

    /// Replaces the bindings; takes effect before the next key event.
    pub fn set_bindings(&self, chords: Vec<Chord>) {
        if let Ok(mut pending) = self.shared.pending.lock() {
            *pending = Some(chords);
        }
        self.post(MSG_REBIND);
    }

    pub fn set_paused(&self, paused: bool) {
        if let Ok(mut flag) = self.shared.paused.lock() {
            *flag = paused;
        }
        self.post(MSG_PAUSE);
    }

    /// Removes and re-installs the hook, forgetting held keys. Windows drops a
    /// hook that once timed out without telling anyone; doing this after the
    /// session unlocks or the machine resumes puts it back.
    pub fn reinstall(&self) {
        self.post(MSG_REINSTALL);
    }
}

impl Drop for Hook {
    fn drop(&mut self) {
        self.post(WM_QUIT);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn install() -> HHOOK {
    // SAFETY: a null module name returns this executable's handle.
    let module = unsafe { GetModuleHandleW(std::ptr::null()) };
    // SAFETY: `hook_proc` has the HOOKPROC signature and lives for the program.
    unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), module, 0) }
}

fn run(
    mode: Mode,
    chords: Vec<Chord>,
    report: Reporter,
    shared: Arc<Shared>,
    ready: std::sync::mpsc::Sender<Result<u32, String>>,
) {
    // SAFETY: no arguments.
    let thread_id = unsafe { GetCurrentThreadId() };
    STATE.with(|state| {
        *state.borrow_mut() = Some(State {
            matcher: Matcher::new(chords),
            report,
            mode,
            thread_id,
        });
    });
    let mut hook = install();
    if hook.is_null() {
        let _ = ready.send(Err("SetWindowsHookExW failed".to_owned()));
        return;
    }
    let _ = ready.send(Ok(thread_id));

    let mut msg = MSG::default();
    loop {
        // SAFETY: `msg` is a valid MSG; a null window takes thread messages.
        let got = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
        if got <= 0 {
            break;
        }
        match msg.message {
            MSG_MASK => {
                inject::mask_key();
            }
            MSG_REBIND => {
                let chords = shared
                    .pending
                    .lock()
                    .ok()
                    .and_then(|mut pending| pending.take());
                if let Some(chords) = chords {
                    with_state(|state| state.matcher.replace_bindings(chords));
                }
            }
            MSG_PAUSE => {
                let paused = shared.paused.lock().map(|flag| *flag).unwrap_or(false);
                with_state(|state| state.matcher.set_paused(paused));
            }
            MSG_REINSTALL => {
                // SAFETY: `hook` is the hook this thread installed.
                unsafe { UnhookWindowsHookEx(hook) };
                with_state(|state| state.matcher.reset());
                hook = install();
                if hook.is_null() {
                    tracing::error!("re-installing the keyboard hook failed");
                }
            }
            _ => {}
        }
    }
    if !hook.is_null() {
        // SAFETY: `hook` is the hook this thread installed.
        unsafe { UnhookWindowsHookEx(hook) };
    }
}

fn with_state(f: impl FnOnce(&mut State)) {
    STATE.with(|state| {
        if let Ok(mut state) = state.try_borrow_mut()
            && let Some(state) = state.as_mut()
        {
            f(state);
        }
    });
}

/// The modifiers Windows itself believes are down.
fn async_modifiers() -> Modifiers {
    // SAFETY: GetAsyncKeyState takes a virtual-key code and touches no memory.
    let down = |vk: u16| unsafe { GetAsyncKeyState(i32::from(vk)) } < 0;
    Modifiers {
        ctrl: down(VK_CONTROL),
        alt: down(VK_MENU),
        shift: down(VK_SHIFT),
        win: down(VK_LWIN) || down(VK_RWIN),
    }
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == i32::try_from(HC_ACTION).unwrap_or(0) && lparam != 0 {
        // SAFETY: for WH_KEYBOARD_LL with HC_ACTION, lparam points at a
        // KBDLLHOOKSTRUCT valid for the duration of this call.
        let info = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        let message = u32::try_from(wparam).unwrap_or(0);
        let down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
        let event = KeyEvent {
            key: PhysicalKey {
                scan: u16::try_from(info.scanCode).unwrap_or(0),
                extended: info.flags & LLKHF_EXTENDED != 0,
            },
            vk: u16::try_from(info.vkCode).unwrap_or(0),
            down,
            extra_info: info.dwExtraInfo,
        };
        let injected = info.flags & LLKHF_INJECTED != 0;
        let swallow = STATE.with(|state| {
            let Ok(mut state) = state.try_borrow_mut() else {
                return false;
            };
            let Some(state) = state.as_mut() else {
                return false;
            };
            let decision = state.matcher.on_event(event, Some(async_modifiers()));
            match state.mode {
                Mode::Probe => {
                    (state.report)(HookEvent::Probed(Probe {
                        key: event.key,
                        vk: event.vk,
                        down,
                        injected,
                        extra_info: event.extra_info,
                        modifiers: state.matcher.modifiers(),
                        decision,
                    }));
                    false
                }
                Mode::Active => match decision {
                    Decision::Pass => false,
                    Decision::Swallow => true,
                    Decision::Trigger { index, mask } => {
                        (state.report)(HookEvent::Triggered(index));
                        if mask {
                            // Sent from the message loop, after this callback returns.
                            // SAFETY: posts to this very thread; no pointers.
                            unsafe { PostThreadMessageW(state.thread_id, MSG_MASK, 0, 0) };
                        }
                        true
                    }
                },
            }
        });
        if swallow {
            return 1;
        }
    }
    // SAFETY: passes the event on unchanged, as every hook must.
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}
