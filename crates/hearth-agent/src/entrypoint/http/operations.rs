//! Suivi des opérations par clé (BR-RESIL-010) : la couche qui enregistre une requête qui
//! modifie avant de l'exécuter et retient son résultat, et la route `GET /operations/{id}`.
//!
//! Rejouer la même clé renvoie le premier résultat sans ré-exécuter ; une clé dont l'exécution
//! n'est pas finie répond `409 OPERATION_IN_PROGRESS`. `POST /sessions` n'est pas suivi : sa
//! réponse contient un jeton, qu'on ne conserve pas en base.

use axum::Json;
use axum::body::{Body, to_bytes};
use axum::extract::{Path, Request, State};
use axum::http::{HeaderName, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use hearth_proto::api::operations::{OperationResponse, OperationStatus as WireStatus};
use hearth_proto::error::ErrorCode;
use hearth_proto::headers;
use serde_json::{Value, json};

use super::auth::{self, Authenticated};
use super::{ApiError, AppState};
use crate::application::operations::Begin;
use crate::domain::operations::{Operation, OperationKey, OperationStatus};

/// Taille maximale d'une réponse retenue (les réponses de l'API sont petites).
const MAX_RESULT_BYTES: usize = 1 << 20;

/// Une requête est suivie si elle modifie et porte une clé, hors connexion.
fn is_tracked(request: &Request) -> bool {
    let modifies = !matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    );
    let is_login = *request.method() == Method::POST && request.uri().path().ends_with("/sessions");
    modifies && !is_login && request.headers().contains_key(headers::IDEMPOTENCY_KEY)
}

pub async fn layer(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if !is_tracked(&request) {
        return next.run(request).await;
    }
    let (mut parts, body) = request.into_parts();
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
    // Il faut connaître l'appelant avant d'exécuter : une clé est celle d'un compte.
    let caller = match auth::resolve(&state, &mut parts).await {
        Ok(session) => session,
        Err(error) => return error.into_response(),
    };
    let kind = format!("{} {}", parts.method, parts.uri.path());
    match state
        .operations
        .begin(&key, &caller.account.id, &kind)
        .await
    {
        Err(error) => return ApiError::from(error).into_response(),
        Ok(Begin::Replay(operation)) => return replay(&operation),
        Ok(Begin::InProgress) => {
            return ApiError::new(
                ErrorCode::OperationInProgress,
                "Cette opération est déjà en cours",
            )
            .into_response();
        }
        Ok(Begin::ForeignKey) => {
            return ApiError::invalid(
                headers::IDEMPOTENCY_KEY,
                "Cette clé d'opération est déjà utilisée",
            )
            .into_response();
        }
        Ok(Begin::Execute) => {}
    }

    let response = next.run(Request::from_parts(parts, body)).await;
    retain(&state, &key, response).await
}

/// Retient le résultat de la réponse sous la clé, puis la rend telle quelle. Une erreur
/// interne n'est pas retenue : la clé est oubliée, le client peut relancer.
async fn retain(state: &AppState, key: &OperationKey, response: Response) -> Response {
    let (parts, body) = response.into_parts();
    let status = parts.status;
    let bytes = match to_bytes(body, MAX_RESULT_BYTES).await {
        Ok(bytes) => bytes,
        Err(error) => {
            discard(state, key).await;
            return ApiError::internal(&error).into_response();
        }
    };
    if status.is_server_error() {
        discard(state, key).await;
    } else {
        let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        let stored = json!({ "status": status.as_u16(), "body": body }).to_string();
        if let Err(error) = state
            .operations
            .finish(key, status.is_success(), &stored)
            .await
        {
            tracing::error!(%error, "résultat d'opération non retenu");
        }
    }
    Response::from_parts(parts, Body::from(bytes))
}

async fn discard(state: &AppState, key: &OperationKey) {
    if let Err(error) = state.operations.discard(key).await {
        tracing::error!(%error, "clé d'opération non oubliée");
    }
}

/// Le premier résultat, rejoué.
fn replay(operation: &Operation) -> Response {
    let stored: Value = operation
        .result_json
        .as_deref()
        .and_then(|text| serde_json::from_str(text).ok())
        .unwrap_or(Value::Null);
    let status = stored
        .get("status")
        .and_then(Value::as_u64)
        .and_then(|code| u16::try_from(code).ok())
        .and_then(|code| StatusCode::from_u16(code).ok())
        .unwrap_or(StatusCode::OK);
    let body = stored.get("body").cloned().unwrap_or(Value::Null);
    let mut response = if body.is_null() {
        status.into_response()
    } else {
        (status, Json(body)).into_response()
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
    Authenticated(caller): Authenticated,
    Path(id): Path<String>,
) -> Result<Json<OperationResponse>, ApiError> {
    let not_found = || ApiError::new(ErrorCode::NotFound, "Opération inconnue");
    let key = OperationKey::parse(&id).map_err(|_| not_found())?;
    let operation = state
        .operations
        .find(&key, &caller.account.id)
        .await?
        .ok_or_else(not_found)?;
    let result = operation
        .result_json
        .as_deref()
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .and_then(|stored| stored.get("body").cloned());
    Ok(Json(OperationResponse {
        id: operation.key.to_string(),
        kind: operation.kind,
        status: match operation.status {
            OperationStatus::Running => WireStatus::Running,
            OperationStatus::Succeeded => WireStatus::Succeeded,
            OperationStatus::Failed => WireStatus::Failed,
        },
        result,
    }))
}

#[cfg(test)]
mod tests {
    use axum::http::Request as HttpRequest;
    use time::OffsetDateTime;

    use super::*;
    use crate::domain::accounts::AccountId;

    fn request(method: Method, path: &str, key: bool) -> Request {
        let mut builder = HttpRequest::builder().method(method).uri(path);
        if key {
            builder = builder.header(headers::IDEMPOTENCY_KEY, "01J9ZY0G3Q8M2K6W4T7V5N1B9D");
        }
        builder.body(Body::empty()).unwrap()
    }

    #[test]
    fn only_modifying_requests_with_a_key_are_tracked() {
        assert!(is_tracked(&request(
            Method::PUT,
            "/api/v1/me/password",
            true
        )));
        assert!(is_tracked(&request(
            Method::DELETE,
            "/api/v1/accounts/A",
            true
        )));
        assert!(!is_tracked(&request(
            Method::PUT,
            "/api/v1/me/password",
            false
        )));
        assert!(!is_tracked(&request(Method::GET, "/api/v1/me", true)));
    }

    #[test]
    fn the_login_is_never_tracked_because_its_response_holds_a_token() {
        assert!(!is_tracked(&request(
            Method::POST,
            "/api/v1/sessions",
            true
        )));
        assert!(is_tracked(&request(Method::POST, "/api/v1/accounts", true)));
    }

    #[test]
    fn a_finished_operation_is_replayed_with_its_status_and_body() {
        let operation = Operation {
            key: OperationKey::parse("K1").unwrap(),
            account: AccountId::new("A"),
            kind: "PUT /x".into(),
            status: OperationStatus::Succeeded,
            result_json: Some(r#"{"status":201,"body":{"a":1}}"#.into()),
            created_at: OffsetDateTime::UNIX_EPOCH,
            finished_at: None,
        };
        let response = replay(&operation);
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
        let operation = Operation {
            key: OperationKey::parse("K1").unwrap(),
            account: AccountId::new("A"),
            kind: "DELETE /x".into(),
            status: OperationStatus::Succeeded,
            result_json: Some(r#"{"status":204,"body":null}"#.into()),
            created_at: OffsetDateTime::UNIX_EPOCH,
            finished_at: None,
        };
        assert_eq!(replay(&operation).status(), StatusCode::NO_CONTENT);
    }
}
