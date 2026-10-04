//! Réglages locaux. Deux sources indépendantes : l'entrée de démarrage de
//! Windows (greffon autostart, source de vérité de « Lancer Hearth au démarrage
//! de Windows ») et un fichier du greffon store pour ce qui reste interne
//! (explication de fermeture déjà montrée). Un fichier illisible ne bloque
//! jamais le réglage de démarrage.

use std::path::Path;

use serde_json::json;
use tauri::{AppHandle, Runtime};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_store::StoreExt as _;

use crate::domain::{Settings, flag_from};
use crate::error::AppError;

/// Fichier des réglages internes, dans le dossier de données de l'application.
pub const STORE_FILE: &str = "settings.json";
const KEY_CLOSE_HINT_SEEN: &str = "closeHintSeen";

/// Entrée de démarrage de Windows (port, pour pouvoir la simuler en test).
pub trait Autostart {
    fn is_enabled(&self) -> Result<bool, AppError>;
    fn set_enabled(&self, enabled: bool) -> Result<(), AppError>;
}

fn store_error(error: impl std::fmt::Display) -> AppError {
    AppError::Store(error.to_string())
}

fn autostart_error(error: impl std::fmt::Display) -> AppError {
    AppError::Autostart(error.to_string())
}

impl<R: Runtime> Autostart for AppHandle<R> {
    fn is_enabled(&self) -> Result<bool, AppError> {
        self.autolaunch().is_enabled().map_err(autostart_error)
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), AppError> {
        let manager = self.autolaunch();
        if enabled {
            manager.enable().map_err(autostart_error)
        } else {
            manager.disable().map_err(autostart_error)
        }
    }
}

/// Réglages courants (ne dépend que de l'entrée de démarrage).
pub fn read(autostart: &dyn Autostart) -> Result<Settings, AppError> {
    Ok(Settings {
        launch_at_startup: autostart.is_enabled()?,
    })
}

/// Active ou désactive le lancement au démarrage de Windows (BR-CLIENT-007),
/// puis relit l'état réel.
pub fn set_launch_at_startup(
    autostart: &dyn Autostart,
    enabled: bool,
) -> Result<Settings, AppError> {
    autostart.set_enabled(enabled)?;
    read(autostart)
}

/// L'explication de fermeture a-t-elle déjà été demandée au système ?
pub fn close_hint_seen<R: Runtime>(app: &AppHandle<R>, file: &Path) -> Result<bool, AppError> {
    let store = app.store(file).map_err(store_error)?;
    Ok(flag_from(store.get(KEY_CLOSE_HINT_SEEN).as_ref()))
}

/// Mémorise que l'explication a été demandée au système (BR-CLIENT-005).
pub fn mark_close_hint_seen<R: Runtime>(app: &AppHandle<R>, file: &Path) -> Result<(), AppError> {
    let store = app.store(file).map_err(store_error)?;
    store.set(KEY_CLOSE_HINT_SEEN, json!(true));
    store.save().map_err(store_error)
}
