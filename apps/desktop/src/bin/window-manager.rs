//! `Window Manager.exe`, a Windows release's tray app: the app with UI Access
//! in its manifest, which lets it move windows of apps running as
//! administrator. The command line beside it starts it.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> std::process::ExitCode {
    universal_tiling_window_manager::main(universal_tiling_window_manager::Executable::Tray)
}
