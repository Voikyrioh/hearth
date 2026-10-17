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
mod devices;
mod error;
mod hello;
mod metrics;
mod operations;
mod reauth;
mod security;
mod server;
mod service_ops;
mod services;
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
use crate::application::security::SecurityService;
use crate::application::service_adoption::ServiceAdoption;
use crate::application::service_lifecycle::ServiceLifecycle;
use crate::application::service_logs::ServiceLogs;
use crate::application::service_op_access::ServiceOpAccess;
use crate::application::services::ServicesRegistry;
use crate::application::sessions::SessionService;
use crate::application::update::UpdateService;
use crate::domain::audit::AuditAction;
use crate::entrypoint::ws::{self, StreamContext};

pub use error::ApiError;
pub(crate) use security::view as security_view;
pub use server::{ServerError, ServerHandle, spawn};
pub(crate) use wire::audit_item;

/// État partagé des routes : les cas d'usage, jamais d'infrastructure directe.
#[derive(Clone)]
pub struct AppState {
    pub hello: Arc<HelloService>,
    pub accounts: Arc<AccountService>,
    pub sessions: Arc<SessionService>,
    /// État de sécurité et alerte (HRT-24).
    pub security: Arc<SecurityService>,
    pub operations: Arc<OperationService>,
    pub audit: Arc<AuditService>,
    /// Écrit au journal les refus et les échecs relevés par la couche d'accès.
    pub sink: Arc<dyn AuditSink>,
    pub metrics: Arc<MetricsService>,
    /// Mise à jour de l'agent à distance (HRT-17).
    pub update: Arc<UpdateService>,
    /// Moteur de conteneurs et liste des services, en lecture seule (HRT-61).
    pub services: Arc<ServicesRegistry>,
    /// Lectures de journaux d'un service en direct, dix au plus (HRT-66).
    pub service_logs: Arc<ServiceLogs>,
    /// Adopter un conteneur existant et le relâcher (HRT-80, ADR-0043).
    pub service_adoption: Arc<ServiceAdoption>,
    /// Lire, annuler et décider les opérations de service, et les suivre en direct (HRT-144, ADR-0045).
    pub service_ops: Arc<ServiceOpAccess>,
    /// Démarrer, arrêter, redémarrer un service (HRT-64, ADR-0037, ADR-0045).
    pub service_lifecycle: Arc<ServiceLifecycle>,
    pub stream: StreamContext,
    /// L'effacement physique des anciennes empreintes de requêtes est en attente (ADR-0034) : dit aux
    /// administrateurs par `GET /security`, sans rien de sensible.
    pub erasure_pending: bool,
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
    /// Une session valable d'un compte qui a le droit de gérer les services (ADR-0041, BR-GRANT-003),
    /// quel que soit son rôle. Le niveau de TOUT acte de service, existant ou à venir : le droit est
    /// jugé par la couche d'accès (`auth::allows`), à chaque requête, et nulle part ailleurs.
    Services,
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

/// Les routes `POST` qui ne modifient rien : un calcul sans effet ni écriture (le défi de la clé
/// d'appareil) ou une lecture relancée (`/engine/recheck` relance la détection du moteur, sans rien
/// écrire ni dans le moteur ni dans la base). Elles n'ont ni action de journal ni entrée de réussite.
const READ_ONLY_POSTS: &[&str] = &["/sessions/challenge", "/engine/recheck"];

impl Endpoint {
    /// La route modifie-t-elle quelque chose ?
    pub fn modifies(&self) -> bool {
        let reads = matches!(self.method, Method::GET | Method::HEAD | Method::OPTIONS)
            || (self.method == Method::POST && READ_ONLY_POSTS.contains(&self.path));
        !reads
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
        method: Method::POST,
        path: "/sessions/challenge",
        access: Access::Public,
        version_checked: true,
        tracked: false,
        audit: None,
        route: || post(sessions::challenge),
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
        method: Method::GET,
        path: "/me/devices",
        access: Access::Authenticated,
        version_checked: true,
        tracked: false,
        audit: None,
        route: || get(devices::list),
    },
    Endpoint {
        method: Method::DELETE,
        path: "/me/devices/{id}",
        access: Access::Authenticated,
        version_checked: true,
        tracked: true,
        audit: Some(AuditAction::DeviceRemove),
        route: || delete(devices::remove),
    },
    Endpoint {
        method: Method::GET,
        path: "/security",
        access: Access::Authenticated,
        version_checked: true,
        tracked: false,
        audit: None,
        route: || get(security::state),
    },
    Endpoint {
        method: Method::PUT,
        path: "/security/attack-mode",
        access: Access::Admin,
        version_checked: true,
        tracked: true,
        // Le geste (activer ou désactiver) se lit dans le corps : la couche d'accès l'y prend.
        audit: Some(AuditAction::AttackModeEnable),
        route: || put(security::set_attack_mode),
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
        method: Method::PUT,
        path: "/me/reauth",
        access: Access::Authenticated,
        version_checked: true,
        tracked: true,
        audit: Some(AuditAction::ReauthSetting),
        route: || put(security::set_reauth),
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
        path: "/services",
        access: Access::Authenticated,
        version_checked: true,
        tracked: false,
        audit: None,
        route: || get(services::list),
    },
    Endpoint {
        method: Method::POST,
        path: "/engine/recheck",
        access: Access::Authenticated,
        version_checked: true,
        tracked: false,
        audit: None,
        route: || post(services::recheck),
    },
    // Les journaux d'un service en direct (HRT-66) : un WebSocket par lecture, authentifié par son premier
    // message comme `/stream`. Le niveau `Access::Services` ne s'applique pas à un socket : le droit est
    // jugé par `domain::services::logs::may_read` (HRT-79), à l'ouverture puis toutes les 2 s et à chaque
    // changement du compte. Service créé par Hearth : tous les comptes ; service adopté : le droit de
    // gérer les services ; conteneur non adopté : réponse d'un service inconnu (BR-ADOPT-004,
    // BR-ADOPT-015, BR-ADOPT-016). Lire n'est pas journalisé (BR-SLOG-022). Classée `ServiceStream`.
    Endpoint {
        method: Method::GET,
        path: "/services/{key}/logs",
        access: Access::FirstMessage,
        version_checked: true,
        tracked: false,
        audit: None,
        route: || get(ws::logs),
    },
    // Révéler les valeurs des variables d'un service : réservé aux comptes qui ont le droit de gérer les
    // services, quel que soit leur rôle (BR-SVC-009, BR-GRANT-003). Le refus faute du droit est
    // consigné par la couche d'accès, la réussite par le cas d'usage ; une consultation, pas un acte
    // d'administration du socle (aucun mot de passe).
    Endpoint {
        method: Method::GET,
        path: "/services/{key}/env",
        access: Access::Services,
        version_checked: true,
        tracked: false,
        audit: Some(AuditAction::ServiceEnvReveal),
        route: || get(services::env),
    },
    // Adopter des conteneurs existants (HRT-80, ADR-0043, BR-ADOPT-005 à BR-ADOPT-013). Les trois exigent le
    // droit « gérer les services » (niveau `Services`, refus consigné par la couche d'accès). La liste des
    // candidats est consignée quand elle réussit (sans nom) comme quand elle est refusée, jamais sur le flux. Adopter est
    // un acte d'administration du socle (preuve de clé du poste, mot de passe À CHAQUE FOIS : jamais
    // couvert par l'élévation de 5 minutes) ; relâcher en est un aussi, couvert par l'élévation (il retire
    // un accès, il n'en donne pas). Classées `Service`.
    Endpoint {
        method: Method::GET,
        path: "/services/external",
        access: Access::Services,
        version_checked: true,
        tracked: false,
        audit: Some(AuditAction::ServiceExternalRead),
        route: || get(services::external),
    },
    Endpoint {
        method: Method::POST,
        path: "/services/adopt",
        access: Access::Services,
        version_checked: true,
        tracked: true,
        audit: Some(AuditAction::ServiceAdopt),
        route: || post(services::adopt),
    },
    Endpoint {
        method: Method::POST,
        path: "/services/{key}/release",
        access: Access::Services,
        version_checked: true,
        tracked: true,
        audit: Some(AuditAction::ServiceRelease),
        route: || post(services::release),
    },
    // Démarrer, arrêter, redémarrer un service (HRT-64, ADR-0037) : trois actes d'administration confirmés
    // (`ServiceStart` `0x0C`, `ServiceStop` `0x0D`, `ServiceRestart` `0x0E`, couverts par l'élévation),
    // asynchrones (`202`), réservés au droit « gérer les services » (BR-LIFE-001). La couche d'accès consigne
    // les refus et les échecs sous l'action de la ligne ; la réussite est écrite à la fin de l'action.
    Endpoint {
        method: Method::POST,
        path: "/services/{key}/start",
        access: Access::Services,
        version_checked: true,
        tracked: true,
        audit: Some(AuditAction::ServiceStart),
        route: || post(services::start),
    },
    Endpoint {
        method: Method::POST,
        path: "/services/{key}/stop",
        access: Access::Services,
        version_checked: true,
        tracked: true,
        audit: Some(AuditAction::ServiceStop),
        route: || post(services::stop),
    },
    Endpoint {
        method: Method::POST,
        path: "/services/{key}/restart",
        access: Access::Services,
        version_checked: true,
        tracked: true,
        audit: Some(AuditAction::ServiceRestart),
        route: || post(services::restart),
    },
    // Les opérations de service (HRT-144, ADR-0045). Lire : tout compte authentifié, mais une opération sur
    // un service que le compte ne voit pas est une opération inconnue (`service_op_access::read`). Annuler
    // et décider : le droit « gérer les services » (niveau `Services`), actes d'administration confirmés
    // (`OperationCancel` `0x10`, `OperationDecide` `0x1D`, couverts par l'élévation). La réussite s'écrit
    // sous l'action du genre de l'opération ; l'action de la ligne est celle des refus et des échecs.
    Endpoint {
        method: Method::GET,
        path: "/operations/services/{id}",
        access: Access::Authenticated,
        version_checked: true,
        tracked: false,
        audit: None,
        route: || get(service_ops::get),
    },
    Endpoint {
        method: Method::DELETE,
        path: "/operations/services/{id}",
        access: Access::Services,
        version_checked: true,
        tracked: true,
        audit: Some(AuditAction::OperationCancel),
        route: || delete(service_ops::cancel),
    },
    Endpoint {
        method: Method::POST,
        path: "/operations/services/{id}/decision",
        access: Access::Services,
        version_checked: true,
        tracked: true,
        audit: Some(AuditAction::OperationDecide),
        route: || post(service_ops::decide),
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
        // La confirmation des actes d'administration (HRT-28) : posée d'après la table des actes de
        // `hearth-proto`, AVANT la couche d'accès (donc dedans : session, rôle, suivi, puis confirmation).
        if let Some(act) =
            hearth_proto::admin_act::route_act(endpoint.method.as_str(), endpoint.path)
            && act.contract == hearth_proto::admin_act::Contract::Reauth
        {
            route = route.route_layer(middleware::from_fn_with_state(
                reauth::ReauthState {
                    app: state.clone(),
                    route: act,
                },
                reauth::layer,
            ));
        }
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
    fn only_hello_the_login_and_its_challenge_are_public() {
        let public: Vec<_> = ENDPOINTS
            .iter()
            .filter(|endpoint| endpoint.access == Access::Public)
            .map(|endpoint| (endpoint.method.clone(), endpoint.path))
            .collect();
        assert_eq!(
            public,
            vec![
                (Method::GET, "/hello"),
                (Method::POST, "/sessions"),
                (Method::POST, "/sessions/challenge"),
            ]
        );
    }

    #[test]
    fn the_challenge_is_a_post_that_modifies_nothing_and_writes_no_journal_entry() {
        let challenge = ENDPOINTS
            .iter()
            .find(|endpoint| endpoint.path == "/sessions/challenge")
            .unwrap();
        assert_eq!(challenge.method, Method::POST);
        assert!(!challenge.modifies());
        assert!(!challenge.tracked);
        assert_eq!(challenge.audit, None);
        // Aucune autre route POST n'est exemptée par erreur : les seules lectures en POST sont le
        // défi de la clé d'appareil et la relance de la détection du moteur.
        for endpoint in ENDPOINTS.iter().filter(|e| e.method == Method::POST) {
            assert_eq!(
                endpoint.modifies(),
                !matches!(endpoint.path, "/sessions/challenge" | "/engine/recheck"),
                "{}",
                endpoint.path
            );
        }
    }

    #[test]
    fn the_services_routes_are_reads_open_to_both_roles_and_never_journaled() {
        for (method, path) in [
            (Method::GET, "/services"),
            (Method::POST, "/engine/recheck"),
        ] {
            let endpoint = ENDPOINTS
                .iter()
                .find(|endpoint| endpoint.method == method && endpoint.path == path)
                .unwrap();
            assert_eq!(endpoint.access, Access::Authenticated, "{path}");
            assert!(!endpoint.modifies() && !endpoint.tracked, "{path}");
            assert_eq!(endpoint.audit, None, "{path}");
            assert!(endpoint.version_checked, "{path}");
        }
        // La révélation des variables : réservée au droit de gérer les services, journalisée, jamais suivie.
        let reveal = ENDPOINTS
            .iter()
            .find(|endpoint| endpoint.path == "/services/{key}/env")
            .unwrap();
        assert_eq!(reveal.method, Method::GET);
        assert_eq!(reveal.access, Access::Services);
        assert!(!reveal.modifies() && !reveal.tracked && reveal.version_checked);
        assert_eq!(reveal.audit, Some(AuditAction::ServiceEnvReveal));
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
            if endpoint.modifies() || matches!(endpoint.access, Access::Admin | Access::Services) {
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

    /// Ce qu'une route est, au regard de BR-GRANT-001 à BR-GRANT-005. CHAQUE route de `ENDPOINTS` est
    /// classée ici, une fois : une route nouvelle non classée fait échouer
    /// `every_route_is_classified_and_every_service_route_is_at_the_services_level`, et c'est le point
    /// de revue de qui l'ajoute.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Class {
        /// Aucune session (`/hello`, connexion, défi).
        Public,
        /// Authentifiée par le premier message (le flux).
        Stream,
        /// Une session, tout rôle ; lit ou agit sur le compte de l'APPELANT seulement.
        Own,
        /// Une lecture ouverte à tout rôle, y compris sur les services (liste, détection du moteur) :
        /// la liste fermée des routes de service ouvertes à tous.
        OpenRead,
        /// Réservée aux administrateurs. Le texte dit pourquoi elle ne livre à un administrateur SANS le
        /// droit ni le droit ni le compte d'un titulaire (inventaire de BR-GRANT-005) : « PROTÉGÉE : … »
        /// si le code refuse, « sans danger : … » si la preuve est que la route n'y mène pas.
        Admin(&'static str),
        /// Tout acte (ou lecture réservée) de service : niveau `Services`.
        Service,
        /// Une lecture de service en direct, authentifiée par son premier message comme le flux, jamais
        /// consignée (BR-SLOG-022). Le rôle n'y entre pas ; le droit de gérer les services et l'adoption
        /// sont jugés par `may_read` (HRT-79), seul point de contrôle, rejoué pendant la lecture : un
        /// service créé par Hearth est lisible de tous, un adopté du seul droit, un conteneur non adopté
        /// de personne (réponse d'un service inconnu). `Access::Services` ne peut pas l'exprimer : il est
        /// jugé à l'ouverture d'une requête, pas rejoué pendant un socket.
        ServiceStream,
    }

    use Class::{Admin, OpenRead, Own, Public, Service, ServiceStream, Stream};

    const CLASSES: &[(&str, &str, Class)] = &[
        ("GET", "/hello", Public),
        ("POST", "/sessions", Public),
        ("POST", "/sessions/challenge", Public),
        ("DELETE", "/sessions/current", Own),
        ("GET", "/me", Own),
        ("GET", "/me/devices", Own),
        ("DELETE", "/me/devices/{id}", Own),
        ("GET", "/security", Own),
        (
            "PUT",
            "/security/attack-mode",
            Admin(
                "sans danger : réglage du serveur entier, ne vise aucun compte et n'ajoute de moyen de connexion à personne ; exige la preuve de clé de l'appelant",
            ),
        ),
        ("PUT", "/me/password", Own),
        ("PUT", "/me/reauth", Own),
        ("GET", "/operations/{id}", Own),
        ("GET", "/machine", Own),
        ("GET", "/metrics/history", Own),
        ("GET", "/stream", Stream),
        ("GET", "/services", OpenRead),
        ("POST", "/engine/recheck", OpenRead),
        // Journaux en direct (HRT-66, HRT-79) : le droit et l'adoption sont jugés par `may_read`, pas par
        // le niveau d'accès de la route (voir `Class::ServiceStream`).
        ("GET", "/services/{key}/logs", ServiceStream),
        ("GET", "/services/{key}/env", Service),
        // Adopter et relâcher (HRT-80) : le droit est exigé par le niveau, la confirmation par la table
        // des actes de `hearth-proto`. Aucune route n'adopte « tout » (BR-ADOPT-021).
        ("GET", "/services/external", Service),
        ("POST", "/services/adopt", Service),
        ("POST", "/services/{key}/release", Service),
        // Démarrer, arrêter, redémarrer (HRT-64) : le droit est exigé par le niveau, la confirmation par la table
        // des actes de `hearth-proto` (BR-LIFE-001, BR-LIFE-016).
        ("POST", "/services/{key}/start", Service),
        ("POST", "/services/{key}/stop", Service),
        ("POST", "/services/{key}/restart", Service),
        // Les opérations de service (HRT-144) : lire est ouvert à tout compte (la visibilité du service est
        // jugée par `ServiceOpAccess`), annuler et décider exigent le droit et la confirmation de l'acte.
        ("GET", "/operations/services/{id}", OpenRead),
        ("DELETE", "/operations/services/{id}", Service),
        ("POST", "/operations/services/{id}/decision", Service),
        (
            "GET",
            "/accounts",
            Admin("sans danger : lecture, aucun haché ni moyen de connexion"),
        ),
        (
            "POST",
            "/accounts",
            Admin(
                "sans danger : un compte naît sans le droit quel que soit son rôle (BR-GRANT-002) ; son créateur choisit le mot de passe d'un compte NEUF, pas celui d'un titulaire",
            ),
        ),
        (
            "PATCH",
            "/accounts/{id}",
            Admin(
                "sans danger : le rôle est un axe indépendant du droit, il ne donne ni mot de passe ni session ni droit",
            ),
        ),
        (
            "DELETE",
            "/accounts/{id}",
            Admin(
                "PROTÉGÉE : refusée sur un compte qui a le droit sauf par lui-même, par un compte qui l'a aussi ou par la ligne de commande (le droit se donne par nom : supprimer puis recréer un homonyme le ferait redonner à un inconnu ; la suppression efface aussi les postes de confiance)",
            ),
        ),
        (
            "PUT",
            "/accounts/{id}/password",
            Admin(
                "PROTÉGÉE : refusée sur un compte qui a le droit sauf par lui-même, par un compte qui l'a aussi ou par la ligne de commande",
            ),
        ),
        (
            "DELETE",
            "/accounts/{id}/sessions",
            Admin(
                "sans danger : ferme des sessions, n'ajoute aucun moyen de connexion et, sur un titulaire par un administrateur sans le droit, ne touche ni à ses postes de confiance ni à ses adresses retenues (BR-ACCT-017, 018) ; un poste ne s'inscrit qu'à la connexion, avec le mot de passe du titulaire",
            ),
        ),
        ("GET", "/agent/update", Own),
        ("GET", "/agent/update/last", Own),
        (
            "POST",
            "/agent/update",
            Admin(
                "sans danger : n'installe qu'un binaire signé par la clé de publication ; aucune entrée du demandeur n'est du code, la migration 0010 laisse le défaut « aucun »",
            ),
        ),
        ("GET", "/audit", Admin("sans danger : lecture du journal")),
        (
            "GET",
            "/audit/export",
            Admin("sans danger : lecture du journal"),
        ),
    ];

    /// Les préfixes des routes de service : sous l'un d'eux, une route est ouverte à tous (la liste
    /// fermée `OpenRead`) ou de niveau `Services`, jamais autre chose.
    const SERVICE_PREFIXES: &[&str] = &[
        "/services",
        "/deployments",
        "/engine",
        "/adoptions",
        "/containers",
        "/operations/services",
    ];

    /// BR-GRANT-003 et BR-GRANT-005 : le classement est EXHAUSTIF par construction. Toute route déclarée
    /// y figure (une route nouvelle non classée fait échouer ce test), toute ligne a sa route, et le
    /// niveau déclaré est celui de la classe. Sous un préfixe de service, seules les routes de la liste
    /// fermée `OpenRead` échappent au niveau `Services`.
    #[test]
    fn every_route_is_classified_and_every_service_route_is_at_the_services_level() {
        for endpoint in ENDPOINTS {
            let (method, path) = (&endpoint.method, endpoint.path);
            let rows: Vec<_> = CLASSES
                .iter()
                .filter(|(m, p, _)| *m == method.as_str() && *p == path)
                .collect();
            assert_eq!(
                rows.len(),
                1,
                "{method} {path} n'est pas classée (ou l'est deux fois) : la classer dans CLASSES est le point de revue de toute route nouvelle"
            );
            let class = rows[0].2;
            let expected = match class {
                Public => Access::Public,
                Stream | ServiceStream => Access::FirstMessage,
                Own | OpenRead => Access::Authenticated,
                Admin(_) => Access::Admin,
                Service => Access::Services,
            };
            assert_eq!(endpoint.access, expected, "{method} {path} : {class:?}");
            let under_service_prefix = SERVICE_PREFIXES
                .iter()
                .any(|prefix| path == *prefix || path.starts_with(&format!("{prefix}/")));
            if under_service_prefix {
                assert!(
                    matches!(class, OpenRead | Service | ServiceStream),
                    "{method} {path} : une route de service est ouverte à tous (liste fermée) ou de niveau `services`"
                );
            }
            match class {
                ServiceStream => {
                    // Un socket de lecture : premier message, jamais consigné, ne modifie rien.
                    assert!(
                        under_service_prefix && !endpoint.modifies() && !endpoint.tracked,
                        "{method} {path} : une lecture en direct ne modifie rien"
                    );
                    assert_eq!(endpoint.audit, None, "{method} {path}");
                }
                OpenRead => {
                    assert!(
                        under_service_prefix && !endpoint.modifies() && !endpoint.tracked,
                        "{method} {path} : une route ouverte à tous ne modifie rien"
                    );
                    assert_eq!(endpoint.audit, None, "{method} {path}");
                }
                Service => {
                    // Chaque acte de service est consigné : un refus sans droit s'écrit sous son action.
                    assert!(endpoint.audit.is_some(), "{method} {path}");
                    assert!(endpoint.version_checked, "{method} {path}");
                    if endpoint.modifies() {
                        assert!(endpoint.tracked, "{method} {path}");
                    }
                }
                Own if endpoint.modifies() => {
                    // Une route de tout rôle qui modifie n'agit que sur l'appelant : aucune autre cible
                    // que `/me` ou la session courante.
                    assert!(
                        path.starts_with("/me") || path == "/sessions/current",
                        "{method} {path} : une route de tout rôle qui modifie ne vise que l'appelant"
                    );
                }
                Admin(verdict) => {
                    assert!(
                        verdict.starts_with("PROTÉGÉE") || verdict.starts_with("sans danger"),
                        "{method} {path} : {verdict}"
                    );
                }
                _ => {}
            }
        }
        for (method, path, _) in CLASSES {
            assert!(
                ENDPOINTS
                    .iter()
                    .any(|e| e.method.as_str() == *method && e.path == *path),
                "{method} {path} : ligne de CLASSES sans route"
            );
        }
        // Les routes de niveau `Services` : la révélation des valeurs, la liste des candidats et les deux
        // actes d'adoption. Une route nouvelle de ce niveau change cette liste, donc se relit ici.
        let mut service_routes: Vec<_> = CLASSES
            .iter()
            .filter(|row| row.2 == Service)
            .map(|row| (row.0, row.1))
            .collect();
        service_routes.sort_unstable();
        assert_eq!(
            service_routes,
            [
                ("DELETE", "/operations/services/{id}"),
                ("GET", "/services/external"),
                ("GET", "/services/{key}/env"),
                ("POST", "/operations/services/{id}/decision"),
                ("POST", "/services/adopt"),
                ("POST", "/services/{key}/release"),
                ("POST", "/services/{key}/restart"),
                ("POST", "/services/{key}/start"),
                ("POST", "/services/{key}/stop"),
            ]
        );
        // Et les deux seules routes protégées de l'inventaire : le mot de passe d'autrui et la suppression.
        assert_eq!(
            CLASSES
                .iter()
                .filter(|row| matches!(row.2, Admin(verdict) if verdict.starts_with("PROTÉGÉE")))
                .count(),
            2
        );
    }

    #[test]
    fn only_the_two_sockets_authenticate_by_their_first_message_and_both_are_version_checked() {
        // Le flux commun et le socket d'une lecture de journaux (HRT-66) : deux WebSocket, deux lectures.
        let mut first_message: Vec<_> = ENDPOINTS
            .iter()
            .filter(|endpoint| endpoint.access == Access::FirstMessage)
            .map(|endpoint| endpoint.path)
            .collect();
        first_message.sort_unstable();
        assert_eq!(first_message, ["/services/{key}/logs", "/stream"]);
        for endpoint in ENDPOINTS
            .iter()
            .filter(|endpoint| endpoint.access == Access::FirstMessage)
        {
            assert_eq!(endpoint.method, Method::GET);
            assert!(endpoint.version_checked);
            assert!(!endpoint.tracked);
            // Une consultation : jamais d'entrée au journal d'activité (BR-SLOG-022).
            assert!(endpoint.audit.is_none() && !endpoint.modifies());
        }
    }

    /// Le test de garde de la liste des actes d'administration (HRT-28, BR-TRUST-037) : toute route
    /// non publique qui modifie est un acte de `hearth_proto::admin_act::ROUTES` ou une exception
    /// nommée de `NOT_AN_ACT`. Une route qui modifie ajoutée à `ENDPOINTS` sans être classée fait
    /// échouer ce test ; la couche de confirmation est posée d'après cette même table.
    #[test]
    fn every_modifying_route_is_an_admin_act_or_a_named_exception() {
        use hearth_proto::admin_act::{NOT_AN_ACT, ROUTES, is_exception, route_act};
        for endpoint in ENDPOINTS {
            let (method, path) = (endpoint.method.as_str(), endpoint.path);
            let act = route_act(method, path);
            let exception = is_exception(method, path);
            assert!(
                !(act.is_some() && exception),
                "{method} {path} est à la fois un acte et une exception"
            );
            if endpoint.access == Access::Public {
                assert!(
                    act.is_none() && !exception,
                    "{method} {path} : une route publique n'est ni un acte ni une exception"
                );
                continue;
            }
            if endpoint.modifies() {
                assert!(
                    act.is_some() || exception,
                    "{method} {path} modifie quelque chose sans être classée (acte d'administration ou exception nommée de hearth-proto)"
                );
            }
            if endpoint.access == Access::Admin && endpoint.modifies() {
                assert!(
                    act.is_some(),
                    "{method} {path} : une route d'administrateur qui modifie est un acte"
                );
            }
            // Un acte est suivi par clé d'opération et consigné.
            if let Some(act) = act {
                assert!(endpoint.tracked, "{method} {path}");
                let audit = endpoint.audit.expect("un acte a une action de journal");
                if !act.kinds.is_empty() {
                    assert!(
                        act.kinds
                            .iter()
                            .any(|kind| kind.audit_code() == audit.code()),
                        "{method} {path} : l'action de la route n'est celle d'aucun de ses actes"
                    );
                }
            }
        }
        // Chaque ligne de la table a sa route, et une route qui modifie.
        for route in ROUTES {
            assert!(
                ENDPOINTS
                    .iter()
                    .any(|endpoint| endpoint.method.as_str() == route.method
                        && endpoint.path == route.pattern
                        && endpoint.modifies()),
                "{} {} : ligne de ROUTES sans route",
                route.method,
                route.pattern
            );
        }
        for exception in NOT_AN_ACT {
            assert!(
                ENDPOINTS
                    .iter()
                    .any(|endpoint| endpoint.method.as_str() == exception.method
                        && endpoint.path == exception.pattern
                        && endpoint.modifies()),
                "{} {} : exception sans route",
                exception.method,
                exception.pattern
            );
        }
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
