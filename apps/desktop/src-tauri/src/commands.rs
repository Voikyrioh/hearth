//! Commandes exposées à l'interface (liste blanche : `build.rs` et
//! `capabilities/default.json`). Aucune ne touche au réseau.

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use tauri::{AppHandle, State};

use crate::alerts::Alerts;
use crate::link::LinkRuntime;

use crate::domain::{
    FRONTEND_MESSAGE_MAX_CHARS, FrontendErrorLimiter, Settings, single_line, truncate_chars,
};
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
            source = %single_line(&truncate_chars(&source, 64)),
            message = %single_line(&truncate_chars(&message, FRONTEND_MESSAGE_MAX_CHARS)),
            "erreur de l'interface"
        );
    }
}

/// Réglage « Notifier quand un serveur devient hors ligne ou revient » (BR-RESIL-015) : activé par défaut.
#[tauri::command]
#[specta::specta]
pub fn get_notify_on_link_change(app: AppHandle) -> Result<bool, AppError> {
    settings::notify_on_link_change(&app, Path::new(settings::STORE_FILE))
}

#[tauri::command]
#[specta::specta]
pub fn set_notify_on_link_change(
    app: AppHandle,
    alerts: State<'_, Arc<Alerts>>,
    enabled: bool,
) -> Result<bool, AppError> {
    let now = settings::set_notify_on_link_change(&app, Path::new(settings::STORE_FILE), enabled)?;
    alerts.set_enabled(now);
    Ok(now)
}

/// Le serveur affiché dans la fenêtre (`None` : aucun, réglages par exemple) : l'icône de la zone de
/// notification reflète son état (BR-RESIL-016).
#[tauri::command]
#[specta::specta]
pub fn set_displayed_server(
    alerts: State<'_, Arc<Alerts>>,
    link: State<'_, Arc<LinkRuntime>>,
    server_id: Option<String>,
) {
    alerts.set_displayed(link.known_server(server_id).as_deref());
}
