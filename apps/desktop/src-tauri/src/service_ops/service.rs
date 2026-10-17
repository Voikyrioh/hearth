//! La décision d'une opération de service (HRT-84) : un acte d'administration confirmé (`OperationDecide`,
//! jamais couvert par l'élévation : le mot de passe est redemandé à chaque réponse). Construit la requête
//! TYPÉE (méthode, chemin et corps du contrat de l'agent, `docs/open-api/service-ops.md`), l'envoie par
//! `LinkManager::execute_act` (clé d'opération, résultat inconnu à la coupure, jamais rejouée) et lit la
//! réponse par son code. Pur côté lecture ; sans Tauri.

use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use hearth_link::domain::service_ops::{
    DecisionReason, OperationRefusal, operation_path, refusal_of,
};
use hearth_link::ports::transport::Method;
use hearth_link::{ActionOutcome, ActionRequest, LinkManager};
use hearth_proto::api::service_ops::{OperationChoice, OperationView};
use hearth_proto::error::{ErrorBody, ErrorCode};
use serde_json::{Value, json};

use super::dto::{DecisionOutcome, DecisionRefusal, OperationChoiceDto, OperationDto};
use crate::link_dto::{InvalidField, LinkFailure};
use crate::reauth::wire::{Confirmation, confirmation_of};

fn invalid() -> LinkFailure {
    LinkFailure::InvalidInput {
        field: InvalidField::Other,
    }
}

/// La requête d'une décision. L'identifiant ne peut pas devenir un autre chemin ; un choix inconnu de ce
/// client ne part jamais.
pub fn plan(op_id: &str, choice: OperationChoiceDto) -> Result<ActionRequest, LinkFailure> {
    let choice = OperationChoice::from(choice);
    if choice == OperationChoice::Unknown {
        return Err(invalid());
    }
    let path = operation_path(op_id).ok_or_else(invalid)?;
    Ok(ActionRequest {
        method: Method::Post,
        path: format!("{path}/decision"),
        body: Some(json!({ "choice": choice.as_str() })),
    })
}

/// Décide de l'opération `op_id`. `admin_password` : le mot de passe de confirmation (toujours exigé pour cet
/// acte, mais l'agent est l'arbitre : le champ est celui de la fenêtre).
pub async fn decide(
    manager: &LinkManager,
    id: &ServerId,
    op_id: &str,
    choice: OperationChoiceDto,
    admin_password: Option<&Secret>,
) -> Result<DecisionOutcome, LinkFailure> {
    let request = plan(op_id, choice)?;
    match manager.execute_act(id, request, admin_password).await? {
        ActionOutcome::Completed { status, body, .. } => interpret(status, &body),
        ActionOutcome::ResultUnknown { id } => Ok(DecisionOutcome::Unknown {
            op_id: id.as_str().to_owned(),
        }),
    }
}

/// La réponse de l'agent : l'opération après la décision, ou un refus typé par son code. Les refus propres à
/// une opération sont lus par la liaison (`refusal_of`, source unique) ; seuls ceux de la confirmation et le
/// droit retiré se lisent ici, comme pour tout acte de la coquille.
pub fn interpret(status: u16, body: &Value) -> Result<DecisionOutcome, LinkFailure> {
    if (200..300).contains(&status) {
        let view: OperationView =
            serde_json::from_value(body.clone()).map_err(|_| LinkFailure::NotAgent)?;
        return Ok(DecisionOutcome::Done {
            operation: Box::new(OperationDto::from(&view)),
        });
    }
    match serde_json::from_value::<ErrorBody>(body.clone()) {
        Ok(parsed) => {
            // Les refus de la confirmation se lisent partout pareil (BR-TRUST-040, 045).
            if let Some(confirmation) = confirmation_of(&parsed)? {
                return Ok(refused(match confirmation {
                    Confirmation::WrongPassword => DecisionRefusal::WrongPassword,
                    Confirmation::PasswordRequired => DecisionRefusal::PasswordRequired,
                    Confirmation::TooManyAttempts { retry_after_s } => {
                        DecisionRefusal::TooManyAttempts { retry_after_s }
                    }
                    Confirmation::Busy => DecisionRefusal::Busy,
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
    Ok(refused(decision_refusal(refusal_of(status, body))))
}

fn refused(refusal: DecisionRefusal) -> DecisionOutcome {
    DecisionOutcome::Refused { refusal }
}

/// Le refus lu par la liaison, dans la liste fermée de l'interface.
fn decision_refusal(refusal: OperationRefusal) -> DecisionRefusal {
    match refusal {
        OperationRefusal::NotFound
        | OperationRefusal::Other {
            status: 404,
            code: None,
        } => DecisionRefusal::NotFound,
        OperationRefusal::TooLate => DecisionRefusal::TooLate,
        OperationRefusal::DecisionNotOffered(reason) => match reason {
            DecisionReason::NotWaiting => DecisionRefusal::NotWaiting,
            DecisionReason::NotOffered => DecisionRefusal::NotOffered,
            DecisionReason::NotApplicable => DecisionRefusal::NotApplicable,
            DecisionReason::Expired => DecisionRefusal::Expired,
            DecisionReason::Unknown => DecisionRefusal::Other,
        },
        OperationRefusal::Changed => DecisionRefusal::Changed,
        OperationRefusal::Other {
            code: Some(ErrorCode::Busy),
            ..
        } => DecisionRefusal::Busy,
        OperationRefusal::NotCancellable { .. }
        | OperationRefusal::MissingRight(_)
        | OperationRefusal::Other { .. } => DecisionRefusal::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error(code: &str, details: Value) -> Value {
        json!({ "error": { "code": code, "message": "x", "details": details } })
    }

    #[test]
    fn the_decision_request_is_fixed_and_the_identifier_never_becomes_another_path() {
        let request = plan("01J9ZY0G3Q8M2K6W4T7V5N1B9D", OperationChoiceDto::Revert).unwrap();
        assert_eq!(request.method, Method::Post);
        assert_eq!(
            request.path,
            "/operations/services/01J9ZY0G3Q8M2K6W4T7V5N1B9D/decision"
        );
        assert_eq!(request.body, Some(json!({ "choice": "revert" })));
        for bad in ["", "../x", "a/b", "a b", "a?x=1", &"A".repeat(65)] {
            assert!(plan(bad, OperationChoiceDto::Continue).is_err(), "{bad}");
        }
        // Un choix que ce client ne connaît pas ne part jamais.
        assert!(plan("01J", OperationChoiceDto::Unknown).is_err());
    }

    #[test]
    fn each_refusal_is_read_by_its_code() {
        let read = |status, body: Value| match interpret(status, &body).unwrap() {
            DecisionOutcome::Refused { refusal } => refusal,
            other => panic!("refus attendu, reçu {other:?}"),
        };
        assert_eq!(
            read(404, error("OPERATION_NOT_FOUND", json!({}))),
            DecisionRefusal::NotFound
        );
        assert_eq!(
            read(409, error("TOO_LATE_TO_CANCEL", json!({}))),
            DecisionRefusal::TooLate
        );
        for (raw, expected) in [
            ("not_waiting", DecisionRefusal::NotWaiting),
            ("not_offered", DecisionRefusal::NotOffered),
            ("not_applicable", DecisionRefusal::NotApplicable),
            ("expired", DecisionRefusal::Expired),
            ("something_new", DecisionRefusal::Other),
        ] {
            assert_eq!(
                read(409, error("DECISION_NOT_OFFERED", json!({ "reason": raw }))),
                expected
            );
        }
        assert_eq!(
            read(
                409,
                error("CONFLICT", json!({ "reason": "operation_changed" }))
            ),
            DecisionRefusal::Changed
        );
        assert_eq!(
            read(422, error("WRONG_PASSWORD", json!(null))),
            DecisionRefusal::WrongPassword
        );
        assert_eq!(
            read(
                429,
                error("TOO_MANY_ATTEMPTS", json!({ "retry_after_s": 30 }))
            ),
            DecisionRefusal::TooManyAttempts { retry_after_s: 30 }
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

    #[test]
    fn a_success_carries_the_operation_after_the_decision() {
        let body = json!({
            "id": "01J", "key": "h_01J", "kind": "update", "status": "running",
            "cancellable": true
        });
        let DecisionOutcome::Done { operation } = interpret(200, &body).unwrap() else {
            panic!("succès attendu");
        };
        assert_eq!(operation.id, "01J");
    }
}
