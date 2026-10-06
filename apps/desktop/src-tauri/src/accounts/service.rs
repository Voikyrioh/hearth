//! Cas d'usage des comptes : construit la requête TYPÉE d'une action (`wire`), l'envoie par
//! `LinkManager::execute` (clé d'opération, résultat inconnu à la coupure, jamais rejouée) et rend
//! l'issue. Sans Tauri : les commandes ne font que l'appeler. Les règles de rôle sont à l'AGENT :
//! rien ici ne décide qui a le droit, un refus de l'agent est rendu tel quel (`Forbidden`).

use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use hearth_link::{ActionOutcome, LinkManager};

use super::dto::{AccountInputCheck, AccountListDto, AccountOutcome};
use super::wire::{self, Planned, Stop};
use crate::link_dto::{LinkFailure, RoleDto};

/// L'identifiant d'un serveur du carnet reçu de l'interface.
pub fn server(text: &str) -> Result<ServerId, LinkFailure> {
    ServerId::parse(text).map_err(|_| LinkFailure::UnknownServer)
}

/// Identifiant du compte connecté à ce serveur, tel que l'agent l'a rendu à la connexion.
fn current_username(manager: &LinkManager, id: &ServerId) -> Result<String, LinkFailure> {
    manager
        .servers()
        .into_iter()
        .find(|record| &record.id == id)
        .map(|record| record.username)
        .ok_or(LinkFailure::UnknownServer)
}

/// Envoie une action planifiée ; un refus de la validation locale devient un refus typé, sans rien
/// envoyer.
async fn send(
    manager: &LinkManager,
    id: &ServerId,
    planned: Result<Planned, Stop>,
) -> Result<AccountOutcome, LinkFailure> {
    let planned = match planned {
        Ok(planned) => planned,
        Err(Stop::Refused(refusal)) => return Ok(AccountOutcome::Refused { refusal }),
        Err(Stop::Failed(failure)) => return Err(failure),
    };
    match manager.execute(id, planned.request).await? {
        ActionOutcome::Completed { status, body, .. } => {
            wire::interpret(planned.expect, status, &body)
        }
        ActionOutcome::ResultUnknown { id } => Ok(AccountOutcome::Unknown {
            op_id: id.as_str().to_owned(),
        }),
    }
}

/// Saisie en direct : la même règle que celle de l'agent (`hearth-proto`), sans réseau.
pub fn check_input(username: &str, password: &Secret) -> AccountInputCheck {
    hearth_proto::account_rules::check_input(username, password.expose()).into()
}

/// La liste, et qui est « moi » : l'identifiant de l'AGENT du compte de la session. Lecture typée de
/// la bibliothèque (`LinkManager::accounts_list`) : un compte Lecture seule est refusé par l'agent
/// (`LinkFailure::Forbidden`), et le compte de la session n'est lu que si la liste l'a été.
pub async fn list(manager: &LinkManager, id: &ServerId) -> Result<AccountListDto, LinkFailure> {
    let read = manager.accounts_list(id).await?;
    Ok(AccountListDto {
        accounts: read.accounts.into_iter().map(Into::into).collect(),
        me: read.me,
    })
}

pub async fn create(
    manager: &LinkManager,
    id: &ServerId,
    username: &str,
    password: &Secret,
    role: RoleDto,
) -> Result<AccountOutcome, LinkFailure> {
    send(manager, id, wire::create(username, password.expose(), role)).await
}

pub async fn change_role(
    manager: &LinkManager,
    id: &ServerId,
    account: &str,
    role: RoleDto,
) -> Result<AccountOutcome, LinkFailure> {
    send(
        manager,
        id,
        wire::change_role(account, role).map_err(Stop::from),
    )
    .await
}

pub async fn set_password(
    manager: &LinkManager,
    id: &ServerId,
    account: &str,
    password: &Secret,
) -> Result<AccountOutcome, LinkFailure> {
    send(manager, id, wire::set_password(account, password.expose())).await
}

/// Changement de SON mot de passe, et du mot de passe mémorisé au coffre (reconnexion silencieuse).
/// L'entrée du coffre est retirée AVANT l'envoi : une application tuée en route laisse une entrée
/// effacée (l'utilisateur ressaisira), jamais l'ancien mot de passe. Puis : réussi, le nouveau est
/// rangé (si « se souvenir » est actif, sinon rien n'est écrit) ; refusé ou non parti, l'ancien est
/// remis tel qu'il était ; résultat inconnu (coupure), l'entrée reste effacée car on ne sait pas
/// lequel des deux est le bon.
pub async fn change_own_password(
    manager: &LinkManager,
    id: &ServerId,
    current: &Secret,
    password: &Secret,
) -> Result<AccountOutcome, LinkFailure> {
    let username = current_username(manager, id)?;
    let planned = wire::change_own_password(&username, current.expose(), password.expose());
    if planned.is_err() {
        return send(manager, id, planned).await;
    }
    let previous = manager.take_remembered_password(id)?;
    let result = send(manager, id, planned).await;
    match &result {
        Ok(AccountOutcome::Done { .. }) => {
            // Coffre en échec : l'entrée reste effacée, jamais fausse.
            let _ = manager.remember_password(id, password);
        }
        Ok(AccountOutcome::Unknown { .. }) => {}
        Ok(AccountOutcome::Refused { .. }) | Err(_) => {
            if let Some(previous) = previous {
                let _ = manager.remember_password(id, &previous);
            }
        }
    }
    result
}

pub async fn close_sessions(
    manager: &LinkManager,
    id: &ServerId,
    account: &str,
) -> Result<AccountOutcome, LinkFailure> {
    send(
        manager,
        id,
        wire::close_sessions(account).map_err(Stop::from),
    )
    .await
}

pub async fn delete(
    manager: &LinkManager,
    id: &ServerId,
    account: &str,
    confirmation: Option<String>,
) -> Result<AccountOutcome, LinkFailure> {
    send(
        manager,
        id,
        wire::delete(account, confirmation).map_err(Stop::from),
    )
    .await
}
