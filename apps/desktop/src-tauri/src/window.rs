//! Fenêtre principale : afficher, cacher, réagir à la fermeture.

use std::path::Path;

use tauri::{AppHandle, Manager, Runtime, Window, WindowEvent};
use tauri_plugin_notification::NotificationExt as _;

use crate::domain::{MAIN_WINDOW, hides_on_close, should_explain_close};
use crate::{settings, texts};

/// Ramène la fenêtre au premier plan, restaurée si elle était réduite ou
/// cachée (BR-CLIENT-003, BR-CLIENT-011).
pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        tracing::warn!("fenêtre principale introuvable");
        return;
    };
    if let Err(error) = window.unminimize() {
        tracing::warn!(%error, "restauration de la fenêtre impossible");
    }
    if let Err(error) = window.show() {
        tracing::warn!(%error, "affichage de la fenêtre impossible");
    }
    if let Err(error) = window.set_focus() {
        tracing::warn!(%error, "mise au premier plan impossible");
    }
}

/// La croix de la fenêtre principale la cache au lieu de quitter
/// (BR-CLIENT-004) ; les autres fenêtres se ferment normalement.
pub fn on_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if !hides_on_close(window.label()) {
        return;
    }
    if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let app = window.app_handle().clone();
        hide_to_tray(window, Path::new(settings::STORE_FILE), move || {
            notify_close_hint(&app);
        });
    }
}

/// Cache la fenêtre ; à la première fois seulement, demande l'explication
/// (`notify`) puis le mémorise dans `store_file` (BR-CLIENT-005). Si le fichier
/// des réglages est illisible, on se tait plutôt que de répéter l'explication.
pub fn hide_to_tray<R: Runtime>(window: &Window<R>, store_file: &Path, notify: impl FnOnce()) {
    if let Err(error) = window.hide() {
        tracing::warn!(%error, "fenêtre non cachée");
    }
    let app = window.app_handle();
    let seen = settings::close_hint_seen(app, store_file).unwrap_or_else(|error| {
        tracing::warn!(%error, "lecture de « explication déjà vue » impossible");
        true
    });
    if should_explain_close(seen) {
        notify();
        if let Err(error) = settings::mark_close_hint_seen(app, store_file) {
            tracing::warn!(%error, "mémorisation de l'explication impossible");
        }
    }
}

/// Demande la notification d'explication au système. Le greffon ne dit pas si
/// Windows l'a réellement affichée : « demandée » est tout ce qu'on peut garantir.
fn notify_close_hint<R: Runtime>(app: &AppHandle<R>) {
    if let Err(error) = app
        .notification()
        .builder()
        .title(texts::APP_NAME)
        .body(texts::CLOSE_HINT)
        .show()
    {
        tracing::warn!(%error, "notification d'explication refusée");
    }
}
