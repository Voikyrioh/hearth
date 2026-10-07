//! `POST /sessions` (connexion), `DELETE /sessions/current` (déconnexion), `GET /me`.

use std::net::SocketAddr;

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{ConnectInfo, FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode};
use hearth_proto::api::sessions::{
    ChallengeRequest, ChallengeResponse, DeviceLoginRequest, DeviceLoginResponse, LoginResponse,
    MeResponse,
};
use hearth_proto::error::ErrorCode;
use hearth_proto::headers;
use tracing::Instrument;

use super::auth::{Caller, Requester};
use super::{ApiError, AppState, wire};
use crate::application::sessions::ClientInfo;
use crate::domain::audit::ClientName;
use crate::domain::secret::Secret;

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

/// Nom du poste annoncé par le client : le nettoyage du domaine (`ClientName`), « inconnu » s'il
/// n'en reste rien.
pub(super) fn client_name(headers: &HeaderMap) -> String {
    headers
        .get(headers::CLIENT)
        .and_then(|value| value.to_str().ok())
        .and_then(ClientName::parse)
        .map_or_else(|| "inconnu".to_owned(), |name| name.as_str().to_owned())
}

/// `POST /api/v1/sessions` : ouvre une session. Rend le jeton une seule fois.
pub async fn login(
    State(state): State<AppState>,
    ClientAddr(addr): ClientAddr,
    request_headers: HeaderMap,
    body: Result<Json<DeviceLoginRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<DeviceLoginResponse>), ApiError> {
    let Json(DeviceLoginRequest { login, device }) = body?;
    let request = login;
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
                .login_with_device(
                    &request.username,
                    Secret::from(request.password),
                    &client,
                    device.as_ref(),
                )
                .await
        }
        .in_current_span(),
    );
    let outcome = attempt
        .await
        .map_err(|error| ApiError::internal(&error))??;
    let response = DeviceLoginResponse {
        login: LoginResponse {
            token: outcome.token.encode(),
            expires_at: wire::date(outcome.expires_at)?,
            account: wire::account_info(&outcome.account),
        },
        device: outcome.device,
    };
    Ok((StatusCode::CREATED, Json(response)))
}

/// `POST /api/v1/sessions/challenge` : rend un défi à signer avec la clé d'appareil. Route
/// publique, **sans lecture en base** et sans état retenu : la réponse est la même pour un
/// identifiant existant et pour un identifiant qui n'existe pas (HRT-22, absence d'oracle). Sur un
/// agent sans identité d'appareil, `404` comme une route inconnue : c'est ce que le client lit comme
/// « clé non prise en charge ».
pub async fn challenge(
    State(state): State<AppState>,
    ClientAddr(addr): ClientAddr,
    body: Result<Json<ChallengeRequest>, JsonRejection>,
) -> Result<Json<ChallengeResponse>, ApiError> {
    let trust = state
        .sessions
        .trust()
        .ok_or_else(|| ApiError::new(ErrorCode::NotFound, "Route inconnue"))?;
    let Json(request) = body?;
    let response = trust
        .issue_challenge(&request.username, request.purpose, &addr)
        .map_err(|error| ApiError::internal(&error))?;
    Ok(Json(response))
}

/// `DELETE /api/v1/sessions/current` : ferme la session de l'appelant.
pub async fn logout(
    State(state): State<AppState>,
    Caller(caller): Caller,
    Requester(by): Requester,
) -> Result<StatusCode, ApiError> {
    state.sessions.logout(&caller.session_id, &by).await?;
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
            crate::domain::audit::MAX_CLIENT_NAME
        );
    }

    #[test]
    fn a_missing_or_blank_client_name_is_unknown() {
        assert_eq!(client_name(&HeaderMap::new()), "inconnu");
        assert_eq!(client_name(&headers_with("   ")), "inconnu");
    }
}
