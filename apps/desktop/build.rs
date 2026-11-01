use std::path::Path;

fn main() {
    // The app has no webview and so no capabilities. Left to its default,
    // tauri-build watches a `capabilities` folder that doesn't exist, which
    // Cargo counts as changed on every build, rebuilding the app each time;
    // naming the pattern here leaves the watch out.
    //
    // On Windows the resource tauri-build links into every executable (the
    // icon and the version) carries no manifest: each executable gets its
    // own below, since Windows reads one per executable and the two ask for
    // different things.
    let attributes = tauri_build::Attributes::new()
        .capabilities_path_pattern("./capabilities/**/*")
        .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
    if let Err(error) = tauri_build::try_build(attributes) {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
    if std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows") {
        embed_manifest(
            "universal-tiling-window-manager",
            "windows/command-line.exe.manifest",
        );
        // The tray app's own executable, with UI Access, is a release's: the
        // feature that builds it is on only in the release job.
        if std::env::var_os("CARGO_FEATURE_UI_ACCESS").is_some() {
            embed_manifest("window-manager", "windows/tray.exe.manifest");
        }
    }
}

/// Links `manifest` into the executable `bin` as its application manifest,
/// resource 1 of type 24, where Windows looks for it.
fn embed_manifest(bin: &str, manifest: &str) {
    println!("cargo:rerun-if-changed={manifest}");
    let Some(out_dir) = std::env::var_os("OUT_DIR") else {
        eprintln!("no OUT_DIR for the {bin} manifest");
        std::process::exit(1);
    };
    let manifest = match std::path::absolute(manifest) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("can't find {manifest}: {error}");
            std::process::exit(1);
        }
    };
    // Resource scripts take C string escapes, so the backslashes double.
    let script = format!(
        "1 24 \"{}\"\n",
        manifest.display().to_string().replace('\\', "\\\\")
    );
    let rc = Path::new(&out_dir).join(format!("{bin}-manifest.rc"));
    if let Err(error) = std::fs::write(&rc, script) {
        eprintln!("can't write {}: {error}", rc.display());
        std::process::exit(1);
    }
    if let Err(error) =
        embed_resource::compile_for(&rc, [bin], embed_resource::NONE).manifest_required()
    {
        eprintln!("can't compile the {bin} manifest: {error}");
        std::process::exit(1);
    }
}
