//! Commandes exposées à l'interface (liste blanche : `build.rs` et
//! `capabilities/default.json`). Aucune ne touche au réseau.

use tauri::AppHandle;

use crate::domain::Settings;
use crate::error::AppError;
use crate::settings;

#[tauri::command]
#[specta::specta]
pub fn get_settings(app: AppHandle) -> Result<Settings, AppError> {
    settings::read(&app)
}

#[tauri::command]
#[specta::specta]
pub fn set_launch_at_startup(app: AppHandle, enabled: bool) -> Result<Settings, AppError> {
    settings::set_launch_at_startup(&app, enabled)
}

#[tauri::command]
#[specta::specta]
pub fn get_app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}
