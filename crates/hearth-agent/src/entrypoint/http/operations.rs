//! Suivi des opérations par clé (BR-RESIL-010) : la couche qui enregistre une requête qui
//! modifie avant de l'exécuter et retient son résultat, et la route `GET /operations/{id}`.
//!
//! Posée par la couche d'accès (`auth::guard`) sur les routes que `ENDPOINTS` déclare suivies.
//!
//! - La clé est celle d'un compte et liée à la requête (méthode, chemin, corps) : la même clé
//!   avec une autre requête est refusée (`422 IDEMPOTENCY_KEY_REUSED`), sans exécuter.
//! - Rejouer la même clé avec la même requête rend le premier résultat sans ré-exécuter ; une clé
//!   dont l'exécution n'est pas finie répond `409 OPERATION_IN_PROGRESS`.
//! - La requête s'exécute dans une tâche détachée : si le client coupe avant la réponse, le
//!   résultat est tout de même retenu, c'est le cas pour lequel la clé existe. Au démarrage de
//!   l'agent, une opération restée en cours devient « interrompue ».

use std::future::Future;
use std::sync::Arc;

use axum::Json;
use axum::body::{Body, to_bytes};
use axum::extract::{Path, Request, State};
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use hearth_proto::api::operations::{OperationResponse, OperationStatus as WireStatus};
use hearth_proto::error::ErrorCode;
use hearth_proto::headers;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::Instrument;

use super::auth::Caller;
use super::{ApiError, AppState};
use crate::application::operations::OperationService;
use crate::application::sessions::CurrentSession;
use crate::domain::accounts::AccountId;
use crate::domain::operations::{
    Operation, OperationKey, OperationStatus, Replay, RequestFingerprint,
};

/// Taille maximale d'une requête ou d'une réponse retenue (celles de l'API sont petites).
const MAX_BYTES: usize = 1 << 20;

/// Le résultat retenu d'une opération : statut HTTP et corps de la réponse. Une seule forme,
/// à l'écriture, au rejeu et pour `GET /operations/{id}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct StoredResult {
    status: u16,
    body: Value,
}

impl StoredResult {
    fn of(operation: &Operation) -> Option<Self> {
        serde_json::from_str(operation.result_json.as_deref()?).ok()
    }
}

/// Suit la requête si elle porte une clé d'opération ; la couche d'accès l'appelle pour les
/// routes que la table `ENDPOINTS` déclare suivies, une fois l'appelant connu (une clé est celle
/// d'un compte). Sans clé, la requête s'exécute sans suivi.
pub(super) async fn track(
    state: &AppState,
    caller: CurrentSession,
    request: Request,
    next: Next,
) -> Response {
    if !request.headers().contains_key(headers::IDEMPOTENCY_KEY) {
        return next.run(request).await;
    }
    let (parts, body) = request.into_parts();
    let key = parts
        .headers
        .get(headers::IDEMPOTENCY_KEY)
        .and_then(|value| value.to_str().ok())
        .map(OperationKey::parse);
    let key = match key {
        Some(Ok(key)) => key,
        _ => {
            return ApiError::invalid(
                headers::IDEMPOTENCY_KEY,
                "La clé d'opération est invalide : 1 à 64 caractères, lettres, chiffres, tiret ou souligné",
            )
            .into_response();
        }
    };
    let account = caller.account.id;
    let bytes = match to_bytes(body, MAX_BYTES).await {
        Ok(bytes) => bytes,
        Err(error) if is_too_large(&error) => {
            return ApiError::new(
                ErrorCode::PayloadTooLarge,
                "Corps de requête trop volumineux (1 Mio au plus)",
            )
            .into_response();
        }
        Err(error) => {
            return ApiError::invalid("body", format!("Corps de requête illisible : {error}"))
                .into_response();
        }
    };
    let request_fingerprint =
        RequestFingerprint::of(parts.method.as_str(), parts.uri.path(), &bytes);
    let kind = format!("{} {}", parts.method, parts.uri.path());
    match state
        .operations
        .begin(&key, &account, &kind, &request_fingerprint)
        .await
    {
        Err(error) => return ApiError::from(error).into_response(),
        Ok(Replay::Return(operation)) => return replay(&operation),
        Ok(Replay::InProgress) => {
            return ApiError::new(
                ErrorCode::OperationInProgress,
                "Cette opération est déjà en cours",
            )
            .into_response();
        }
        Ok(Replay::Interrupted) => {
            return ApiError::new(
                ErrorCode::Conflict,
                "Le résultat de cette opération est inconnu : vérifie l'état avant de relancer",
            )
            .into_response();
        }
        Ok(Replay::KeyReused) => {
            return ApiError::new(
                ErrorCode::IdempotencyKeyReused,
                "Cette clé d'opération a déjà servi pour une autre requête",
            )
            .into_response();
        }
        Ok(Replay::Execute) => {}
    }

    // Tâche détachée : si le client coupe, la requête va jusqu'au bout et son résultat est retenu.
    let request = Request::from_parts(parts, Body::from(bytes));
    execute_detached(state.operations.clone(), account, key, next.run(request)).await
}

/// Exécute `work` (la requête suivie) dans une tâche détachée, dans le span de la requête, et
/// retient son résultat sous la clé. Un client qui coupe n'interrompt rien. Si le travail
/// panique, la clé est oubliée dans la tâche même (jamais « en cours » pour toujours) et la
/// réponse est une erreur interne.
pub async fn execute_detached(
    operations: Arc<OperationService>,
    account: AccountId,
    key: OperationKey,
    work: impl Future<Output = Response> + Send + 'static,
) -> Response {
    let task = tokio::spawn(
        async move {
            match tokio::spawn(work.in_current_span()).await {
                Ok(response) => retain(&operations, &account, &key, response).await,
                Err(error) => {
                    discard(&operations, &account, &key).await;
                    ApiError::internal(&error).into_response()
                }
            }
        }
        .in_current_span(),
    );
    match task.await {
        Ok(response) => response,
        Err(error) => ApiError::internal(&error).into_response(),
    }
}

/// La limite de taille a-t-elle été dépassée (et non une lecture interrompue) ?
fn is_too_large(error: &axum::Error) -> bool {
    error.to_string().contains("length limit")
}

/// Retient le résultat de la réponse sous la clé, puis la rend telle quelle. Une erreur
/// interne n'est pas retenue : la clé est oubliée, le client peut relancer.
async fn retain(
    operations: &OperationService,
    account: &AccountId,
    key: &OperationKey,
    response: Response,
) -> Response {
    let (parts, body) = response.into_parts();
    let status = parts.status;
    let bytes = match to_bytes(body, MAX_BYTES).await {
        Ok(bytes) => bytes,
        Err(error) => {
            discard(operations, account, key).await;
            return ApiError::internal(&error).into_response();
        }
    };
    if status.is_server_error() {
        discard(operations, account, key).await;
    } else {
        let result = StoredResult {
            status: status.as_u16(),
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        };
        match serde_json::to_string(&result) {
            Ok(stored) => {
                if let Err(error) = operations
                    .finish(account, key, status.is_success(), &stored)
                    .await
                {
                    tracing::error!(%error, "résultat d'opération non retenu");
                }
            }
            Err(error) => tracing::error!(%error, "résultat d'opération non sérialisable"),
        }
    }
    Response::from_parts(parts, Body::from(bytes))
}

async fn discard(operations: &OperationService, account: &AccountId, key: &OperationKey) {
    if let Err(error) = operations.discard(account, key).await {
        tracing::error!(%error, "clé d'opération non oubliée");
    }
}

/// Le premier résultat, rejoué.
fn replay(operation: &Operation) -> Response {
    let stored = StoredResult::of(operation).unwrap_or(StoredResult {
        status: StatusCode::OK.as_u16(),
        body: Value::Null,
    });
    let status = StatusCode::from_u16(stored.status).unwrap_or(StatusCode::OK);
    let mut response = if stored.body.is_null() {
        status.into_response()
    } else {
        (status, Json(stored.body)).into_response()
    };
    response.headers_mut().insert(
        HeaderName::from_static(headers::IDEMPOTENT_REPLAYED),
        HeaderValue::from_static("true"),
    );
    response
}

/// `GET /api/v1/operations/{id}` : l'état d'une opération de l'appelant, `404` si l'agent n'a
/// jamais reçu cette clé (l'action n'a pas été exécutée).
pub async fn get(
    State(state): State<AppState>,
    Caller(caller): Caller,
    Path(id): Path<String>,
) -> Result<Json<OperationResponse>, ApiError> {
    let not_found = || ApiError::new(ErrorCode::NotFound, "Opération inconnue");
    let key = OperationKey::parse(&id).map_err(|_| not_found())?;
    let operation = state
        .operations
        .find(&key, &caller.account.id)
        .await?
        .ok_or_else(not_found)?;
    let result = StoredResult::of(&operation).map(|stored| stored.body);
    Ok(Json(OperationResponse {
        id: operation.key.to_string(),
        kind: operation.kind,
        status: match operation.status {
            OperationStatus::Running => WireStatus::Running,
            OperationStatus::Succeeded => WireStatus::Succeeded,
            OperationStatus::Failed => WireStatus::Failed,
            OperationStatus::Interrupted => WireStatus::Interrupted,
        },
        result,
    }))
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;

    use super::*;

    fn operation(result: &str) -> Operation {
        Operation {
            key: OperationKey::parse("K1").unwrap(),
            account: AccountId::new("A"),
            kind: "PUT /x".into(),
            request: RequestFingerprint::of("PUT", "/x", b""),
            status: OperationStatus::Succeeded,
            result_json: Some(result.into()),
            created_at: OffsetDateTime::UNIX_EPOCH,
            finished_at: None,
        }
    }

    #[test]
    fn one_envelope_is_written_and_read_back() {
        let written = serde_json::to_string(&StoredResult {
            status: 201,
            body: serde_json::json!({ "a": 1 }),
        })
        .unwrap();
        let stored = StoredResult::of(&operation(&written)).unwrap();
        assert_eq!(
            (stored.status, stored.body),
            (201, serde_json::json!({ "a": 1 }))
        );
    }

    #[test]
    fn a_finished_operation_is_replayed_with_its_status_and_body() {
        let response = replay(&operation(r#"{"status":201,"body":{"a":1}}"#));
        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(
            response
                .headers()
                .get(headers::IDEMPOTENT_REPLAYED)
                .unwrap(),
            "true"
        );
    }

    #[test]
    fn an_empty_result_is_replayed_without_a_body() {
        assert_eq!(
            replay(&operation(r#"{"status":204,"body":null}"#)).status(),
            StatusCode::NO_CONTENT
        );
    }
}
