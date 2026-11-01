//! `universal-tiling-window-manager`, the command line: with no command it
//! runs the tray app, or where a Windows release's tray app sits beside it,
//! starts that.

// Release builds have no console window; `check` and `probe` attach to the
// terminal that launched them instead.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> std::process::ExitCode {
    universal_tiling_window_manager::main(universal_tiling_window_manager::Executable::CommandLine)
}
