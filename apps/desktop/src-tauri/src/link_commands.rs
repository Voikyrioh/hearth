//! Commandes de liaison exposées à l'interface (liste blanche : `build.rs` et
//! `capabilities/default.json`). Chacune délègue à [`LinkRuntime`] ; les échecs sont typés
//! ([`LinkFailure`]), l'interface choisit son texte. Aucun mot de passe n'est journalisé.

use std::sync::Arc;

use tauri::{AppHandle, Emitter as _, Runtime, State};

use crate::link::{LinkRuntime, UiSink};
use crate::link_dto::{LinkFailure, LinkStateDto, LoginDto, ProbeDto, ServerDto};

/// Les événements de la liaison vont à la fenêtre.
pub struct TauriSink<R: Runtime>(pub AppHandle<R>);

impl<R: Runtime> UiSink for TauriSink<R> {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        if let Err(error) = self.0.emit(event, payload) {
            tracing::warn!(event, %error, "événement non livré à l'interface");
        }
    }
}

type Runtime_<'a> = State<'a, Arc<LinkRuntime>>;

#[tauri::command]
#[specta::specta]
pub fn list_servers(link: Runtime_<'_>) -> Vec<ServerDto> {
    link.servers()
}

#[tauri::command]
#[specta::specta]
pub fn list_link_states(link: Runtime_<'_>) -> Vec<LinkStateDto> {
    link.states()
}

#[tauri::command]
#[specta::specta]
pub async fn probe_server(
    link: Runtime_<'_>,
    host: String,
    port: Option<u16>,
) -> Result<ProbeDto, LinkFailure> {
    link.probe(&host, port).await
}

#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn add_server(
    app: AppHandle,
    link: Runtime_<'_>,
    name: String,
    color: u8,
    host: String,
    port: Option<u16>,
    fingerprint: String,
    mac_addresses: Vec<String>,
) -> Result<ServerDto, LinkFailure> {
    link.add_server(
        name,
        color,
        host,
        port,
        &fingerprint,
        mac_addresses,
        &TauriSink(app),
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn login(
    app: AppHandle,
    link: Runtime_<'_>,
    server_id: String,
    username: String,
    password: String,
    remember: bool,
) -> Result<LoginDto, LinkFailure> {
    link.login(&server_id, &username, password, remember, &TauriSink(app))
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn logout(
    app: AppHandle,
    link: Runtime_<'_>,
    server_id: String,
) -> Result<(), LinkFailure> {
    link.logout(&server_id, &TauriSink(app)).await
}

#[tauri::command]
#[specta::specta]
pub fn retry_now(link: Runtime_<'_>, server_id: String) -> Result<(), LinkFailure> {
    link.retry_now(&server_id)
}

#[tauri::command]
#[specta::specta]
pub async fn accept_fingerprint(
    link: Runtime_<'_>,
    server_id: String,
    fingerprint: String,
) -> Result<(), LinkFailure> {
    link.accept_fingerprint(&server_id, &fingerprint).await
}

#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn update_server(
    app: AppHandle,
    link: Runtime_<'_>,
    server_id: String,
    name: String,
    color: u8,
    host: String,
    port: Option<u16>,
    fingerprint: Option<String>,
) -> Result<ServerDto, LinkFailure> {
    link.update_server(
        &server_id,
        name,
        color,
        host,
        port,
        fingerprint,
        &TauriSink(app),
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn remove_server(
    app: AppHandle,
    link: Runtime_<'_>,
    server_id: String,
) -> Result<(), LinkFailure> {
    link.remove_server(&server_id, &TauriSink(app)).await
}

#[tauri::command]
#[specta::specta]
pub async fn forget_credentials(
    app: AppHandle,
    link: Runtime_<'_>,
    server_id: String,
) -> Result<(), LinkFailure> {
    link.forget_credentials(&server_id, &TauriSink(app)).await
}
