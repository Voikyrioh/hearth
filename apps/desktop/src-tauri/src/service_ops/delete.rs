//! Supprimer un service depuis le client (HRT-86, `BR-DEL-*`) : la consultation qui bâtit la fenêtre et l'acte
//! `ServiceDelete` (jamais couvert par l'élévation, `BR-DEL-012`). Chaque requête est construite par la liaison
//! (`LinkManager::{delete_preview, delete_service}`, chemins fixes, clé vérifiée puis encodée) ; l'agent répond
//! `202` et l'issue ne se lit JAMAIS ici : elle arrive par le suivi des opérations (HRT-84). Un acte n'est jamais
//! rejoué : un lien tombé avant la réponse donne `Unknown` (`BR-DEL-023`). Les refus se lisent par leur CODE,
//! jamais par leur texte. Pur côté lecture ; sans Tauri.

use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use hearth_link::domain::service_lifecycle::{Refusal, refusal_of};
use hearth_link::{ActionOutcome, LinkError, LinkManager};
use hearth_proto::api::service_delete::DeleteAccepted;
use hearth_proto::error::{ErrorBody, ErrorCode};
use serde_json::Value;

use super::delete_dto::{
    DeleteOutcome, DeletePreviewDto, DeletePreviewOutcome, DeletePreviewRefusal, DeleteRefusal,
    DeleteRequestDto,
};
use crate::link_dto::LinkFailure;
use crate::reauth::wire::{Confirmation, confirmation_of};

/// De quoi bâtir la fenêtre « Supprimer {nom} ? » (`GET /services/{key}/delete-preview`). Une consultation : rien
/// n'est écrit. Un service inconnu, adopté ou un moteur pas prêt est un refus lu par son code ; le droit absent,
/// le lien coupé et le reste sont des échecs comme pour toute lecture.
pub async fn preview(
    manager: &LinkManager,
    id: &ServerId,
    key: &str,
) -> Result<DeletePreviewOutcome, LinkFailure> {
    match manager.delete_preview(id, key).await {
        Ok(preview) => Ok(DeletePreviewOutcome::Ready {
            preview: Box::new(DeletePreviewDto::from(&preview)),
        }),
        Err(LinkError::Rejected(Some(code))) => Ok(DeletePreviewOutcome::Refused {
            refusal: match code {
                ErrorCode::ServiceNotFound | ErrorCode::NotFound => {
                    DeletePreviewRefusal::ServiceNotFound
                }
                ErrorCode::ServiceAdopted => DeletePreviewRefusal::ServiceAdopted,
                // L'état du moteur n'est pas dans l'erreur typée de la liaison : l'interface le relit de la liste.
                ErrorCode::EngineUnavailable => {
                    DeletePreviewRefusal::EngineUnavailable { engine: None }
                }
                _ => DeletePreviewRefusal::Other,
            },
        }),
        Err(other) => Err(other.into()),
    }
}

/// Supprime le service `key`. `admin_password` : le mot de passe de confirmation, exigé à chaque fois (jamais
/// couvert par l'élévation ; l'agent est l'arbitre).
pub async fn delete(
    manager: &LinkManager,
    id: &ServerId,
    key: &str,
    request: DeleteRequestDto,
    admin_password: Option<&Secret>,
) -> Result<DeleteOutcome, LinkFailure> {
    match manager
        .delete_service(id, key, &request.into(), admin_password)
        .await?
    {
        ActionOutcome::Completed { status, body, .. } => interpret(status, &body),
        ActionOutcome::ResultUnknown { id } => Ok(DeleteOutcome::Unknown {
            op_id: id.as_str().to_owned(),
        }),
    }
}

/// Le champ qu'une `VALIDATION_ERROR` désigne (`details.field`) : seuls deux champs de la demande sont jugés.
fn field_of(body: &Value) -> Option<&str> {
    body.pointer("/error/details/field").and_then(Value::as_str)
}

/// La réponse de l'agent à `DELETE /services/{key}` : `202` avec l'opération, ou un refus typé par son code.
pub fn interpret(status: u16, body: &Value) -> Result<DeleteOutcome, LinkFailure> {
    if (200..300).contains(&status) {
        let accepted: DeleteAccepted =
            serde_json::from_value(body.clone()).map_err(|_| LinkFailure::NotAgent)?;
        return Ok(DeleteOutcome::Accepted {
            operation_id: accepted.operation_id,
        });
    }
    match serde_json::from_value::<ErrorBody>(body.clone()) {
        Ok(parsed) => {
            if let Some(confirmation) = confirmation_of(&parsed)? {
                return Ok(refused(match confirmation {
                    Confirmation::WrongPassword => DeleteRefusal::WrongPassword,
                    Confirmation::PasswordRequired => DeleteRefusal::PasswordRequired,
                    Confirmation::TooManyAttempts { retry_after_s } => {
                        DeleteRefusal::TooManyAttempts { retry_after_s }
                    }
                    Confirmation::Busy => DeleteRefusal::Busy,
                }));
            }
            // Le droit retiré entre-temps : l'interface le relit et le dit (BR-DEL-027).
            if parsed.error.code.or_from_status(status) == ErrorCode::ForbiddenRole {
                return Err(LinkFailure::forbidden(&parsed.error));
            }
            match parsed.error.code {
                ErrorCode::ServiceAdopted => return Ok(refused(DeleteRefusal::ServiceAdopted)),
                ErrorCode::ValidationError => {
                    return Ok(refused(match field_of(body) {
                        Some("confirm_name") => DeleteRefusal::NameMismatch,
                        Some("erase_backups") => DeleteRefusal::BackupsUnavailable,
                        _ => DeleteRefusal::Other,
                    }));
                }
                _ => {}
            }
        }
        Err(_) if status == 403 => return Err(LinkFailure::Forbidden),
        Err(_) => {}
    }
    Ok(refused(match refusal_of(status, body) {
        Refusal::EngineUnavailable { engine } => DeleteRefusal::EngineUnavailable {
            engine: engine.map(Into::into),
        },
        Refusal::ServiceNotFound
        | Refusal::Other {
            status: 404,
            code: None,
        } => DeleteRefusal::ServiceNotFound,
        Refusal::ServiceBusy { by, kind } => DeleteRefusal::ServiceBusy {
            by,
            action: kind.map(Into::into),
        },
        Refusal::NotDrivable { state } => DeleteRefusal::NotDrivable {
            state: state.map(Into::into),
        },
        Refusal::AlreadyInState { .. } | Refusal::Other { .. } => DeleteRefusal::Other,
    }))
}

fn refused(refusal: DeleteRefusal) -> DeleteOutcome {
    DeleteOutcome::Refused { refusal }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::service_ops::dto::OperationKindDto;
    use crate::services::dto::{EngineStateDto, ServiceStateDto};

    fn error(code: &str, details: Value) -> Value {
        json!({ "error": { "code": code, "message": "x", "details": details } })
    }

    fn read(status: u16, body: Value) -> DeleteRefusal {
        match interpret(status, &body).unwrap() {
            DeleteOutcome::Refused { refusal } => refusal,
            other => panic!("refus attendu, reçu {other:?}"),
        }
    }

    #[test]
    fn an_accepted_deletion_carries_the_operation_never_the_outcome() {
        assert_eq!(
            interpret(202, &json!({ "operation_id": "01J" })).unwrap(),
            DeleteOutcome::Accepted {
                operation_id: "01J".into()
            }
        );
        assert!(matches!(
            interpret(202, &json!({ "nope": 1 })),
            Err(LinkFailure::NotAgent)
        ));
    }

    #[test]
    fn each_refusal_is_read_by_its_code() {
        assert_eq!(
            read(422, error("WRONG_PASSWORD", json!(null))),
            DeleteRefusal::WrongPassword
        );
        assert_eq!(
            read(
                429,
                error("TOO_MANY_ATTEMPTS", json!({ "retry_after_s": 30 }))
            ),
            DeleteRefusal::TooManyAttempts { retry_after_s: 30 }
        );
        assert_eq!(
            read(
                503,
                error("ENGINE_UNAVAILABLE", json!({ "engine": "stopped" }))
            ),
            DeleteRefusal::EngineUnavailable {
                engine: Some(EngineStateDto::Stopped)
            }
        );
        assert_eq!(
            read(404, error("SERVICE_NOT_FOUND", json!({}))),
            DeleteRefusal::ServiceNotFound
        );
        assert_eq!(
            read(409, error("SERVICE_ADOPTED", json!({}))),
            DeleteRefusal::ServiceAdopted
        );
        assert_eq!(
            read(
                409,
                error("SERVICE_BUSY", json!({ "by": "marie", "kind": "backup" }))
            ),
            DeleteRefusal::ServiceBusy {
                by: Some("marie".into()),
                action: Some(OperationKindDto::Backup)
            }
        );
        assert_eq!(
            read(
                409,
                error("SERVICE_NOT_DRIVABLE", json!({ "state": "paused" }))
            ),
            DeleteRefusal::NotDrivable {
                state: Some(ServiceStateDto::Paused)
            }
        );
        assert_eq!(
            read(
                422,
                error("VALIDATION_ERROR", json!({ "field": "confirm_name" }))
            ),
            DeleteRefusal::NameMismatch
        );
        assert_eq!(
            read(
                422,
                error("VALIDATION_ERROR", json!({ "field": "erase_backups" }))
            ),
            DeleteRefusal::BackupsUnavailable
        );
        assert_eq!(
            read(422, error("VALIDATION_ERROR", json!({ "field": "x" }))),
            DeleteRefusal::Other
        );
        assert_eq!(
            read(418, error("FROM_THE_FUTURE", json!({}))),
            DeleteRefusal::Other
        );
    }

    #[test]
    fn the_right_withdrawn_in_between_is_a_failure_the_interface_reads_apart() {
        let body = error("FORBIDDEN_ROLE", json!({ "missing": "services" }));
        assert!(matches!(
            interpret(403, &body),
            Err(LinkFailure::MissingServicesRight)
        ));
    }
}
