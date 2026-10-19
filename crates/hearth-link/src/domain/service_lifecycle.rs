//! Démarrer, arrêter, redémarrer un service : la requête et la lecture du refus (HRT-65, BR-LIFE-003, 013, 014,
//! 015, 019). Règles pures, sans E/S. L'agent répond par un code d'erreur stable et des `details` fermés
//! (`engine`, `state`, `by`, `kind`) ; la liaison les range dans une liste fermée que l'interface met en phrases.
//! Tout ce qu'elle ne connaît pas (un code plus récent, un état inconnu) tombe dans une variante de repli :
//! jamais une erreur de lecture, jamais un refus confondu avec un autre. Aucun texte de l'agent n'est repris.

use hearth_proto::api::service_ops::OperationKind;
use hearth_proto::api::services::{EngineState, ServiceState};
use hearth_proto::error::{ErrorBody, ErrorCode};
use serde_json::Value;

/// Les trois gestes de pilotage d'un service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gesture {
    Start,
    Stop,
    Restart,
}

impl Gesture {
    /// Le dernier segment du chemin de l'acte (`/services/{key}/{segment}`).
    pub const fn segment(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
        }
    }
}

/// Pourquoi l'agent refuse un geste de pilotage (rien n'a été ordonné ni modifié).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// Le moteur n'est pas prêt (BR-LIFE-019) ; son état si l'agent l'a dit et que ce client le connaît.
    EngineUnavailable { engine: Option<EngineState> },
    /// Le service n'existe plus, ou n'est pas publié : même réponse (BR-LIFE-015).
    ServiceNotFound,
    /// Une action tient déjà le service (BR-LIFE-013) : le compte qui l'a lancée et son genre, si l'agent les dit.
    ServiceBusy {
        by: Option<String>,
        kind: Option<OperationKind>,
    },
    /// Le service est déjà dans l'état demandé (BR-LIFE-014).
    AlreadyInState { state: Option<ServiceState> },
    /// L'état du service ne permet pas ce geste (BR-LIFE-003).
    NotDrivable { state: Option<ServiceState> },
    /// Tout autre refus (confirmation, session, droit, version…), avec son code s'il se lit.
    Other {
        status: u16,
        code: Option<ErrorCode>,
    },
}

fn state_of(details: &Value) -> Option<ServiceState> {
    details
        .get("state")
        .and_then(|state| serde_json::from_value(state.clone()).ok())
}

/// Lit la réponse d'erreur de l'agent (`status` HTTP et corps).
pub fn refusal_of(status: u16, body: &Value) -> Refusal {
    let Ok(parsed) = serde_json::from_value::<ErrorBody>(body.clone()) else {
        return Refusal::Other { status, code: None };
    };
    let error = parsed.error;
    match error.code {
        ErrorCode::EngineUnavailable => Refusal::EngineUnavailable {
            engine: error
                .details
                .get("engine")
                .and_then(|engine| serde_json::from_value(engine.clone()).ok()),
        },
        ErrorCode::ServiceNotFound => Refusal::ServiceNotFound,
        ErrorCode::ServiceBusy => Refusal::ServiceBusy {
            by: error
                .details
                .get("by")
                .and_then(Value::as_str)
                .map(str::to_owned),
            kind: error
                .details
                .get("kind")
                .and_then(|kind| serde_json::from_value(kind.clone()).ok()),
        },
        ErrorCode::ServiceAlreadyInState => Refusal::AlreadyInState {
            state: state_of(&error.details),
        },
        ErrorCode::ServiceNotDrivable => Refusal::NotDrivable {
            state: state_of(&error.details),
        },
        // Un code d'un agent plus récent : le sens de son statut HTTP (BR-COMPAT-001 point 8).
        ErrorCode::Unknown => Refusal::Other {
            status,
            code: match ErrorCode::Unknown.or_from_status(status) {
                ErrorCode::Unknown => None,
                meaning => Some(meaning),
            },
        },
        code => Refusal::Other {
            status,
            code: Some(code),
        },
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn body(code: &str, details: Value) -> Value {
        json!({ "error": { "code": code, "message": "x", "details": details } })
    }

    #[test]
    fn each_refusal_of_the_agent_has_its_own_variant() {
        assert_eq!(
            refusal_of(
                503,
                &body("ENGINE_UNAVAILABLE", json!({ "engine": "stopped" }))
            ),
            Refusal::EngineUnavailable {
                engine: Some(EngineState::Stopped)
            }
        );
        assert_eq!(
            refusal_of(404, &body("SERVICE_NOT_FOUND", json!({}))),
            Refusal::ServiceNotFound
        );
        assert_eq!(
            refusal_of(
                409,
                &body("SERVICE_BUSY", json!({ "by": "marie", "kind": "backup" }))
            ),
            Refusal::ServiceBusy {
                by: Some("marie".into()),
                kind: Some(OperationKind::Backup)
            }
        );
        assert_eq!(
            refusal_of(
                409,
                &body("SERVICE_ALREADY_IN_STATE", json!({ "state": "running" }))
            ),
            Refusal::AlreadyInState {
                state: Some(ServiceState::Running)
            }
        );
        assert_eq!(
            refusal_of(
                409,
                &body("SERVICE_NOT_DRIVABLE", json!({ "state": "paused" }))
            ),
            Refusal::NotDrivable {
                state: Some(ServiceState::Paused)
            }
        );
    }

    #[test]
    fn what_the_link_does_not_know_never_becomes_an_error_of_reading() {
        // Un état inconnu se lit `Unknown`, un détail absent `None`, un code inconnu garde le sens de son statut.
        assert_eq!(
            refusal_of(
                409,
                &body("SERVICE_NOT_DRIVABLE", json!({ "state": "levitating" }))
            ),
            Refusal::NotDrivable {
                state: Some(ServiceState::Unknown)
            }
        );
        assert_eq!(
            refusal_of(409, &body("SERVICE_BUSY", json!(null))),
            Refusal::ServiceBusy {
                by: None,
                kind: None
            }
        );
        assert_eq!(
            refusal_of(418, &body("SOMETHING_FROM_THE_FUTURE", json!({}))),
            Refusal::Other {
                status: 418,
                code: None
            }
        );
        assert_eq!(
            refusal_of(500, &json!("pas du json d'erreur")),
            Refusal::Other {
                status: 500,
                code: None
            }
        );
        assert_eq!(
            refusal_of(409, &body("POST_NOT_RECOGNIZED", json!({}))),
            Refusal::Other {
                status: 409,
                code: Some(ErrorCode::PostNotRecognized)
            }
        );
    }
}
