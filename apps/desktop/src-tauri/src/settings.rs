//! Accès aux réglages locaux : fichier `settings.json` (greffon store) et
//! entrée de démarrage de Windows (greffon autostart, source de vérité du
//! réglage « Lancer Hearth au démarrage de Windows »).

use serde_json::json;
use tauri::{AppHandle, Runtime};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_store::StoreExt as _;

use crate::domain::{Settings, flag_from};
use crate::error::AppError;

const STORE_FILE: &str = "settings.json";
const KEY_CLOSE_HINT_SEEN: &str = "closeHintSeen";

fn store_error(error: impl std::fmt::Display) -> AppError {
    AppError::Store(error.to_string())
}

fn autostart_error(error: impl std::fmt::Display) -> AppError {
    AppError::Autostart(error.to_string())
}

/// L'explication de fermeture a-t-elle déjà été montrée ?
pub fn close_hint_seen<R: Runtime>(app: &AppHandle<R>) -> Result<bool, AppError> {
    let store = app.store(STORE_FILE).map_err(store_error)?;
    Ok(flag_from(store.get(KEY_CLOSE_HINT_SEEN).as_ref()))
}

/// Mémorise que l'explication a été montrée (BR-CLIENT-005).
pub fn mark_close_hint_seen<R: Runtime>(app: &AppHandle<R>) -> Result<(), AppError> {
    let store = app.store(STORE_FILE).map_err(store_error)?;
    store.set(KEY_CLOSE_HINT_SEEN, json!(true));
    store.save().map_err(store_error)
}

/// Réglages courants.
pub fn read<R: Runtime>(app: &AppHandle<R>) -> Result<Settings, AppError> {
    Ok(Settings {
        launch_at_startup: app.autolaunch().is_enabled().map_err(autostart_error)?,
        close_hint_seen: close_hint_seen(app)?,
    })
}

/// Active ou désactive le lancement au démarrage de Windows (BR-CLIENT-007).
pub fn set_launch_at_startup<R: Runtime>(
    app: &AppHandle<R>,
    enabled: bool,
) -> Result<Settings, AppError> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(autostart_error)?;
    } else {
        manager.disable().map_err(autostart_error)?;
    }
    read(app)
}
