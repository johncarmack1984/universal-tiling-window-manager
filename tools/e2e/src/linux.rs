//! The end-to-end check on Linux, under GNOME. It opens a window of its own,
//! has GNOME Shell put it at a known place and focus it, presses the
//! configured shortcuts the way a keyboard does, and checks that the window
//! lands exactly where the layout engine says it should, given the same
//! export. It never touches any other window.
//!
//! Keys go in through a Mutter remote desktop session, by Linux key code, so
//! each lands on its key whatever the layout; GNOME shows its remote access
//! indicator in the top bar while the check runs.
//!
//! By default it checks the app running in this session (`just run`), with
//! its GNOME Shell extension active and GNOME's own shortcuts on its keys
//! freed, both of which the app's menu offers. `--headless` instead starts a
//! GNOME Shell of its own, with no screen and two virtual displays, installs
//! the extension and runs the app inside it, and checks there. That session
//! has its own bus, settings, home and runtime folders, and runs outside the
//! login session, so it leaves this one alone.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use tao::dpi::LogicalSize;
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::WindowBuilder;
use utwm_config::bindings::Resolved;
use utwm_config::{ActionId, Chord, PhysicalKey, Platform, Target};
use utwm_layout::{Engine, PixelRect, Plan, Request};
use utwm_linux::window::{self, Window};
use utwm_linux::{control, extension, monitors};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::OwnedObjectPath;

/// Where the window starts, in GNOME's coordinates; Restore brings it back
/// here.
const START: PixelRect = PixelRect::new(300, 200, 1100, 800);

/// What gets pressed, in order, as on the Mac. With Rectangle's default cycle
/// sizes the three left halves are ½, ⅔ and ⅓. The display moves need a second
/// display; with one, the engine has nothing to plan for them and they are
/// skipped.
const PRESSES: &[ActionId] = &[
    ActionId::LeftHalf,
    ActionId::LeftHalf,
    ActionId::LeftHalf,
    ActionId::RightHalf,
    ActionId::NextDisplay,
    ActionId::PreviousDisplay,
    ActionId::TopLeft,
    ActionId::NextDisplay,
    ActionId::BottomRight,
    ActionId::PreviousDisplay,
    ActionId::FirstThird,
    ActionId::CenterTwoThirds,
    ActionId::Maximize,
    ActionId::NextDisplay,
    ActionId::PreviousDisplay,
    ActionId::Smaller,
    ActionId::Center,
    ActionId::Restore,
];

pub fn run(args: &[String]) -> ExitCode {
    match args.first().map(String::as_str) {
        None => live(),
        Some("--headless") => headless(),
        Some(INSIDE) => inside_headless(),
        Some(other) => {
            eprintln!("unknown argument `{other}`; the argument is --headless");
            ExitCode::FAILURE
        }
    }
}

/// Checks the app running in this session.
fn live() -> ExitCode {
    let config = match load() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("could not load the configuration: {error}");
            return ExitCode::FAILURE;
        }
    };
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("utwm-e2e")
        .with_inner_size(LogicalSize::new(400.0, 300.0))
        .build(&event_loop);
    let window = match window {
        Ok(window) => window,
        Err(error) => {
            eprintln!("could not open the test window: {error}");
            return ExitCode::FAILURE;
        }
    };
    std::thread::spawn(move || {
        let passed = checks(&config);
        std::process::exit(if passed { 0 } else { 1 });
    });
    event_loop.run(move |_, _, control_flow| {
        // The window stays open for as long as the check runs.
        let _ = &window;
        *control_flow = ControlFlow::Wait;
    })
}

/// The app's own configuration, found the way the app finds it.
fn load() -> Result<Resolved, Box<dyn std::error::Error>> {
    let home = std::env::home_dir().ok_or("no home directory")?;
    let explicit = std::env::var_os("UTWM_CONFIG").map(PathBuf::from);
    let named = utwm_config::load::named(&home, explicit.as_deref());
    Ok(utwm_config::load(&home, named.as_deref())?)
}

/// Polls `done` every 100 ms for up to `limit`.
fn wait_for(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + limit;
    loop {
        if done() {
            return true;
        }
        if Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// This process's window, once GNOME Shell lists it.
fn own_window() -> Option<Window> {
    let mut found = None;
    wait_for(Duration::from_secs(10), || {
        found = window::windows_of(std::process::id()).first().copied();
        found.is_some()
    });
    found
}

fn checks(config: &Resolved) -> bool {
    let Some(window) = own_window() else {
        println!("FAIL GNOME Shell never listed the test window; is the app's extension running?");
        return false;
    };
    let screens = monitors::screens();
    // GNOME places a new window as it first appears, undoing a move made
    // before then, so the window starts where it was put only once it has
    // held still.
    let mut last = None;
    wait_for(Duration::from_secs(5), || {
        let now = window::visible_frame(window);
        let still = now.is_some() && now == last;
        last = now;
        still
    });
    let started = wait_for(Duration::from_secs(5), || {
        window::place(window, START, false, &screens).is_ok_and(|frame| frame == START)
    });
    if !started {
        println!(
            "FAIL the test window wouldn't start at {START}: it is at {:?}",
            window::visible_frame(window)
        );
        return false;
    }
    if let Err(error) = window::activate(window) {
        println!("FAIL couldn't focus the test window: {error}");
        return false;
    }
    if !wait_for(Duration::from_secs(10), || {
        window::focused() == Some(window)
    }) {
        println!("FAIL the test window never got the focus; click it and run again");
        return false;
    }
    let keyboard = match Keyboard::open() {
        Ok(keyboard) => keyboard,
        Err(error) => {
            println!("FAIL couldn't press keys through Mutter's remote desktop: {error}");
            return false;
        }
    };
    std::thread::sleep(Duration::from_millis(300));

    let mut engine: Engine<()> = Engine::new();
    let mut passed = true;
    for action in PRESSES {
        let Some(chord) = config.chord_for(Target::Action(*action)) else {
            println!("skip {action}: not bound in this configuration");
            continue;
        };
        // Which action a cycling shortcut runs depends on the window's
        // history in the app, which this check can't see.
        if config
            .binding_for(&chord)
            .is_some_and(|binding| binding.is_cycle())
        {
            println!("skip {action}: its shortcut cycles through several actions");
            continue;
        }
        let Some(before) = window::visible_frame(window) else {
            println!("FAIL the test window is gone");
            return false;
        };
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
        let (shortcut, name) = (chord.label(Platform::Linux), action.name());
        // A shortcut moves whichever window has the focus, so each press
        // waits on the test window still having it.
        if window::focused() != Some(window) {
            println!(
                "FAIL the test window lost the focus; stopping before {shortcut} moves another window"
            );
            return false;
        }
        keyboard.press(chord);
        let landed = settle(window, before, planned.rect);
        engine.landed(&(), &planned, landed);
        if landed == planned.rect {
            println!("ok   {shortcut:<22} {name:<18} {landed}");
        } else {
            println!(
                "FAIL {shortcut:<22} {name:<18} {landed}, expected {}",
                planned.rect
            );
            passed = false;
        }
    }
    println!("{}", if passed { "passed" } else { "failed" });
    passed
}

/// Waits for the window to reach `expected`, or to have moved and then held
/// still for 300 ms, up to two seconds.
fn settle(window: Window, before: PixelRect, expected: PixelRect) -> PixelRect {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut last = before;
    let mut still_since = Instant::now();
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(25));
        let Some(now) = window::visible_frame(window) else {
            return last;
        };
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

const REMOTE_DESKTOP: &str = "org.gnome.Mutter.RemoteDesktop";
const REMOTE_DESKTOP_PATH: &str = "/org/gnome/Mutter/RemoteDesktop";

/// Presses keys the way a keyboard does: through a Mutter remote desktop
/// session, by Linux key code.
struct Keyboard {
    session: Proxy<'static>,
}

impl Keyboard {
    fn open() -> zbus::Result<Self> {
        let connection = Connection::session()?;
        let remote = Proxy::new(
            &connection,
            REMOTE_DESKTOP,
            REMOTE_DESKTOP_PATH,
            REMOTE_DESKTOP,
        )?;
        let path: OwnedObjectPath = remote.call("CreateSession", &())?;
        // Mutter only takes a session's input from the connection that made it.
        let session = Proxy::new_owned(
            connection,
            REMOTE_DESKTOP.to_owned(),
            path,
            format!("{REMOTE_DESKTOP}.Session"),
        )?;
        session.call::<_, _, ()>("Start", &())?;
        Ok(Self { session })
    }

    fn key(&self, key: PhysicalKey, down: bool) {
        if let Some(code) = key.evdev() {
            let _ = self
                .session
                .call::<_, _, ()>("NotifyKeyboardKeycode", &(u32::from(code), down));
        }
    }

    /// Presses the chord's modifiers in order, taps its key, then releases
    /// the modifiers in reverse.
    fn press(&self, chord: Chord) {
        let modifiers: Vec<PhysicalKey> = [
            (chord.modifiers.ctrl, "LCtrl"),
            (chord.modifiers.alt, "LAlt"),
            (chord.modifiers.shift, "LShift"),
            (chord.modifiers.win, "LWin"),
        ]
        .into_iter()
        .filter(|(on, _)| *on)
        .filter_map(|(_, name)| PhysicalKey::from_windows_name(name))
        .collect();
        for key in &modifiers {
            self.key(*key, true);
        }
        self.key(chord.key, true);
        std::thread::sleep(Duration::from_millis(20));
        self.key(chord.key, false);
        for key in modifiers.iter().rev() {
            self.key(*key, false);
        }
    }
}

impl Drop for Keyboard {
    fn drop(&mut self) {
        let _ = self.session.call::<_, _, ()>("Stop", &());
    }
}

/// The argument the headless check runs itself with, inside its session.
const INSIDE: &str = "--inside-headless";
/// The test session's Wayland display, in its own runtime folder.
const TEST_DISPLAY: &str = "utwm-e2e";
/// GNOME's own shortcuts on the app's default keys, cleared in the test
/// session; gnome-settings-daemon, which holds Ctrl+Alt+T, doesn't run there.
const TEST_SETTINGS: &str = "\
[org/gnome/shell]
start-in-overview=false
welcome-dialog-last-shown-version='999'

[org/gnome/desktop/screensaver]
lock-enabled=false

[org/gnome/desktop/wm/keybindings]
switch-to-workspace-left=['<Super>Page_Up']
switch-to-workspace-right=['<Super>Page_Down']
switch-to-workspace-up=@as []
switch-to-workspace-down=@as []
move-to-workspace-up=@as []
move-to-workspace-down=@as []
show-desktop=@as []
";

/// `--headless`: makes a throwaway home for a test session and runs this
/// check again inside it, outside the login session, on a bus of its own.
fn headless() -> ExitCode {
    use std::os::unix::fs::DirBuilderExt;

    let Ok(this) = std::env::current_exe() else {
        eprintln!("can't tell where utwm-e2e is");
        return ExitCode::FAILURE;
    };
    let Some(app) = this
        .parent()
        .map(|folder| folder.join("universal-tiling-window-manager"))
        .filter(|app| app.is_file())
    else {
        eprintln!("build the app first: cargo build -p universal-tiling-window-manager");
        return ExitCode::FAILURE;
    };
    let root = std::env::temp_dir().join(format!("utwm-e2e-{}", std::process::id()));
    // Made new, for this user alone: a folder someone else made first could
    // hand the test session their own settings and extensions.
    if let Err(error) = std::fs::DirBuilder::new().mode(0o700).create(&root) {
        eprintln!("could not make {}: {error}", root.display());
        return ExitCode::FAILURE;
    }
    let result = prepare(&root).and_then(|()| {
        let mut command = Command::new("systemd-run");
        command
            .args(["--user", "--scope", "--quiet", "--collect", "--"])
            .arg("/usr/bin/env")
            .arg("-i");
        for (key, value) in test_environment(&root, &app) {
            command.arg(format!("{key}={}", value.display()));
        }
        // What the session's own services say goes to its log; the check's
        // results come out on standard output.
        command
            .args(["/usr/bin/dbus-run-session", "--"])
            .arg(&this)
            .arg(INSIDE)
            .stderr(log(&root, "session.log"));
        command.status()
    });
    match result {
        Ok(status) if status.success() => {
            remove(&root);
            ExitCode::SUCCESS
        }
        Ok(_) => {
            eprintln!(
                "the test session's files, logs included, are in {}",
                root.display()
            );
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("could not start the test session: {error}");
            remove(&root);
            ExitCode::FAILURE
        }
    }
}

/// Deletes the test session's folder. Its services outlive the session by a
/// moment, the document portal's mount in `runtime/doc` among them, so this
/// tries again until they have let go.
fn remove(root: &Path) {
    let removed = wait_for(Duration::from_secs(10), || {
        match std::fs::remove_dir_all(root) {
            Ok(()) => true,
            Err(error) => error.kind() == std::io::ErrorKind::NotFound,
        }
    });
    if !removed {
        eprintln!(
            "couldn't delete the test session's files in {}",
            root.display()
        );
    }
}

/// The test session's folders, with its settings in GSettings' keyfile.
fn prepare(root: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    for folder in ["home", "config", "data", "state", "cache", "runtime"] {
        std::fs::create_dir_all(root.join(folder))?;
    }
    std::fs::set_permissions(root.join("runtime"), std::fs::Permissions::from_mode(0o700))?;
    let settings = root.join("config").join("glib-2.0").join("settings");
    std::fs::create_dir_all(&settings)?;
    std::fs::write(settings.join("keyfile"), TEST_SETTINGS)
}

/// Everything the test session sees: its own home and folders, settings kept
/// in a file rather than dconf, and nothing of this session's displays.
fn test_environment(root: &Path, app: &Path) -> Vec<(&'static str, PathBuf)> {
    let mut environment = vec![
        ("HOME", root.join("home")),
        ("XDG_CONFIG_HOME", root.join("config")),
        ("XDG_DATA_HOME", root.join("data")),
        ("XDG_STATE_HOME", root.join("state")),
        ("XDG_CACHE_HOME", root.join("cache")),
        ("XDG_RUNTIME_DIR", root.join("runtime")),
        ("GSETTINGS_BACKEND", PathBuf::from("keyfile")),
        ("UTWM_E2E_APP", app.to_path_buf()),
        ("UTWM_E2E_ROOT", root.to_path_buf()),
    ];
    for key in ["PATH", "LANG", "USER", "LOGNAME", "XDG_DATA_DIRS"] {
        if let Some(value) = std::env::var_os(key) {
            environment.push((key, PathBuf::from(value)));
        }
    }
    environment
}

/// Whether this process belongs to a logind session, which a second GNOME
/// Shell must stay out of: it would follow that session's lock and report
/// its own.
fn in_login_session() -> bool {
    std::fs::read_to_string("/proc/self/cgroup").is_ok_and(|cgroups| cgroups.contains("/session-"))
}

/// Inside the test session: starts GNOME Shell with the extension installed,
/// then the app, then the live check against them, and stops the two. Its
/// standard error goes to the session's log, so it reports on standard output.
fn inside_headless() -> ExitCode {
    if in_login_session() {
        println!("refusing to start a second GNOME Shell inside the login session");
        return ExitCode::FAILURE;
    }
    let (Some(app), Some(root)) = (
        std::env::var_os("UTWM_E2E_APP").map(PathBuf::from),
        std::env::var_os("UTWM_E2E_ROOT").map(PathBuf::from),
    ) else {
        println!("{INSIDE} is for `utwm-e2e --headless`");
        return ExitCode::FAILURE;
    };
    if let Err(error) = extension::install() {
        println!("could not install the extension in the test session: {error}");
        return ExitCode::FAILURE;
    }
    let mut running = Running(Vec::new());
    let shell = Command::new("gnome-shell")
        .args(["--wayland", "--headless", "--no-x11"])
        .args([
            "--virtual-monitor",
            "1920x1080",
            "--virtual-monitor",
            "2560x1440",
        ])
        .args(["--wayland-display", TEST_DISPLAY])
        .stdout(log(&root, "gnome-shell.log"))
        .stderr(log(&root, "gnome-shell.log"))
        .spawn();
    match shell {
        Ok(shell) => running.0.push(shell),
        Err(error) => {
            println!("could not start GNOME Shell: {error}");
            return ExitCode::FAILURE;
        }
    }
    if !wait_for(Duration::from_secs(30), || {
        extension::state() == extension::State::Active
    }) {
        println!(
            "the extension never started in the test session ({:?}); see {}",
            extension::state(),
            root.join("gnome-shell.log").display()
        );
        return ExitCode::FAILURE;
    }
    match Command::new(&app)
        .env("WAYLAND_DISPLAY", TEST_DISPLAY)
        .env("UTWM_LOG", "debug")
        .stdout(Stdio::null())
        .stderr(log(&root, "app.log"))
        .spawn()
    {
        Ok(app) => running.0.push(app),
        Err(error) => {
            println!("could not start the app: {error}");
            return ExitCode::FAILURE;
        }
    }
    // Running once it answers the command line; the bindings go to the
    // extension as soon as it sees the extension, a moment later.
    if !wait_for(Duration::from_secs(20), control::is_running) {
        println!(
            "the app never started; see its log under {}",
            root.display()
        );
        return ExitCode::FAILURE;
    }
    std::thread::sleep(Duration::from_secs(2));
    let checked = std::env::current_exe().and_then(|this| {
        Command::new(this)
            .env("WAYLAND_DISPLAY", TEST_DISPLAY)
            .status()
    });
    control::send_control(control::Control::Quit);
    match checked {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(error) => {
            println!("could not run the check: {error}");
            ExitCode::FAILURE
        }
    }
}

fn log(root: &Path, name: &str) -> Stdio {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join(name))
        .map_or_else(|_| Stdio::null(), Stdio::from)
}

/// The processes the test session started, stopped when it ends.
struct Running(Vec<Child>);

impl Drop for Running {
    fn drop(&mut self) {
        // The app first, so it leaves before the shell it talks to.
        for child in self.0.iter_mut().rev() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
