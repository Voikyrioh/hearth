//! L'adresse que le propriétaire donne à ses joueurs, côté coquille (HRT-215, `BR-JEU-022`) : la modification d'un service
//! qui ne change que cette adresse, un acte d'administration confirmé (`ServiceUpdate`, jamais couvert par
//! l'élévation, `BR-EDIT-017`), jamais rejoué. La requête est construite par la liaison (`LinkManager::set_public_address`
//! : lit la révision, n'envoie que l'adresse). Les refus se lisent par leur CODE : aucune valeur reçue n'est répétée.
//! L'adresse n'est pas un secret mais n'entre dans aucune trace. Sans Tauri.

use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use hearth_link::{ActionOutcome, LinkManager};
use hearth_proto::error::{ErrorBody, ErrorCode};
use serde::Serialize;
use serde_json::Value;
use specta::Type;

use crate::link_dto::LinkFailure;
use crate::reauth::wire::{Confirmation, confirmation_of};

/// Pourquoi l'agent a refusé le changement d'adresse (rien n'a été modifié). Des CODES, jamais un texte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PublicAddressRefusal {
    WrongPassword,
    PasswordRequired,
    TooManyAttempts {
        retry_after_s: u32,
    },
    Busy,
    /// La description du service a changé entre la lecture et l'envoi (autre révision) : relire et réessayer.
    Conflict,
    /// Une autre action tient le service pour l'instant.
    ServiceBusy {
        by: Option<String>,
    },
    /// Le service est adopté ou n'existe plus : rien à modifier.
    Gone,
    EngineUnavailable,
    /// L'agent refuse la forme de l'adresse.
    Invalid,
    Other,
}

/// Ce que l'envoi a donné : enregistrée, refusée, ou résultat inconnu (le lien est tombé avant la réponse : jamais
/// rejoué, l'interface relit la vue du jeu).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PublicAddressOutcome {
    Saved,
    Refused {
        refusal: PublicAddressRefusal,
    },
    Unknown {
        #[serde(rename = "opId")]
        op_id: String,
    },
}

fn refused(refusal: PublicAddressRefusal) -> Result<PublicAddressOutcome, LinkFailure> {
    Ok(PublicAddressOutcome::Refused { refusal })
}

/// Un nom de compte tel que l'agent l'écrit : court et imprimable, sinon rien.
fn actor_of(details: &Value) -> Option<String> {
    let text = details.get("by").and_then(Value::as_str)?;
    (!text.is_empty() && text.chars().count() <= 64 && !text.chars().any(char::is_control))
        .then(|| text.to_owned())
}

/// Change l'adresse des joueurs du service `key` (`""` l'efface). `admin_password` : le mot de passe de confirmation,
/// toujours demandé.
pub async fn set(
    manager: &LinkManager,
    id: &ServerId,
    key: &str,
    address: &str,
    admin_password: Option<&Secret>,
) -> Result<PublicAddressOutcome, LinkFailure> {
    match manager
        .set_public_address(id, key, address, admin_password)
        .await?
    {
        ActionOutcome::Completed { status, body, .. } => interpret(status, &body),
        ActionOutcome::ResultUnknown { id } => Ok(PublicAddressOutcome::Unknown {
            op_id: id.as_str().to_owned(),
        }),
    }
}

/// La réponse de l'agent à `PUT /services/{key}/spec` : `202` (l'opération court) ou `200` (rien ne change : l'adresse
/// est déjà celle-là), ou un refus typé par son code.
pub fn interpret(status: u16, body: &Value) -> Result<PublicAddressOutcome, LinkFailure> {
    if (200..300).contains(&status) {
        return Ok(PublicAddressOutcome::Saved);
    }
    let Ok(parsed) = serde_json::from_value::<ErrorBody>(body.clone()) else {
        return match status {
            403 => Err(LinkFailure::Forbidden),
            _ => refused(PublicAddressRefusal::Other),
        };
    };
    if let Some(confirmation) = confirmation_of(&parsed)? {
        return refused(match confirmation {
            Confirmation::WrongPassword => PublicAddressRefusal::WrongPassword,
            Confirmation::PasswordRequired => PublicAddressRefusal::PasswordRequired,
            Confirmation::TooManyAttempts { retry_after_s } => {
                PublicAddressRefusal::TooManyAttempts { retry_after_s }
            }
            Confirmation::Busy => PublicAddressRefusal::Busy,
        });
    }
    let details = &parsed.error.details;
    match parsed.error.code.or_from_status(status) {
        // Le droit retiré entre-temps : l'interface le relit et le dit (BR-GRANT-010).
        ErrorCode::ForbiddenRole => Err(LinkFailure::forbidden(&parsed.error)),
        ErrorCode::SpecConflict => refused(PublicAddressRefusal::Conflict),
        ErrorCode::ServiceBusy | ErrorCode::OperationInProgress => {
            refused(PublicAddressRefusal::ServiceBusy {
                by: actor_of(details),
            })
        }
        ErrorCode::ServiceAdopted | ErrorCode::ServiceNotFound => {
            refused(PublicAddressRefusal::Gone)
        }
        ErrorCode::EngineUnavailable => refused(PublicAddressRefusal::EngineUnavailable),
        ErrorCode::ValidationError | ErrorCode::PlanSettingRefused => {
            refused(PublicAddressRefusal::Invalid)
        }
        _ => refused(PublicAddressRefusal::Other),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn error(code: &str, details: Value) -> Value {
        json!({ "error": { "code": code, "message": "SENTINEL-MESSAGE", "details": details } })
    }

    fn read(status: u16, body: Value) -> PublicAddressOutcome {
        interpret(status, &body).expect("lu")
    }

    #[test]
    fn an_accepted_change_and_an_unchanged_one_are_both_saved() {
        assert_eq!(
            read(202, json!({ "applied": true, "operation_id": "01OP" })),
            PublicAddressOutcome::Saved
        );
        assert_eq!(
            read(200, json!({ "applied": false })),
            PublicAddressOutcome::Saved
        );
    }

    #[test]
    fn a_refusal_is_read_by_its_code_and_never_repeats_a_text() {
        let refused = |refusal| PublicAddressOutcome::Refused { refusal };
        assert_eq!(
            read(409, error("SPEC_CONFLICT", json!({ "current_rev": 4 }))),
            refused(PublicAddressRefusal::Conflict)
        );
        assert_eq!(
            read(
                409,
                error("SERVICE_BUSY", json!({ "by": "marie", "kind": "backup" }))
            ),
            refused(PublicAddressRefusal::ServiceBusy {
                by: Some("marie".into())
            })
        );
        assert_eq!(
            read(409, error("SERVICE_ADOPTED", json!({}))),
            refused(PublicAddressRefusal::Gone)
        );
        assert_eq!(
            read(
                422,
                error(
                    "VALIDATION_ERROR",
                    json!({ "field": "public_address", "reason": "bad_format" })
                )
            ),
            refused(PublicAddressRefusal::Invalid)
        );
        assert_eq!(
            read(500, error("INTERNAL_ERROR", json!({}))),
            refused(PublicAddressRefusal::Other)
        );
        let shown = format!("{:?}", read(409, error("SPEC_CONFLICT", json!({}))));
        assert!(!shown.contains("SENTINEL"), "{shown}");
    }
}
