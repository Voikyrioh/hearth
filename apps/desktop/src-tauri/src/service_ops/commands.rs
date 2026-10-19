//! Commandes du suivi des opérations de service exposées à l'interface (liste blanche : `build.rs` et
//! `capabilities/default.json`). UNE commande par geste (ADR-0016), aucun paramètre libre : ni route, ni
//! méthode. Aucune commande n'annule ni ne crée une opération (HRT-84) ; celles qui agissent sont la décision
//! et les trois gestes de pilotage (HRT-65), des actes d'administration confirmés.

use std::sync::Arc;

use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use tauri::State;

use super::dto::{DecisionOutcome, OperationChoiceDto, ServiceOperationEvent};
use super::lifecycle::{self, GestureDto, LifecycleOutcome};
use super::service;
use crate::link::LinkRuntime;
use crate::link_dto::LinkFailure;

type Runtime_<'a> = State<'a, Arc<LinkRuntime>>;

/// Les opérations en cours de chaque serveur et les issues pas encore annoncées, sans lecture réseau : une
/// par événement, que l'interface rejoue à son abonnement (ADR-0013 point 3).
#[tauri::command]
#[specta::specta]
pub async fn list_service_operations(
    link: Runtime_<'_>,
) -> Result<Vec<ServiceOperationEvent>, LinkFailure> {
    Ok(link.service_operations())
}

/// L'interface a annoncé l'issue de ces opérations : elles ne sont plus rejouées (BR-RESIL-010).
#[tauri::command]
#[specta::specta]
pub async fn ack_service_operations(
    link: Runtime_<'_>,
    server_id: String,
    op_ids: Vec<String>,
) -> Result<(), LinkFailure> {
    let id = ServerId::parse(&server_id).map_err(|_| LinkFailure::UnknownServer)?;
    link.ack_service_operations(id.as_str(), &op_ids);
    Ok(())
}

/// Décide d'une opération qui attend le compte (`BR-SVC-029`, `BR-EDIT-031`) : un acte d'administration,
/// mot de passe redemandé à chaque réponse. Jamais rejoué : un résultat inconnu se lit sur le flux. Sans le
/// droit « gérer les services » : `MissingServicesRight`.
#[tauri::command]
#[specta::specta]
pub async fn decide_service_operation(
    link: Runtime_<'_>,
    server_id: String,
    op_id: String,
    choice: OperationChoiceDto,
    admin_password: Option<String>,
) -> Result<DecisionOutcome, LinkFailure> {
    let id = ServerId::parse(&server_id).map_err(|_| LinkFailure::UnknownServer)?;
    let admin_password = admin_password.map(Secret::new);
    service::decide(link.manager(), &id, &op_id, choice, admin_password.as_ref()).await
}

/// Un geste de pilotage d'un service (HRT-65, BR-LIFE-016) : un acte d'administration confirmé, couvert par
/// l'élévation de cinq minutes. L'agent répond `202` : l'issue ne se lit jamais ici, elle arrive par le suivi
/// des opérations. Jamais rejoué : un résultat inconnu se lit sur le flux. Sans le droit « gérer les
/// services » : `MissingServicesRight`.
async fn gesture(
    link: Runtime_<'_>,
    server_id: String,
    key: String,
    gesture: GestureDto,
    admin_password: Option<String>,
) -> Result<LifecycleOutcome, LinkFailure> {
    let id = ServerId::parse(&server_id).map_err(|_| LinkFailure::UnknownServer)?;
    let admin_password = admin_password.map(Secret::new);
    lifecycle::act(link.manager(), &id, &key, gesture, admin_password.as_ref()).await
}

/// Démarre un service (`POST /services/{key}/start`).
#[tauri::command]
#[specta::specta]
pub async fn service_start(
    link: Runtime_<'_>,
    server_id: String,
    key: String,
    admin_password: Option<String>,
) -> Result<LifecycleOutcome, LinkFailure> {
    gesture(link, server_id, key, GestureDto::Start, admin_password).await
}

/// Arrête un service (`POST /services/{key}/stop`).
#[tauri::command]
#[specta::specta]
pub async fn service_stop(
    link: Runtime_<'_>,
    server_id: String,
    key: String,
    admin_password: Option<String>,
) -> Result<LifecycleOutcome, LinkFailure> {
    gesture(link, server_id, key, GestureDto::Stop, admin_password).await
}

/// Redémarre un service (`POST /services/{key}/restart`).
#[tauri::command]
#[specta::specta]
pub async fn service_restart(
    link: Runtime_<'_>,
    server_id: String,
    key: String,
    admin_password: Option<String>,
) -> Result<LifecycleOutcome, LinkFailure> {
    gesture(link, server_id, key, GestureDto::Restart, admin_password).await
}
