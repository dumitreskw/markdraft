//! MarkDraft desktop app: wires the core crates to the WebView through
//! commands and events. No business logic lives here.

mod commands;
mod error;
mod events;
mod protocol;
mod roots;
mod state;

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use clap::Parser;
use tauri::{Emitter, Manager};
use tracing_subscriber::EnvFilter;

use crate::events::OpenRequest;
use crate::state::AppState;

/// Parses process arguments, tolerating unknown flags that some platforms
/// inject (for example macOS `-psn_*`). `--help` and `--version` exit.
pub fn parse_cli<I, T>(args: I) -> cli::Cli
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    match cli::Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(e) if !e.use_stderr() => e.exit(),
        Err(e) => {
            tracing::warn!("ignoring command-line arguments: {e}");
            cli::Cli {
                path: None,
                command: None,
            }
        }
    }
}

/// Starts the app, optionally opening `path` once the UI asks for it.
pub fn run(path: Option<PathBuf>) -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let cwd = std::env::current_dir()?;
    let initial = path.map(|p| OpenRequest {
        path: absolutize(&p, &cwd),
    });

    let app = tauri::Builder::default()
        // Must be registered first so a second launch exits before doing work.
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            if let Some(path) = parse_cli(argv).path {
                forward_open(app, absolutize(&path, Path::new(&cwd)));
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .register_asynchronous_uri_scheme_protocol(protocol::SCHEME, protocol::handle)
        .manage(AppState::new(initial))
        .setup(|_| {
            // Load syntax definitions before the first document is rendered.
            std::thread::spawn(md_core::warm_up);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app::app_info,
            commands::app::pick_path,
            commands::app::pick_save_path,
            commands::app::take_open_request,
            commands::app::get_settings,
            commands::app::set_settings,
            commands::app::list_recents,
            commands::app::remove_recent,
            commands::app::get_session,
            commands::app::set_session,
            commands::app::highlight_css,
            commands::app::open_external,
            commands::app::reveal_path,
            commands::roots::open_path,
            commands::roots::add_root,
            commands::roots::remove_root,
            commands::roots::rescan_root,
            commands::roots::current_roots,
            commands::roots::save_workspace,
            commands::docs::read_file,
            commands::docs::save_file,
            commands::docs::render_markdown,
            commands::docs::create_entry,
            commands::docs::rename_entry,
            commands::docs::trash_entry,
            commands::docs::duplicate_entry,
            commands::docs::write_export,
            commands::docs::save_image,
            commands::docs::import_image,
            commands::export::export_pdf,
            commands::export::export_folder_pdf,
            commands::search::quick_open,
            commands::search::search_text,
            commands::search::backlinks,
            commands::git::git_info,
            commands::git::git_status,
            commands::git::git_history,
            commands::git::diff_document,
        ])
        .build(tauri::generate_context!())?;

    app.run(|_app, _event| {
        // macOS delivers files opened from Finder as an event, not argv.
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Opened { urls } = _event {
            for path in urls.iter().filter_map(|u| u.to_file_path().ok()) {
                forward_open(_app, path);
            }
        }
    });
    Ok(())
}

/// Hands a path to the UI: queued if the UI has not started yet, and
/// emitted for a UI that is already listening.
fn forward_open(app: &tauri::AppHandle, path: PathBuf) {
    let req = OpenRequest { path };
    app.state::<AppState>().queue_open(req.clone());
    if let Err(e) = app.emit(events::OPEN_PATH, req) {
        tracing::error!("failed to forward open request: {e}");
    }
}

fn absolutize(path: &Path, cwd: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        cwd.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_flags_are_ignored() {
        let cli = parse_cli(["markdraft", "-psn_0_12345"]);
        assert_eq!(cli.path, None);
    }

    #[test]
    fn path_argument_is_kept() {
        let cli = parse_cli(["markdraft", "notes.md"]);
        assert_eq!(cli.path, Some(PathBuf::from("notes.md")));
    }

    #[test]
    fn relative_paths_resolve_against_cwd() {
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(absolutize(Path::new("docs"), &cwd), cwd.join("docs"));
        assert_eq!(absolutize(&cwd, Path::new("ignored")), cwd);
    }
}
