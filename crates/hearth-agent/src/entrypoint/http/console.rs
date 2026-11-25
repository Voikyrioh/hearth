//! La console d'un serveur de jeu (HRT-268, ADR-0067, BR-CONSOLE-001 à 011) : `GET /services/{key}/console`,
//! `POST /services/{key}/console` (acte `ConsoleCommand` `0x30`) et `GET /services/{key}/players`.
//!
//! Aucune erreur ne répète un texte de commande, une réponse du jeu, une adresse ni un secret : des textes FIXES.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Extension, Path, State};
use hearth_proto::api::console::{ConsoleReply, ConsoleRequest, ConsoleView};
use hearth_proto::api::players::PlayersView;
use hearth_proto::console::LineError;
use hearth_proto::error::{ErrorBody, ErrorCode};

use super::auth::{Caller, Requester};
use super::{ApiError, AppState};
use crate::application::console::ConsoleRefusal;
use crate::application::sessions::Reauthenticated;
use crate::domain::services::ServiceKey;

fn key_of(raw: &str) -> Result<ServiceKey, ApiError> {
    ServiceKey::parse(raw)
        .ok_or_else(|| ApiError::new(ErrorCode::ServiceNotFound, "Ce service n'existe pas."))
}

fn with_reason(code: ErrorCode, message: &'static str, reason: &str) -> ApiError {
    ApiError(ErrorBody::with_details(
        code,
        message,
        serde_json::json!({ "reason": reason }),
    ))
}

/// Les refus de la console, dans l'ordre du tableau de la conception (section 7).
fn refusal(error: ConsoleRefusal) -> ApiError {
    match error {
        ConsoleRefusal::NotFound => {
            ApiError::new(ErrorCode::ServiceNotFound, "Ce service n'existe pas.")
        }
        ConsoleRefusal::Adopted => ApiError::new(
            ErrorCode::ServiceAdopted,
            "Hearth ne parle pas à la console d'un service adopté.",
        ),
        ConsoleRefusal::NotAvailable => ApiError::new(
            ErrorCode::ConsoleNotAvailable,
            "La console de ce serveur n'est pas ouverte à Hearth.",
        ),
        ConsoleRefusal::Invalid(reason) => with_reason(
            ErrorCode::ConsoleCommandInvalid,
            "Cette ligne n'est pas une commande permise.",
            match reason {
                LineError::Empty => "empty",
                LineError::TooLong => "too_long",
                LineError::ForbiddenChar => "control_char",
            },
        ),
        ConsoleRefusal::Refused(reason) => with_reason(
            ErrorCode::ConsoleCommandRefused,
            "Hearth n'envoie pas cette commande : elle se demande autrement.",
            reason.code(),
        ),
        ConsoleRefusal::ClassMismatch => ApiError::new(
            ErrorCode::ConsoleClassMismatch,
            "La classe de cette commande n'est pas celle que l'agent juge : mettez le client à jour.",
        ),
        ConsoleRefusal::NotRunning => ApiError::new(
            ErrorCode::ServiceNotRunning,
            "Le serveur est arrêté : sa console n'écoute pas.",
        ),
        ConsoleRefusal::Starting => ApiError::new(
            ErrorCode::ServiceStarting,
            "Le serveur démarre : le jeu n'est pas encore prêt.",
        ),
        ConsoleRefusal::Busy => ApiError::new(
            ErrorCode::OperationInProgress,
            "Une opération est déjà en cours sur ce serveur.",
        ),
        ConsoleRefusal::RateLimited { retry_after_s } => ApiError(ErrorBody::with_details(
            ErrorCode::ConsoleRateLimited,
            "Trop de commandes à la suite : attendez un instant.",
            serde_json::json!({ "retry_after_s": retry_after_s }),
        )),
        ConsoleRefusal::Unreachable => ApiError::new(
            ErrorCode::ConsoleUnreachable,
            "La console du jeu ne répond pas.",
        ),
        ConsoleRefusal::Internal(message) => {
            tracing::error!(error = %message, "console en échec");
            ApiError::new(ErrorCode::InternalError, "Une erreur interne est survenue.")
        }
    }
}

/// `GET /api/v1/services/{key}/console` : ce que la console permet sur ce service. Ouvert à tout compte authentifié.
pub async fn describe(
    State(state): State<AppState>,
    Caller(session): Caller,
    Path(key): Path<String>,
) -> Result<Json<ConsoleView>, ApiError> {
    let key = key_of(&key)?;
    let can_write = session.account.services_right.can_manage_services();
    state
        .console
        .describe(&key, can_write)
        .await
        .map(Json)
        .map_err(refusal)
}

/// `POST /api/v1/services/{key}/console` : envoie UNE commande. Droit « gérer les services », acte signé
/// `ConsoleCommand` ; le client n'est jamais cru : l'agent normalise, reclasse, refuse ou envoie.
pub async fn send(
    State(state): State<AppState>,
    _confirmed: Extension<Reauthenticated>,
    Requester(actor): Requester,
    Path(key): Path<String>,
    body: Result<Json<ConsoleRequest>, JsonRejection>,
) -> Result<Json<ConsoleReply>, ApiError> {
    let key = key_of(&key)?;
    let Json(request) = body?;
    state
        .console
        .send(&key, actor, &request)
        .await
        .map(Json)
        .map_err(refusal)
}

/// `GET /api/v1/services/{key}/players` : les joueurs connectés (cache de 10 secondes, aucun journal).
pub async fn players(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<PlayersView>, ApiError> {
    let key = key_of(&key)?;
    state.console.players(&key).await.map(Json).map_err(refusal)
}
