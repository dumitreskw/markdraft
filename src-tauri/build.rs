fn main() {
    // Declaring the app's commands generates one permission per command, so
    // the capability files decide exactly which commands the WebView may call.
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&["app_info", "pick_path", "take_open_request"]),
    ))
    .expect("failed to run tauri-build");
}
