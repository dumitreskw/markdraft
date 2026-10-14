//! Settings > System: the `markdraft` command and folder menus.

use crate::error::{CmdError, blocking};
use crate::integration::{self, Feature, IntegrationStatus};

#[tauri::command]
pub async fn integration_status() -> Result<IntegrationStatus, CmdError> {
    blocking(|| Ok(integration::status(&integration::app_exe()?))).await
}

#[tauri::command]
pub async fn set_cli_on_path(enable: bool) -> Result<Feature, CmdError> {
    blocking(move || Ok(integration::set_cli(&integration::app_exe()?, enable)?)).await
}

#[tauri::command]
pub async fn set_context_menu(enable: bool) -> Result<Feature, CmdError> {
    blocking(move || {
        Ok(integration::set_context_menu(
            &integration::app_exe()?,
            enable,
        )?)
    })
    .await
}
