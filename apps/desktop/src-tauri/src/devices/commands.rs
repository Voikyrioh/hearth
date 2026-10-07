//! Commandes de postes de confiance exposées à l'interface (liste blanche : `build.rs` et
//! `capabilities/default.json`). UNE commande par action (ADR-0016) : paramètres métier typés, la
//! méthode, le chemin, le défi et la signature sont construits côté Rust. Aucune commande ne crée,
//! n'exporte ni ne signe quoi que ce soit : la clé n'a pas de commande. Le mot de passe est
//! enveloppé dans un `Secret` dès l'entrée ; aucun n'est journalisé ni renvoyé.

use std::sync::Arc;

use hearth_link::domain::secret::Secret;
use tauri::State;

use super::dto::{DeviceRemovalOutcome, TrustedDevicesDto};
use super::service;
use crate::link::LinkRuntime;
use crate::link_dto::LinkFailure;

type Runtime_<'a> = State<'a, Arc<LinkRuntime>>;

/// Les postes de confiance du compte de la session (lecture, session seule).
#[tauri::command]
#[specta::specta]
pub async fn list_trusted_devices(
    link: Runtime_<'_>,
    server_id: String,
) -> Result<TrustedDevicesDto, LinkFailure> {
    service::list(link.manager(), &service::server(&server_id)?).await
}

/// Retire un poste de confiance : acte d'administration (mot de passe actuel ET preuve de la clé de
/// ce PC, Q16). `device_id` est l'identifiant rendu par la liste.
#[tauri::command]
#[specta::specta]
pub async fn remove_trusted_device(
    link: Runtime_<'_>,
    server_id: String,
    device_id: String,
    password: String,
) -> Result<DeviceRemovalOutcome, LinkFailure> {
    let id = service::server(&server_id)?;
    service::remove(link.manager(), &id, &device_id, &Secret::new(password)).await
}
