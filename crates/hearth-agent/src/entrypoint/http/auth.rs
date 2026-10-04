//! Contrôle d'accès : l'unique couche qui décide qui est l'appelant et ce qu'il a le droit de faire.
//!
//! Le routeur pose cette couche (`guard`) sur chaque route non publique, d'après le niveau
//! d'accès déclaré dans `ENDPOINTS` : session valable, puis rôle pour une route réservée aux
//! administrateurs (BR-ACCT-013 et BR-ACCT-014), avant toute lecture du corps. Un handler ne
//! redéclare rien : il reçoit le contexte authentifié par l'extracteur `Caller`. La couche pose
//! aussi le suivi des opérations quand la table le demande.

use axum::extract::{FromRequestParts, Request, State};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use hearth_proto::error::ErrorCode;

use super::{Access, ApiError, AppState, operations};
use crate::application::sessions::CurrentSession;

/// Ce que la couche d'accès a décidé pour une route : son niveau, et si ses requêtes qui
/// portent une clé d'opération sont suivies.
#[derive(Clone)]
pub struct GuardState {
    pub app: AppState,
    pub access: Access,
    pub tracked: bool,
}

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
async fn authenticate(state: &AppState, parts: &Parts) -> Result<CurrentSession, ApiError> {
    let token = bearer(parts).ok_or_else(|| {
        ApiError::new(
            ErrorCode::Unauthenticated,
            "Jeton de session absent ou illisible",
        )
    })?;
    Ok(state.sessions.authenticate(token).await?)
}

/// Un niveau d'accès autorise-t-il ce compte ?
fn allows(access: Access, session: &CurrentSession) -> bool {
    match access {
        // `FirstMessage` n'a pas de couche d'accès : le flux s'authentifie lui-même.
        Access::Public | Access::FirstMessage | Access::Authenticated => true,
        Access::Admin => session.account.role.can_manage_accounts(),
    }
}

/// La couche d'accès d'une route non publique.
pub async fn guard(State(guard): State<GuardState>, request: Request, next: Next) -> Response {
    let (mut parts, body) = request.into_parts();
    let session = match authenticate(&guard.app, &parts).await {
        Ok(session) => session,
        Err(error) => return error.into_response(),
    };
    if !allows(guard.access, &session) {
        return ApiError::new(
            ErrorCode::ForbiddenRole,
            "Tu n'as pas la permission pour accéder à la gestion des comptes",
        )
        .into_response();
    }
    parts.extensions.insert(Caller(session.clone()));
    let request = Request::from_parts(parts, body);
    if guard.tracked {
        operations::track(&guard.app, session, request, next).await
    } else {
        next.run(request).await
    }
}

/// Le compte et la session de l'appelant, posés par la couche d'accès. Un handler de route
/// publique n'en a pas ; l'utiliser sans couche d'accès est une erreur de câblage (`500`).
#[derive(Clone)]
pub struct Caller(pub CurrentSession);

impl<S: Send + Sync> FromRequestParts<S> for Caller {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Caller>()
            .cloned()
            .ok_or_else(|| ApiError::internal(&"route sans couche d'accès"))
    }
}

#[cfg(test)]
mod tests {
    use axum::http::Request;

    use super::*;
    use crate::application::accounts::AccountView;
    use crate::domain::accounts::{AccountId, Role, Username};
    use crate::domain::sessions::SessionId;
    use time::OffsetDateTime;

    fn parts(authorization: Option<&str>) -> Parts {
        let mut builder = Request::builder().uri("/x");
        if let Some(value) = authorization {
            builder = builder.header(AUTHORIZATION, value);
        }
        builder.body(()).unwrap().into_parts().0
    }

    fn session(role: Role) -> CurrentSession {
        CurrentSession {
            account: AccountView {
                id: AccountId::new("A"),
                username: Username::parse("marie").unwrap(),
                role,
                created_at: OffsetDateTime::UNIX_EPOCH,
                password_changed_at: OffsetDateTime::UNIX_EPOCH,
                last_login_at: None,
            },
            session_id: SessionId::new("S"),
            expires_at: OffsetDateTime::UNIX_EPOCH,
        }
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

    #[test]
    fn only_an_administrator_passes_the_admin_level() {
        assert!(allows(Access::Admin, &session(Role::Admin)));
        assert!(!allows(Access::Admin, &session(Role::ReadOnly)));
        for access in [Access::Public, Access::FirstMessage, Access::Authenticated] {
            assert!(allows(access, &session(Role::ReadOnly)));
        }
    }

    #[tokio::test]
    async fn a_handler_without_the_access_layer_is_a_wiring_error() {
        let mut parts = parts(None);
        let result = <Caller as FromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_err());
    }
}
