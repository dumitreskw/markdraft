/// Every command the UI may call. Declaring them generates one permission
/// per command (`allow-<name>`), and `capabilities/default.json` grants
/// them explicitly, so nothing is callable by accident.
const COMMANDS: &[&str] = &[
    "app_info",
    "pick_path",
    "pick_save_path",
    "take_open_request",
    "get_settings",
    "set_settings",
    "list_recents",
    "remove_recent",
    "get_session",
    "set_session",
    "highlight_css",
    "open_external",
    "reveal_path",
    "open_path",
    "add_root",
    "remove_roots",
    "get_window_state",
    "set_window_state",
    "rescan_root",
    "current_roots",
    "save_workspace",
    "read_file",
    "save_file",
    "render_markdown",
    "create_entry",
    "rename_entry",
    "trash_entry",
    "duplicate_entry",
    "write_export",
    "save_image",
    "import_image",
    "export_pdf",
    "export_folder_pdf",
    "quick_open",
    "search_text",
    "backlinks",
    "git_info",
    "git_status",
    "git_history",
    "diff_document",
    "integration_status",
    "set_cli_on_path",
    "set_context_menu",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
}
