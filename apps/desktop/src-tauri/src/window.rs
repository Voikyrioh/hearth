//! Fenêtre principale : afficher, cacher, réagir à la fermeture.

use std::path::Path;
use std::sync::Arc;

use tauri::{AppHandle, Emitter as _, Manager, Runtime, Window, WindowEvent};
use tauri_plugin_notification::NotificationExt as _;

use crate::domain::{MAIN_WINDOW, hides_on_close, should_explain_close};
use crate::link::UiSink;
use crate::services::logs::LogSessions;
use crate::services::logs_dto::LogClosedCauseDto;
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
    // Une fenêtre détruite ne lit plus rien, quelle qu'elle soit (HRT-67, ADR-0040).
    if matches!(event, WindowEvent::Destroyed) {
        close_log_reads(window.app_handle(), window.label(), None);
        return;
    }
    if !hides_on_close(window.label()) {
        return;
    }
    if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        // La croix CACHE la fenêtre sans décharger la page : la lecture des journaux s'arrête ici, côté Rust,
        // les lignes sont effacées par la page à l'annonce `closed` (BR-SLOG-019, BR-SLOG-020).
        close_log_reads(
            window.app_handle(),
            window.label(),
            Some(LogClosedCauseDto::WindowHidden),
        );
        let app = window.app_handle().clone();
        hide_to_tray(window, Path::new(settings::STORE_FILE), move || {
            notify_close_hint(&app);
        });
    }
}

/// Ferme les lectures de journaux de cette fenêtre. `cause` : la page en est avertie (fenêtre cachée, elle vit
/// encore) ; `None` : fenêtre détruite, personne à avertir et plus aucune page ne s'annoncera.
pub fn close_log_reads<R: Runtime>(
    app: &AppHandle<R>,
    label: &str,
    cause: Option<LogClosedCauseDto>,
) {
    let Some(sessions) = app.try_state::<Arc<LogSessions>>() else {
        return;
    };
    match cause {
        Some(cause) => {
            sessions.close_window(label, cause);
        }
        None => {
            sessions.forget_window(label);
        }
    }
}

/// Cache la fenêtre ; à la première fois seulement, demande l'explication
/// (`notify`) puis le mémorise dans `store_file` (BR-CLIENT-005). Un fichier
/// corrompu est avalé par le greffon, qui rend « jamais vu » : l'explication est
/// alors redonnée une fois et le fichier réécrit. Seule une vraie erreur du
/// greffon (fichier impossible à ouvrir) fait taire l'explication, pour ne pas la
/// répéter à chaque fermeture.
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

/// Les événements d'UNE fenêtre : ils ne vont qu'à elle (par son étiquette), jamais aux autres fenêtres.
pub struct WindowSink<R: Runtime> {
    app: AppHandle<R>,
    label: String,
}

impl<R: Runtime> WindowSink<R> {
    pub fn new(app: AppHandle<R>, label: String) -> Self {
        Self { app, label }
    }
}

impl<R: Runtime> UiSink for WindowSink<R> {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        if let Err(error) = self.app.emit_to(self.label.as_str(), event, payload) {
            tracing::warn!(event, %error, "événement non livré à la fenêtre");
        }
    }
}
