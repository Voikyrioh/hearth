//! Commandes de la mise à jour de l'agent exposées à l'interface (liste blanche : `build.rs` et
//! `capabilities/default.json`). UNE commande par action (ADR-0016). Aucune ne prend d'adresse, de
//! signature, de somme ni de chemin : `update_agent` ne reçoit que le serveur et le NUMÉRO de la
//! version que l'utilisateur a sous les yeux ; la cible envoyée à l'agent est celle que la coquille
//! a lue elle-même dans le flux de versions (ADR-0021). Aucune logique ici : chaque commande délègue
//! au service.

use std::sync::Arc;

use hearth_link::domain::secret::Secret;
use tauri::{AppHandle, Manager as _, State};

use super::dto::{AgentUpdateOutcome, AgentUpdateRefusal, AgentUpdateView};
use super::service;
use crate::link::LinkRuntime;
use crate::link_dto::LinkFailure;
use crate::update::SharedUpdates;
use crate::update::service::AckRefusal;

type Runtime_<'a> = State<'a, Arc<LinkRuntime>>;

fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_secs()).ok())
        .unwrap_or(0)
}

/// L'état de la mise à jour de l'agent d'un serveur : version, installation gérée, mise à jour en
/// cours, dernier résultat, et la version disponible dans le flux de versions. Une lecture : sans
/// suivi, refaite par l'interface au retour du lien (BR-UPDATE-017).
#[tauri::command]
#[specta::specta]
pub async fn get_agent_update(
    app: AppHandle,
    link: Runtime_<'_>,
    server_id: String,
) -> Result<AgentUpdateView, LinkFailure> {
    let updates = app.try_state::<SharedUpdates>();
    let target = updates.as_ref().and_then(|updates| updates.agent_target());
    let seen = updates
        .as_ref()
        .and_then(|updates| updates.agent_result_seen(&server_id));
    let view = service::view(
        link.manager(),
        &service::server(&server_id)?,
        target.as_ref(),
        now_seconds(),
        seen.as_deref(),
    )
    .await?;
    // Le résultat que la coquille tient pour ce serveur : le seul que `ack_agent_result` acceptera.
    if let (Some(updates), Some(last)) = (updates.as_ref(), view.last.as_ref()) {
        updates.note_agent_result_read(&server_id, &last.at);
    }
    Ok(view)
}

/// Note que le résultat daté `at` de ce serveur a été annoncé : il ne le sera plus, même après un
/// redémarrage du client (BR-UPDATE-015). Ne parle pas à l'agent.
#[tauri::command]
#[specta::specta]
pub fn ack_agent_result(
    app: AppHandle,
    link: Runtime_<'_>,
    server_id: String,
    at: String,
) -> Result<(), LinkFailure> {
    let Some(updates) = app.try_state::<SharedUpdates>() else {
        return Ok(());
    };
    let known: Vec<String> = link.servers().into_iter().map(|server| server.id).collect();
    updates
        .ack_agent_result(&server_id, &at, &known)
        .map_err(|refusal| match refusal {
            AckRefusal::UnknownServer => LinkFailure::UnknownServer,
            AckRefusal::InvalidDate | AckRefusal::NotTheResultRead => LinkFailure::InvalidInput {
                field: crate::link_dto::InvalidField::Other,
            },
        })
}

/// « Mettre à jour l'agent » : l'administrateur a confirmé `version`, celle qu'il a vue. La cible
/// (adresse, signature, somme) est celle que la coquille retient ; l'agent décide du rôle, de la
/// signature, de la somme et de l'adresse. Une action : clé d'opération, résultat inconnu à la
/// coupure, jamais rejouée.
#[tauri::command]
#[specta::specta]
pub async fn update_agent(
    app: AppHandle,
    link: Runtime_<'_>,
    server_id: String,
    version: String,
    admin_password: String,
) -> Result<AgentUpdateOutcome, LinkFailure> {
    let id = service::server(&server_id)?;
    let Some(updates) = app.try_state::<SharedUpdates>() else {
        // La mise à jour du client n'a pas démarré : aucune cible, rien n'est envoyé.
        return Ok(AgentUpdateOutcome::Refused {
            refusal: AgentUpdateRefusal::NoTarget,
        });
    };
    let target = updates.agent_target();
    service::start(
        link.manager(),
        &id,
        target.as_ref(),
        &version,
        &Secret::new(admin_password),
    )
    .await
}
