//! La requête de mise à jour de l'agent et la lecture de sa réponse. Pur : sans E/S, sans Tauri. La
//! méthode, le chemin et le corps sont construits ICI, à partir d'une cible VALIDÉE
//! (`domain::AgentTarget`) : rien n'en vient de l'interface (ADR-0016, ADR-0021).

use hearth_link::ActionRequest;
use hearth_link::ports::transport::Method;
use hearth_proto::api::update::{AgentUpdateAccepted, AgentUpdateRequest};
use hearth_proto::error::{ErrorBody, ErrorCode};
use serde_json::Value;

use super::domain::AgentTarget;
use super::dto::{AgentUpdateOutcome, AgentUpdateRefusal};
use crate::link_dto::LinkFailure;
use crate::reauth::wire::{Confirmation, confirmation_of};

/// `POST /agent/update` avec la cible validée : version, adresse, signature, somme.
pub fn start(target: &AgentTarget) -> Result<ActionRequest, LinkFailure> {
    let body = AgentUpdateRequest {
        version: target.version().to_string(),
        url: target.url().to_string(),
        signature: target.signature().to_owned(),
        sha256: target.sha256().to_owned(),
    };
    Ok(ActionRequest {
        method: Method::Post,
        path: "/agent/update".into(),
        body: Some(serde_json::to_value(&body).map_err(|_| LinkFailure::Internal)?),
    })
}

/// Ce que dit un refus de l'agent : le code stable de l'erreur, jamais son texte. Le refus de rôle
/// est `LinkFailure::Forbidden`.
pub fn refusal_from_error(status: u16, body: &Value) -> Result<AgentUpdateRefusal, LinkFailure> {
    let Ok(error) = serde_json::from_value::<ErrorBody>(body.clone()) else {
        return match status {
            403 => Err(LinkFailure::Forbidden),
            _ => Ok(AgentUpdateRefusal::Other),
        };
    };
    // Les refus de la confirmation se lisent partout pareil (BR-TRUST-040, 045).
    if let Some(confirmation) = confirmation_of(&error)? {
        return Ok(match confirmation {
            Confirmation::WrongPassword => AgentUpdateRefusal::WrongPassword,
            Confirmation::PasswordRequired => AgentUpdateRefusal::PasswordRequired,
            Confirmation::TooManyAttempts { retry_after_s } => {
                AgentUpdateRefusal::TooManyAttempts { retry_after_s }
            }
            Confirmation::Busy => AgentUpdateRefusal::Busy,
        });
    }
    Ok(match error.error.code {
        ErrorCode::ForbiddenRole => return Err(LinkFailure::Forbidden),
        ErrorCode::ManagedInstall => AgentUpdateRefusal::ManagedInstall,
        ErrorCode::OperationInProgress => AgentUpdateRefusal::InProgress,
        ErrorCode::BadSignature => AgentUpdateRefusal::BadSignature,
        ErrorCode::ValidationError => AgentUpdateRefusal::InvalidTarget,
        _ => AgentUpdateRefusal::Other,
    })
}

/// La réponse de l'agent : `202` accepté, ou un refus typé.
pub fn interpret(status: u16, body: &Value) -> Result<AgentUpdateOutcome, LinkFailure> {
    if status == 202 {
        let accepted: AgentUpdateAccepted =
            serde_json::from_value(body.clone()).map_err(|_| LinkFailure::NotAgent)?;
        return Ok(AgentUpdateOutcome::Accepted {
            version: accepted.version,
        });
    }
    if (200..300).contains(&status) {
        // Un succès qui n'est pas celui du contrat : on ne sait pas ce que l'agent a fait.
        return Err(LinkFailure::NotAgent);
    }
    Ok(AgentUpdateOutcome::Refused {
        refusal: refusal_from_error(status, body)?,
    })
}
