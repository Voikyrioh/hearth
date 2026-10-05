//! Commandes exposées à l'interface (liste blanche : `build.rs` et
//! `capabilities/default.json`). Aucune ne touche au réseau.

use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use tauri::AppHandle;

use crate::domain::{FRONTEND_MESSAGE_MAX_CHARS, FrontendErrorLimiter, Settings, truncate_chars};
use crate::error::AppError;
use crate::{logging, settings};

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

#[tauri::command]
#[specta::specta]
pub fn open_logs_folder() -> Result<(), AppError> {
    logging::open_log_dir()
}

/// Erreur de l'interface (gestionnaire global Vue, rejet de promesse) à écrire au
/// journal du client. Jamais d'erreur en retour : l'interface ne doit pas
/// échouer en rapportant un échec. Message borné en taille, débit limité.
#[tauri::command]
#[specta::specta]
pub fn log_frontend_error(source: String, message: String) {
    static LIMITER: Mutex<Option<FrontendErrorLimiter>> = Mutex::new(None);
    static START: OnceLock<Instant> = OnceLock::new();
    let now = START.get_or_init(Instant::now).elapsed().as_secs();
    let allowed = LIMITER
        .lock()
        .map(|mut guard| {
            guard
                .get_or_insert_with(FrontendErrorLimiter::default)
                .allow(now)
        })
        .unwrap_or(false);
    if allowed {
        tracing::error!(
            source = %truncate_chars(&source, 64),
            message = %truncate_chars(&message, FRONTEND_MESSAGE_MAX_CHARS),
            "erreur de l'interface"
        );
    }
}
