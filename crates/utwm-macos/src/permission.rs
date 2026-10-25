//! Accessibility access, which macOS requires before one app may move
//! another's windows. The user grants it once in System Settings → Privacy &
//! Security → Accessibility; until then every move fails.

use objc2_app_kit::NSWorkspace;
use objc2_application_services::{
    AXIsProcessTrusted, AXIsProcessTrustedWithOptions, kAXTrustedCheckOptionPrompt,
};
use objc2_core_foundation::{CFBoolean, CFDictionary};
use objc2_foundation::{NSString, NSURL};

const SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

/// Whether this app may move other apps' windows.
pub fn is_granted() -> bool {
    // SAFETY: no arguments.
    unsafe { AXIsProcessTrusted() }
}

/// Asks for access: macOS shows its own prompt, which leads to System
/// Settings, and adds this app to the list there, so granting it is one
/// switch. Returns whether access is already granted.
pub fn request() -> bool {
    // SAFETY: reads a constant the framework defines.
    let key = unsafe { kAXTrustedCheckOptionPrompt };
    let options = CFDictionary::from_slices(&[key], &[CFBoolean::new(true)]);
    // SAFETY: the options dictionary maps the prompt key to a CFBoolean, as
    // documented.
    unsafe { AXIsProcessTrustedWithOptions(Some(options.as_opaque())) }
}

/// Opens System Settings at the Accessibility list.
pub fn open_settings() {
    if let Some(url) = NSURL::URLWithString(&NSString::from_str(SETTINGS_URL)) {
        NSWorkspace::sharedWorkspace().openURL(&url);
    }
}
