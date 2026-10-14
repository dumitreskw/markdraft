//! MarkDraft desktop app: wires the core crates to the WebView through
//! commands and events. No business logic lives here.

mod commands;
mod error;
mod events;
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

    tauri::Builder::default()
        // Must be registered first so a second launch exits before doing work.
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            if let Some(path) = parse_cli(argv).path {
                let req = OpenRequest {
                    path: absolutize(&path, Path::new(&cwd)),
                };
                if let Err(e) = app.emit(events::OPEN_PATH, req) {
                    tracing::error!("failed to forward open request: {e}");
                }
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new(initial))
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::pick_path,
            commands::take_open_request,
        ])
        .run(tauri::generate_context!())?;
    Ok(())
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
