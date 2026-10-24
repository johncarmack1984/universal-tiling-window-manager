//! `cargo run -p utwm-windows --example probe -- [export.json] [seconds]`
//!
//! Logs every key event the low-level hook sees for a while, with what the
//! bindings would do with it, and swallows nothing. Used to check what a
//! keyboard (or RustDesk) actually delivers before trusting the bindings.

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::time::{Duration, Instant};
    use utwm_config::matcher::Decision;
    use utwm_windows::hook::{Hook, HookEvent, Mode};

    let mut args = std::env::args().skip(1);
    let path = args.next().map(std::path::PathBuf::from);
    let seconds: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(60);
    let home = std::env::home_dir().ok_or("no home directory")?;
    let resolved = utwm_config::load(&home, path.as_deref())?;
    let chords: Vec<_> = resolved.bindings.iter().map(|b| b.chord).collect();

    let (tx, rx) = std::sync::mpsc::channel();
    let _hook = Hook::start(
        Mode::Probe,
        chords,
        Box::new(move |event| {
            let _ = tx.send(event);
        }),
    )?;
    println!(
        "probing for {seconds}s; bindings from {}",
        resolved.source.describe()
    );
    let started = Instant::now();
    let deadline = started + Duration::from_secs(seconds);
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        let Ok(HookEvent::Probed(p)) = rx.recv_timeout(left) else {
            continue;
        };
        let what = match p.decision {
            Decision::Trigger { index, mask } => {
                let b = &resolved.bindings[index];
                let names: Vec<&str> = b.entries.iter().map(|e| e.name.as_str()).collect();
                format!(
                    "MATCH {} -> {}{}",
                    b.chord,
                    names.join(" / "),
                    if mask { " (mask)" } else { "" }
                )
            }
            Decision::Swallow => "swallow".to_owned(),
            Decision::Pass => String::new(),
        };
        println!(
            "{:7.3}s {} scan={:#06x}{} vk={:#04x} {:<12} mods={}{} {}",
            started.elapsed().as_secs_f64(),
            if p.down { "down" } else { "up  " },
            p.key.scan,
            if p.key.extended { " ext" } else { "    " },
            p.vk,
            p.key.windows_name(),
            p.modifiers,
            if p.injected {
                format!(" injected(extra={:#x})", p.extra_info)
            } else {
                String::new()
            },
            what
        );
    }
    Ok(())
}

#[cfg(not(windows))]
fn main() {}
