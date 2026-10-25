//! `cargo run -p utwm-macos --example e2e`
//!
//! End-to-end check against the running app on the Mac, on whatever displays
//! it has. It opens a window of its own, brings it to the front, presses the
//! configured shortcuts the way a keyboard does (posted key events), and checks
//! that the window lands exactly where the layout engine says it should, given
//! the same export. It never touches any other window.
//!
//! Before running it: the app is running with Accessibility access (`just
//! run`), Rectangle Pro is quit (it answers the same shortcuts), and the
//! terminal running this has Accessibility access too, which posting key
//! events needs.

#[cfg(target_os = "macos")]
fn main() -> std::process::ExitCode {
    e2e::run()
}

#[cfg(not(target_os = "macos"))]
fn main() {}

#[cfg(target_os = "macos")]
mod e2e {
    use std::path::PathBuf;
    use std::process::ExitCode;
    use std::time::{Duration, Instant};

    use dispatch2::MainThreadBound;
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSScreen, NSWindow,
        NSWindowStyleMask, NSWorkspace,
    };
    use objc2_core_graphics::{CGEvent, CGEventFlags, CGEventTapLocation};
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
    use utwm_config::bindings::Resolved;
    use utwm_config::{ActionId, Target};
    use utwm_layout::{Engine, PixelRect, Plan, Request, Screen};

    /// Where the window starts, in points from the top left of the main
    /// display; Restore brings it back here.
    const START: PixelRect = PixelRect::new(300, 200, 1100, 800);

    /// What gets pressed, in order. With Rectangle's default cycle sizes the
    /// three left halves are ½, ⅔ and ⅓.
    const PRESSES: &[ActionId] = &[
        ActionId::LeftHalf,
        ActionId::LeftHalf,
        ActionId::LeftHalf,
        ActionId::RightHalf,
        ActionId::TopLeft,
        ActionId::BottomRight,
        ActionId::FirstThird,
        ActionId::CenterTwoThirds,
        ActionId::Maximize,
        ActionId::Smaller,
        ActionId::Center,
        ActionId::Restore,
    ];

    pub fn run() -> ExitCode {
        let Some(mtm) = MainThreadMarker::new() else {
            eprintln!("the e2e check must start on the main thread");
            return ExitCode::FAILURE;
        };
        let config = match load() {
            Ok(config) => config,
            Err(error) => {
                eprintln!("could not load the configuration: {error}");
                return ExitCode::FAILURE;
            }
        };
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
        let window = MainThreadBound::new(open_window(mtm), mtm);
        #[expect(
            deprecated,
            reason = "the only activation a terminal-launched process gets"
        )]
        app.activateIgnoringOtherApps(true);
        std::thread::spawn(move || {
            let passed = checks(&config, &window);
            std::process::exit(if passed { 0 } else { 1 });
        });
        app.run();
        ExitCode::SUCCESS
    }

    /// The app's own configuration, found the way the app finds it.
    fn load() -> Result<Resolved, Box<dyn std::error::Error>> {
        let home = std::env::home_dir().ok_or("no home directory")?;
        let explicit = std::env::var_os("UTWM_CONFIG").map(PathBuf::from);
        Ok(utwm_config::load(&home, explicit.as_deref())?)
    }

    fn open_window(mtm: MainThreadMarker) -> Retained<NSWindow> {
        let style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Resizable
            | NSWindowStyleMask::Miniaturizable;
        let content = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(400.0, 300.0));
        // SAFETY: a plain titled window; it stays owned by the Retained below,
        // so AppKit must not also release it when closed.
        let window = unsafe {
            let window = NSWindow::initWithContentRect_styleMask_backing_defer(
                mtm.alloc(),
                content,
                style,
                NSBackingStoreType::Buffered,
                false,
            );
            window.setReleasedWhenClosed(false);
            window
        };
        window.setTitle(&NSString::from_str("utwm-e2e"));
        window.setFrame_display(appkit_rect(START, main_height()), true);
        window.makeKeyAndOrderFront(None);
        window
    }

    fn main_height() -> f64 {
        dispatch2::run_on_main(|mtm| {
            NSScreen::screens(mtm)
                .firstObject()
                .map_or(0.0, |main| main.frame().size.height)
        })
    }

    /// A screen rect (top left origin, y down) as an AppKit frame.
    fn appkit_rect(rect: PixelRect, main_height: f64) -> NSRect {
        NSRect::new(
            NSPoint::new(f64::from(rect.left), main_height - f64::from(rect.bottom)),
            NSSize::new(f64::from(rect.width()), f64::from(rect.height())),
        )
    }

    /// The window's frame in screen coordinates.
    fn frame(window: &MainThreadBound<Retained<NSWindow>>) -> PixelRect {
        let height = main_height();
        let frame = window.get_on_main(|window| window.frame());
        utwm_layout::Rect {
            x: frame.origin.x,
            y: height - (frame.origin.y + frame.size.height),
            w: frame.size.width,
            h: frame.size.height,
        }
        .to_pixels()
    }

    fn is_frontmost() -> bool {
        let own = i32::try_from(std::process::id()).unwrap_or(-1);
        NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .is_some_and(|app| app.processIdentifier() == own)
    }

    fn checks(config: &Resolved, window: &MainThreadBound<Retained<NSWindow>>) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !is_frontmost() {
            if Instant::now() > deadline {
                println!("FAIL the test window never came to the front; click it and run again");
                return false;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        std::thread::sleep(Duration::from_millis(300));

        let screens: Vec<Screen> = utwm_macos::monitors::screens();
        let mut engine: Engine<()> = Engine::new();
        let mut passed = true;
        for action in PRESSES {
            let Some(chord) = config.chord_for(Target::Action(*action)) else {
                println!("skip {action}: not bound in this configuration");
                continue;
            };
            let Some(mac) = chord.to_mac() else {
                println!("skip {action}: {chord} has no key on a Mac keyboard");
                continue;
            };
            let before = frame(window);
            let request = Request {
                target: Target::Action(*action),
                window: before,
                screens: &screens,
                settings: &config.settings,
                specs: &config.specs,
                others: &[],
            };
            let planned = match engine.plan(&(), &request) {
                Plan::Move(planned) => planned,
                Plan::AlreadyThere => {
                    println!("skip {action}: the window is already there");
                    continue;
                }
                Plan::Nothing(why) => {
                    println!("skip {action}: {why}");
                    continue;
                }
            };
            press(
                u16::try_from(mac.key_code).unwrap_or(0),
                flags(mac.modifiers),
            );
            let landed = settle(window, before, planned.rect);
            engine.landed(&(), &planned, landed);
            let (shortcut, name) = (mac.to_string(), action.name());
            if landed == planned.rect {
                println!("ok   {shortcut:<10} {name:<18} {landed}");
            } else {
                println!(
                    "FAIL {shortcut:<10} {name:<18} {landed}, expected {}",
                    planned.rect
                );
                passed = false;
            }
        }
        println!("{}", if passed { "passed" } else { "failed" });
        passed
    }

    /// Waits for the window to reach `expected`, or to have moved and then
    /// held still for 300 ms (a move is a size, a position and a size again),
    /// up to two seconds.
    fn settle(
        window: &MainThreadBound<Retained<NSWindow>>,
        before: PixelRect,
        expected: PixelRect,
    ) -> PixelRect {
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut last = before;
        let mut still_since = Instant::now();
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(25));
            let now = frame(window);
            if now == expected {
                return now;
            }
            if now != last {
                last = now;
                still_since = Instant::now();
            } else if now != before && still_since.elapsed() >= Duration::from_millis(300) {
                return now;
            }
        }
        last
    }

    fn flags(modifiers: utwm_config::chord::MacModifiers) -> CGEventFlags {
        let mut flags = CGEventFlags::empty();
        for (on, flag) in [
            (modifiers.control, CGEventFlags::MaskControl),
            (modifiers.option, CGEventFlags::MaskAlternate),
            (modifiers.shift, CGEventFlags::MaskShift),
            (modifiers.command, CGEventFlags::MaskCommand),
        ] {
            if on {
                flags |= flag;
            }
        }
        flags
    }

    /// Presses and releases one key with `flags` held, as the keyboard would.
    fn press(key_code: u16, flags: CGEventFlags) {
        for down in [true, false] {
            if let Some(event) = CGEvent::new_keyboard_event(None, key_code, down) {
                CGEvent::set_flags(Some(&event), flags);
                CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
