fn main() {
    // The app has no webview and so no capabilities. Left to its default,
    // tauri-build watches a `capabilities` folder that doesn't exist, which
    // Cargo counts as changed on every build, rebuilding the app each time;
    // naming the pattern here leaves the watch out.
    let attributes =
        tauri_build::Attributes::new().capabilities_path_pattern("./capabilities/**/*");
    if let Err(error) = tauri_build::try_build(attributes) {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
