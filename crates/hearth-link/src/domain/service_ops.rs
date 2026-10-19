//! Le refus d'une annulation ou d'une décision d'opération, typé avec sa raison (HRT-144, ADR-0045) : règles
//! pures. L'agent répond par un code d'erreur stable et, pour une décision, un `details.reason` ; la
//! liaison les range dans une liste fermée que l'interface met en phrases. Tout ce qu'elle ne connaît pas
//! (un code plus récent, une raison inconnue) tombe dans une variante de repli : jamais une erreur de
//! lecture, jamais un refus confondu avec un autre.

use hearth_proto::api::service_ops::OperationStatus;
use hearth_proto::error::{ErrorBody, ErrorCode};
use serde_json::Value;

/// Un identifiant d'opération de l'agent : un ULID, lettres et chiffres. Même borne que l'agent côté acte.
const OPERATION_ID_MAX_LEN: usize = 64;

/// Le chemin d'une opération (`/operations/services/{id}`), ou `None` si l'identifiant n'en est pas un : il ne
/// devient jamais un autre chemin. Source unique de la liaison et de la coquille (HRT-144, HRT-84).
pub fn operation_path(op_id: &str) -> Option<String> {
    let valid = !op_id.is_empty()
        && op_id.len() <= OPERATION_ID_MAX_LEN
        && op_id.bytes().all(|b| b.is_ascii_alphanumeric());
    valid.then(|| format!("/operations/services/{op_id}"))
}

/// Pourquoi l'agent refuse une décision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionReason {
    /// L'opération n'attend aucune décision.
    NotWaiting,
    /// Ce choix n'est pas parmi ceux qu'elle offre.
    NotOffered,
    /// Ce choix ne s'applique pas à son statut.
    NotApplicable,
    /// La décision attendait depuis 15 minutes : elle a expiré.
    Expired,
    /// Une raison que cette liaison ne connaît pas (agent plus récent).
    Unknown,
}

/// Le refus de l'agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationRefusal {
    /// Opération inconnue, purgée, ou sur un service que ce compte ne voit pas (l'agent ne distingue pas).
    NotFound,
    /// Le point de non-retour est passé : l'opération ne s'annule plus.
    TooLate,
    /// L'opération n'est pas dans un statut qui s'annule ; `status` est celui que l'agent donne.
    NotCancellable { status: Option<OperationStatus> },
    /// La décision est refusée.
    DecisionNotOffered(DecisionReason),
    /// L'opération a changé pendant la demande : à relire.
    Changed,
    /// Le compte n'a pas le droit « gérer les services » (`details.missing`).
    MissingRight(String),
    /// Tout autre refus (confirmation, session, droit de rôle, version…), avec son code s'il se lit.
    Other {
        status: u16,
        code: Option<ErrorCode>,
    },
}

fn reason_of(details: &Value) -> DecisionReason {
    match details.get("reason").and_then(Value::as_str) {
        Some("not_waiting") => DecisionReason::NotWaiting,
        Some("not_offered") => DecisionReason::NotOffered,
        Some("not_applicable") => DecisionReason::NotApplicable,
        Some("expired") => DecisionReason::Expired,
        _ => DecisionReason::Unknown,
    }
}

/// Lit la réponse d'erreur de l'agent (`status` HTTP et corps).
pub fn refusal_of(status: u16, body: &Value) -> OperationRefusal {
    let Ok(parsed) = serde_json::from_value::<ErrorBody>(body.clone()) else {
        return OperationRefusal::Other { status, code: None };
    };
    let error = parsed.error;
    if let Some(right) = error.missing_right() {
        return OperationRefusal::MissingRight(right.to_owned());
    }
    match error.code {
        ErrorCode::OperationNotFound => OperationRefusal::NotFound,
        ErrorCode::TooLateToCancel => OperationRefusal::TooLate,
        ErrorCode::DecisionNotOffered => {
            OperationRefusal::DecisionNotOffered(reason_of(&error.details))
        }
        ErrorCode::Conflict => match error.details.get("reason").and_then(Value::as_str) {
            Some("not_cancellable") => OperationRefusal::NotCancellable {
                status: error
                    .details
                    .get("status")
                    .and_then(|status| serde_json::from_value(status.clone()).ok()),
            },
            Some("operation_changed") => OperationRefusal::Changed,
            _ => OperationRefusal::Other {
                status,
                code: Some(ErrorCode::Conflict),
            },
        },
        // Un code d'un agent plus récent : le sens de son statut HTTP (BR-COMPAT-001 point 8), jamais un des
        // refus propres à l'opération ci-dessus, dont les `details` ne seraient pas ceux qu'on croit.
        ErrorCode::Unknown => OperationRefusal::Other {
            status,
            code: match ErrorCode::Unknown.or_from_status(status) {
                ErrorCode::Unknown => None,
                meaning => Some(meaning),
            },
        },
        code => OperationRefusal::Other {
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
            refusal_of(404, &body("OPERATION_NOT_FOUND", json!({}))),
            OperationRefusal::NotFound
        );
        assert_eq!(
            refusal_of(409, &body("TOO_LATE_TO_CANCEL", json!({}))),
            OperationRefusal::TooLate
        );
        for (raw, reason) in [
            ("not_waiting", DecisionReason::NotWaiting),
            ("not_offered", DecisionReason::NotOffered),
            ("not_applicable", DecisionReason::NotApplicable),
            ("expired", DecisionReason::Expired),
            ("something_new", DecisionReason::Unknown),
        ] {
            assert_eq!(
                refusal_of(409, &body("DECISION_NOT_OFFERED", json!({ "reason": raw }))),
                OperationRefusal::DecisionNotOffered(reason)
            );
        }
        assert_eq!(
            refusal_of(
                409,
                &body(
                    "CONFLICT",
                    json!({ "reason": "not_cancellable", "status": "succeeded" })
                )
            ),
            OperationRefusal::NotCancellable {
                status: Some(OperationStatus::Succeeded)
            }
        );
        assert_eq!(
            refusal_of(
                409,
                &body("CONFLICT", json!({ "reason": "operation_changed" }))
            ),
            OperationRefusal::Changed
        );
        assert_eq!(
            refusal_of(
                403,
                &body("FORBIDDEN_ROLE", json!({ "missing": "services" }))
            ),
            OperationRefusal::MissingRight("services".into())
        );
    }

    #[test]
    fn what_the_link_does_not_know_never_becomes_an_error_of_reading() {
        // Un statut inconnu se lit `Unknown`, un code inconnu `None`, un corps illisible garde son statut.
        assert_eq!(
            refusal_of(
                409,
                &body(
                    "CONFLICT",
                    json!({ "reason": "not_cancellable", "status": "levitating" })
                )
            ),
            OperationRefusal::NotCancellable {
                status: Some(OperationStatus::Unknown)
            }
        );
        assert_eq!(
            refusal_of(418, &body("SOMETHING_FROM_THE_FUTURE", json!({}))),
            OperationRefusal::Other {
                status: 418,
                code: None
            }
        );
        // Un code inconnu d'un statut que le protocole emploie : le sens du statut, pas un refus d'opération.
        assert_eq!(
            refusal_of(
                409,
                &body("SOMETHING_FROM_THE_FUTURE", json!({ "reason": "expired" }))
            ),
            OperationRefusal::Other {
                status: 409,
                code: Some(ErrorCode::Conflict)
            }
        );
        assert_eq!(
            refusal_of(500, &json!("pas du json d'erreur")),
            OperationRefusal::Other {
                status: 500,
                code: None
            }
        );
        assert_eq!(
            refusal_of(409, &body("POST_NOT_RECOGNIZED", json!({}))),
            OperationRefusal::Other {
                status: 409,
                code: Some(ErrorCode::PostNotRecognized)
            }
        );
    }
}
