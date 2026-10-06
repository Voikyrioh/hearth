//! Contrôle d'accès : l'unique couche qui décide qui est l'appelant et ce qu'il a le droit de faire.
//!
//! Le routeur pose cette couche (`guard`) sur chaque route non publique, d'après le niveau
//! d'accès déclaré dans `ENDPOINTS` : session valable, puis rôle pour une route réservée aux
//! administrateurs (BR-ACCT-013 et BR-ACCT-014), avant toute lecture du corps. Un handler ne
//! redéclare rien : il reçoit le contexte authentifié par l'extracteur `Caller`. La couche pose
//! aussi le suivi des opérations quand la table le demande, et consigne au journal d'activité les
//! refus faute de droits et les échecs des requêtes qui modifient (BR-AUDIT-003), d'après la
//! colonne « action de journal » de la table : un handler n'y pense pas.

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, FromRequestParts, Request, State};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use hearth_proto::error::ErrorCode;
use hearth_proto::headers;

use tracing::Instrument;

use super::error::ErrorMark;
use super::{Access, ApiError, AppState, operations};
use crate::application::sessions::CurrentSession;
use crate::domain::accounts::AccountId;
use crate::domain::audit::{
    Actor, AuditAction, Origin, Outcome, Reason, RequestKind, Target, is_journaled,
};

/// Ce que la couche d'accès a décidé pour une route : son niveau, et si ses requêtes qui
/// portent une clé d'opération sont suivies.
#[derive(Clone)]
pub struct GuardState {
    pub app: AppState,
    pub access: Access,
    pub tracked: bool,
    /// L'action du journal d'activité de la route (`Endpoint::audit`).
    pub audit: Option<AuditAction>,
    /// La route modifie quelque chose (`Endpoint::modifies`).
    pub modifies: bool,
    /// Le motif de la route (`/accounts/{id}`) : la cible d'un refus ou d'un échec.
    pub route: &'static str,
}

impl GuardState {
    /// Consigne un refus ou un échec si la route a une action et si la règle du journal le veut
    /// (`domain::audit::is_journaled`). Un échec d'écriture est tracé, jamais subi par l'appelant.
    async fn journal(&self, actor: &Actor, target: Target, outcome: Outcome) {
        let Some(action) = self.audit else {
            return;
        };
        let kind = if self.modifies {
            RequestKind::Modification
        } else {
            RequestKind::Consultation
        };
        if !is_journaled(kind, outcome.kind()) {
            return;
        }
        // Tâche détachée, dans le span de la requête : un client qui coupe n'annule pas l'écriture.
        let sink = self.app.sink.clone();
        let actor = actor.clone();
        let write = tokio::spawn(
            async move { sink.record(actor, action, target, outcome).await }.in_current_span(),
        );
        if let Err(error) = write.await {
            tracing::error!(%error, "écriture du journal interrompue");
        }
    }

    /// La cible d'une action sur `/accounts/{id}…` : le nom du compte, résolu **avant** l'action
    /// (l'action peut supprimer le compte). Sans identifiant dans la route, ou compte inconnu : le
    /// motif de la route.
    async fn target_of(&self, parts: &Parts) -> Target {
        if self.audit.is_none() {
            return Target::Route(self.route);
        }
        let Some(id) = path_param(self.route, parts.uri.path(), "id") else {
            return Target::Route(self.route);
        };
        match self.app.accounts.username_of(&AccountId::new(id)).await {
            Ok(Some(username)) => Target::Account(username),
            Ok(None) => Target::Route(self.route),
            Err(error) => {
                tracing::warn!(%error, "cible du journal non résolue");
                Target::Route(self.route)
            }
        }
    }
}

/// La valeur du paramètre `name` du chemin `path` d'après le motif de la route (`/accounts/{id}`) :
/// les segments se comparent par la fin, que le chemin porte ou non son préfixe `/api/v1`.
fn path_param(pattern: &str, path: &str, name: &str) -> Option<String> {
    let wanted = format!("{{{name}}}");
    let pattern: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let path: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let offset = path.len().checked_sub(pattern.len())?;
    pattern
        .iter()
        .position(|segment| *segment == wanted)
        .and_then(|index| path.get(offset + index))
        .map(|value| (*value).to_owned())
}

/// Le refus faute de droits, dit pour ce que la route protège.
fn forbidden_message(action: Option<AuditAction>) -> &'static str {
    match action {
        Some(AuditAction::AuditRead) => "Tu n'as pas la permission de lire le journal d'activité",
        Some(AuditAction::AgentUpdate) => "Seul un administrateur peut mettre à jour l'agent",
        _ => "Tu n'as pas la permission pour accéder à la gestion des comptes",
    }
}

/// Ce que le journal retient d'une réponse d'erreur : un refus faute de droits, ou un échec.
/// `None` pour ce qui n'est pas une action ratée : pas d'appelant reconnu, connexion (qui se
/// consigne elle-même), requête qui ne s'est pas exécutée (clé d'opération rejouée ou déjà en
/// cours), version incompatible, route ou méthode inconnue. Exhaustif : un nouveau code oblige à
/// choisir.
fn failure_of(code: ErrorCode) -> Option<Outcome> {
    match code {
        ErrorCode::ForbiddenRole => Some(Outcome::Denied(Reason::ReadOnly)),
        ErrorCode::ValidationError | ErrorCode::WeakPassword | ErrorCode::PayloadTooLarge => {
            Some(Outcome::Failed(Reason::Validation))
        }
        ErrorCode::UsernameTaken => Some(Outcome::Failed(Reason::UsernameTaken)),
        ErrorCode::WrongPassword => Some(Outcome::Failed(Reason::WrongPassword)),
        ErrorCode::LastAdmin => Some(Outcome::Failed(Reason::LastAdmin)),
        ErrorCode::Conflict => Some(Outcome::Failed(Reason::Conflict)),
        ErrorCode::NotFound => Some(Outcome::Failed(Reason::NotFound)),
        ErrorCode::Busy => Some(Outcome::Failed(Reason::Busy)),
        ErrorCode::InternalError => Some(Outcome::Failed(Reason::Internal)),
        ErrorCode::ManagedInstall => Some(Outcome::Failed(Reason::ManagedInstall)),
        ErrorCode::BadSignature => Some(Outcome::Failed(Reason::BadSignature)),
        ErrorCode::Unauthenticated
        | ErrorCode::InvalidCredentials
        | ErrorCode::SessionExpired
        | ErrorCode::SessionRevoked
        | ErrorCode::OperationInProgress
        | ErrorCode::IdempotencyKeyReused
        | ErrorCode::IncompatibleVersion
        | ErrorCode::TooManyAttempts
        | ErrorCode::MethodNotAllowed => None,
    }
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
    // L'adresse de la connexion TCP : l'usage d'une session depuis une adresse déjà retenue la
    // rafraîchit (HRT-22). Jamais lue d'un en-tête de mandataire.
    let addr = match origin_of(parts).addr() {
        Some(addr) if !addr.is_empty() => Some(addr.to_owned()),
        _ => None,
    };
    Ok(match addr {
        Some(addr) => state.sessions.authenticate_at(token, &addr).await?,
        None => state.sessions.authenticate(token).await?,
    })
}

/// Un niveau d'accès autorise-t-il ce compte ?
fn allows(access: Access, session: &CurrentSession) -> bool {
    match access {
        // `FirstMessage` n'a pas de couche d'accès : le flux s'authentifie lui-même.
        Access::Public | Access::FirstMessage | Access::Authenticated => true,
        Access::Admin => session.account.role.can_manage_accounts(),
    }
}

/// D'où vient la requête : l'adresse de la connexion TCP (jamais un en-tête de mandataire) et le
/// nom du poste annoncé par le client.
fn origin_of(parts: &Parts) -> Origin {
    let addr = parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip().to_canonical().to_string())
        .unwrap_or_default();
    let name = parts
        .headers
        .get(headers::CLIENT)
        .and_then(|value| value.to_str().ok());
    Origin::client(name, &addr)
}

/// La couche d'accès d'une route non publique.
pub async fn guard(State(guard): State<GuardState>, request: Request, next: Next) -> Response {
    let (mut parts, body) = request.into_parts();
    let session = match authenticate(&guard.app, &parts).await {
        Ok(session) => session,
        Err(error) => return error.into_response(),
    };
    let actor = Actor::new(Some(session.account.username.clone()), origin_of(&parts));
    if !allows(guard.access, &session) {
        // Le nom du compte visé n'est lu que pour un refus à écrire.
        let target = guard.target_of(&parts).await;
        // BR-AUDIT-003, BR-AUDIT-021 : toute action refusée faute de droits est consignée,
        // consultation du journal comprise.
        guard
            .journal(&actor, target, Outcome::Denied(Reason::ReadOnly))
            .await;
        return ApiError::new(ErrorCode::ForbiddenRole, forbidden_message(guard.audit))
            .into_response();
    }
    // Pour une requête qui modifie seulement : l'action peut supprimer le compte visé, il faut le
    // nommer avant. Une lecture réussie n'écrit rien : rien à résoudre.
    let target = if guard.modifies {
        guard.target_of(&parts).await
    } else {
        Target::Route(guard.route)
    };
    parts.extensions.insert(Requester(actor.clone()));
    parts.extensions.insert(Caller(session.clone()));
    let request = Request::from_parts(parts, body);
    let response = if guard.tracked {
        operations::track(&guard.app, session, request, next).await
    } else {
        next.run(request).await
    };
    // Un échec de la requête (jamais le succès : le cas d'usage l'a écrit dans sa transaction).
    // Une réponse rejouée depuis la clé d'opération ne s'est pas exécutée : rien à consigner.
    let replayed = response
        .headers()
        .contains_key(headers::IDEMPOTENT_REPLAYED);
    if !replayed
        && let Some(ErrorMark(code)) = response.extensions().get::<ErrorMark>().copied()
        && let Some(outcome) = failure_of(code)
    {
        guard.journal(&actor, target, outcome).await;
    }
    response
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

/// Qui fait la requête, pour le journal d'activité : le compte de l'appelant et l'origine de la
/// demande. Posé par la couche d'accès, comme `Caller`.
#[derive(Clone)]
pub struct Requester(pub Actor);

impl<S: Send + Sync> FromRequestParts<S> for Requester {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Requester>()
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

    #[test]
    fn the_account_id_is_read_from_the_route_pattern_with_or_without_the_prefix() {
        for path in ["/accounts/01ABC", "/api/v1/accounts/01ABC"] {
            assert_eq!(
                path_param("/accounts/{id}", path, "id").as_deref(),
                Some("01ABC")
            );
        }
        assert_eq!(
            path_param(
                "/accounts/{id}/password",
                "/api/v1/accounts/01ABC/password",
                "id"
            )
            .as_deref(),
            Some("01ABC")
        );
        assert_eq!(path_param("/accounts", "/api/v1/accounts", "id"), None);
        assert_eq!(path_param("/accounts/{id}", "/x", "id"), None);
    }

    #[test]
    fn a_refusal_speaks_of_what_the_route_protects() {
        assert!(forbidden_message(Some(AuditAction::AuditRead)).contains("journal"));
        assert!(forbidden_message(Some(AuditAction::AccountCreate)).contains("comptes"));
        assert!(forbidden_message(None).contains("comptes"));
    }

    #[test]
    fn an_error_is_journaled_as_a_denial_or_a_failure_and_never_as_noise() {
        use ErrorCode::*;
        let outcome = |code| failure_of(code).map(|outcome| outcome.kind().code());
        assert_eq!(outcome(ForbiddenRole), Some("denied"));
        for code in [
            ValidationError,
            WeakPassword,
            PayloadTooLarge,
            UsernameTaken,
            WrongPassword,
            LastAdmin,
            Conflict,
            NotFound,
            Busy,
            InternalError,
        ] {
            assert_eq!(outcome(code), Some("failed"), "{code:?}");
        }
        // Pas une action ratée : pas d'appelant, connexion, requête non exécutée, routage.
        for code in [
            Unauthenticated,
            InvalidCredentials,
            SessionExpired,
            SessionRevoked,
            OperationInProgress,
            IdempotencyKeyReused,
            IncompatibleVersion,
            TooManyAttempts,
            MethodNotAllowed,
        ] {
            assert_eq!(outcome(code), None, "{code:?}");
        }
    }

    #[tokio::test]
    async fn a_handler_without_the_access_layer_is_a_wiring_error() {
        let mut parts = parts(None);
        let result = <Caller as FromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_err());
    }
}
