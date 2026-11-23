//! Les sauvegardes d'un service depuis le client (HRT-98, HRT-97, `BR-BACKUP-*`) : la liste, l'estimation de la
//! fenêtre de création, la vérification, et les trois actes (créer, supprimer, régler les éléments ignorés). Chaque
//! requête est construite par la liaison (`LinkManager::{list_backups, backup_preview, create_backup, …}`, chemins
//! fixes, clé vérifiée puis encodée) ; l'agent répond `202` à la création et l'issue ne se lit JAMAIS ici : elle
//! arrive par le suivi des opérations (HRT-84). Un acte n'est jamais rejoué : un lien tombé avant la réponse donne
//! `Unknown`. Les refus se lisent par leur CODE, jamais par leur texte. Sans Tauri.

use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use hearth_link::domain::service_lifecycle::{Refusal, refusal_of};
use hearth_link::{ActionOutcome, LinkError, LinkManager};
use hearth_proto::api::backups::{BackupAccepted, BackupDeleted, BackupSettings};
use hearth_proto::error::{ErrorBody, ErrorCode};
use serde_json::Value;

use super::backups_dto::{
    BackupCreateOutcome, BackupDeleteOutcome, BackupPreviewDto, BackupPreviewOutcome,
    BackupReadRefusal, BackupRefusal, BackupRequestDto, BackupSettingsDto, BackupSettingsOutcome,
    BackupVerifyOutcome, BackupsOutcome, BackupsViewDto, VerifyResultDto,
};
use crate::link_dto::LinkFailure;
use crate::reauth::wire::{Confirmation, confirmation_of};

/// Un refus de lecture, lu par son code ; tout autre échec (droit, lien, protocole) reste un échec.
fn read_refusal(error: LinkError) -> Result<BackupReadRefusal, LinkFailure> {
    match error {
        LinkError::Rejected(Some(code)) => Ok(match code {
            ErrorCode::ServiceNotFound | ErrorCode::NotFound => BackupReadRefusal::ServiceNotFound,
            ErrorCode::ServiceAdopted => BackupReadRefusal::ServiceAdopted,
            // L'état du moteur n'est pas dans l'erreur typée de la liaison : l'interface le relit de la liste.
            ErrorCode::EngineUnavailable => BackupReadRefusal::EngineUnavailable { engine: None },
            ErrorCode::PathRefused => BackupReadRefusal::PathRefused { reason: None },
            ErrorCode::BackupNotFound => BackupReadRefusal::BackupNotFound,
            ErrorCode::BackupInUse => BackupReadRefusal::BackupInUse,
            _ => BackupReadRefusal::Other,
        }),
        other => Err(other.into()),
    }
}

/// La liste des sauvegardes d'un service (`GET /services/{key}/backups`).
pub async fn list(
    manager: &LinkManager,
    id: &ServerId,
    key: &str,
) -> Result<BackupsOutcome, LinkFailure> {
    match manager.list_backups(id, key).await {
        Ok(view) => Ok(BackupsOutcome::Ready {
            view: Box::new(BackupsViewDto::from(&view)),
        }),
        Err(error) => read_refusal(error).map(|refusal| BackupsOutcome::Refused { refusal }),
    }
}

/// De quoi bâtir la fenêtre de création (`POST …/backups/preview`) : une consultation, rien n'est écrit.
pub async fn preview(
    manager: &LinkManager,
    id: &ServerId,
    key: &str,
) -> Result<BackupPreviewOutcome, LinkFailure> {
    match manager.backup_preview(id, key).await {
        Ok(preview) => Ok(BackupPreviewOutcome::Ready {
            preview: Box::new(BackupPreviewDto::from(&preview)),
        }),
        Err(error) => read_refusal(error).map(|refusal| BackupPreviewOutcome::Refused { refusal }),
    }
}

/// Relit l'archive et la juge (`POST /backups/{id}/verify`).
pub async fn verify(
    manager: &LinkManager,
    id: &ServerId,
    backup: &str,
) -> Result<BackupVerifyOutcome, LinkFailure> {
    match manager.verify_backup(id, backup).await {
        Ok(result) => Ok(BackupVerifyOutcome::Done {
            result: VerifyResultDto::from(&result),
        }),
        Err(error) => read_refusal(error).map(|refusal| BackupVerifyOutcome::Refused { refusal }),
    }
}

/// Crée une sauvegarde. Un mode que ce client ne sait pas demander ne part jamais.
pub async fn create(
    manager: &LinkManager,
    id: &ServerId,
    key: &str,
    request: BackupRequestDto,
    admin_password: Option<&Secret>,
) -> Result<BackupCreateOutcome, LinkFailure> {
    let Some(request) = request.wire() else {
        return Ok(BackupCreateOutcome::Refused {
            refusal: BackupRefusal::ModeRefused,
        });
    };
    match manager
        .create_backup(id, key, &request, admin_password)
        .await?
    {
        ActionOutcome::Completed { status, body, .. } => interpret_create(status, &body),
        ActionOutcome::ResultUnknown { id } => Ok(BackupCreateOutcome::Unknown {
            op_id: id.as_str().to_owned(),
        }),
    }
}

/// Supprime une sauvegarde : le mot de passe à chaque fois (jamais couvert par l'élévation).
pub async fn delete(
    manager: &LinkManager,
    id: &ServerId,
    backup: &str,
    admin_password: Option<&Secret>,
) -> Result<BackupDeleteOutcome, LinkFailure> {
    match manager.delete_backup(id, backup, admin_password).await? {
        ActionOutcome::Completed { status, body, .. } => interpret_delete(status, &body),
        ActionOutcome::ResultUnknown { id } => Ok(BackupDeleteOutcome::Unknown {
            op_id: id.as_str().to_owned(),
        }),
    }
}

/// Enregistre les éléments ignorés du service.
pub async fn save_settings(
    manager: &LinkManager,
    id: &ServerId,
    key: &str,
    settings: BackupSettingsDto,
    admin_password: Option<&Secret>,
) -> Result<BackupSettingsOutcome, LinkFailure> {
    let settings: BackupSettings = settings.into();
    match manager
        .save_backup_settings(id, key, &settings, admin_password)
        .await?
    {
        ActionOutcome::Completed { status, body, .. } => interpret_settings(status, &body),
        ActionOutcome::ResultUnknown { id } => Ok(BackupSettingsOutcome::Unknown {
            op_id: id.as_str().to_owned(),
        }),
    }
}

fn number(body: &Value, name: &str) -> Option<f64> {
    body.pointer(&format!("/error/details/{name}"))
        .and_then(Value::as_u64)
        .map(|value| value as f64)
}

fn text(body: &Value, name: &str) -> Option<String> {
    body.pointer(&format!("/error/details/{name}"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// Le refus d'un acte de sauvegarde, lu par son code. Le droit retiré entre-temps est un échec que l'interface lit à
/// part (elle relit le droit et le dit).
pub(super) fn refusal(status: u16, body: &Value) -> Result<BackupRefusal, LinkFailure> {
    match serde_json::from_value::<ErrorBody>(body.clone()) {
        Ok(parsed) => {
            if let Some(confirmation) = confirmation_of(&parsed)? {
                return Ok(match confirmation {
                    Confirmation::WrongPassword => BackupRefusal::WrongPassword,
                    Confirmation::PasswordRequired => BackupRefusal::PasswordRequired,
                    Confirmation::TooManyAttempts { retry_after_s } => {
                        BackupRefusal::TooManyAttempts { retry_after_s }
                    }
                    Confirmation::Busy => BackupRefusal::Busy,
                });
            }
            if parsed.error.code.or_from_status(status) == ErrorCode::ForbiddenRole {
                return Err(LinkFailure::forbidden(&parsed.error));
            }
            match parsed.error.code {
                ErrorCode::ServiceAdopted => return Ok(BackupRefusal::ServiceAdopted),
                ErrorCode::HeavyIoBusy => return Ok(BackupRefusal::HeavyIoBusy),
                ErrorCode::BackupNotFound => return Ok(BackupRefusal::BackupNotFound),
                ErrorCode::BackupInUse => return Ok(BackupRefusal::BackupInUse),
                ErrorCode::PathRefused => {
                    return Ok(BackupRefusal::PathRefused {
                        reason: text(body, "reason"),
                    });
                }
                ErrorCode::NoSpace => {
                    return Ok(BackupRefusal::NoSpace {
                        needed_bytes: number(body, "needed_bytes"),
                        free_bytes: number(body, "free_bytes"),
                        reserve_bytes: number(body, "reserve_bytes"),
                    });
                }
                ErrorCode::ValidationError => {
                    let field = text(body, "field");
                    return Ok(match (field.as_deref(), text(body, "reason").as_deref()) {
                        (Some("service"), Some("nothing_to_save")) => BackupRefusal::NothingToSave,
                        (Some("mode"), _) => BackupRefusal::ModeRefused,
                        _ => BackupRefusal::InvalidInput { field },
                    });
                }
                _ => {}
            }
        }
        Err(_) if status == 403 => return Err(LinkFailure::Forbidden),
        Err(_) => {}
    }
    Ok(match refusal_of(status, body) {
        Refusal::EngineUnavailable { engine } => BackupRefusal::EngineUnavailable {
            engine: engine.map(Into::into),
        },
        Refusal::ServiceNotFound
        | Refusal::Other {
            status: 404,
            code: None,
        } => BackupRefusal::ServiceNotFound,
        Refusal::ServiceBusy { by, kind } => BackupRefusal::ServiceBusy {
            by,
            action: kind.map(Into::into),
        },
        Refusal::NotDrivable { state } => BackupRefusal::NotDrivable {
            state: state.map(Into::into),
        },
        Refusal::AlreadyInState { .. } | Refusal::Other { .. } => BackupRefusal::Other,
    })
}

/// La réponse de l'agent à `POST /services/{key}/backups` : `202` avec l'opération, ou un refus typé.
pub fn interpret_create(status: u16, body: &Value) -> Result<BackupCreateOutcome, LinkFailure> {
    if (200..300).contains(&status) {
        let accepted: BackupAccepted =
            serde_json::from_value(body.clone()).map_err(|_| LinkFailure::NotAgent)?;
        return Ok(BackupCreateOutcome::Accepted {
            operation_id: accepted.operation_id,
            backup_id: accepted.backup_id,
        });
    }
    Ok(BackupCreateOutcome::Refused {
        refusal: refusal(status, body)?,
    })
}

/// La réponse de l'agent à `DELETE /backups/{id}`.
pub fn interpret_delete(status: u16, body: &Value) -> Result<BackupDeleteOutcome, LinkFailure> {
    if (200..300).contains(&status) {
        let deleted: BackupDeleted =
            serde_json::from_value(body.clone()).map_err(|_| LinkFailure::NotAgent)?;
        return Ok(BackupDeleteOutcome::Done {
            removed_files: deleted.removed_files,
        });
    }
    Ok(BackupDeleteOutcome::Refused {
        refusal: refusal(status, body)?,
    })
}

/// La réponse de l'agent à `PUT …/backup-settings` : la liste enregistrée.
pub fn interpret_settings(status: u16, body: &Value) -> Result<BackupSettingsOutcome, LinkFailure> {
    if (200..300).contains(&status) {
        let saved: BackupSettings =
            serde_json::from_value(body.clone()).map_err(|_| LinkFailure::NotAgent)?;
        return Ok(BackupSettingsOutcome::Done {
            settings: BackupSettingsDto::from(&saved),
        });
    }
    Ok(BackupSettingsOutcome::Refused {
        refusal: refusal(status, body)?,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::service_ops::dto::OperationKindDto;
    use crate::services::dto::EngineStateDto;

    fn error(code: &str, details: Value) -> Value {
        json!({ "error": { "code": code, "message": "x", "details": details } })
    }

    fn read(status: u16, body: Value) -> BackupRefusal {
        match interpret_create(status, &body).unwrap() {
            BackupCreateOutcome::Refused { refusal } => refusal,
            other => panic!("refus attendu, reçu {other:?}"),
        }
    }

    #[test]
    fn an_accepted_backup_carries_the_operation_never_the_outcome() {
        assert_eq!(
            interpret_create(202, &json!({ "operation_id": "01J", "backup_id": "01K" })).unwrap(),
            BackupCreateOutcome::Accepted {
                operation_id: "01J".into(),
                backup_id: "01K".into()
            }
        );
        assert!(matches!(
            interpret_create(202, &json!({ "nope": 1 })),
            Err(LinkFailure::NotAgent)
        ));
    }

    #[test]
    fn each_refusal_is_read_by_its_code() {
        assert_eq!(
            read(422, error("WRONG_PASSWORD", json!(null))),
            BackupRefusal::WrongPassword
        );
        assert_eq!(
            read(
                503,
                error("ENGINE_UNAVAILABLE", json!({ "engine": "stopped" }))
            ),
            BackupRefusal::EngineUnavailable {
                engine: Some(EngineStateDto::Stopped)
            }
        );
        assert_eq!(
            read(409, error("SERVICE_ADOPTED", json!({}))),
            BackupRefusal::ServiceAdopted
        );
        assert_eq!(
            read(409, error("HEAVY_IO_BUSY", json!({}))),
            BackupRefusal::HeavyIoBusy
        );
        assert_eq!(
            read(
                409,
                error("SERVICE_BUSY", json!({ "by": "marie", "kind": "backup" }))
            ),
            BackupRefusal::ServiceBusy {
                by: Some("marie".into()),
                action: Some(OperationKindDto::Backup)
            }
        );
        assert_eq!(
            read(
                507,
                error(
                    "NO_SPACE",
                    json!({ "needed_bytes": 10, "free_bytes": 4, "reserve_bytes": 2 })
                )
            ),
            BackupRefusal::NoSpace {
                needed_bytes: Some(10.0),
                free_bytes: Some(4.0),
                reserve_bytes: Some(2.0)
            }
        );
        assert_eq!(
            read(
                422,
                error(
                    "VALIDATION_ERROR",
                    json!({ "field": "service", "reason": "nothing_to_save" })
                )
            ),
            BackupRefusal::NothingToSave
        );
        assert_eq!(
            read(422, error("VALIDATION_ERROR", json!({ "field": "note" }))),
            BackupRefusal::InvalidInput {
                field: Some("note".into())
            }
        );
        assert_eq!(
            read(
                422,
                error("PATH_REFUSED", json!({ "reason": "backups_root_overlap" }))
            ),
            BackupRefusal::PathRefused {
                reason: Some("backups_root_overlap".into())
            }
        );
        assert_eq!(
            read(418, error("FROM_THE_FUTURE", json!({}))),
            BackupRefusal::Other
        );
    }

    #[test]
    fn deleting_and_saving_read_their_own_answers() {
        assert_eq!(
            interpret_delete(200, &json!({ "removed_files": true })).unwrap(),
            BackupDeleteOutcome::Done {
                removed_files: true
            }
        );
        assert!(matches!(
            interpret_delete(409, &error("BACKUP_IN_USE", json!({}))).unwrap(),
            BackupDeleteOutcome::Refused {
                refusal: BackupRefusal::BackupInUse
            }
        ));
        let saved = interpret_settings(
            200,
            &json!({ "ignored": [{ "root": "/data", "path": "logs" }], "included": [] }),
        )
        .unwrap();
        assert!(matches!(saved, BackupSettingsOutcome::Done { .. }));
    }

    #[test]
    fn the_right_withdrawn_in_between_is_a_failure_the_interface_reads_apart() {
        let body = error("FORBIDDEN_ROLE", json!({ "missing": "services" }));
        assert!(matches!(
            interpret_create(403, &body),
            Err(LinkFailure::MissingServicesRight)
        ));
    }
}
