//! Commandes de comptes exposées à l'interface (liste blanche : `build.rs` et
//! `capabilities/default.json`). UNE commande par action (ADR-0016) : paramètres métier typés, la
//! méthode et le chemin sont construits côté Rust (`wire`). Aucune logique ici : chaque commande
//! délègue au service. Un mot de passe est enveloppé dans un `Secret` dès l'entrée (effacé à la
//! libération) ; aucun n'est journalisé ni renvoyé.

use std::sync::Arc;

use hearth_link::domain::secret::Secret;
use tauri::State;

use super::dto::{AccountInputCheck, AccountListDto, AccountOutcome};
use super::service;
use crate::link::LinkRuntime;
use crate::link_dto::{LinkFailure, RoleDto};

type Runtime_<'a> = State<'a, Arc<LinkRuntime>>;

/// Saisie en direct d'un identifiant et d'un mot de passe : règles de `hearth-proto`, aucun
/// réseau. L'agent reste l'arbitre à l'envoi.
#[tauri::command]
#[specta::specta]
pub fn check_account_input(username: String, password: String) -> AccountInputCheck {
    service::check_input(&username, &Secret::new(password))
}

/// La liste des comptes (administrateurs ; l'agent refuse les autres) et l'identifiant de l'agent
/// du compte de la session.
#[tauri::command]
#[specta::specta]
pub async fn list_accounts(
    link: Runtime_<'_>,
    server_id: String,
) -> Result<AccountListDto, LinkFailure> {
    service::list(link.manager(), &service::server(&server_id)?).await
}

#[tauri::command]
#[specta::specta]
pub async fn create_account(
    link: Runtime_<'_>,
    server_id: String,
    username: String,
    password: String,
    role: RoleDto,
    admin_password: Option<String>,
) -> Result<AccountOutcome, LinkFailure> {
    let id = service::server(&server_id)?;
    let admin_password = admin_password.map(Secret::new);
    service::create(
        link.manager(),
        &id,
        &username,
        &Secret::new(password),
        role,
        admin_password.as_ref(),
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn change_account_role(
    link: Runtime_<'_>,
    server_id: String,
    account_id: String,
    role: RoleDto,
    admin_password: Option<String>,
) -> Result<AccountOutcome, LinkFailure> {
    let id = service::server(&server_id)?;
    let admin_password = admin_password.map(Secret::new);
    service::change_role(
        link.manager(),
        &id,
        &account_id,
        role,
        admin_password.as_ref(),
    )
    .await
}

/// Un administrateur définit le mot de passe d'un autre compte (ferme ses sessions). La règle « ne
/// contient pas l'identifiant » est celle de l'agent, qui lit le compte lui-même.
#[tauri::command]
#[specta::specta]
pub async fn set_account_password(
    link: Runtime_<'_>,
    server_id: String,
    account_id: String,
    password: String,
    admin_password: String,
) -> Result<AccountOutcome, LinkFailure> {
    let id = service::server(&server_id)?;
    // Prendre le contrôle d'un compte n'est jamais couvert par l'élévation : mot de passe obligatoire.
    let admin_password = Secret::new(admin_password);
    service::set_password(
        link.manager(),
        &id,
        &account_id,
        &Secret::new(password),
        Some(&admin_password),
    )
    .await
}

/// Le titulaire change son propre mot de passe (ferme ses AUTRES sessions, garde la courante) ;
/// le mot de passe mémorisé au coffre suit (voir `service::change_own_password`).
#[tauri::command]
#[specta::specta]
pub async fn change_own_password(
    link: Runtime_<'_>,
    server_id: String,
    current: String,
    password: String,
    keep_address: bool,
) -> Result<AccountOutcome, LinkFailure> {
    let id = service::server(&server_id)?;
    service::change_own_password(
        link.manager(),
        &id,
        &Secret::new(current),
        &Secret::new(password),
        keep_address,
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn close_account_sessions(
    link: Runtime_<'_>,
    server_id: String,
    account_id: String,
    admin_password: Option<String>,
) -> Result<AccountOutcome, LinkFailure> {
    let id = service::server(&server_id)?;
    let admin_password = admin_password.map(Secret::new);
    service::close_sessions(link.manager(), &id, &account_id, admin_password.as_ref()).await
}

/// Supprime un compte. `confirmation` : l'identifiant retapé quand on supprime son propre compte.
#[tauri::command]
#[specta::specta]
pub async fn delete_account(
    link: Runtime_<'_>,
    server_id: String,
    account_id: String,
    confirmation: Option<String>,
    admin_password: Option<String>,
) -> Result<AccountOutcome, LinkFailure> {
    let id = service::server(&server_id)?;
    let admin_password = admin_password.map(Secret::new);
    service::delete(
        link.manager(),
        &id,
        &account_id,
        confirmation,
        admin_password.as_ref(),
    )
    .await
}
