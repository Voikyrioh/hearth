//! Commandes de sécurité exposées à l'interface (liste blanche : `build.rs` et
//! `capabilities/default.json`). UNE commande par action (ADR-0016) : paramètres métier typés, la
//! méthode, le chemin, le défi et la signature sont construits côté Rust. Aucune commande ne crée,
//! n'exporte ni ne signe quoi que ce soit : la clé n'a pas de commande. Le mot de passe est
//! enveloppé dans un `Secret` dès l'entrée ; aucun n'est journalisé ni renvoyé.

use std::sync::Arc;

use hearth_link::domain::secret::Secret;
use tauri::State;

use super::dto::{AttackModeOutcome, SecurityEvent, SecurityRead};
use super::service;
use crate::link::LinkRuntime;
use crate::link_dto::LinkFailure;

type Runtime_<'a> = State<'a, Arc<LinkRuntime>>;

/// Lit l'état de sécurité d'un serveur (lecture, sans suivi) : alerte, mode attaque, et ce que
/// l'agent dit de la session de ce poste. L'interface la refait au retour du lien.
#[tauri::command]
#[specta::specta]
pub async fn get_security(
    link: Runtime_<'_>,
    server_id: String,
) -> Result<SecurityRead, LinkFailure> {
    service::read(&link, &service::server(&server_id)?).await
}

/// Le dernier état de sécurité connu de chaque serveur, sans lecture réseau : l'interface le rejoue à
/// son abonnement (ADR-0013 point 3).
#[tauri::command]
#[specta::specta]
pub fn list_security_states(link: Runtime_<'_>) -> Vec<SecurityEvent> {
    link.security_states()
}

/// Active (`active: true`) ou désactive le mode attaque : acte d'administration (mot de passe actuel
/// ET preuve de la clé de ce PC, Q14 point 3 et Q16). `NotRecognized` sans rien envoyer si ce PC n'a
/// pas de clé ; `Forbidden` pour un compte Lecture seule.
#[tauri::command]
#[specta::specta]
pub async fn set_attack_mode(
    link: Runtime_<'_>,
    server_id: String,
    active: bool,
    password: String,
) -> Result<AttackModeOutcome, LinkFailure> {
    let id = service::server(&server_id)?;
    service::set(link.manager(), &id, active, &Secret::new(password)).await
}
