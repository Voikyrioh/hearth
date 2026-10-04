//! Fenêtre principale : afficher, cacher, réagir à la fermeture.

use tauri::{AppHandle, Manager, Runtime, Window, WindowEvent};
use tauri_plugin_notification::NotificationExt as _;

use crate::domain::{CloseOutcome, on_close_requested};
use crate::{settings, texts};

pub const MAIN_WINDOW: &str = "main";

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

/// La croix cache la fenêtre au lieu de quitter (BR-CLIENT-004) et explique
/// la première fois seulement (BR-CLIENT-005).
pub fn on_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    let WindowEvent::CloseRequested { api, .. } = event else {
        return;
    };
    api.prevent_close();
    if let Err(error) = window.hide() {
        tracing::warn!(%error, "fenêtre non cachée");
    }
    let app = window.app_handle();
    let seen = settings::close_hint_seen(app).unwrap_or_else(|error| {
        tracing::warn!(%error, "lecture de « explication déjà vue » impossible");
        true
    });
    if on_close_requested(seen) == CloseOutcome::HideAndExplain {
        explain_close(app);
    }
}

fn explain_close<R: Runtime>(app: &AppHandle<R>) {
    let shown = app
        .notification()
        .builder()
        .title(texts::APP_NAME)
        .body(texts::CLOSE_HINT)
        .show();
    match shown {
        Ok(()) => {
            if let Err(error) = settings::mark_close_hint_seen(app) {
                tracing::warn!(%error, "mémorisation de l'explication impossible");
            }
        }
        Err(error) => tracing::warn!(%error, "notification d'explication non affichée"),
    }
}
