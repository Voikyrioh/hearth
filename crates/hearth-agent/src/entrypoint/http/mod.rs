//! API HTTPS `/api/v1`.
//!
//! Conventions :
//! - Un handler lit l'état partagé (cas d'usage), puis convertit la structure applicative en type
//!   du fil de `hearth-proto` (`wire.rs`) : le contrat JSON n'est connu qu'ici.
//! - **Toute route est déclarée dans `ENDPOINTS`** : méthode, chemin, niveau d'accès, suivie ou non
//!   par clé d'opération. Le routeur est construit depuis cette table et pose lui-même, par route,
//!   la couche d'accès (`auth::guard`, session puis rôle) et le suivi des opérations : un handler
//!   ne redéclare rien, il reçoit l'appelant par `Caller`. Le test de balayage
//!   (`tests/http_api.rs`) parcourt la table et vérifie le comportement réel de chaque route.
//! - La version d'interface (`version.rs`) est contrôlée sur toutes les routes sauf `/hello`.
//! - Toute erreur de routage, d'extraction ou de méthode sort au format `ErrorBody` (`error.rs`).

mod accounts;
mod audit;
mod auth;
mod error;
mod hello;
mod metrics;
mod operations;
mod server;
mod sessions;
mod update;
mod version;
mod wire;

use std::sync::Arc;

use axum::Router;
use axum::http::Method;
use axum::middleware;
use axum::routing::{MethodRouter, delete, get, patch, post, put};
use tower_http::LatencyUnit;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::application::accounts::AccountService;
use crate::application::audit::AuditService;
use crate::application::hello::HelloService;
use crate::application::metrics::MetricsService;
use crate::application::operations::OperationService;
use crate::application::ports::AuditSink;
use crate::application::sessions::SessionService;
use crate::application::update::UpdateService;
use crate::domain::audit::AuditAction;
use crate::entrypoint::ws::{self, StreamContext};

pub use error::ApiError;
pub use server::{ServerError, ServerHandle, spawn};
pub(crate) use wire::audit_item;

/// État partagé des routes : les cas d'usage, jamais d'infrastructure directe.
#[derive(Clone)]
pub struct AppState {
    pub hello: Arc<HelloService>,
    pub accounts: Arc<AccountService>,
    pub sessions: Arc<SessionService>,
    pub operations: Arc<OperationService>,
    pub audit: Arc<AuditService>,
    /// Écrit au journal les refus et les échecs relevés par la couche d'accès.
    pub sink: Arc<dyn AuditSink>,
    pub metrics: Arc<MetricsService>,
    /// Mise à jour de l'agent à distance (HRT-17).
    pub update: Arc<UpdateService>,
    pub stream: StreamContext,
}

/// Qui peut appeler une route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Personne n'a à s'authentifier (`/hello`, connexion).
    Public,
    /// Une session valable, quel que soit le rôle.
    Authenticated,
    /// L'authentification se fait dans le protocole de la route, par son premier message (le
    /// flux WebSocket) : aucune couche d'accès n'est posée sur la requête d'ouverture.
    FirstMessage,
    /// Une session valable d'un compte qui gère les comptes.
    Admin,
}

/// Une route de l'API : méthode, chemin (sous `/api/v1`), accès, handler.
pub struct Endpoint {
    pub method: Method,
    pub path: &'static str,
    pub access: Access,
    /// Le contrôle de version d'interface s'applique (tout sauf `/hello`).
    pub version_checked: bool,
    /// Les requêtes qui portent une `Idempotency-Key` sont suivies (`operations.rs`) : routes
    /// authentifiées qui modifient, hors connexion (son résultat contient un jeton).
    pub tracked: bool,
    /// L'action du journal d'activité de la route : celle de ses refus faute de droits et, pour
    /// une route qui modifie, de ses échecs, que la couche d'accès consigne (BR-AUDIT-003). Les
    /// succès sont écrits par les cas d'usage, dans la transaction de l'action ; la connexion
    /// (route publique) se consigne elle-même. `None` : consultation sans droit particulier
    /// (BR-AUDIT-004).
    pub audit: Option<AuditAction>,
    route: fn() -> MethodRouter<AppState>,
}

impl Endpoint {
    /// La route modifie-t-elle quelque chose ?
    pub fn modifies(&self) -> bool {
        !matches!(self.method, Method::GET | Method::HEAD | Method::OPTIONS)
    }
}

/// Table de toutes les routes. Ajouter une route = ajouter une ligne ici.
pub static ENDPOINTS: &[Endpoint] = &[
    Endpoint {
        method: Method::GET,
        path: "/hello",
        access: Access::Public,
        version_checked: false,
        tracked: false,
        audit: None,
        route: || get(hello::hello),
    },
    Endpoint {
        method: Method::POST,
        path: "/sessions",
        access: Access::Public,
        version_checked: true,
        tracked: false,
        audit: Some(AuditAction::Login),
        route: || post(sessions::login),
    },
    Endpoint {
        method: Method::DELETE,
        path: "/sessions/current",
        access: Access::Authenticated,
        version_checked: true,
        tracked: false,
        audit: Some(AuditAction::Logout),
        route: || delete(sessions::logout),
    },
    Endpoint {
        method: Method::GET,
        path: "/me",
        access: Access::Authenticated,
        version_checked: true,
        tracked: false,
        audit: None,
        route: || get(sessions::me),
    },
    Endpoint {
        method: Method::PUT,
        path: "/me/password",
        access: Access::Authenticated,
        version_checked: true,
        tracked: true,
        audit: Some(AuditAction::OwnPassword),
        route: || put(accounts::change_own_password),
    },
    Endpoint {
        method: Method::GET,
        path: "/operations/{id}",
        access: Access::Authenticated,
        version_checked: true,
        tracked: false,
        audit: None,
        route: || get(operations::get),
    },
    Endpoint {
        method: Method::GET,
        path: "/machine",
        access: Access::Authenticated,
        version_checked: true,
        tracked: false,
        audit: None,
        route: || get(metrics::machine),
    },
    Endpoint {
        method: Method::GET,
        path: "/metrics/history",
        access: Access::Authenticated,
        version_checked: true,
        tracked: false,
        audit: None,
        route: || get(metrics::history),
    },
    Endpoint {
        method: Method::GET,
        path: "/stream",
        access: Access::FirstMessage,
        version_checked: true,
        tracked: false,
        audit: None,
        route: || get(ws::stream),
    },
    Endpoint {
        method: Method::GET,
        path: "/accounts",
        access: Access::Admin,
        version_checked: true,
        tracked: false,
        audit: Some(AuditAction::AccountsRead),
        route: || get(accounts::list),
    },
    Endpoint {
        method: Method::POST,
        path: "/accounts",
        access: Access::Admin,
        version_checked: true,
        tracked: true,
        audit: Some(AuditAction::AccountCreate),
        route: || post(accounts::create),
    },
    Endpoint {
        method: Method::PATCH,
        path: "/accounts/{id}",
        access: Access::Admin,
        version_checked: true,
        tracked: true,
        audit: Some(AuditAction::AccountRole),
        route: || patch(accounts::change_role),
    },
    Endpoint {
        method: Method::DELETE,
        path: "/accounts/{id}",
        access: Access::Admin,
        version_checked: true,
        tracked: true,
        audit: Some(AuditAction::AccountDelete),
        route: || delete(accounts::delete),
    },
    Endpoint {
        method: Method::PUT,
        path: "/accounts/{id}/password",
        access: Access::Admin,
        version_checked: true,
        tracked: true,
        audit: Some(AuditAction::AccountPassword),
        route: || put(accounts::set_password),
    },
    Endpoint {
        method: Method::DELETE,
        path: "/accounts/{id}/sessions",
        access: Access::Admin,
        version_checked: true,
        tracked: true,
        audit: Some(AuditAction::SessionsRevoke),
        route: || delete(accounts::revoke_sessions),
    },
    Endpoint {
        method: Method::GET,
        path: "/agent/update",
        access: Access::Authenticated,
        version_checked: true,
        tracked: false,
        audit: None,
        route: || get(update::status),
    },
    Endpoint {
        method: Method::GET,
        path: "/agent/update/last",
        access: Access::Authenticated,
        version_checked: true,
        tracked: false,
        audit: None,
        route: || get(update::last),
    },
    Endpoint {
        method: Method::POST,
        path: "/agent/update",
        access: Access::Admin,
        version_checked: true,
        tracked: true,
        audit: Some(AuditAction::AgentUpdate),
        route: || post(update::start),
    },
    Endpoint {
        method: Method::GET,
        path: "/audit",
        access: Access::Admin,
        version_checked: true,
        tracked: false,
        audit: Some(AuditAction::AuditRead),
        route: || get(audit::list),
    },
    Endpoint {
        method: Method::GET,
        path: "/audit/export",
        access: Access::Admin,
        version_checked: true,
        tracked: false,
        audit: Some(AuditAction::AuditRead),
        route: || get(audit::export),
    },
];

/// Routeur complet : routes sous `/api/v1` construites depuis `ENDPOINTS`, erreurs de routage au
/// format `ErrorBody`, une ligne de journal par requête servie (méthode, chemin, statut, durée).
pub fn router(state: AppState) -> Router {
    let mut unchecked = Router::new();
    let mut checked = Router::new();
    for endpoint in ENDPOINTS {
        let mut route = (endpoint.route)();
        if !matches!(endpoint.access, Access::Public | Access::FirstMessage) {
            route = route.route_layer(middleware::from_fn_with_state(
                auth::GuardState {
                    app: state.clone(),
                    access: endpoint.access,
                    tracked: endpoint.tracked,
                    audit: endpoint.audit,
                    modifies: endpoint.modifies(),
                    route: endpoint.path,
                },
                auth::guard,
            ));
        }
        if endpoint.version_checked {
            checked = checked.route(endpoint.path, route);
        } else {
            unchecked = unchecked.route(endpoint.path, route);
        }
    }
    // La version est contrôlée avant tout (même sans jeton), puis l'accès et le suivi par route.
    let checked = checked.route_layer(middleware::from_fn(version::layer));
    let v1 = unchecked.merge(checked);
    with_error_fallbacks(Router::new().nest("/api/v1", v1))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_response(
                    DefaultOnResponse::new()
                        .level(Level::INFO)
                        .latency_unit(LatencyUnit::Millis),
                ),
        )
        .with_state(state)
}

fn with_error_fallbacks<S: Clone + Send + Sync + 'static>(router: Router<S>) -> Router<S> {
    router
        .fallback(error::not_found)
        .method_not_allowed_fallback(error::method_not_allowed)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use axum::Json;
    use axum::body::Body;
    use axum::extract::rejection::JsonRejection;
    use axum::http::{Request, StatusCode};
    use hearth_proto::error::{ErrorBody, ErrorCode};
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn extraction_errors_answer_in_the_error_format() {
        async fn echo(
            body: Result<Json<serde_json::Value>, JsonRejection>,
        ) -> Result<(), ApiError> {
            let _json = body?;
            Ok(())
        }
        let router = with_error_fallbacks(Router::new().route("/echo", post(echo)));
        let request = Request::builder()
            .method(Method::POST)
            .uri("/echo")
            .header("content-type", "application/json")
            .body(Body::from("pas du json"))
            .expect("request");
        let response = router.oneshot(request).await.expect("réponse");
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .expect("body");
        let body: ErrorBody = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(body.error.code, ErrorCode::ValidationError);
    }

    #[test]
    fn a_method_and_path_pair_is_declared_once() {
        let mut seen = HashSet::new();
        for endpoint in ENDPOINTS {
            assert!(
                seen.insert((endpoint.method.clone(), endpoint.path)),
                "{} {} déclarée deux fois",
                endpoint.method,
                endpoint.path
            );
        }
    }

    #[test]
    fn only_hello_and_the_login_are_public() {
        let public: Vec<_> = ENDPOINTS
            .iter()
            .filter(|endpoint| endpoint.access == Access::Public)
            .map(|endpoint| (endpoint.method.clone(), endpoint.path))
            .collect();
        assert_eq!(
            public,
            vec![(Method::GET, "/hello"), (Method::POST, "/sessions")]
        );
    }

    #[test]
    fn tracked_routes_are_the_authenticated_ones_that_modify_but_never_the_login() {
        for endpoint in ENDPOINTS {
            if endpoint.tracked {
                assert!(endpoint.modifies(), "{}", endpoint.path);
                assert_ne!(endpoint.access, Access::Public, "{}", endpoint.path);
            }
        }
        let login = ENDPOINTS
            .iter()
            .find(|endpoint| endpoint.path == "/sessions" && endpoint.method == Method::POST)
            .unwrap();
        assert!(!login.tracked);
        assert!(ENDPOINTS.iter().filter(|endpoint| endpoint.tracked).count() >= 5);
    }

    #[test]
    fn every_route_that_modifies_or_is_reserved_has_a_journal_action() {
        for endpoint in ENDPOINTS {
            if endpoint.modifies() || endpoint.access == Access::Admin {
                assert!(
                    endpoint.audit.is_some(),
                    "{} {} sans action de journal",
                    endpoint.method,
                    endpoint.path
                );
            }
        }
        // Les consultations sans droit particulier ne s'écrivent pas (BR-AUDIT-004).
        for path in ["/hello", "/me", "/operations/{id}"] {
            let endpoint = ENDPOINTS
                .iter()
                .find(|endpoint| endpoint.path == path)
                .unwrap();
            assert_eq!(endpoint.audit, None, "{path}");
        }
        // Lire le journal refusé s'écrit, sous le libellé de la spec (BR-AUDIT-021).
        for path in ["/audit", "/audit/export"] {
            let endpoint = ENDPOINTS
                .iter()
                .find(|endpoint| endpoint.path == path)
                .unwrap();
            assert_eq!(endpoint.access, Access::Admin);
            assert_eq!(endpoint.audit, Some(AuditAction::AuditRead));
            assert!(!endpoint.modifies() && !endpoint.tracked);
        }
    }

    #[test]
    fn only_the_stream_authenticates_by_its_first_message_and_it_is_version_checked() {
        let first_message: Vec<_> = ENDPOINTS
            .iter()
            .filter(|endpoint| endpoint.access == Access::FirstMessage)
            .collect();
        assert_eq!(first_message.len(), 1);
        assert_eq!(first_message[0].path, "/stream");
        assert_eq!(first_message[0].method, Method::GET);
        assert!(first_message[0].version_checked);
        assert!(!first_message[0].tracked);
    }

    #[test]
    fn only_hello_skips_the_version_check() {
        for endpoint in ENDPOINTS {
            assert_eq!(
                endpoint.version_checked,
                endpoint.path != "/hello",
                "{}",
                endpoint.path
            );
        }
    }
}
