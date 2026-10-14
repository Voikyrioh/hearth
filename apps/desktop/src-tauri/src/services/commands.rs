//! Commandes du moteur de conteneurs et des services exposées à l'interface (liste blanche : `build.rs`
//! et `capabilities/default.json`). UNE commande par geste (ADR-0016), aucun paramètre libre : ni route,
//! ni méthode, ni adresse. Lecture seule : « Vérifier à nouveau » relance la détection du moteur,
//! `POST /engine/recheck` ne modifie rien.

use std::process::Command;
use std::sync::Arc;

use hearth_link::domain::server::ServerId;
use tauri::State;

use super::dto::{EngineDto, EnvRevealDto, ServiceRightsDto, ServicesEvent};
use crate::link::LinkRuntime;
use crate::link_dto::LinkFailure;

type Runtime_<'a> = State<'a, Arc<LinkRuntime>>;

/// La documentation officielle d'installation de Docker : une constante de la coquille, jamais fournie par
/// l'interface (conception technique, section 8, écart 8).
pub const ENGINE_DOCS_URL: &str = "https://docs.docker.com/engine/install/";

/// Le dernier état des services de chaque serveur du carnet, sans lecture réseau : l'interface le rejoue à
/// son abonnement (ADR-0013 point 3). Un serveur dont la coquille n'a encore rien reçu en cette exécution
/// donne la dernière liste enregistrée sur le disque, périmée.
#[tauri::command]
#[specta::specta]
pub async fn list_services_states(link: Runtime_<'_>) -> Result<Vec<ServicesEvent>, LinkFailure> {
    Ok(link.services_states().await)
}

/// « Vérifier à nouveau » : relance la détection du moteur tout de suite et rend son état. Une lecture :
/// sans suivi, refaite par l'interface si le lien tombe. Hors « Connecté », `NotConnected` sans rien envoyer.
#[tauri::command]
#[specta::specta]
pub async fn engine_recheck(
    link: Runtime_<'_>,
    server_id: String,
) -> Result<EngineDto, LinkFailure> {
    let id = ServerId::parse(&server_id).map_err(|_| LinkFailure::UnknownServer)?;
    let engine = link.manager().engine_recheck(&id).await?;
    Ok(EngineDto::from(&engine))
}

/// Le droit « gérer les services » du compte connecté (BR-GRANT-011) : une lecture (`GET /me` et `GET /hello`),
/// relue à l'ouverture de la section, à chaque reconnexion et après tout refus pour absence du droit. Aucune
/// commande de cette coquille n'accorde ni ne retire le droit : il ne se donne que depuis le serveur
/// (BR-GRANT-005). Hors « Connecté » : `NotConnected` sans rien envoyer.
#[tauri::command]
#[specta::specta]
pub async fn read_service_rights(
    link: Runtime_<'_>,
    server_id: String,
) -> Result<ServiceRightsDto, LinkFailure> {
    let id = ServerId::parse(&server_id).map_err(|_| LinkFailure::UnknownServer)?;
    Ok(link.manager().service_rights(&id).await?.into())
}

/// Révèle les valeurs des variables d'UN service (HRT-63, `BR-SVC-009`) : une lecture que l'agent réserve aux
/// comptes qui ont le droit de gérer les services et qu'il consigne au journal. La clé est celle du service
/// ouvert (vérifiée par la liaison, qui construit le chemin). Les valeurs sont rendues à l'appelant et à lui
/// seul : la coquille ne les garde pas, ne les trace pas, ne les met pas dans un événement. Hors « Connecté » :
/// `NotConnected`, rien n'est envoyé ; sans le droit : `MissingServicesRight`.
#[tauri::command]
#[specta::specta]
pub async fn reveal_service_env(
    link: Runtime_<'_>,
    server_id: String,
    key: String,
) -> Result<EnvRevealDto, LinkFailure> {
    let id = ServerId::parse(&server_id).map_err(|_| LinkFailure::UnknownServer)?;
    let revealed = link.manager().reveal_service_env(&id, &key).await?;
    Ok(EnvRevealDto::from(&revealed))
}

/// Ouvre la documentation officielle de Docker dans le navigateur du PC. Aucun paramètre : l'adresse est
/// la constante [`ENGINE_DOCS_URL`].
#[tauri::command]
#[specta::specta]
pub fn open_engine_docs() -> Result<(), LinkFailure> {
    Command::new("explorer")
        .arg(ENGINE_DOCS_URL)
        .spawn()
        .map(drop)
        .map_err(|_| LinkFailure::Internal)
}
