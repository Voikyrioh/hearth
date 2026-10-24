//! Déployer une image depuis le client (HRT-69, ADR-0039) : l'acte `ServiceDeploy` (jamais couvert par
//! l'élévation), l'annulation du déploiement (`OperationCancel`, couverte par l'élévation), la consultation du
//! formulaire et les bornes de la machine. Chaque requête est construite par la liaison (`LinkManager`), chemins
//! fixes ; l'agent répond `202` et l'issue ne se lit JAMAIS ici : elle arrive par le suivi des opérations
//! (HRT-84). Un acte n'est jamais rejoué : un lien tombé avant la réponse donne `Unknown` (BR-DEPLOY-028).
//! Les refus se lisent par leur CODE, jamais par leur texte : aucune valeur reçue n'est répétée (BR-DEPLOY-025).
//! Pur côté lecture ; sans Tauri.

use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use hearth_link::domain::service_ops::{OperationRefusal, operation_path, refusal_of};
use hearth_link::ports::transport::Method;
use hearth_link::{ActionOutcome, ActionRequest, LinkManager};
use hearth_proto::api::deploy::{DeployAccepted, PortStatus};
use hearth_proto::api::service_ops::OperationView;
use hearth_proto::api::services::EngineState;
use hearth_proto::error::{ErrorBody, ErrorCode};
use serde_json::{Value, json};

use super::deploy_dto::{
    CancelOutcome, CancelRefusal, DeployCheckDto, DeployCheckInputDto, DeployFieldDto,
    DeployOutcome, DeployProtocolDto, DeployRefusal, DeployRequestDto, EngineCapacityDto,
    FicheLackDto, PortStatusDto,
};
use super::dto::OperationDto;
use crate::link_dto::{InvalidField, LinkFailure};
use crate::reauth::wire::{Confirmation, confirmation_of};
use crate::services::dto::EngineStateDto;

/// Les constats du formulaire (`POST /deployments/check`) : une consultation, sans secret ni journal.
pub async fn check(
    manager: &LinkManager,
    id: &ServerId,
    input: DeployCheckInputDto,
) -> Result<DeployCheckDto, LinkFailure> {
    let answer = manager.deploy_check(id, &input.into()).await?;
    Ok(DeployCheckDto::from(&answer))
}

/// Les bornes de la machine (`GET /engine/capacity`).
pub async fn capacity(
    manager: &LinkManager,
    id: &ServerId,
) -> Result<EngineCapacityDto, LinkFailure> {
    let answer = manager.engine_capacity(id).await?;
    Ok(EngineCapacityDto::from(&answer))
}

/// Déploie une image. `admin_password` : le mot de passe de confirmation, exigé à chaque fois (jamais couvert par
/// l'élévation ; l'agent est l'arbitre).
pub async fn deploy(
    manager: &LinkManager,
    id: &ServerId,
    request: DeployRequestDto,
    admin_password: Option<&Secret>,
) -> Result<DeployOutcome, LinkFailure> {
    let spec = request.into_spec()?;
    match manager.deploy_service(id, &spec, admin_password).await? {
        ActionOutcome::Completed { status, body, .. } => interpret_deploy(status, &body),
        ActionOutcome::ResultUnknown { id } => Ok(DeployOutcome::Unknown {
            op_id: id.as_str().to_owned(),
        }),
    }
}

/// La requête d'une annulation. L'identifiant ne peut pas devenir un autre chemin.
pub fn cancel_plan(op_id: &str) -> Result<ActionRequest, LinkFailure> {
    let path = operation_path(op_id).ok_or(LinkFailure::InvalidInput {
        field: InvalidField::Other,
    })?;
    Ok(ActionRequest {
        method: Method::Delete,
        path,
        body: Some(json!({})),
    })
}

/// Annule l'opération `op_id` (le déploiement, tant que l'image se récupère). `admin_password` : `None` sous
/// l'élévation de cinq minutes.
pub async fn cancel(
    manager: &LinkManager,
    id: &ServerId,
    op_id: &str,
    admin_password: Option<&Secret>,
) -> Result<CancelOutcome, LinkFailure> {
    let request = cancel_plan(op_id)?;
    match manager.execute_act(id, request, admin_password).await? {
        ActionOutcome::Completed { status, body, .. } => interpret_cancel(status, &body),
        ActionOutcome::ResultUnknown { id } => Ok(CancelOutcome::Unknown {
            op_id: id.as_str().to_owned(),
        }),
    }
}

/// Un code stable de l'agent (`reason`) : minuscules, chiffres et tiret bas, court. Tout autre texte n'est pas
/// un code et n'est jamais répété.
fn stable_code(text: &str) -> Option<String> {
    let valid = !text.is_empty()
        && text.len() <= 48
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    valid.then(|| text.to_owned())
}

/// Le code stable d'un refus (`details.reason`), ou `invalid` : jamais un texte libre de l'agent.
fn reason_code(details: &Value) -> String {
    details
        .get("reason")
        .and_then(Value::as_str)
        .and_then(stable_code)
        .unwrap_or_else(|| "invalid".to_owned())
}

/// Un identifiant de fiche, de question ou de contrat, tel que la liaison le définit ; tout autre texte n'est jamais
/// répété.
fn stable_id(text: &str) -> Option<String> {
    hearth_link::is_stable_id(text).then(|| text.to_owned())
}

/// Ce qui manque à la machine, dit par l'agent : un autre mot n'est pas un manque.
fn lack_of(text: &str) -> Option<FicheLackDto> {
    match text {
        "cpu" => Some(FicheLackDto::Cpu),
        "memory" => Some(FicheLackDto::Memory),
        _ => None,
    }
}

/// Les contrats nommés dans un refus : au plus ce que la fiche peut en porter.
const MAX_REFUSED_IDS: usize = 4;

fn number<T: TryFrom<u64>>(details: &Value, key: &str) -> Option<T> {
    details
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| T::try_from(value).ok())
}

/// La réponse de l'agent à `POST /deployments` : `202` avec l'opération, ou un refus typé par son code.
pub fn interpret_deploy(status: u16, body: &Value) -> Result<DeployOutcome, LinkFailure> {
    if (200..300).contains(&status) {
        let accepted: DeployAccepted =
            serde_json::from_value(body.clone()).map_err(|_| LinkFailure::NotAgent)?;
        return Ok(DeployOutcome::Accepted {
            operation_id: accepted.operation_id,
            service_key: accepted.service_key,
        });
    }
    let refused = |refusal| Ok(DeployOutcome::Refused { refusal });
    let Ok(parsed) = serde_json::from_value::<ErrorBody>(body.clone()) else {
        if status == 403 {
            return Err(LinkFailure::Forbidden);
        }
        return refused(DeployRefusal::Other);
    };
    if let Some(confirmation) = confirmation_of(&parsed)? {
        return refused(match confirmation {
            Confirmation::WrongPassword => DeployRefusal::WrongPassword,
            Confirmation::PasswordRequired => DeployRefusal::PasswordRequired,
            Confirmation::TooManyAttempts { retry_after_s } => {
                DeployRefusal::TooManyAttempts { retry_after_s }
            }
            Confirmation::Busy => DeployRefusal::Busy,
        });
    }
    let error = &parsed.error;
    let details = &error.details;
    // Code inconnu (agent plus récent) : le sens du statut HTTP (BR-COMPAT-001 point 8).
    match error.code.or_from_status(status) {
        // Le droit retiré entre-temps : l'interface le relit et le dit (BR-GRANT-010).
        ErrorCode::ForbiddenRole => Err(LinkFailure::forbidden(error)),
        ErrorCode::EngineUnavailable => refused(DeployRefusal::EngineUnavailable {
            engine: details
                .get("engine")
                .and_then(|engine| serde_json::from_value::<EngineState>(engine.clone()).ok())
                .map(EngineStateDto::from),
        }),
        ErrorCode::ValidationError => {
            let field = details
                .get("field")
                .and_then(Value::as_str)
                .and_then(DeployFieldDto::parse);
            match field {
                Some(field) => refused(DeployRefusal::Invalid {
                    field,
                    reason: reason_code(details),
                }),
                None => refused(DeployRefusal::Other),
            }
        }
        ErrorCode::FolderRefused => refused(DeployRefusal::Invalid {
            field: DeployFieldDto::Mounts,
            reason: reason_code(details),
        }),
        ErrorCode::PlanSettingRefused => refused(DeployRefusal::SettingRefused),
        ErrorCode::LimitsRequired => refused(DeployRefusal::LimitsRequired),
        ErrorCode::NameTaken => refused(DeployRefusal::NameTaken),
        ErrorCode::NameBeingDeleted => refused(DeployRefusal::NameBeingDeleted),
        ErrorCode::PortInUse => match number::<u16>(details, "port") {
            Some(port) => refused(DeployRefusal::PortInUse {
                port,
                protocol: details
                    .get("protocol")
                    .and_then(|protocol| serde_json::from_value(protocol.clone()).ok())
                    .map_or(
                        DeployProtocolDto::Tcp,
                        |protocol: hearth_proto::api::deploy::DeployProtocol| protocol.into(),
                    ),
                status: details
                    .get("status")
                    .and_then(|status| serde_json::from_value::<PortStatus>(status.clone()).ok())
                    .map_or(PortStatusDto::Unknown, Into::into),
                holder: details
                    .get("holder")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                suggested: number(details, "suggested"),
            }),
            None => refused(DeployRefusal::Other),
        },
        ErrorCode::DeployLimit => refused(DeployRefusal::DeployLimit),
        ErrorCode::NoSpace => {
            let install = details.get("reason").and_then(Value::as_str) == Some("install");
            match (
                install,
                number::<u64>(details, "needed_bytes"),
                number::<u64>(details, "free_bytes"),
                number::<u64>(details, "missing_bytes"),
            ) {
                (true, Some(needed), Some(free), Some(missing)) => {
                    refused(DeployRefusal::NoSpaceToInstall {
                        needed_bytes: needed as f64,
                        free_bytes: free as f64,
                        missing_bytes: missing as f64,
                    })
                }
                _ => refused(DeployRefusal::NoSpace {
                    free_bytes: number::<u64>(details, "free_bytes").map(|bytes| bytes as f64),
                }),
            }
        }
        // Installer depuis une fiche (HRT-102) : des refus typés par leur code, jamais par leur texte.
        ErrorCode::FicheUnknown => refused(DeployRefusal::FicheUnknown),
        ErrorCode::FicheChanged => refused(DeployRefusal::FicheChanged),
        ErrorCode::AgreementRequired => refused(DeployRefusal::AgreementRequired {
            ids: details
                .get("ids")
                .and_then(Value::as_array)
                .map(|ids| {
                    ids.iter()
                        .filter_map(Value::as_str)
                        .filter_map(stable_id)
                        .take(MAX_REFUSED_IDS)
                        .collect()
                })
                .unwrap_or_default(),
        }),
        ErrorCode::SettingRefused => refused(DeployRefusal::AnswerRefused {
            id: details
                .get("id")
                .and_then(Value::as_str)
                .and_then(stable_id),
            reason: reason_code(details),
            // Au plus UN caractère : jamais la valeur reçue.
            character: details
                .get("char")
                .and_then(Value::as_str)
                .and_then(|text| text.chars().next())
                .map(String::from),
        }),
        ErrorCode::HostTooSmall => match (
            details
                .get("lack")
                .and_then(Value::as_str)
                .and_then(lack_of),
            number::<u64>(details, "needed"),
            number::<u64>(details, "has"),
        ) {
            (Some(lack), Some(needed), Some(has)) => refused(DeployRefusal::HostTooSmall {
                lack,
                needed: needed as f64,
                has: has as f64,
            }),
            _ => refused(DeployRefusal::Other),
        },
        _ => refused(DeployRefusal::Other),
    }
}

/// La réponse de l'agent à `DELETE /operations/services/{id}` : `200` avec l'opération, ou un refus typé.
pub fn interpret_cancel(status: u16, body: &Value) -> Result<CancelOutcome, LinkFailure> {
    if (200..300).contains(&status) {
        let view: OperationView =
            serde_json::from_value(body.clone()).map_err(|_| LinkFailure::NotAgent)?;
        return Ok(CancelOutcome::Done {
            operation: Box::new(OperationDto::from(&view)),
        });
    }
    match serde_json::from_value::<ErrorBody>(body.clone()) {
        Ok(parsed) => {
            if let Some(confirmation) = confirmation_of(&parsed)? {
                return Ok(refused(match confirmation {
                    Confirmation::WrongPassword => CancelRefusal::WrongPassword,
                    Confirmation::PasswordRequired => CancelRefusal::PasswordRequired,
                    Confirmation::TooManyAttempts { retry_after_s } => {
                        CancelRefusal::TooManyAttempts { retry_after_s }
                    }
                    Confirmation::Busy => CancelRefusal::Busy,
                }));
            }
            // Le droit retiré entre-temps : l'interface le relit et le dit (BR-GRANT-010).
            if parsed.error.code.or_from_status(status) == ErrorCode::ForbiddenRole {
                return Err(LinkFailure::forbidden(&parsed.error));
            }
        }
        Err(_) if status == 403 => return Err(LinkFailure::Forbidden),
        Err(_) => {}
    }
    Ok(refused(match refusal_of(status, body) {
        OperationRefusal::NotFound
        | OperationRefusal::Other {
            status: 404,
            code: None,
        } => CancelRefusal::NotFound,
        OperationRefusal::TooLate => CancelRefusal::TooLate,
        OperationRefusal::NotCancellable { .. } => CancelRefusal::NotCancellable,
        OperationRefusal::Changed => CancelRefusal::Changed,
        OperationRefusal::Other {
            code: Some(ErrorCode::Busy),
            ..
        } => CancelRefusal::Busy,
        OperationRefusal::DecisionNotOffered(_)
        | OperationRefusal::MissingRight(_)
        | OperationRefusal::Other { .. } => CancelRefusal::Other,
    }))
}

fn refused(refusal: CancelRefusal) -> CancelOutcome {
    CancelOutcome::Refused { refusal }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service_ops::deploy_dto::{
        DeployAcceptDto, DeployEnvDto, DeployPortDto, DeployResourcesDto,
    };

    fn error(code: &str, details: Value) -> Value {
        json!({ "error": { "code": code, "message": "SENTINEL-MESSAGE", "details": details } })
    }

    fn read(status: u16, body: Value) -> DeployRefusal {
        match interpret_deploy(status, &body).unwrap() {
            DeployOutcome::Refused { refusal } => refusal,
            other => panic!("refus attendu, reçu {other:?}"),
        }
    }

    fn request() -> DeployRequestDto {
        DeployRequestDto {
            name: "web".into(),
            image: "nginx".into(),
            tag: Some("1.27".into()),
            ports: vec![DeployPortDto {
                host_port: 8080,
                container_port: 80,
                protocol: DeployProtocolDto::Tcp,
            }],
            env: vec![DeployEnvDto {
                name: "DB_PASSWORD".into(),
                value: "SENTINEL-VALUE".into(),
            }],
            command: Some("run --token SENTINEL-CMD".into()),
            resources: DeployResourcesDto {
                memory_bytes: 1_073_741_824.0,
                cpu_millis: 1000,
            },
            accept: DeployAcceptDto {
                reserved_ports: true,
                low_disk: false,
            },
        }
    }

    #[test]
    fn the_request_becomes_the_closed_plan_without_any_folder() {
        let spec = request().into_spec().unwrap();
        assert_eq!(spec.name, "web");
        assert_eq!(spec.tag.as_deref(), Some("1.27"));
        assert_eq!(spec.ports.len(), 1);
        assert!(spec.mounts.is_empty());
        assert!(spec.extra.is_empty());
        assert_eq!(spec.resources.memory_bytes, Some(1_073_741_824));
        assert_eq!(spec.resources.cpu_millis, Some(1000));
        assert!(spec.accept.reserved_ports && !spec.accept.low_disk);
        // Le corps est exactement celui que l'agent relit (sans membre inconnu).
        let body = serde_json::to_value(&spec).unwrap();
        assert!(
            body.get("mounts")
                .is_some_and(|mounts| mounts == &json!([]))
        );
    }

    #[test]
    fn a_memory_that_is_not_an_exact_count_of_bytes_never_leaves() {
        for bad in [f64::NAN, f64::INFINITY, -1.0, 1.5, 1e300] {
            let mut asked = request();
            asked.resources.memory_bytes = bad;
            assert!(
                matches!(
                    asked.into_spec(),
                    Err(LinkFailure::InvalidInput {
                        field: InvalidField::Other
                    })
                ),
                "{bad}"
            );
        }
    }

    #[test]
    fn debug_never_shows_a_value_a_variable_name_or_the_command() {
        let shown = format!("{:?}", request());
        assert!(
            !shown.contains("SENTINEL") && !shown.contains("DB_PASSWORD"),
            "{shown}"
        );
    }

    #[test]
    fn an_accepted_deployment_carries_the_operation_and_the_key_never_the_outcome() {
        assert_eq!(
            interpret_deploy(
                202,
                &json!({ "operation_id": "01J", "service_key": "h_01J" })
            )
            .unwrap(),
            DeployOutcome::Accepted {
                operation_id: "01J".into(),
                service_key: "h_01J".into()
            }
        );
        // Une réponse illisible n'est pas celle d'un agent de Hearth.
        assert!(matches!(
            interpret_deploy(202, &json!({ "nope": 1 })),
            Err(LinkFailure::NotAgent)
        ));
    }

    #[test]
    fn each_refusal_before_any_gesture_is_read_by_its_code() {
        assert_eq!(
            read(422, error("WRONG_PASSWORD", json!(null))),
            DeployRefusal::WrongPassword
        );
        assert_eq!(
            read(
                429,
                error("TOO_MANY_ATTEMPTS", json!({ "retry_after_s": 30 }))
            ),
            DeployRefusal::TooManyAttempts { retry_after_s: 30 }
        );
        assert_eq!(
            read(
                503,
                error("ENGINE_UNAVAILABLE", json!({ "engine": "stopped" }))
            ),
            DeployRefusal::EngineUnavailable {
                engine: Some(EngineStateDto::Stopped)
            }
        );
        assert_eq!(
            read(
                422,
                error(
                    "VALIDATION_ERROR",
                    json!({ "field": "name", "reason": "too_long" })
                )
            ),
            DeployRefusal::Invalid {
                field: DeployFieldDto::Name,
                reason: "too_long".into()
            }
        );
        assert_eq!(
            read(
                422,
                error(
                    "FOLDER_REFUSED",
                    json!({ "field": "mounts", "index": 0, "reason": "protected" })
                )
            ),
            DeployRefusal::Invalid {
                field: DeployFieldDto::Mounts,
                reason: "protected".into()
            }
        );
        assert_eq!(
            read(422, error("PLAN_SETTING_REFUSED", json!(null))),
            DeployRefusal::SettingRefused
        );
        assert_eq!(
            read(422, error("LIMITS_REQUIRED", json!(null))),
            DeployRefusal::LimitsRequired
        );
        assert_eq!(
            read(409, error("NAME_TAKEN", json!(null))),
            DeployRefusal::NameTaken
        );
        assert_eq!(
            read(409, error("NAME_BEING_DELETED", json!(null))),
            DeployRefusal::NameBeingDeleted
        );
        assert_eq!(
            read(
                409,
                error(
                    "PORT_IN_USE",
                    json!({ "port": 8080, "protocol": "udp", "status": "held_by_program",
                            "holder": "sshd", "suggested": 8081 })
                )
            ),
            DeployRefusal::PortInUse {
                port: 8080,
                protocol: DeployProtocolDto::Udp,
                status: PortStatusDto::HeldByProgram,
                holder: Some("sshd".into()),
                suggested: Some(8081)
            }
        );
        assert_eq!(
            read(409, error("DEPLOY_LIMIT", json!(null))),
            DeployRefusal::DeployLimit
        );
        assert_eq!(
            read(
                507,
                error(
                    "NO_SPACE",
                    json!({ "reason": "low_disk", "free_bytes": 1024 })
                )
            ),
            DeployRefusal::NoSpace {
                free_bytes: Some(1024.0)
            }
        );
        assert_eq!(
            read(418, error("FROM_THE_FUTURE", json!({}))),
            DeployRefusal::Other
        );
    }

    #[test]
    fn what_the_client_does_not_know_never_becomes_a_phrase_of_the_agent() {
        // Un champ inconnu n'est pas un champ ; une raison qui n'est pas un code n'est pas répétée.
        assert_eq!(
            read(
                422,
                error(
                    "VALIDATION_ERROR",
                    json!({ "field": "teleporter", "reason": "x" })
                )
            ),
            DeployRefusal::Other
        );
        assert_eq!(
            read(
                422,
                error(
                    "VALIDATION_ERROR",
                    json!({ "field": "env", "reason": "SENTINEL token=abc" })
                )
            ),
            DeployRefusal::Invalid {
                field: DeployFieldDto::Env,
                reason: "invalid".into()
            }
        );
        // Un port illisible : pas un refus de port.
        assert_eq!(
            read(409, error("PORT_IN_USE", json!({ "port": "x" }))),
            DeployRefusal::Other
        );
        // Un état de port inconnu se lit inconnu.
        let DeployRefusal::PortInUse { status, .. } = read(
            409,
            error("PORT_IN_USE", json!({ "port": 80, "status": "levitating" })),
        ) else {
            panic!("port attendu");
        };
        assert_eq!(status, PortStatusDto::Unknown);
        // Le message de l'agent n'est jamais dans ce que l'interface reçoit.
        let shown = format!("{:?}", read(409, error("NAME_TAKEN", json!(null))));
        assert!(!shown.contains("SENTINEL"));
    }

    #[test]
    fn the_right_withdrawn_in_between_is_a_failure_the_interface_reads_apart() {
        let body = error("FORBIDDEN_ROLE", json!({ "missing": "services" }));
        assert!(matches!(
            interpret_deploy(403, &body),
            Err(LinkFailure::MissingServicesRight)
        ));
        assert!(matches!(
            interpret_cancel(403, &body),
            Err(LinkFailure::MissingServicesRight)
        ));
    }

    #[test]
    fn the_cancel_request_is_fixed_and_the_identifier_never_becomes_another_path() {
        let request = cancel_plan("01J9ZY0G3Q8M2K6W4T7V5N1B9D").unwrap();
        assert_eq!(request.method, Method::Delete);
        assert_eq!(
            request.path,
            "/operations/services/01J9ZY0G3Q8M2K6W4T7V5N1B9D"
        );
        for bad in ["", "../x", "a/b", "a b", "a?x=1", &"A".repeat(65)] {
            assert!(cancel_plan(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn each_refusal_of_a_cancellation_is_read_by_its_code() {
        let read = |status, body: Value| match interpret_cancel(status, &body).unwrap() {
            CancelOutcome::Refused { refusal } => refusal,
            other => panic!("refus attendu, reçu {other:?}"),
        };
        assert_eq!(
            read(404, error("OPERATION_NOT_FOUND", json!({}))),
            CancelRefusal::NotFound
        );
        assert_eq!(
            read(409, error("TOO_LATE_TO_CANCEL", json!({}))),
            CancelRefusal::TooLate
        );
        assert_eq!(
            read(
                409,
                error(
                    "CONFLICT",
                    json!({ "reason": "not_cancellable", "status": "succeeded" })
                )
            ),
            CancelRefusal::NotCancellable
        );
        assert_eq!(
            read(
                409,
                error("CONFLICT", json!({ "reason": "operation_changed" }))
            ),
            CancelRefusal::Changed
        );
        assert_eq!(
            read(422, error("WRONG_PASSWORD", json!(null))),
            CancelRefusal::WrongPassword
        );
        assert_eq!(
            read(418, error("FROM_THE_FUTURE", json!({}))),
            CancelRefusal::Other
        );
    }

    #[test]
    fn a_cancellation_carries_the_operation_after_the_cancel() {
        let body = json!({
            "id": "01J", "key": "h_01J", "kind": "deploy", "status": "cancelling",
            "cancellable": false
        });
        let CancelOutcome::Done { operation } = interpret_cancel(200, &body).unwrap() else {
            panic!("succès attendu");
        };
        assert_eq!(operation.id, "01J");
    }

    #[test]
    fn the_check_never_carries_a_field_this_client_does_not_know() {
        let check: hearth_proto::api::deploy::DeployCheck = serde_json::from_value(json!({
            "name": "taken",
            "invalid_fields": ["image", "SENTINEL token", "mounts"],
            "ports": [{ "host_port": 80, "protocol": "tcp", "status": "free", "privileged": true }],
            "mounts": [],
            "disk": { "free_bytes": 2048, "low": true },
            "image_present": true
        }))
        .unwrap();
        let dto = DeployCheckDto::from(&check);
        assert_eq!(dto.invalid_fields, vec!["image", "mounts"]);
        assert_eq!(dto.ports.len(), 1);
        assert!(dto.ports[0].privileged);
        assert_eq!(dto.disk.map(|disk| disk.free_bytes), Some(2048.0));
        assert!(dto.image_present);
    }
}
