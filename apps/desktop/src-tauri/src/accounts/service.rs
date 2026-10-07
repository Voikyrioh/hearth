//! Cas d'usage des comptes : construit la requête TYPÉE d'une action (`wire`), l'envoie par
//! `LinkManager::execute` (clé d'opération, résultat inconnu à la coupure, jamais rejouée) et rend
//! l'issue. Sans Tauri : les commandes ne font que l'appeler. Les règles de rôle sont à l'AGENT :
//! rien ici ne décide qui a le droit, un refus de l'agent est rendu tel quel (`Forbidden`).

use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use hearth_link::{ActionOutcome, LinkManager};

use super::dto::{AccountInputCheck, AccountListDto, AccountOutcome, AccountRefusal};
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
/// envoyer. TOUTE action de compte est un acte d'administration : elle part par `execute_act` (mot de
/// passe `admin_password` s'il y en a un, ET preuve de la clé de ce poste), jamais par `execute`.
async fn send(
    manager: &LinkManager,
    id: &ServerId,
    planned: Result<Planned, Stop>,
    admin_password: Option<&Secret>,
) -> Result<AccountOutcome, LinkFailure> {
    let planned = match planned {
        Ok(planned) => planned,
        Err(Stop::Refused(refusal)) => return Ok(AccountOutcome::Refused { refusal }),
        Err(Stop::Failed(failure)) => return Err(failure),
    };
    match manager
        .execute_act(id, planned.request, admin_password)
        .await?
    {
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
    admin_password: Option<&Secret>,
) -> Result<AccountOutcome, LinkFailure> {
    send(
        manager,
        id,
        wire::create(username, password.expose(), role),
        admin_password,
    )
    .await
}

pub async fn change_role(
    manager: &LinkManager,
    id: &ServerId,
    account: &str,
    role: RoleDto,
    admin_password: Option<&Secret>,
) -> Result<AccountOutcome, LinkFailure> {
    send(
        manager,
        id,
        wire::change_role(account, role).map_err(Stop::from),
        admin_password,
    )
    .await
}

pub async fn set_password(
    manager: &LinkManager,
    id: &ServerId,
    account: &str,
    password: &Secret,
    admin_password: Option<&Secret>,
) -> Result<AccountOutcome, LinkFailure> {
    send(
        manager,
        id,
        wire::set_password(account, password.expose()),
        admin_password,
    )
    .await
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
    keep_address: bool,
) -> Result<AccountOutcome, LinkFailure> {
    let username = current_username(manager, id)?;
    let planned =
        wire::change_own_password(&username, current.expose(), password.expose(), keep_address);
    if planned.is_err() {
        return send(manager, id, planned, Some(current)).await;
    }
    let previous = manager.take_remembered_password(id).await?;
    // `current` est AUSSI le mot de passe de confirmation de l'acte : l'ancien mot de passe est ce que
    // l'agent vérifie, par le chemin de la connexion (BR-ACCT-009). Le mot de passe mémorisé au coffre
    // n'est lu par aucun chemin de confirmation.
    let result = send(manager, id, planned, Some(current)).await;
    match &result {
        Ok(AccountOutcome::Done { .. }) => {
            // Coffre en échec : l'entrée reste effacée, jamais fausse.
            let _ = manager.remember_password(id, password).await;
        }
        other => {
            // Liste FERMÉE des cas où l'ancien mot de passe est remis ; tout le reste efface.
            if old_password_stands(other)
                && let Some(previous) = previous
            {
                let _ = manager.remember_password(id, &previous).await;
            }
        }
    }
    result
}

/// L'ancien mot de passe est-il encore le bon ? SEULEMENT si l'agent a refusé de façon EXPLICITE
/// (une réponse d'erreur comprise qui dit que rien n'a changé) ou si la requête n'est prouvablement
/// pas partie. Tout le reste (résultat inconnu, erreur de protocole, tâche redémarrée, conflit,
/// réponse illisible…) = on ne sait pas : l'entrée du coffre reste EFFACÉE et l'utilisateur
/// ressaisira. Les `match` sont EXHAUSTIFS, sans `_` : un nouveau cas d'erreur ne compile pas
/// sans qu'on choisisse son côté, et on choisit « efface ».
pub fn old_password_stands(result: &Result<AccountOutcome, LinkFailure>) -> bool {
    match result {
        Ok(AccountOutcome::Refused { refusal }) => match refusal {
            // Refus explicites, avant toute exécution : le mot de passe n'a pas changé. `Busy` :
            // l'agent le rend quand le plafond de calculs de mot de passe est atteint
            // (`infrastructure/argon2.rs::acquire`), pour la vérification de l'ancien mot de passe ou
            // le hachage du nouveau (`application/accounts.rs:310` et `:317`), TOUS DEUX avant
            // `apply_password` (`:318`), la seule écriture : rien n'a été exécuté.
            AccountRefusal::WrongPassword
            | AccountRefusal::WeakPassword { .. }
            | AccountRefusal::Busy
            // Refus de la confirmation, avant l'exécution : élévation fermée, attente de la connexion.
            | AccountRefusal::PasswordRequired
            | AccountRefusal::TooManyAttempts { .. } => true,
            AccountRefusal::InvalidUsername { .. }
            | AccountRefusal::UsernameTaken
            | AccountRefusal::LastAdmin
            | AccountRefusal::NotFound
            | AccountRefusal::ConfirmationMismatch
            | AccountRefusal::Conflict
            | AccountRefusal::SessionEnded
            | AccountRefusal::SessionRevoked
            | AccountRefusal::Other => false,
        },
        Ok(AccountOutcome::Done { .. } | AccountOutcome::Unknown { .. }) => false,
        Err(failure) => match failure {
            // Rien n'est parti (hors « Connecté », suivi impossible) ou refus de rôle de l'agent.
            // Sans clé au coffre, preuve non reconnue, défi indisponible, client trop ancien : l'agent
            // refuse AVANT d'exécuter (ou rien n'est parti), l'ancien mot de passe est toujours le bon.
            LinkFailure::NotConnected
            | LinkFailure::TrackingUnavailable
            | LinkFailure::TrackingSlow
            | LinkFailure::Forbidden
            | LinkFailure::NotRecognized
            | LinkFailure::DeviceChallengeUnavailable
            | LinkFailure::IncompatibleClient => true,
            LinkFailure::Unreachable
            | LinkFailure::NotAgent
            | LinkFailure::IncompatibleAgent
            | LinkFailure::InvalidCredentials
            | LinkFailure::TooManyAttempts { .. }
            | LinkFailure::FingerprintChanged
            | LinkFailure::NameTaken
            | LinkFailure::AlreadyExists
            | LinkFailure::InvalidInput { .. }
            | LinkFailure::VerificationRequired
            | LinkFailure::UnknownServer
            | LinkFailure::Storage
            | LinkFailure::Vault
            | LinkFailure::Internal => false,
        },
    }
}

pub async fn close_sessions(
    manager: &LinkManager,
    id: &ServerId,
    account: &str,
    admin_password: Option<&Secret>,
) -> Result<AccountOutcome, LinkFailure> {
    send(
        manager,
        id,
        wire::close_sessions(account).map_err(Stop::from),
        admin_password,
    )
    .await
}

pub async fn delete(
    manager: &LinkManager,
    id: &ServerId,
    account: &str,
    confirmation: Option<String>,
    admin_password: Option<&Secret>,
) -> Result<AccountOutcome, LinkFailure> {
    send(
        manager,
        id,
        wire::delete(account, confirmation).map_err(Stop::from),
        admin_password,
    )
    .await
}
