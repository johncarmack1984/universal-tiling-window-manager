//! `cargo run -p utwm-e2e --bin utwm-inject -- <key>...`
//!
//! Presses the given keys (Windows names, e.g. `LCtrl LAlt F16`) in order and
//! releases them in reverse, by scan code, the way RustDesk delivers a Mac
//! keyboard. For checking the hook without a second machine.

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let keys: Vec<utwm_config::PhysicalKey> = std::env::args()
        .skip(1)
        .map(|name| {
            utwm_config::PhysicalKey::from_windows_name(&name).ok_or(format!("unknown key {name}"))
        })
        .collect::<Result<_, _>>()?;
    let (last, modifiers) = keys.split_last().ok_or("name at least one key")?;
    if !utwm_windows::inject::chord(modifiers, *last) {
        return Err("SendInput failed".into());
    }
    Ok(())
}

#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    eprintln!("utwm-inject presses keys by Windows scan code, so it runs on Windows only");
    std::process::ExitCode::FAILURE
}
