//! Updates: checks the release feed and installs signed updates in place.
//!
//! Copies the updater can replace (the Windows setup or MSI install, the
//! macOS app, an AppImage) update themselves. Portable copies, `.deb`/`.rpm`
//! installs and development builds only learn that a new version exists and
//! link to the release page, so a package manager or the user stays in charge.

use std::sync::Mutex;

use serde::Serialize;
use tauri::utils::config::BundleType;
use tauri::utils::platform::bundle_type;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::error::CmdError;
use crate::events::{self, UpdateProgress};

/// Where people download releases by hand.
pub const RELEASES_URL: &str = "https://github.com/dumitreskw/markdraft/releases";

/// The update found by the last check, kept for `install_update`.
#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<Update>>);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    /// RFC 3339 publish date, when the feed has one.
    pub date: Option<String>,
    pub notes: Option<String>,
    /// False when this copy cannot replace itself (see the module docs).
    pub can_install: bool,
    /// The release page, for the notes or a manual download.
    pub page_url: String,
}

/// Whether this copy can install updates over itself.
fn self_updating() -> bool {
    if cfg!(debug_assertions) || crate::portable_data_dir().is_some() {
        return false;
    }
    match bundle_type() {
        Some(BundleType::Nsis | BundleType::Msi | BundleType::App) => true,
        Some(BundleType::AppImage) => std::env::var_os("APPIMAGE").is_some(),
        _ => false,
    }
}

fn update_error(e: tauri_plugin_updater::Error) -> CmdError {
    use tauri_plugin_updater::Error;
    match e {
        Error::Reqwest(_) | Error::Network(_) => {
            CmdError::Io(format!("Could not reach the update server: {e}"))
        }
        Error::Minisign(_)
        | Error::SignatureUtf8(_)
        | Error::Base64(_)
        | Error::SignedVersionMismatch { .. }
        | Error::MissingSignedVersion => CmdError::NotAllowed(format!(
            "The update is not signed by MarkDraft and was not installed: {e}"
        )),
        _ => CmdError::Internal(e.to_string()),
    }
}

fn lock(pending: &PendingUpdate) -> std::sync::MutexGuard<'_, Option<Update>> {
    pending.0.lock().unwrap_or_else(|p| p.into_inner())
}

/// Asks the release feed for a newer version. `None` when up to date.
#[tauri::command]
pub async fn check_update(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
) -> Result<Option<UpdateInfo>, CmdError> {
    let found = app
        .updater()
        .map_err(update_error)?
        .check()
        .await
        .map_err(update_error)?;
    let info = found.as_ref().map(|u| UpdateInfo {
        version: u.version.clone(),
        current_version: u.current_version.clone(),
        date: u.raw_json["pub_date"].as_str().map(str::to_owned),
        notes: u.body.clone().filter(|b| !b.trim().is_empty()),
        can_install: self_updating(),
        page_url: format!("{RELEASES_URL}/tag/v{}", u.version),
    });
    *lock(&pending) = found;
    Ok(info)
}

/// Downloads the update found by `check_update` (emitting progress), checks
/// its signature, installs it and restarts. On Windows the installer takes
/// over and this process exits, so the UI saves everything first.
#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
) -> Result<(), CmdError> {
    if !self_updating() {
        return Err(CmdError::NotAllowed(
            "This copy of MarkDraft cannot update itself. Download the new version instead.".into(),
        ));
    }
    let update = lock(&pending)
        .clone()
        .ok_or_else(|| CmdError::Invalid("Check for updates first.".into()))?;
    let mut downloaded = 0u64;
    let bytes = update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = app.emit(
                    events::UPDATE_PROGRESS,
                    UpdateProgress { downloaded, total },
                );
            },
            || {},
        )
        .await
        .map_err(update_error)?;
    tracing::info!("installing MarkDraft {}", update.version);
    update.install(bytes).map_err(update_error)?;
    app.restart()
}
