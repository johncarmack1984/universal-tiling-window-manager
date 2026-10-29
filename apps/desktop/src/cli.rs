//! Command-line parsing and the subcommands that don't need the tray.

use std::path::PathBuf;
use std::process::ExitCode;

pub const USAGE: &str = "\
usage: universal-tiling-window-manager [--config <export.json>] [command]

With no command, runs in the tray (the menu bar on the Mac).

commands:
  check [export.json]   show what each shortcut becomes on this machine
  config [export.json]  show or choose the configuration this machine uses
  config --default      look for the configuration in the usual places again
  probe [seconds]       log every key the keyboard delivers (default 60s; Windows)
  reload                make the running instance re-read its configuration
  pause                 pause or resume the running instance's shortcuts
  quit                  stop the running instance
  update                make the running instance install the newest release
  install               put this copy in ~/.local/bin and start it (Linux)
  version               print the version

Configuration lives in ~/.config/window-manager: config.json, a Rectangle Pro
export, and config.toml, the app's own settings. `config <export.json>` or the
menu's Choose configuration… uses another export on this machine from then on;
--config or UTWM_CONFIG names one for a single run.";

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Run,
    Check,
    Config(ConfigAction),
    Probe { seconds: u64 },
    Reload,
    Pause,
    Quit,
    Update,
    Install,
    Version,
    Help,
}

/// What `config` does with the configuration this machine uses.
#[derive(Debug, PartialEq, Eq)]
pub enum ConfigAction {
    /// Shows the one chosen, if any.
    Show,
    /// Uses this export from now on.
    Choose(PathBuf),
    /// Forgets the choice, so the usual places count again.
    Default,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Invocation {
    pub command: Command,
    /// `--config`, or `check`'s argument, or `UTWM_CONFIG`.
    pub config: Option<PathBuf>,
}

pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Invocation, String> {
    let mut config: Option<PathBuf> = None;
    let mut positional: Vec<String> = Vec::new();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" | "-c" => {
                let path = args.next().ok_or("--config needs a path")?;
                config = Some(PathBuf::from(path));
            }
            "--help" | "-h" => positional.insert(0, "help".to_owned()),
            "--version" | "-V" => positional.insert(0, "version".to_owned()),
            _ => positional.push(arg),
        }
    }
    let mut positional = positional.into_iter();
    let command = match positional.next().as_deref() {
        None | Some("run") => Command::Run,
        Some("check") => {
            if let Some(path) = positional.next() {
                config = Some(PathBuf::from(path));
            }
            Command::Check
        }
        Some("config") => Command::Config(match positional.next() {
            None => ConfigAction::Show,
            Some(arg) if arg == "--default" => ConfigAction::Default,
            Some(path) => ConfigAction::Choose(PathBuf::from(path)),
        }),
        Some("probe") => {
            let seconds = match positional.next() {
                Some(text) => text
                    .parse()
                    .map_err(|_| format!("probe takes a number of seconds, not `{text}`"))?,
                None => 60,
            };
            Command::Probe { seconds }
        }
        Some("reload") => Command::Reload,
        Some("pause") => Command::Pause,
        Some("quit") => Command::Quit,
        Some("update") => Command::Update,
        Some("install") => Command::Install,
        Some("version") => Command::Version,
        Some("help") => Command::Help,
        Some(other) => return Err(format!("unknown command `{other}`")),
    };
    if let Some(extra) = positional.next() {
        return Err(format!("unexpected argument `{extra}`"));
    }
    let config = config.or_else(|| std::env::var_os("UTWM_CONFIG").map(PathBuf::from));
    Ok(Invocation { command, config })
}

/// Prints to the launching terminal, attaching to it first in release builds.
pub fn print_to_terminal(text: &str) {
    #[cfg(windows)]
    utwm_windows::console::attach();
    println!("{text}");
}

pub fn run(invocation: Invocation) -> ExitCode {
    match invocation.command {
        Command::Help => {
            print_to_terminal(USAGE);
            ExitCode::SUCCESS
        }
        Command::Version => {
            print_to_terminal(env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Command::Check => check(invocation.config),
        Command::Config(action) => configure(action),
        #[cfg(target_os = "linux")]
        Command::Install => install(),
        #[cfg(not(target_os = "linux"))]
        Command::Install => {
            print_to_terminal(
                "install is for Linux; the installer installs the app on Windows, and on the Mac it goes in /Applications.",
            );
            ExitCode::FAILURE
        }
        #[cfg(windows)]
        Command::Probe { seconds } => probe(invocation.config, seconds),
        #[cfg(not(windows))]
        Command::Probe { .. } => {
            print_to_terminal(
                "probe is for Windows, where a keyboard hook sees every key; elsewhere the system only reports the shortcuts it grabbed, and `check` lists them.",
            );
            ExitCode::FAILURE
        }
        #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
        Command::Reload => control(crate::platform::Control::Reload),
        #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
        Command::Pause => control(crate::platform::Control::TogglePause),
        #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
        Command::Quit => control(crate::platform::Control::Quit),
        #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
        Command::Update => control(crate::platform::Control::Update),
        #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
        Command::Run => match crate::app::run(invocation.config) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                print_to_terminal(&format!("universal-tiling-window-manager: {error}"));
                ExitCode::FAILURE
            }
        },
        #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
        _ => {
            print_to_terminal(
                "universal-tiling-window-manager runs on Windows, macOS and Linux; `check` works anywhere.",
            );
            ExitCode::FAILURE
        }
    }
}

fn home() -> Option<PathBuf> {
    std::env::home_dir()
}

fn check(config: Option<PathBuf>) -> ExitCode {
    let Some(home) = home() else {
        print_to_terminal("no home directory");
        return ExitCode::FAILURE;
    };
    let named = utwm_config::load::named(&home, config.as_deref());
    match utwm_config::load(&home, named.as_deref()) {
        Ok(resolved) => {
            print_to_terminal(utwm_config::report::render(&resolved).trim_end());
            ExitCode::SUCCESS
        }
        Err(error) => {
            print_to_terminal(&format!("error: {error}"));
            ExitCode::FAILURE
        }
    }
}

/// Shows, chooses or forgets the configuration this machine uses. A choice
/// is checked before it is remembered, and the running app reloads at once.
fn configure(action: ConfigAction) -> ExitCode {
    let Some(home) = home() else {
        print_to_terminal("no home directory");
        return ExitCode::FAILURE;
    };
    let file = utwm_config::load::chosen_file(&home);
    let path = match action {
        ConfigAction::Show => {
            print_to_terminal(&match utwm_config::load::read_chosen(&file) {
                Some(path) => format!("{} (chosen for this machine)", path.display()),
                None => "None chosen for this machine; `check` shows where the configuration comes from."
                    .to_owned(),
            });
            return ExitCode::SUCCESS;
        }
        ConfigAction::Choose(path) => {
            let path = utwm_config::overrides::expand_home(&path.to_string_lossy(), &home);
            let path = std::path::absolute(&path).unwrap_or(path);
            match utwm_config::load(&home, Some(&path)) {
                Ok(resolved) => print_to_terminal(&format!(
                    "Chosen for this machine: {} shortcuts {}",
                    resolved.bindings.len(),
                    resolved.source.describe()
                )),
                Err(error) => {
                    print_to_terminal(&format!("error: {error}"));
                    return ExitCode::FAILURE;
                }
            }
            Some(path)
        }
        ConfigAction::Default => {
            print_to_terminal(
                "This machine looks in the usual places again; `check` shows what it finds.",
            );
            None
        }
    };
    if let Err(error) = utwm_config::load::write_chosen(&file, path.as_deref()) {
        print_to_terminal(&format!("could not remember the choice: {error}"));
        return ExitCode::FAILURE;
    }
    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
    let _ = crate::platform::send_control(crate::platform::Control::Reload);
    ExitCode::SUCCESS
}

/// Installs this copy for the user, as a release's download is installed:
/// in `~/.local/bin`, with its desktop entry, icons and GNOME Shell
/// extension, replacing a running copy, and started.
#[cfg(target_os = "linux")]
fn install() -> ExitCode {
    let installed = match std::env::current_exe() {
        Ok(source) => utwm_linux::install::install(&source, crate::platform::ICONS),
        Err(error) => Err(error.into()),
    };
    match installed {
        Ok(installed) => {
            print_to_terminal(&format!("installed {} and started it", installed.display()));
            if let Some(advice) = utwm_linux::install::advice() {
                print_to_terminal(advice);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            print_to_terminal(&format!("could not install: {error}"));
            ExitCode::FAILURE
        }
    }
}

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
fn control(control: crate::platform::Control) -> ExitCode {
    if crate::platform::send_control(control) {
        ExitCode::SUCCESS
    } else {
        print_to_terminal("universal-tiling-window-manager is not running");
        ExitCode::FAILURE
    }
}

#[cfg(windows)]
fn probe(config: Option<PathBuf>, seconds: u64) -> ExitCode {
    use std::time::{Duration, Instant};
    use utwm_config::matcher::Decision;
    use utwm_windows::hook::{Hook, HookEvent, Mode};

    utwm_windows::console::attach();
    let Some(home) = home() else {
        println!("no home directory");
        return ExitCode::FAILURE;
    };
    let named = utwm_config::load::named(&home, config.as_deref());
    let resolved = match utwm_config::load(&home, named.as_deref()) {
        Ok(resolved) => resolved,
        Err(error) => {
            println!("error: {error}");
            return ExitCode::FAILURE;
        }
    };
    let chords = resolved
        .bindings
        .iter()
        .map(|binding| binding.chord)
        .collect();
    let (sender, receiver) = std::sync::mpsc::channel();
    let hook = Hook::start(
        Mode::Probe,
        chords,
        Box::new(move |event| {
            let _ = sender.send(event);
        }),
    );
    let _hook = match hook {
        Ok(hook) => hook,
        Err(error) => {
            println!("could not install the keyboard hook: {error}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "Logging keys for {seconds}s. Nothing is swallowed. Bindings: {}",
        resolved.source.describe()
    );
    let started = Instant::now();
    let deadline = started + Duration::from_secs(seconds);
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        let Ok(HookEvent::Probed(probe)) = receiver.recv_timeout(left) else {
            continue;
        };
        let outcome = match probe.decision {
            Decision::Trigger { index, .. } => {
                resolved
                    .bindings
                    .get(index)
                    .map_or_else(String::new, |binding| {
                        let names: Vec<&str> = binding
                            .entries
                            .iter()
                            .map(|entry| entry.name.as_str())
                            .collect();
                        format!("  → {} ({})", names.join(" / "), binding.chord)
                    })
            }
            Decision::Swallow | Decision::Pass => String::new(),
        };
        println!(
            "{:8.3}s {} {:<11} scan {:#04x}{} vk {:#04x} held {}{}{}",
            started.elapsed().as_secs_f64(),
            if probe.down { "↓" } else { "↑" },
            probe.key.windows_name(),
            probe.key.scan,
            if probe.key.extended { " ext" } else { "    " },
            probe.vk,
            if probe.modifiers.is_empty() {
                "-".to_owned()
            } else {
                probe.modifiers.to_string()
            },
            if probe.injected {
                format!(" injected by {:#x}", probe.extra_info)
            } else {
                String::new()
            },
            outcome
        );
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_args(args: &[&str]) -> Result<Invocation, String> {
        parse(args.iter().map(|arg| (*arg).to_owned()))
    }

    #[test]
    fn no_arguments_runs_the_tray() {
        assert_eq!(parse_args(&[]).map(|i| i.command), Ok(Command::Run));
    }

    #[test]
    fn check_takes_an_optional_path() {
        let invocation = parse_args(&["check", "x.json"]).expect("parses");
        assert_eq!(invocation.command, Command::Check);
        assert_eq!(invocation.config, Some(PathBuf::from("x.json")));
    }

    #[test]
    fn probe_takes_seconds() {
        assert_eq!(
            parse_args(&["probe", "30"]).map(|i| i.command),
            Ok(Command::Probe { seconds: 30 })
        );
        assert!(parse_args(&["probe", "soon"]).is_err());
    }

    #[test]
    fn config_flag_and_errors() {
        let invocation = parse_args(&["--config", "y.json", "reload"]).expect("parses");
        assert_eq!(invocation.command, Command::Reload);
        assert_eq!(invocation.config, Some(PathBuf::from("y.json")));
        assert!(parse_args(&["frobnicate"]).is_err());
        assert!(parse_args(&["quit", "now"]).is_err());
        assert!(parse_args(&["--config"]).is_err());
    }

    #[test]
    fn config_shows_chooses_or_goes_back_to_the_default() {
        assert_eq!(
            parse_args(&["config"]).map(|i| i.command),
            Ok(Command::Config(ConfigAction::Show))
        );
        assert_eq!(
            parse_args(&["config", "x.json"]).map(|i| i.command),
            Ok(Command::Config(ConfigAction::Choose(PathBuf::from(
                "x.json"
            ))))
        );
        assert_eq!(
            parse_args(&["config", "--default"]).map(|i| i.command),
            Ok(Command::Config(ConfigAction::Default))
        );
        assert!(parse_args(&["config", "a.json", "b.json"]).is_err());
    }

    #[test]
    fn install_takes_nothing() {
        assert_eq!(
            parse_args(&["install"]).map(|i| i.command),
            Ok(Command::Install)
        );
        assert!(parse_args(&["install", "/opt"]).is_err());
    }
}
