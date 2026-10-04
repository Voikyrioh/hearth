//! `POST /sessions` (connexion), `DELETE /sessions/current` (déconnexion), `GET /me`.

use std::net::SocketAddr;

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{ConnectInfo, FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode};
use hearth_proto::api::sessions::{LoginRequest, LoginResponse, MeResponse};
use hearth_proto::headers;
use tracing::Instrument;

use super::auth::Caller;
use super::{ApiError, AppState, wire};
use crate::application::sessions::ClientInfo;
use crate::domain::secret::Secret;

/// Longueur maximale retenue du nom du poste (`X-Hearth-Client`).
const MAX_CLIENT_NAME: usize = 128;

/// Adresse IP du client, telle que vue sur la connexion TCP. Jamais lue d'un en-tête de
/// mandataire (`X-Forwarded-For`…) : un client la forgerait pour échapper au verrouillage
/// (BR-CONN-007). Une adresse IPv4 reçue sur une socket double pile est ramenée à sa forme IPv4.
pub struct ClientAddr(pub String);

impl<S: Send + Sync> FromRequestParts<S> for ClientAddr {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let ConnectInfo(addr) = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .copied()
            .ok_or_else(|| ApiError::internal(&"adresse du client indisponible"))?;
        Ok(Self(addr.ip().to_canonical().to_string()))
    }
}

/// Nom du poste annoncé par le client, nettoyé (caractères de contrôle retirés, longueur bornée).
fn client_name(headers: &HeaderMap) -> String {
    let name: String = headers
        .get(headers::CLIENT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_CLIENT_NAME)
        .collect();
    let name = name.trim();
    if name.is_empty() {
        "inconnu".to_owned()
    } else {
        name.to_owned()
    }
}

/// `POST /api/v1/sessions` : ouvre une session. Rend le jeton une seule fois.
pub async fn login(
    State(state): State<AppState>,
    ClientAddr(addr): ClientAddr,
    request_headers: HeaderMap,
    body: Result<Json<LoginRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<LoginResponse>), ApiError> {
    let Json(request) = body?;
    let client = ClientInfo {
        name: client_name(&request_headers),
        addr,
    };
    // La tentative (verrouillage, vérification, écriture du résultat) va jusqu'au bout même si le
    // client coupe : un échec est toujours compté. Tâche détachée, dans le span de la requête.
    let sessions = state.sessions.clone();
    let attempt = tokio::spawn(
        async move {
            sessions
                .login(&request.username, Secret::from(request.password), &client)
                .await
        }
        .in_current_span(),
    );
    let outcome = attempt
        .await
        .map_err(|error| ApiError::internal(&error))??;
    let response = LoginResponse {
        token: outcome.token.encode(),
        expires_at: wire::date(outcome.expires_at)?,
        account: wire::account_info(&outcome.account),
    };
    Ok((StatusCode::CREATED, Json(response)))
}

/// `DELETE /api/v1/sessions/current` : ferme la session de l'appelant.
pub async fn logout(
    State(state): State<AppState>,
    Caller(caller): Caller,
) -> Result<StatusCode, ApiError> {
    state.sessions.logout(&caller.session_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/v1/me` : le compte de l'appelant et son rôle.
pub async fn me(Caller(caller): Caller) -> Result<Json<MeResponse>, ApiError> {
    Ok(Json(MeResponse {
        account: wire::account_info(&caller.account),
        session_expires_at: wire::date(caller.expires_at)?,
    }))
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn headers_with(name: &str) -> HeaderMap {
        let mut map = HeaderMap::new();
        map.insert(headers::CLIENT, HeaderValue::from_str(name).unwrap());
        map
    }

    #[test]
    fn the_client_name_is_trimmed_and_bounded() {
        assert_eq!(
            client_name(&headers_with("  poste-de-marie/1.2  ")),
            "poste-de-marie/1.2"
        );
        assert_eq!(
            client_name(&headers_with(&"x".repeat(500))).chars().count(),
            MAX_CLIENT_NAME
        );
    }

    #[test]
    fn a_missing_or_blank_client_name_is_unknown() {
        assert_eq!(client_name(&HeaderMap::new()), "inconnu");
        assert_eq!(client_name(&headers_with("   ")), "inconnu");
    }
}
