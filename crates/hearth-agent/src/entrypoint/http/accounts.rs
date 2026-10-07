//! Routes des comptes. Toutes (sauf `PUT /me/password`) sont réservées aux administrateurs : la
//! table `ENDPOINTS` leur donne le niveau `Admin`, et la couche d'accès du routeur refuse (`401`
//! ou `403`) avant la lecture du corps (BR-ACCT-013, BR-ACCT-014). Les handlers ne contrôlent rien.

use axum::Json;
use axum::body::Bytes;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use hearth_proto::api::accounts::{
    AccountItem, AccountsResponse, ChangeOwnPasswordRequest, ChangeRoleRequest,
    CreateAccountRequest, DeleteAccountRequest, SessionsClosedResponse, SetPasswordRequest,
};

use super::auth::{Caller, Requester};
use super::{ApiError, AppState, wire};
use crate::domain::accounts::AccountId;
use crate::domain::secret::Secret;

fn closed(count: u64) -> Json<SessionsClosedResponse> {
    Json(SessionsClosedResponse {
        sessions_closed: count,
    })
}

/// `GET /api/v1/accounts` : les comptes, avec sessions ouvertes et dernière connexion.
pub async fn list(State(state): State<AppState>) -> Result<Json<AccountsResponse>, ApiError> {
    let accounts = state
        .accounts
        .list()
        .await?
        .iter()
        .map(wire::summary_item)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(AccountsResponse { accounts }))
}

/// `POST /api/v1/accounts` : crée un compte.
pub async fn create(
    State(state): State<AppState>,
    Requester(by): Requester,
    body: Result<Json<CreateAccountRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<AccountItem>), ApiError> {
    let Json(request) = body?;
    let account = state
        .accounts
        .create(
            &request.username,
            Secret::from(request.password),
            wire::role_from_wire(request.role),
            &by,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(wire::account_item(&account, 0)?)))
}

/// `PATCH /api/v1/accounts/{id}` : change le rôle (jamais celui du dernier administrateur).
pub async fn change_role(
    State(state): State<AppState>,
    Requester(by): Requester,
    Path(id): Path<String>,
    body: Result<Json<ChangeRoleRequest>, JsonRejection>,
) -> Result<StatusCode, ApiError> {
    let Json(request) = body?;
    state
        .accounts
        .change_role(&AccountId::new(id), wire::role_from_wire(request.role), &by)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/v1/accounts/{id}` : supprime un compte et ferme ses sessions. Qui supprime son
/// propre compte retape son identifiant (`confirmation`, BR-ACCT-012).
pub async fn delete(
    State(state): State<AppState>,
    Caller(caller): Caller,
    Requester(by): Requester,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<SessionsClosedResponse>, ApiError> {
    let request = if body.is_empty() {
        DeleteAccountRequest::default()
    } else {
        serde_json::from_slice::<DeleteAccountRequest>(&body)
            .map_err(|error| ApiError::invalid("body", error.to_string()))?
    };
    let count = state
        .accounts
        .delete(
            &AccountId::new(id),
            Some(&caller.account.id),
            request.confirmation.as_deref(),
            &by,
        )
        .await?;
    Ok(closed(count))
}

/// `PUT /api/v1/accounts/{id}/password` : un administrateur définit le mot de passe d'autrui ;
/// ses sessions sont fermées (BR-ACCT-008).
pub async fn set_password(
    State(state): State<AppState>,
    Requester(by): Requester,
    Path(id): Path<String>,
    body: Result<Json<SetPasswordRequest>, JsonRejection>,
) -> Result<Json<SessionsClosedResponse>, ApiError> {
    let Json(request) = body?;
    let count = state
        .accounts
        .set_password(&AccountId::new(id), Secret::from(request.password), &by)
        .await?;
    Ok(closed(count))
}

/// `DELETE /api/v1/accounts/{id}/sessions` : ferme toutes les sessions du compte (BR-ACCT-011).
pub async fn revoke_sessions(
    State(state): State<AppState>,
    Requester(by): Requester,
    Path(id): Path<String>,
) -> Result<Json<SessionsClosedResponse>, ApiError> {
    let count = state
        .accounts
        .revoke_sessions(&AccountId::new(id), &by)
        .await?;
    Ok(closed(count))
}

/// `PUT /api/v1/me/password` : le titulaire change son mot de passe ; ses autres sessions sont
/// fermées, la courante est gardée (BR-ACCT-009). Permis à tout rôle.
pub async fn change_own_password(
    State(state): State<AppState>,
    Caller(caller): Caller,
    Requester(by): Requester,
    body: Result<Json<ChangeOwnPasswordRequest>, JsonRejection>,
) -> Result<Json<SessionsClosedResponse>, ApiError> {
    let Json(request) = body?;
    // Le choix du titulaire (Q15) : garder l'adresse d'où part la requête, la connexion TCP, jamais
    // une valeur du corps ni d'un en-tête.
    let keep = request
        .keep_address
        .then(|| by.origin.addr().map(str::to_owned))
        .flatten();
    let count = state
        .accounts
        .change_own_password_keeping(
            &caller.account.id,
            Secret::from(request.current),
            Secret::from(request.password),
            Some(caller.session_id),
            keep.as_deref(),
            &by,
        )
        .await?;
    Ok(closed(count))
}
