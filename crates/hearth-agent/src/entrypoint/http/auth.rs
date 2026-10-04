//! Contrôle d'accès : l'unique couche qui décide qui est l'appelant et ce qu'il a le droit de faire.
//!
//! Deux extracteurs, rien d'autre : `Authenticated` (une session valable) et `AdminOnly` (une
//! session valable ET un rôle qui gère les comptes, BR-ACCT-013 et BR-ACCT-014). Un handler
//! d'une route réservée les prend en premier argument : le contrôle précède la lecture du
//! corps. Le test de balayage (`tests/http_api.rs`) parcourt `ENDPOINTS` et échoue si une route
//! déclarée réservée ne refuse pas l'appelant sans droit.

use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use hearth_proto::error::ErrorCode;

use super::{ApiError, AppState};
use crate::application::sessions::CurrentSession;

/// Identité déjà résolue pour cette requête (posée par la couche des opérations, qui doit
/// connaître l'appelant avant d'exécuter) : évite de re-vérifier le jeton et de renouveler
/// deux fois la session.
#[derive(Clone)]
struct Resolved(CurrentSession);

/// Lit le jeton de `Authorization: Bearer <jeton>`.
fn bearer(parts: &Parts) -> Option<&str> {
    let value = parts.headers.get(AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then_some(token.trim())
        .filter(|token| !token.is_empty())
}

/// Reconnaît l'appelant : une session valable, sinon l'erreur du protocole.
pub(super) async fn resolve(
    state: &AppState,
    parts: &mut Parts,
) -> Result<CurrentSession, ApiError> {
    if let Some(Resolved(session)) = parts.extensions.get::<Resolved>() {
        return Ok(session.clone());
    }
    let token = bearer(parts).ok_or_else(|| {
        ApiError::new(
            ErrorCode::Unauthenticated,
            "Jeton de session absent ou illisible",
        )
    })?;
    let session = state.sessions.authenticate(token).await?;
    parts.extensions.insert(Resolved(session.clone()));
    Ok(session)
}

/// Une session valable, quel que soit le rôle.
pub struct Authenticated(pub CurrentSession);

impl FromRequestParts<AppState> for Authenticated {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        resolve(state, parts).await.map(Self)
    }
}

/// Une session valable d'un compte qui gère les comptes (`Role::can_manage_accounts`).
pub struct AdminOnly(pub CurrentSession);

impl FromRequestParts<AppState> for AdminOnly {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let session = resolve(state, parts).await?;
        if session.account.role.can_manage_accounts() {
            Ok(Self(session))
        } else {
            Err(ApiError::new(
                ErrorCode::ForbiddenRole,
                "Tu n'as pas la permission pour accéder à la gestion des comptes",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::http::Request;

    use super::*;

    fn parts(authorization: Option<&str>) -> Parts {
        let mut builder = Request::builder().uri("/x");
        if let Some(value) = authorization {
            builder = builder.header(AUTHORIZATION, value);
        }
        builder.body(()).unwrap().into_parts().0
    }

    #[test]
    fn the_token_is_read_from_the_bearer_scheme_only() {
        assert_eq!(bearer(&parts(Some("Bearer abc"))), Some("abc"));
        assert_eq!(bearer(&parts(Some("bearer abc"))), Some("abc"));
        assert_eq!(bearer(&parts(Some("Basic abc"))), None);
        assert_eq!(bearer(&parts(Some("abc"))), None);
        assert_eq!(bearer(&parts(Some("Bearer "))), None);
        assert_eq!(bearer(&parts(None)), None);
    }
}
