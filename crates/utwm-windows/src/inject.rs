//! Synthetic keystrokes: the menu mask key after a swallowed Alt or Win chord,
//! and scan-code injection that reproduces what RustDesk delivers, for tests.

use utwm_config::PhysicalKey;
use utwm_config::matcher::INJECTED_TAG;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
    KEYEVENTF_SCANCODE, SendInput,
};

/// An unassigned virtual-key code. Pressing it between a swallowed key and the
/// release of Alt or Win tells Windows another key was used, so the release
/// doesn't open the menu bar or Start (AutoHotkey's `#MenuMaskKey`).
const MASK_VK: u16 = 0xE8;

fn send(inputs: &[INPUT]) -> bool {
    let Ok(count) = u32::try_from(inputs.len()) else {
        return false;
    };
    let Ok(size) = i32::try_from(std::mem::size_of::<INPUT>()) else {
        return false;
    };
    // SAFETY: `inputs` is a valid slice of `count` INPUT structures.
    let sent = unsafe { SendInput(count, inputs.as_ptr(), size) };
    sent == count
}

fn keyboard(vk: u16, scan: u16, flags: u32, extra: usize) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: extra,
            },
        },
    }
}

/// Presses and releases the mask key, tagged so the hook ignores it.
pub fn mask_key() -> bool {
    send(&[
        keyboard(MASK_VK, 0, 0, INJECTED_TAG),
        keyboard(MASK_VK, 0, KEYEVENTF_KEYUP, INJECTED_TAG),
    ])
}

/// One key transition, injected by scan code as RustDesk's Map mode does.
#[derive(Clone, Copy, Debug)]
pub struct Stroke {
    pub key: PhysicalKey,
    pub down: bool,
}

/// Injects `strokes` by scan code, untagged so the hook treats them like a real
/// keyboard (or RustDesk). For end-to-end tests.
pub fn scan_codes(strokes: &[Stroke]) -> bool {
    let inputs: Vec<INPUT> = strokes
        .iter()
        .map(|stroke| {
            let mut flags = KEYEVENTF_SCANCODE;
            if stroke.key.extended {
                flags |= KEYEVENTF_EXTENDEDKEY;
            }
            if !stroke.down {
                flags |= KEYEVENTF_KEYUP;
            }
            keyboard(0, stroke.key.scan, flags, 0)
        })
        .collect();
    send(&inputs)
}

/// Presses `modifiers` (in order), taps `key`, then releases the modifiers in
/// reverse.
pub fn chord(modifiers: &[PhysicalKey], key: PhysicalKey) -> bool {
    let mut strokes: Vec<Stroke> = modifiers
        .iter()
        .map(|key| Stroke {
            key: *key,
            down: true,
        })
        .collect();
    strokes.push(Stroke { key, down: true });
    strokes.push(Stroke { key, down: false });
    strokes.extend(modifiers.iter().rev().map(|key| Stroke {
        key: *key,
        down: false,
    }));
    scan_codes(&strokes)
}
