//! Commandes de la mise à jour du client exposées à l'interface (liste blanche : `build.rs` et
//! `capabilities/default.json`). Aucune ne prend d'adresse, de chemin ni de clé : l'interface ne
//! fait que demander, la coquille décide et fait. Aucune n'échoue : un échec est un état
//! (`failure`), pas une exception.

use tauri::{AppHandle, Manager as _, State};

use super::SharedUpdates;
use super::dto::UpdateStateDto;

/// L'état courant (au démarrage de l'interface, et pour rattraper un événement manqué).
#[tauri::command]
#[specta::specta]
pub fn get_update_state(updates: State<'_, SharedUpdates>) -> UpdateStateDto {
    updates.state()
}

/// « Vérifier maintenant » (BR-UPDATE-026).
#[tauri::command]
#[specta::specta]
pub async fn check_for_updates(app: AppHandle) -> UpdateStateDto {
    let service = app.state::<SharedUpdates>().inner().clone();
    service.check_now().await
}

/// « Plus tard » : le bandeau disparaît 24 h (BR-UPDATE-006).
#[tauri::command]
#[specta::specta]
pub fn postpone_update(updates: State<'_, SharedUpdates>) -> UpdateStateDto {
    updates.postpone()
}

/// « Mettre à jour maintenant » : le seul chemin vers une installation (BR-UPDATE-002). Rend la
/// main tout de suite ; l'avancement et l'issue arrivent par `update://state`.
#[tauri::command]
#[specta::specta]
pub fn install_update(updates: State<'_, SharedUpdates>) -> UpdateStateDto {
    if updates.begin_install().is_ok() {
        let service = updates.inner().clone();
        tauri::async_runtime::spawn(async move { service.run_install().await });
    }
    updates.state()
}
