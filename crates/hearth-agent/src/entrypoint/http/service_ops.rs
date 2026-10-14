//! Les opérations de service (HRT-144, ADR-0045) : `GET /operations/services/{id}`, `DELETE
//! /operations/services/{id}` (annuler) et `POST /operations/services/{id}/decision` (décider).
//!
//! - **Lire** est ouvert à tout compte authentifié, MAIS une opération sur un service que le compte ne peut
//!   pas voir (service adopté sans le droit, service non publié) a la réponse d'une opération inconnue :
//!   `404 OPERATION_NOT_FOUND`, à l'identique. Une opération se lit 24 heures après sa fin.
//! - **Annuler** et **décider** sont des ACTES d'administration confirmés (`OperationCancel` `0x10`,
//!   `OperationDecide` `0x1D`) : le droit « gérer les services » est exigé par le niveau de la route, la
//!   preuve de clé et le mot de passe par la couche `reauth`. La réussite s'écrit au journal d'activité,
//!   dans la transaction du changement, sous l'action du GENRE de l'opération ; un refus ou un échec de la
//!   route s'écrit sous `operation.cancel` ou `operation.decide` (couche d'accès).
//!
//! Aucune erreur ne répète une clé, un nom, un chemin ni une valeur : les textes sont fixes.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Extension, Path, State};
use hearth_proto::api::service_ops::{OperationDecision, OperationView};
use hearth_proto::error::{ErrorBody, ErrorCode};

use super::auth::{Caller, Requester};
use super::{ApiError, AppState};
use crate::application::service_op_access::{AccessError, DecisionRefusal, Viewer};
use crate::application::sessions::{CurrentSession, Reauthenticated};
use crate::domain::ops::Choice;
use crate::entrypoint::service_ops_wire;

/// Un identifiant d'opération de l'agent : lettres et chiffres, rien d'autre. Un autre texte n'est pas un
/// identifiant, et la réponse est celle d'une opération inconnue.
const ID_MAX_LEN: usize = 64;

fn viewer_of(session: &CurrentSession) -> Viewer {
    Viewer {
        may_manage_services: session.account.may_manage_services(),
    }
}

fn unknown() -> ApiError {
    ApiError::new(
        ErrorCode::OperationNotFound,
        "Cette opération n'existe pas, ou plus.",
    )
}

fn checked_id(id: &str) -> Result<&str, ApiError> {
    let valid =
        !id.is_empty() && id.len() <= ID_MAX_LEN && id.bytes().all(|b| b.is_ascii_alphanumeric());
    if valid { Ok(id) } else { Err(unknown()) }
}

fn refusal_reason(refusal: DecisionRefusal) -> &'static str {
    match refusal {
        DecisionRefusal::NotWaiting => "not_waiting",
        DecisionRefusal::NotOffered => "not_offered",
        DecisionRefusal::NotApplicable => "not_applicable",
        DecisionRefusal::Expired => "expired",
    }
}

fn decision_refused(reason: &'static str) -> ApiError {
    ApiError(ErrorBody::with_details(
        ErrorCode::DecisionNotOffered,
        "Ce choix n'est pas proposé par cette opération.",
        serde_json::json!({ "reason": reason }),
    ))
}

fn access_error(error: AccessError) -> ApiError {
    match error {
        AccessError::NotFound => unknown(),
        AccessError::TooLate => ApiError::new(
            ErrorCode::TooLateToCancel,
            "Trop tard pour annuler : l'opération a passé le point où elle peut s'arrêter sans rien laisser derrière elle.",
        ),
        AccessError::NotCancellable(status) => ApiError(ErrorBody::with_details(
            ErrorCode::Conflict,
            "Cette opération ne s'annule pas dans l'état où elle est.",
            serde_json::json!({ "reason": "not_cancellable", "status": status.as_str() }),
        )),
        AccessError::Decision(refusal) => decision_refused(refusal_reason(refusal)),
        AccessError::Changed => ApiError(ErrorBody::with_details(
            ErrorCode::Conflict,
            "L'opération a changé pendant ta demande. Relis-la et recommence.",
            serde_json::json!({ "reason": "operation_changed" }),
        )),
        AccessError::Store(error) => ApiError::internal(&error),
    }
}

/// `GET /api/v1/operations/services/{id}`.
pub async fn get(
    State(state): State<AppState>,
    Caller(session): Caller,
    Path(id): Path<String>,
) -> Result<Json<OperationView>, ApiError> {
    let id = checked_id(&id)?;
    let snapshot = state
        .service_ops
        .read(id, viewer_of(&session))
        .await
        .map_err(access_error)?;
    Ok(Json(service_ops_wire::view(&snapshot)))
}

/// `DELETE /api/v1/operations/services/{id}` : annule l'opération avant son point de non-retour. Idempotent :
/// une opération déjà annulée par un compte répond `200` comme la première fois, sans nouvelle entrée de
/// journal.
pub async fn cancel(
    State(state): State<AppState>,
    _confirmed: Extension<Reauthenticated>,
    Caller(session): Caller,
    Requester(by): Requester,
    Path(id): Path<String>,
) -> Result<Json<OperationView>, ApiError> {
    use crate::application::service_op_access::CancelDone;
    let id = checked_id(&id)?;
    let done = state
        .service_ops
        .cancel(id, &by, viewer_of(&session))
        .await
        .map_err(access_error)?;
    let snapshot = match done {
        CancelDone::Cancelled(snapshot)
        | CancelDone::Cancelling(snapshot)
        | CancelDone::Already(snapshot) => snapshot,
    };
    Ok(Json(service_ops_wire::view(&snapshot)))
}

/// `POST /api/v1/operations/services/{id}/decision` : le choix du compte parmi ceux que l'opération offre.
pub async fn decide(
    State(state): State<AppState>,
    _confirmed: Extension<Reauthenticated>,
    Caller(session): Caller,
    Requester(by): Requester,
    Path(id): Path<String>,
    body: Result<Json<OperationDecision>, JsonRejection>,
) -> Result<Json<OperationView>, ApiError> {
    let id = checked_id(&id)?;
    let Json(decision) = body?;
    // Un choix que l'agent ne connaît pas n'est jamais offert.
    let Some(choice) = Choice::from_wire(decision.choice) else {
        return Err(decision_refused("not_offered"));
    };
    let snapshot = state
        .service_ops
        .decide(id, choice, &by, viewer_of(&session))
        .await
        .map_err(access_error)?;
    Ok(Json(service_ops_wire::view(&snapshot)))
}
