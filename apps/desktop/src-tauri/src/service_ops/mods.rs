//! Les mods d'un serveur Minecraft depuis le client (HRT-260, ADR-0062, `BR-MOD-020` à `036`) : la liste, le contrôle
//! des dépôts, l'application d'un lot (acte `ModsApply`, jamais couvert par l'élévation), la liste des mods d'une
//! sauvegarde et les dernières lignes du journal d'une opération revenue en arrière. Chaque requête est construite
//! par la liaison (`LinkManager::{list_mods, check_mods, apply_mods, backup_mods, operation_tail}`, chemins fixes,
//! clé vérifiée puis encodée) ; l'agent répond `202` à l'application et l'issue ne se lit JAMAIS ici : elle arrive par
//! le suivi des opérations. Un acte n'est jamais rejoué : un lien tombé avant la réponse donne `Unknown`. Les refus se
//! lisent par leur CODE, jamais par leur texte. Sans Tauri.

use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use hearth_link::{ActionOutcome, LinkError, LinkManager};
use hearth_proto::api::mods::ModsApplyAccepted;
use hearth_proto::error::{ErrorBody, ErrorCode};
use serde_json::Value;

use super::backups;
use super::mods_dto::{
    BackupModsDto, BackupModsOutcome, ModsApplyDto, ModsApplyOutcome, ModsApplyRefusal,
    ModsCheckDto, ModsCheckOutcome, ModsOutcome, ModsReadRefusal, ModsViewDto, OperationTailDto,
    OperationTailOutcome,
};
use crate::link_dto::LinkFailure;

/// Un refus de lecture, lu par son code ; tout autre échec (droit, lien, protocole) reste un échec.
fn read_refusal(error: LinkError) -> Result<ModsReadRefusal, LinkFailure> {
    match error {
        LinkError::Rejected(Some(code)) => Ok(match code {
            ErrorCode::ServiceNotFound | ErrorCode::NotFound | ErrorCode::NotAGameService => {
                ModsReadRefusal::ServiceNotFound
            }
            ErrorCode::ServiceAdopted => ModsReadRefusal::ServiceAdopted,
            ErrorCode::ModsNotApplicable => ModsReadRefusal::NotApplicable,
            ErrorCode::DepositUnknown => ModsReadRefusal::DepositUnknown,
            ErrorCode::ModsFolderBlocked => ModsReadRefusal::FolderBlocked,
            ErrorCode::BackupNotFound => ModsReadRefusal::BackupNotFound,
            _ => ModsReadRefusal::Other,
        }),
        other => Err(other.into()),
    }
}

/// La liste des mods d'un service (`GET /services/{key}/mods`).
pub async fn list(
    manager: &LinkManager,
    id: &ServerId,
    key: &str,
) -> Result<ModsOutcome, LinkFailure> {
    match manager.list_mods(id, key).await {
        Ok(view) => Ok(ModsOutcome::Ready {
            view: Box::new(ModsViewDto::from(&view)),
        }),
        Err(error) => read_refusal(error).map(|refusal| ModsOutcome::Refused { refusal }),
    }
}

/// Le contrôle des dépôts (`POST …/mods/check`) : une consultation, rien n'est écrit sur le service.
pub async fn check(
    manager: &LinkManager,
    id: &ServerId,
    key: &str,
    deposits: &[String],
) -> Result<ModsCheckOutcome, LinkFailure> {
    match manager.check_mods(id, key, deposits).await {
        Ok(view) => Ok(ModsCheckOutcome::Ready {
            view: Box::new(ModsCheckDto::from(&view)),
        }),
        Err(error) => read_refusal(error).map(|refusal| ModsCheckOutcome::Refused { refusal }),
    }
}

/// La liste des mods d'une sauvegarde (`GET …/backups/{id}/mods`), avec ou sans la comparaison.
pub async fn backup_mods(
    manager: &LinkManager,
    id: &ServerId,
    key: &str,
    backup: &str,
    compare: bool,
) -> Result<BackupModsOutcome, LinkFailure> {
    match manager.backup_mods(id, key, backup, compare).await {
        Ok(view) => Ok(BackupModsOutcome::Ready {
            view: Box::new(BackupModsDto::from(&view)),
        }),
        Err(error) => read_refusal(error).map(|refusal| BackupModsOutcome::Refused { refusal }),
    }
}

/// Les dernières lignes du journal d'une opération (`GET /operations/services/{id}/tail`).
pub async fn tail(
    manager: &LinkManager,
    id: &ServerId,
    operation: &str,
) -> Result<OperationTailOutcome, LinkFailure> {
    match manager.operation_tail(id, operation).await {
        Ok(tail) => Ok(OperationTailOutcome::Ready {
            tail: OperationTailDto {
                lines: tail.lines.clone(),
                reason: tail.reason.clone(),
                sentence: tail.sentence.clone(),
            },
        }),
        Err(error) => read_refusal(error).map(|refusal| OperationTailOutcome::Refused { refusal }),
    }
}

/// Applique un lot : le mot de passe est exigé à chaque fois (acte jamais couvert par l'élévation). Un lot vide ne
/// part jamais.
pub async fn apply(
    manager: &LinkManager,
    id: &ServerId,
    key: &str,
    batch: ModsApplyDto,
    admin_password: Option<&Secret>,
) -> Result<ModsApplyOutcome, LinkFailure> {
    let request: hearth_proto::api::mods::ModsApplyRequest = batch.into();
    if request.total() == 0 {
        return Ok(ModsApplyOutcome::Refused {
            refusal: ModsApplyRefusal::Mod {
                code: "MODS_BATCH_INVALID".to_owned(),
                deposit: None,
                reason: None,
            },
        });
    }
    match manager
        .apply_mods(id, key, &request, admin_password)
        .await?
    {
        ActionOutcome::Completed { status, body, .. } => interpret_apply(status, &body),
        ActionOutcome::ResultUnknown { id } => Ok(ModsApplyOutcome::Unknown {
            op_id: id.as_str().to_owned(),
        }),
    }
}

fn details_text(body: &Value, name: &str) -> Option<String> {
    body.pointer(&format!("/error/details/{name}"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// Le code d'un refus propre aux mods, ou `None` pour un refus commun.
fn mod_code(code: ErrorCode) -> Option<&'static str> {
    Some(match code {
        ErrorCode::ModsFolderBlocked => "MODS_FOLDER_BLOCKED",
        ErrorCode::ModsBatchInvalid => "MODS_BATCH_INVALID",
        ErrorCode::ModRefused => "MOD_REFUSED",
        ErrorCode::ModFileMissing => "MOD_FILE_MISSING",
        ErrorCode::ModInModrinthPack => "MOD_IN_MODRINTH_PACK",
        ErrorCode::ModAlreadyActive => "MOD_ALREADY_ACTIVE",
        ErrorCode::ModNameTaken => "MOD_NAME_TAKEN",
        ErrorCode::DepositUnknown => "DEPOSIT_UNKNOWN",
        ErrorCode::ModsNotApplicable => "MODS_NOT_APPLICABLE",
        ErrorCode::NotAGameService => "NOT_A_GAME_SERVICE",
        ErrorCode::OperationInProgress => "OPERATION_IN_PROGRESS",
        _ => return None,
    })
}

/// La réponse de l'agent à `POST /services/{key}/mods/apply` : `202` avec l'opération, ou un refus typé.
pub fn interpret_apply(status: u16, body: &Value) -> Result<ModsApplyOutcome, LinkFailure> {
    if (200..300).contains(&status) {
        let accepted: ModsApplyAccepted =
            serde_json::from_value(body.clone()).map_err(|_| LinkFailure::NotAgent)?;
        return Ok(ModsApplyOutcome::Accepted {
            operation_id: accepted.operation_id,
            backup_id: accepted.backup_id,
        });
    }
    if let Ok(parsed) = serde_json::from_value::<ErrorBody>(body.clone())
        && let Some(code) = mod_code(parsed.error.code.or_from_status(status))
    {
        return Ok(ModsApplyOutcome::Refused {
            refusal: ModsApplyRefusal::Mod {
                code: code.to_owned(),
                deposit: details_text(body, "deposit"),
                reason: details_text(body, "refusal"),
            },
        });
    }
    Ok(ModsApplyOutcome::Refused {
        refusal: ModsApplyRefusal::Common {
            refusal: backups::refusal(status, body)?,
        },
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::service_ops::backups_dto::BackupRefusal;

    fn error(code: &str, details: Value) -> Value {
        json!({ "error": { "code": code, "message": "x", "details": details } })
    }

    #[test]
    fn an_accepted_batch_carries_the_operation_never_the_outcome() {
        assert_eq!(
            interpret_apply(202, &json!({ "operation_id": "01J", "backup_id": "01K" })).unwrap(),
            ModsApplyOutcome::Accepted {
                operation_id: "01J".into(),
                backup_id: "01K".into()
            }
        );
        assert!(matches!(
            interpret_apply(202, &json!({ "nope": 1 })),
            Err(LinkFailure::NotAgent)
        ));
    }

    #[test]
    fn a_refusal_of_the_mods_is_read_by_its_code_and_a_common_one_like_a_backup() {
        assert_eq!(
            interpret_apply(
                422,
                &error(
                    "MOD_REFUSED",
                    json!({ "deposit": "01J", "refusal": "MOD_SAME_VERSION" })
                )
            )
            .unwrap(),
            ModsApplyOutcome::Refused {
                refusal: ModsApplyRefusal::Mod {
                    code: "MOD_REFUSED".into(),
                    deposit: Some("01J".into()),
                    reason: Some("MOD_SAME_VERSION".into()),
                }
            }
        );
        assert!(matches!(
            interpret_apply(422, &error("MOD_ALREADY_ACTIVE", json!({}))).unwrap(),
            ModsApplyOutcome::Refused {
                refusal: ModsApplyRefusal::Mod { .. }
            }
        ));
        assert_eq!(
            interpret_apply(422, &error("WRONG_PASSWORD", json!(null))).unwrap(),
            ModsApplyOutcome::Refused {
                refusal: ModsApplyRefusal::Common {
                    refusal: BackupRefusal::WrongPassword
                }
            }
        );
        assert!(matches!(
            interpret_apply(409, &error("HEAVY_IO_BUSY", json!({}))).unwrap(),
            ModsApplyOutcome::Refused {
                refusal: ModsApplyRefusal::Common {
                    refusal: BackupRefusal::HeavyIoBusy
                }
            }
        ));
    }

    #[test]
    fn the_right_withdrawn_in_between_is_a_failure_the_interface_reads_apart() {
        let body = error("FORBIDDEN_ROLE", json!({ "missing": "services" }));
        assert!(matches!(
            interpret_apply(403, &body),
            Err(LinkFailure::MissingServicesRight)
        ));
    }

    #[test]
    fn a_read_refusal_is_a_code_and_the_rest_stays_a_failure() {
        assert_eq!(
            read_refusal(LinkError::Rejected(Some(ErrorCode::ModsNotApplicable))).unwrap(),
            ModsReadRefusal::NotApplicable
        );
        assert_eq!(
            read_refusal(LinkError::Rejected(Some(ErrorCode::NotAGameService))).unwrap(),
            ModsReadRefusal::ServiceNotFound
        );
        assert!(read_refusal(LinkError::NotConnected).is_err());
    }
}
