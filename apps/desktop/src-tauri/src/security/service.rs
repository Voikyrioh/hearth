//! Cas d'usage de la sécurité d'un serveur. Sans Tauri : les commandes ne font que l'appeler. Les
//! règles (qui peut activer, la preuve de clé, le mot de passe, les essais) sont à l'AGENT ; rien ici
//! ne décide qui a le droit : un refus est rendu typé, d'après le code d'erreur STABLE de l'agent,
//! jamais son texte.

use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use hearth_link::{ActionOutcome, LinkError, LinkManager};
use hearth_proto::api::security::AttackModeInfo;
use hearth_proto::error::{ErrorBody, ErrorCode};
use serde_json::Value;

use super::dto::{AttackModeDto, AttackModeOutcome, AttackModeRefusal, SecurityRead};
use crate::link::LinkRuntime;
use crate::link_dto::LinkFailure;
use crate::reauth::wire::{Confirmation, confirmation_of};

/// L'identifiant d'un serveur du carnet reçu de l'interface.
pub use crate::devices::service::server;

/// Lit l'état de sécurité (`GET /security`, tout rôle) et le range dans le carnet d'états. Un agent
/// d'avant la fonction répond `404` : `Unsupported`, pas une erreur.
pub async fn read(link: &LinkRuntime, id: &ServerId) -> Result<SecurityRead, LinkFailure> {
    match link.manager().security(id).await {
        Ok(response) => Ok(SecurityRead::Known {
            snapshot: link.on_security_read(id, &response),
        }),
        Err(LinkError::Rejected(Some(ErrorCode::NotFound))) => Ok(SecurityRead::Unsupported),
        Err(error) => Err(error.into()),
    }
}

/// Active ou désactive le mode attaque : le mot de passe actuel ET la preuve de la clé de CE PC (Q16).
/// Le mot de passe n'est pas gardé : il est enveloppé dans un `Secret` dès l'entrée, effacé à la
/// libération.
pub async fn set(
    manager: &LinkManager,
    id: &ServerId,
    active: bool,
    password: &Secret,
) -> Result<AttackModeOutcome, LinkFailure> {
    changed(manager.set_attack_mode(id, active, password).await)
}

/// Ce que le changement rend à l'interface : « fait », un refus typé, « résultat inconnu » (coupure),
/// ou l'échec typé. Sans clé au coffre : `NotRecognized`, rien n'est parti.
pub fn changed(result: Result<ActionOutcome, LinkError>) -> Result<AttackModeOutcome, LinkFailure> {
    match result {
        Ok(ActionOutcome::Completed { status, body, .. }) => interpret(status, &body),
        Ok(ActionOutcome::ResultUnknown { id }) => Ok(AttackModeOutcome::Unknown {
            op_id: id.as_str().to_owned(),
        }),
        // Agent d'avant le mode attaque : le défi répond « inconnu », rien n'est parti.
        Err(LinkError::Rejected(Some(ErrorCode::NotFound))) => {
            Ok(refused(AttackModeRefusal::Unsupported))
        }
        Err(error) => Err(error.into()),
    }
}

fn refused(refusal: AttackModeRefusal) -> AttackModeOutcome {
    AttackModeOutcome::Refused { refusal }
}

/// La réponse de l'agent à `PUT /security/attack-mode` : `2xx` = fait (l'état après le changement),
/// sinon un refus typé d'après le code stable de l'erreur.
pub fn interpret(status: u16, body: &Value) -> Result<AttackModeOutcome, LinkFailure> {
    if (200..300).contains(&status) {
        return match serde_json::from_value::<AttackModeInfo>(body.clone()) {
            Ok(info) => Ok(AttackModeOutcome::Done {
                attack_mode: AttackModeDto::from(&info),
            }),
            // Une réponse de succès qu'on ne sait pas lire : l'action est passée, l'état se relit.
            Err(_) => Err(LinkFailure::NotAgent),
        };
    }
    let Ok(error) = serde_json::from_value::<ErrorBody>(body.clone()) else {
        return Ok(refused(match status {
            404 | 405 => AttackModeRefusal::Unsupported,
            _ => AttackModeRefusal::Other,
        }));
    };
    // Les refus de la confirmation (mot de passe faux, attente, preuve non reconnue, client trop ancien)
    // se lisent partout pareil (BR-TRUST-040, 045).
    if let Some(confirmation) = confirmation_of(&error)? {
        return Ok(refused(match confirmation {
            Confirmation::WrongPassword => AttackModeRefusal::WrongPassword,
            Confirmation::PasswordRequired => AttackModeRefusal::PasswordRequired,
            Confirmation::TooManyAttempts { retry_after_s } => {
                AttackModeRefusal::TooManyAttempts { retry_after_s }
            }
            Confirmation::Busy => AttackModeRefusal::Busy,
        }));
    }
    let error = error.error;
    Ok(refused(match error.code {
        ErrorCode::ForbiddenRole => return Err(LinkFailure::Forbidden),
        ErrorCode::NotFound => AttackModeRefusal::Unsupported,
        ErrorCode::Unauthenticated | ErrorCode::SessionExpired => AttackModeRefusal::SessionEnded,
        ErrorCode::SessionRevoked => AttackModeRefusal::SessionRevoked,
        _ => AttackModeRefusal::Other,
    }))
}
