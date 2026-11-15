//! Déposer un fichier dans la zone d'attente de l'agent par un transfert reprenable (HRT-95, ADR-0048,
//! BR-FILES-019, BR-MOD-015).
//!
//! - `POST /deposits` (acte `FileWrite` `0x16`, couvert par l'élévation, droit « gérer les services ») : ouvre un
//!   dépôt, `201 { transfer_id, chunk_bytes, window, expires_at }`. Rien n'est envoyé ici : le fichier passe par le
//!   WebSocket `GET /transfers/{id}` (`ws::transfer`).
//! - `GET /deposits/{id}` : l'état du dépôt de CE compte, pour le retrouver après une coupure ;
//! - `DELETE /transfers/{id}` (exception nommée, `NOT_AN_ACT`) : abandonne le dépôt de CE compte et retire la partie
//!   reçue.
//!
//! Aucun message d'erreur ne répète une valeur reçue (nom de fichier compris) ni un chemin du serveur : les textes
//! sont fixes. L'espace manquant est dans `details`, jamais dans le texte.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use hearth_proto::api::transfers::{
    CHUNK_BYTES, DepositOpened, DepositRequest, DepositState, DepositView, TransferAbandoned,
    WINDOW_CHUNKS, hex_of,
};
use hearth_proto::error::{ErrorBody, ErrorCode};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::auth::{Caller, Requester};
use super::{ApiError, AppState};
use crate::application::sessions::Reauthenticated;
use crate::application::transfers::TransferError;
use crate::domain::audit::{AuditAction, Outcome, Target};
use crate::domain::transfers::{Limit, TransferId};

fn unknown() -> ApiError {
    ApiError::new(ErrorCode::NotFound, "Ce transfert n'existe pas.")
}

fn date(at: OffsetDateTime) -> String {
    at.format(&Rfc3339).unwrap_or_default()
}

fn refusal(error: TransferError) -> ApiError {
    match error {
        TransferError::Unavailable => ApiError(ErrorBody::with_details(
            ErrorCode::PathRefused,
            "La zone d'attente des fichiers n'est pas disponible sur ce serveur.",
            serde_json::json!({ "reason": "transfers_root_unavailable" }),
        )),
        TransferError::Invalid { field } => ApiError::invalid_because(
            field,
            "not_allowed",
            match field {
                "name" => "Le nom du fichier n'est pas valable.",
                _ => "La taille du fichier n'est pas valable.",
            },
        ),
        TransferError::NoSpace(shortfall) => ApiError(ErrorBody::with_details(
            ErrorCode::NoSpace,
            "Il n'y a pas assez de place sur le disque pour écrire ce fichier.",
            serde_json::json!({
                "missing_bytes": shortfall.missing,
                "free_after_bytes": shortfall.free_after,
            }),
        )),
        TransferError::Limit(limit) => ApiError(ErrorBody::with_details(
            ErrorCode::TransferLimit,
            match limit {
                Limit::Open => {
                    "Trop de dépôts sont en attente. Abandonnez-en un ou attendez qu'il expire."
                }
                Limit::Total | Limit::Account => {
                    "Trop de transferts sont en cours. Attendez la fin d'un envoi."
                }
            },
            serde_json::json!({
                "limit": match limit {
                    Limit::Total => "agent",
                    Limit::Account => "account",
                    Limit::Open => "open",
                },
            }),
        )),
        TransferError::NotFound => unknown(),
        TransferError::Expired => ApiError::new(
            ErrorCode::TransferExpired,
            "Ce dépôt a expiré : la partie reçue a été supprimée. Recommencez l'envoi.",
        ),
        TransferError::Storage => ApiError::internal(&"zone d'attente indisponible"),
    }
}

/// `POST /api/v1/deposits` (acte `FileWrite` `0x16`).
pub async fn open(
    State(state): State<AppState>,
    _confirmed: Extension<Reauthenticated>,
    Caller(session): Caller,
    Requester(actor): Requester,
    body: Result<Json<DepositRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<DepositOpened>), ApiError> {
    let Json(request) = body?;
    let opened = state
        .transfers
        .open_deposit(session.account.id.as_str(), &request.name, request.size)
        .await
        .map_err(refusal)?;
    // L'acte est accepté : qui et d'où, jamais le nom du fichier ni sa taille. La FIN de l'envoi s'écrit à part, sous
    // la même action, quand le dernier morceau est arrivé (`ws::transfers`).
    state
        .sink
        .record(
            actor,
            AuditAction::ServiceFileWrite,
            Target::Route("/deposits"),
            Outcome::Succeeded,
        )
        .await;
    Ok((
        StatusCode::CREATED,
        Json(DepositOpened {
            transfer_id: opened.id.to_string(),
            chunk_bytes: CHUNK_BYTES,
            window: WINDOW_CHUNKS,
            expires_at: date(opened.expires_at),
        }),
    ))
}

/// `GET /api/v1/deposits/{id}`.
pub async fn view(
    State(state): State<AppState>,
    Caller(session): Caller,
    Path(id): Path<String>,
) -> Result<Json<DepositView>, ApiError> {
    let id = TransferId::parse(&id).ok_or_else(unknown)?;
    let status = state
        .transfers
        .status(&id, session.account.id.as_str())
        .await
        .map_err(refusal)?;
    Ok(Json(DepositView {
        transfer_id: status.id.to_string(),
        state: if status.complete {
            DepositState::Complete
        } else {
            DepositState::Receiving
        },
        size: status.size,
        received_bytes: status.received_bytes,
        expires_at: date(status.expires_at),
        tree_digest: status.tree_digest.map(|digest| hex_of(&digest)),
    }))
}

/// `DELETE /api/v1/transfers/{id}` : n'efface que la partie reçue par le compte appelant.
pub async fn abandon(
    State(state): State<AppState>,
    Caller(session): Caller,
    Requester(actor): Requester,
    Path(id): Path<String>,
) -> Result<Json<TransferAbandoned>, ApiError> {
    let id = TransferId::parse(&id).ok_or_else(unknown)?;
    let removed = state
        .transfers
        .abandon(&id, session.account.id.as_str())
        .await
        .map_err(refusal)?;
    // Une partie reçue qui disparaît est un geste sur la zone d'attente : consigné (qui et d'où, jamais un nom).
    if removed {
        state
            .sink
            .record(
                actor,
                AuditAction::ServiceFileWrite,
                Target::Route("/transfers/{id}/abandon"),
                Outcome::Succeeded,
            )
            .await;
    }
    Ok(Json(TransferAbandoned { removed }))
}
