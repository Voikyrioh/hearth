//! Commandes de liaison exposées à l'interface (liste blanche : `build.rs` et
//! `capabilities/default.json`). Chacune délègue à [`LinkRuntime`] ; les échecs sont typés
//! ([`LinkFailure`]), l'interface choisit son texte. Aucun mot de passe n'est journalisé.

use std::sync::Arc;

use tauri::{AppHandle, Emitter as _, Runtime, State};

use crate::link::{LinkRuntime, UiSink};
use crate::link_dto::{
    ActionInput, ActionResultDto, AddServerInput, FingerprintEvent, LinkFailure, LinkStateDto,
    LoginDto, NoticeEvent, OperationEventDto, ProbeDto, ServerDto,
};

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

/// Fin de l'assistant d'ajout : le serveur n'est enregistré que si la connexion réussit.
#[tauri::command]
#[specta::specta]
pub async fn add_and_login(
    app: AppHandle,
    link: Runtime_<'_>,
    input: AddServerInput,
) -> Result<ServerDto, LinkFailure> {
    link.add_and_login(
        input.name,
        input.color,
        input.host,
        input.port,
        &input.fingerprint,
        input.mac_addresses,
        &input.username,
        input.password,
        input.remember,
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

/// Alertes d'empreinte en attente de décision : à lire après l'abonnement à `link://fingerprint`.
#[tauri::command]
#[specta::specta]
pub fn list_fingerprint_alerts(link: Runtime_<'_>) -> Vec<FingerprintEvent> {
    link.fingerprint_alerts()
}

/// Avis de la liaison retenus tant qu'ils ne sont pas acquittés (lecture non destructive).
#[tauri::command]
#[specta::specta]
pub fn list_link_notices(link: Runtime_<'_>) -> Vec<NoticeEvent> {
    link.notices()
}

#[tauri::command]
#[specta::specta]
pub fn ack_link_notices(link: Runtime_<'_>, ids: Vec<u32>) {
    link.ack_notices(&ids);
}

/// Issues d'actions retenues tant qu'elles ne sont pas acquittées (lecture non destructive).
#[tauri::command]
#[specta::specta]
pub fn list_unread_operations(link: Runtime_<'_>) -> Vec<OperationEventDto> {
    link.operations()
}

#[tauri::command]
#[specta::specta]
pub fn ack_unread_operations(link: Runtime_<'_>, op_ids: Vec<String>) {
    link.ack_operations(&op_ids);
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

/// Envoie une action à un serveur : refusée sans rien envoyer hors « Connecté » ; résultat inconnu si
/// le lien tombe avant la réponse (jamais rejouée, l'issue arrive par `link://operation`).
#[tauri::command]
#[specta::specta]
pub async fn run_action(
    link: Runtime_<'_>,
    server_id: String,
    action: ActionInput,
) -> Result<ActionResultDto, LinkFailure> {
    link.execute(&server_id, action).await
}
