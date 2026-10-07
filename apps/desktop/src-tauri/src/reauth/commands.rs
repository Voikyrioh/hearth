//! Commandes de la confirmation des actes exposées à l'interface (liste blanche : `build.rs` et
//! `capabilities/default.json`). Aucune ne prend de route, de méthode ni de corps. Le mot de passe est
//! enveloppé dans un `Secret` dès l'entrée ; aucun n'est journalisé ni renvoyé.

use std::sync::Arc;

use hearth_link::domain::secret::Secret;
use tauri::State;

use super::dto::{AdminActKindDto, ReauthModeDto, ReauthSettingOutcome, ReauthStateDto};
use super::service;
use crate::link::LinkRuntime;
use crate::link_dto::{LinkFailure, RoleDto};

type Runtime_<'a> = State<'a, Arc<LinkRuntime>>;

/// Ce que l'agent annonce de la confirmation des actes pour ce serveur, lu à l'instant : l'interface ne
/// devine ni l'élévation ni la capacité. Une lecture, sans suivi.
#[tauri::command]
#[specta::specta]
pub async fn get_reauth_state(
    link: Runtime_<'_>,
    server_id: String,
) -> Result<ReauthStateDto, LinkFailure> {
    service::state(link.manager(), &service::server(&server_id)?).await
}

/// L'élévation de 5 minutes couvre-t-elle cet acte ? La règle est celle de l'agent, lue de
/// `hearth-proto` : la fenêtre ne la recopie pas. Ne parle pas à l'agent.
#[tauri::command]
#[specta::specta]
pub fn reauth_covers(kind: AdminActKindDto, role: Option<RoleDto>) -> bool {
    service::covers(kind, role)
}

/// Change le réglage « Demander mon mot de passe » (à chaque action, ou 5 minutes). `password` est le
/// mot de passe actuel de confirmation, la preuve de la clé de ce PC est faite par la coquille.
#[tauri::command]
#[specta::specta]
pub async fn set_reauth_setting(
    link: Runtime_<'_>,
    server_id: String,
    mode: ReauthModeDto,
    password: String,
) -> Result<ReauthSettingOutcome, LinkFailure> {
    let id = service::server(&server_id)?;
    service::set_setting(link.manager(), &id, mode, &Secret::new(password)).await
}
