//! Démarrer, arrêter, redémarrer un service (HRT-65, BR-LIFE-016) : trois actes d'administration confirmés,
//! couverts par l'élévation de cinq minutes. La requête est construite par la liaison
//! (`LinkManager::service_gesture`, chemin fixe, clé vérifiée et encodée), envoyée par `execute_act` (clé
//! d'opération, résultat inconnu à la coupure, jamais rejouée) et la réponse lue par son code. L'agent répond
//! `202` : l'issue ne se lit JAMAIS ici, elle arrive par le suivi des opérations (HRT-84). Pur côté lecture ;
//! sans Tauri.

use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use hearth_link::domain::service_lifecycle::{Gesture, Refusal, refusal_of};
use hearth_link::{ActionOutcome, LinkManager};
use hearth_proto::api::services::ActionAccepted;
use hearth_proto::error::{ErrorBody, ErrorCode};
use serde::Serialize;
use serde_json::Value;
use specta::Type;

use super::dto::OperationKindDto;
use crate::link_dto::LinkFailure;
use crate::reauth::wire::{Confirmation, confirmation_of};
use crate::services::dto::{EngineStateDto, ServiceStateDto};

/// Le geste demandé par l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum GestureDto {
    Start,
    Stop,
    Restart,
}

impl From<GestureDto> for Gesture {
    fn from(gesture: GestureDto) -> Self {
        match gesture {
            GestureDto::Start => Self::Start,
            GestureDto::Stop => Self::Stop,
            GestureDto::Restart => Self::Restart,
        }
    }
}

/// Pourquoi l'agent a refusé le geste. Sans texte : l'interface choisit le message d'après `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LifecycleRefusal {
    /// Le mot de passe de confirmation est incorrect.
    WrongPassword,
    /// L'élévation s'est fermée côté agent : la fenêtre redemande le mot de passe sans perdre la saisie.
    PasswordRequired,
    /// Trop d'essais de mot de passe.
    TooManyAttempts { retry_after_s: u32 },
    /// L'agent est saturé : réessayer dans un instant.
    Busy,
    /// Le moteur n'est pas prêt (BR-LIFE-019) ; son état, si l'agent l'a dit.
    EngineUnavailable { engine: Option<EngineStateDto> },
    /// Le service n'existe plus (BR-LIFE-015).
    ServiceNotFound,
    /// Une action tient déjà le service (BR-LIFE-013) ; qui, et laquelle.
    ServiceBusy {
        by: Option<String>,
        action: Option<OperationKindDto>,
    },
    /// Le service est déjà dans l'état demandé (BR-LIFE-014).
    AlreadyInState { state: Option<ServiceStateDto> },
    /// L'état du service ne permet pas ce geste (BR-LIFE-003).
    NotDrivable { state: Option<ServiceStateDto> },
    /// Tout autre refus.
    Other,
}

/// L'issue d'un geste de pilotage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LifecycleOutcome {
    /// L'agent a accepté : l'opération est lancée à l'arrière-plan (`202`). L'issue arrive par le flux.
    Accepted {
        #[serde(rename = "actionId")]
        action_id: String,
        /// Le délai d'arrêt du service, en secondes (BR-LIFE-009), pour le décompte.
        #[serde(rename = "graceS")]
        grace_s: u32,
    },
    Refused {
        refusal: LifecycleRefusal,
    },
    /// Le lien est tombé avant la réponse : on ne sait pas. Jamais rejoué ; l'issue se lit sur le flux.
    Unknown {
        #[serde(rename = "opId")]
        op_id: String,
    },
}

/// Envoie le geste `gesture` pour le service `key`. `admin_password` : le mot de passe de confirmation (`None` :
/// l'élévation de cinq minutes est ouverte ; l'agent est l'arbitre).
pub async fn act(
    manager: &LinkManager,
    id: &ServerId,
    key: &str,
    gesture: GestureDto,
    admin_password: Option<&Secret>,
) -> Result<LifecycleOutcome, LinkFailure> {
    match manager
        .service_gesture(id, key, gesture.into(), admin_password)
        .await?
    {
        ActionOutcome::Completed { status, body, .. } => interpret(status, &body),
        ActionOutcome::ResultUnknown { id } => Ok(LifecycleOutcome::Unknown {
            op_id: id.as_str().to_owned(),
        }),
    }
}

/// La réponse de l'agent : `202` avec l'opération, ou un refus typé par son code. Les refus propres au pilotage
/// sont lus par la liaison (`refusal_of`, source unique) ; seuls ceux de la confirmation et le droit retiré se
/// lisent ici, comme pour tout acte de la coquille.
pub fn interpret(status: u16, body: &Value) -> Result<LifecycleOutcome, LinkFailure> {
    if (200..300).contains(&status) {
        let accepted: ActionAccepted =
            serde_json::from_value(body.clone()).map_err(|_| LinkFailure::NotAgent)?;
        return Ok(LifecycleOutcome::Accepted {
            action_id: accepted.action_id,
            grace_s: accepted.grace_s,
        });
    }
    match serde_json::from_value::<ErrorBody>(body.clone()) {
        Ok(parsed) => {
            if let Some(confirmation) = confirmation_of(&parsed)? {
                return Ok(refused(match confirmation {
                    Confirmation::WrongPassword => LifecycleRefusal::WrongPassword,
                    Confirmation::PasswordRequired => LifecycleRefusal::PasswordRequired,
                    Confirmation::TooManyAttempts { retry_after_s } => {
                        LifecycleRefusal::TooManyAttempts { retry_after_s }
                    }
                    Confirmation::Busy => LifecycleRefusal::Busy,
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
        Refusal::EngineUnavailable { engine } => LifecycleRefusal::EngineUnavailable {
            engine: engine.map(Into::into),
        },
        Refusal::ServiceNotFound
        | Refusal::Other {
            status: 404,
            code: None,
        } => LifecycleRefusal::ServiceNotFound,
        Refusal::ServiceBusy { by, kind } => LifecycleRefusal::ServiceBusy {
            by,
            action: kind.map(Into::into),
        },
        Refusal::AlreadyInState { state } => LifecycleRefusal::AlreadyInState {
            state: state.map(Into::into),
        },
        Refusal::NotDrivable { state } => LifecycleRefusal::NotDrivable {
            state: state.map(Into::into),
        },
        Refusal::Other { .. } => LifecycleRefusal::Other,
    }))
}

fn refused(refusal: LifecycleRefusal) -> LifecycleOutcome {
    LifecycleOutcome::Refused { refusal }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn error(code: &str, details: Value) -> Value {
        json!({ "error": { "code": code, "message": "x", "details": details } })
    }

    fn read(status: u16, body: Value) -> LifecycleRefusal {
        match interpret(status, &body).unwrap() {
            LifecycleOutcome::Refused { refusal } => refusal,
            other => panic!("refus attendu, reçu {other:?}"),
        }
    }

    #[test]
    fn an_accepted_gesture_carries_the_operation_and_the_grace_never_the_outcome() {
        assert_eq!(
            interpret(202, &json!({ "action_id": "01J", "grace_s": 120 })).unwrap(),
            LifecycleOutcome::Accepted {
                action_id: "01J".into(),
                grace_s: 120
            }
        );
        // Une réponse illisible n'est pas celle d'un agent de Hearth.
        assert!(matches!(
            interpret(202, &json!({ "nope": 1 })),
            Err(LinkFailure::NotAgent)
        ));
    }

    #[test]
    fn each_refusal_is_read_by_its_code() {
        assert_eq!(
            read(422, error("WRONG_PASSWORD", json!(null))),
            LifecycleRefusal::WrongPassword
        );
        assert_eq!(
            read(
                429,
                error("TOO_MANY_ATTEMPTS", json!({ "retry_after_s": 30 }))
            ),
            LifecycleRefusal::TooManyAttempts { retry_after_s: 30 }
        );
        assert_eq!(
            read(
                503,
                error("ENGINE_UNAVAILABLE", json!({ "engine": "stopped" }))
            ),
            LifecycleRefusal::EngineUnavailable {
                engine: Some(EngineStateDto::Stopped)
            }
        );
        assert_eq!(
            read(404, error("SERVICE_NOT_FOUND", json!({}))),
            LifecycleRefusal::ServiceNotFound
        );
        assert_eq!(
            read(
                409,
                error("SERVICE_BUSY", json!({ "by": "marie", "kind": "backup" }))
            ),
            LifecycleRefusal::ServiceBusy {
                by: Some("marie".into()),
                action: Some(OperationKindDto::Backup)
            }
        );
        assert_eq!(
            read(
                409,
                error("SERVICE_ALREADY_IN_STATE", json!({ "state": "stopped" }))
            ),
            LifecycleRefusal::AlreadyInState {
                state: Some(ServiceStateDto::Stopped)
            }
        );
        assert_eq!(
            read(
                409,
                error("SERVICE_NOT_DRIVABLE", json!({ "state": "paused" }))
            ),
            LifecycleRefusal::NotDrivable {
                state: Some(ServiceStateDto::Paused)
            }
        );
        assert_eq!(
            read(418, error("FROM_THE_FUTURE", json!({}))),
            LifecycleRefusal::Other
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
