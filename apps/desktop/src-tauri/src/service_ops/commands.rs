//! Commandes du suivi des opérations de service exposées à l'interface (liste blanche : `build.rs` et
//! `capabilities/default.json`). UNE commande par geste (ADR-0016), aucun paramètre libre : ni route, ni
//! méthode. Celles qui agissent sont la décision, les trois gestes de pilotage (HRT-65), le déploiement d'une image et
//! l'annulation de son opération (HRT-69), des actes d'administration confirmés ; les deux autres (consultation du
//! formulaire, bornes de la machine) sont des lectures.

use std::sync::Arc;

use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use tauri::State;

use super::catalog;
use super::catalog_dto::{CatalogDto, FicheInstallRequestDto, FicheReadDto};
use super::deploy;
use super::deploy_dto::{
    CancelOutcome, DeployCheckDto, DeployCheckInputDto, DeployOutcome, DeployRequestDto,
    EngineCapacityDto,
};
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

/// Les constats du formulaire de déploiement sur le nom, les ports et l'image (`POST /deployments/check`) : une
/// CONSULTATION, ni suivi ni journal (BR-DEPLOY-033), jamais un secret. Hors « Connecté » : `NotConnected`.
#[tauri::command]
#[specta::specta]
pub async fn deploy_check(
    link: Runtime_<'_>,
    server_id: String,
    input: DeployCheckInputDto,
) -> Result<DeployCheckDto, LinkFailure> {
    let id = ServerId::parse(&server_id).map_err(|_| LinkFailure::UnknownServer)?;
    deploy::check(link.manager(), &id, input).await
}

/// Déploie une image (`POST /deployments`) : un acte d'administration confirmé, JAMAIS couvert par l'élévation
/// (le mot de passe à chaque fois). L'agent répond `202` : l'issue ne se lit jamais ici, elle arrive par le suivi
/// des opérations. Jamais rejoué : un résultat inconnu se lit sur le flux (BR-DEPLOY-028). La valeur d'une variable,
/// son nom et la commande ne sont ni tracés ni gardés. Sans le droit « gérer les services » :
/// `MissingServicesRight`.
#[tauri::command]
#[specta::specta]
pub async fn deploy_service(
    link: Runtime_<'_>,
    server_id: String,
    deployment: DeployRequestDto,
    admin_password: Option<String>,
) -> Result<DeployOutcome, LinkFailure> {
    let id = ServerId::parse(&server_id).map_err(|_| LinkFailure::UnknownServer)?;
    let admin_password = admin_password.map(Secret::new);
    deploy::deploy(link.manager(), &id, deployment, admin_password.as_ref()).await
}

/// Annule une opération de service avant son point de non-retour (`DELETE /operations/services/{id}`) : un acte
/// d'administration confirmé, couvert par l'élévation. Jamais rejoué.
#[tauri::command]
#[specta::specta]
pub async fn cancel_service_operation(
    link: Runtime_<'_>,
    server_id: String,
    op_id: String,
    admin_password: Option<String>,
) -> Result<CancelOutcome, LinkFailure> {
    let id = ServerId::parse(&server_id).map_err(|_| LinkFailure::UnknownServer)?;
    let admin_password = admin_password.map(Secret::new);
    deploy::cancel(link.manager(), &id, &op_id, admin_password.as_ref()).await
}

/// Les bornes des curseurs de ressources et la part de la machine déjà engagée (`GET /engine/capacity`,
/// BR-DEPLOY-038) : une lecture.
#[tauri::command]
#[specta::specta]
pub async fn read_engine_capacity(
    link: Runtime_<'_>,
    server_id: String,
) -> Result<EngineCapacityDto, LinkFailure> {
    let id = ServerId::parse(&server_id).map_err(|_| LinkFailure::UnknownServer)?;
    deploy::capacity(link.manager(), &id).await
}

/// Les fiches de serveurs de jeu que cet agent propose (`GET /catalog`) : une lecture, ouverte à tout compte,
/// jamais journalisée (BR-FICHE-001).
#[tauri::command]
#[specta::specta]
pub async fn read_catalog(
    link: Runtime_<'_>,
    server_id: String,
) -> Result<CatalogDto, LinkFailure> {
    let id = ServerId::parse(&server_id).map_err(|_| LinkFailure::UnknownServer)?;
    catalog::list(link.manager(), &id).await
}

/// Le détail d'une fiche (`GET /catalog/{id}`), avec son empreinte. Une fiche que l'agent ne propose pas se lit
/// `unknown`, ce n'est pas une panne.
#[tauri::command]
#[specta::specta]
pub async fn read_fiche(
    link: Runtime_<'_>,
    server_id: String,
    fiche: String,
) -> Result<FicheReadDto, LinkFailure> {
    let id = ServerId::parse(&server_id).map_err(|_| LinkFailure::UnknownServer)?;
    catalog::fiche(link.manager(), &id, &fiche).await
}

/// Installe un serveur de jeu depuis une fiche (`POST /deployments` avec un membre `source`) : le même acte
/// d'administration confirmé que le déploiement d'une image, JAMAIS couvert par l'élévation (le mot de passe à
/// chaque fois), jamais rejoué. La valeur d'une réponse (mot de passe du serveur, jeton) n'est ni tracée ni gardée.
/// Sans le droit « gérer les services » : `MissingServicesRight`.
#[tauri::command]
#[specta::specta]
pub async fn install_fiche(
    link: Runtime_<'_>,
    server_id: String,
    installation: FicheInstallRequestDto,
    admin_password: Option<String>,
) -> Result<DeployOutcome, LinkFailure> {
    let id = ServerId::parse(&server_id).map_err(|_| LinkFailure::UnknownServer)?;
    let admin_password = admin_password.map(Secret::new);
    catalog::install(link.manager(), &id, installation, admin_password.as_ref()).await
}

/// Ouvre dans le navigateur de l'utilisateur une adresse que la FICHE porte (le contrat de licence, le site où créer un
/// jeton de serveur, la source d'un chiffre de l'éditeur). L'adresse n'est jamais fournie par la page : la coquille relit
/// la fiche chez l'agent et ne retient qu'une adresse `https` sans identifiant (`catalog::link_of`). Hearth ne
/// contacte jamais cette adresse lui-même. `kind` : `agreement`, `token` ou `evidence`.
#[tauri::command]
#[specta::specta]
pub async fn open_fiche_link(
    link: Runtime_<'_>,
    server_id: String,
    fiche: String,
    kind: String,
    key: String,
) -> Result<(), LinkFailure> {
    let id = ServerId::parse(&server_id).map_err(|_| LinkFailure::UnknownServer)?;
    let Some(url) = catalog::link(link.manager(), &id, &fiche, &kind, &key).await? else {
        return Err(LinkFailure::Internal);
    };
    std::process::Command::new("explorer")
        .arg(url)
        .spawn()
        .map(drop)
        .map_err(|_| LinkFailure::Internal)
}
