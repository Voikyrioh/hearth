//! Client de test du routeur `/api/v1` en processus (sans TLS) : requêtes de bout en bout sur
//! une vraie base, avec adresse du client et version d'interface par défaut.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::sync::Arc;

use super::fake_engine::FakeEngine;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Method, Request, StatusCode};
use hearth_agent::application::hello::HelloService;
use hearth_agent::application::metrics::MetricsService;
use hearth_agent::application::ports::MachineInfo;
use hearth_agent::application::services::{EngineTimings, ServicesRegistry};
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::install_id::InstallId;
use hearth_agent::entrypoint::http::{AppState, router};
use hearth_agent::entrypoint::ws::{StreamContext, StreamSettings};
use hearth_agent::infrastructure::clock::SystemMonotonic;
use serde_json::Value;
use tower::ServiceExt;

use super::probe::{FakeGpu, FakeSystem};
use super::{Env, PASSWORD};

struct FakeMachine;

impl MachineInfo for FakeMachine {
    fn machine_name(&self) -> String {
        "forge".into()
    }

    fn mac_addresses(&self) -> Vec<String> {
        Vec::new()
    }
}

pub fn state(env: &Env) -> AppState {
    state_with(env, super::update::Rig::new(env, false, false).service)
}

/// Comme `state`, avec ce service de mise à jour.
pub fn state_with(
    env: &Env,
    update: Arc<hearth_agent::application::update::UpdateService>,
) -> AppState {
    // Le pilotage des services désactivé : ces tests n'ouvrent jamais le socket d'un moteur.
    state_over(env, update, Arc::new(FakeEngine::new()), false)
}

/// Comme `state_with`, avec ce faux moteur ET le pilotage des services permis : le registre sonde ce moteur
/// et lit ses conteneurs avant de rendre la main (le moteur est « prêt »), de quoi tester les routes
/// d'adoption sans démarrer l'agent.
pub async fn state_with_engine(
    env: &Env,
    update: Arc<hearth_agent::application::update::UpdateService>,
    engine: Arc<FakeEngine>,
) -> AppState {
    let state = state_over(env, update, engine, true);
    state.services.recheck().await;
    // Les adoptions déjà écrites en base sont publiées comme au démarrage de l'agent.
    state.services.reload_adopted().await;
    state.services.resync().await.expect("liste des conteneurs");
    state
}

/// Ce que les tests du déploiement règlent : les sockets du serveur, l'espace libre, les dossiers de données
/// (en mémoire) et les délais (HRT-68).
pub struct DeployRig {
    pub host_ports: Arc<super::fake_host::FakeHostPorts>,
    pub free_space: Arc<super::fake_host::FakeFreeSpace>,
    pub safe_fs: Arc<super::fake_safe_fs::FakeSafeFs>,
    pub timings: hearth_agent::application::deployments::DeployTimings,
}

impl DeployRig {
    pub fn new() -> Self {
        Self {
            host_ports: Arc::new(super::fake_host::FakeHostPorts::new()),
            free_space: Arc::new(super::fake_host::FakeFreeSpace::new()),
            safe_fs: Arc::new(super::fake_safe_fs::FakeSafeFs::new()),
            // La surveillance de 10 s devient 300 ms.
            timings: hearth_agent::application::deployments::DeployTimings {
                deadline: std::time::Duration::from_secs(60),
                watch: std::time::Duration::from_millis(300),
                poll: std::time::Duration::from_millis(20),
            },
        }
    }
}

impl Default for DeployRig {
    fn default() -> Self {
        Self::new()
    }
}

/// Comme `state_with_engine`, avec ce décor de déploiement.
pub async fn state_with_deploy(
    env: &Env,
    update: Arc<hearth_agent::application::update::UpdateService>,
    engine: Arc<FakeEngine>,
    rig: &DeployRig,
) -> AppState {
    let state = state_over_rig(env, update, engine, true, rig);
    state.services.recheck().await;
    state.services.reload_adopted().await;
    state.services.resync().await.expect("liste des conteneurs");
    state
}

fn state_over(
    env: &Env,
    update: Arc<hearth_agent::application::update::UpdateService>,
    engine: Arc<FakeEngine>,
    engine_enabled: bool,
) -> AppState {
    state_over_rig(env, update, engine, engine_enabled, &DeployRig::new())
}

fn state_over_rig(
    env: &Env,
    update: Arc<hearth_agent::application::update::UpdateService>,
    engine: Arc<FakeEngine>,
    engine_enabled: bool,
    rig: &DeployRig,
) -> AppState {
    // Avec la VRAIE base des adoptions et des arrêts voulus : `reload_adopted` lit ce que `Env::adopt` écrit, et
    // piloter un service (HRT-64) demande un service publié.
    let services = ServicesRegistry::with_supervision(
        engine.clone(),
        Arc::new(hearth_agent::infrastructure::supervisor::NoSupervisor),
        Arc::new(
            hearth_agent::infrastructure::sqlite::SqliteServiceRepo::new(env.db.pool().clone()),
        ),
        &super::engine::TestHost,
        env.clock.clone(),
        Arc::new(SystemMonotonic::new()),
        engine_enabled,
        EngineTimings::production(),
    );
    let ops = Arc::new(hearth_agent::application::service_ops::ServiceOps::new(
        Arc::new(hearth_agent::infrastructure::sqlite::SqliteOpJournal::new(
            env.db.pool().clone(),
        )),
        Arc::new(hearth_agent::application::service_ops::OpLocks::new()),
        env.clock.clone(),
        Arc::new(hearth_agent::infrastructure::ids::UlidGen),
        Arc::new(hearth_agent::infrastructure::created_objects::FsCreatedObjects::new([])),
    ));
    // Les conteneurs que l'agent crée sont consignés dans la vraie base (HRT-68).
    let repo = Arc::new(
        hearth_agent::infrastructure::sqlite::SqliteServiceRepo::new(env.db.pool().clone()),
    );
    services.set_containers(repo.clone());
    let spec_repo = Arc::new(hearth_agent::infrastructure::sqlite::SqliteSpecRepo::new(
        env.db.pool().clone(),
    ));
    let specs = Arc::new(hearth_agent::application::service_specs::ServiceSpecs::new(
        spec_repo.clone(),
        Arc::new(hearth_agent::application::secret_vault::SecretVault::new(
            Arc::new(
                hearth_agent::infrastructure::services_key::FileServicesKeyStore::new(
                    env.dir.path(),
                ),
            ),
            spec_repo,
            Arc::new(hearth_agent::infrastructure::sealer::RingSealerFactory),
        )),
        env.clock.clone(),
    ));
    let deployments = hearth_agent::application::deployments::Deployments::new(
        services.clone(),
        ops.clone(),
        specs,
        engine.clone(),
        rig.host_ports.clone(),
        rig.free_space.clone(),
        rig.safe_fs.clone(),
        repo,
        env.audit_sink.clone(),
        Arc::new(hearth_agent::infrastructure::ids::UlidGen),
        env.clock.clone(),
        hearth_agent::application::deployments::DeployContext {
            install_id: "install-test".into(),
            agent_port: 7341,
            services_root: "/var/lib/hearth/services".into(),
            folders: None,
            timings: rig.timings,
        },
        Arc::new(hearth_agent::application::catalog::CatalogService::embedded(false)),
        Arc::new(super::fake_random::FakeRandom::counting_from(0)),
    );
    let service_lifecycle = hearth_agent::application::service_lifecycle::ServiceLifecycle::new(
        services.clone(),
        ops.clone(),
        engine.clone(),
        Arc::new(hearth_agent::infrastructure::supervisor::NoSupervisor),
        Arc::new(
            hearth_agent::infrastructure::sqlite::SqliteServiceRepo::new(env.db.pool().clone()),
        ),
        env.audit_sink.clone(),
        env.clock.clone(),
        super::engine::fast_lifecycle(),
    );
    let service_ops = Arc::new(
        hearth_agent::application::service_op_access::ServiceOpAccess::new(
            ops,
            services.clone(),
            Arc::new(hearth_agent::infrastructure::sqlite::SqliteStore::new(
                env.db.pool().clone(),
            )),
            env.clock.clone(),
            env.trail.clone(),
        ),
    );
    AppState {
        service_ops,
        service_lifecycle,
        deployments,
        catalog: Arc::new(hearth_agent::application::catalog::CatalogService::embedded(false)),
        hello: Arc::new(HelloService::new(
            InstallId::from_bytes([3; 16]),
            false,
            &FakeMachine,
        )),
        accounts: env.service.clone(),
        sessions: env.sessions.clone(),
        security: env.security.clone(),
        operations: env.operations.clone(),
        audit: env.audit.clone(),
        sink: env.audit_sink.clone(),
        metrics: Arc::new(MetricsService::new(
            Arc::new(FakeSystem::default()),
            Arc::new(FakeGpu),
            env.clock.clone(),
            Arc::new(SystemMonotonic::new()),
        )),
        update,
        service_adoption: Arc::new(
            hearth_agent::application::service_adoption::ServiceAdoption::new(
                services.clone(),
                Arc::new(hearth_agent::infrastructure::sqlite::SqliteStore::new(
                    env.db.pool().clone(),
                )),
                env.clock.clone(),
                env.trail.clone(),
                true,
            ),
        ),
        service_logs: Arc::new(hearth_agent::application::service_logs::ServiceLogs::new(
            services.clone(),
            engine,
            env.clock.clone(),
            hearth_agent::application::service_logs::LogSettings::default(),
        )),
        services,
        stream: StreamContext::new(StreamSettings::default()),
        erasure_pending: false,
    }
}

pub struct Reply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    /// Le corps lu comme JSON (`null` s'il est vide ou n'est pas du JSON : un export CSV).
    pub body: Value,
    /// Le corps tel quel.
    pub text: String,
}

impl Reply {
    pub fn code(&self) -> &str {
        self.body["error"]["code"].as_str().unwrap_or("")
    }
}

pub struct Api {
    router: Router,
    /// Adresse du client simulé.
    pub addr: SocketAddr,
}

/// Une requête en construction : méthode, chemin sous `/api/v1`, jeton, en-têtes, corps JSON.
pub struct Call<'a> {
    api: &'a Api,
    method: Method,
    path: String,
    token: Option<String>,
    headers: Vec<(String, String)>,
    body: Option<String>,
    version: Option<String>,
}

impl Api {
    pub fn new(env: &Env) -> Self {
        Self::from_state(state(env))
    }

    /// Le routeur de cet état : les tests de mise à jour y mettent leur service.
    pub fn from_state(state: AppState) -> Self {
        Self {
            router: router(state),
            addr: SocketAddr::from(([10, 0, 0, 7], 40_000)),
        }
    }

    pub fn call(&self, method: Method, path: &str) -> Call<'_> {
        Call {
            api: self,
            method,
            path: format!("/api/v1{path}"),
            token: None,
            headers: Vec::new(),
            body: None,
            version: Some("1".into()),
        }
    }

    pub fn get(&self, path: &str) -> Call<'_> {
        self.call(Method::GET, path)
    }

    pub fn post(&self, path: &str) -> Call<'_> {
        self.call(Method::POST, path)
    }

    pub fn put(&self, path: &str) -> Call<'_> {
        self.call(Method::PUT, path)
    }

    pub fn patch(&self, path: &str) -> Call<'_> {
        self.call(Method::PATCH, path)
    }

    pub fn delete(&self, path: &str) -> Call<'_> {
        self.call(Method::DELETE, path)
    }

    /// Ouvre une session par la route de connexion ; rend le jeton.
    pub async fn login(&self, username: &str, password: &str) -> Reply {
        self.post("/sessions")
            .json(&serde_json::json!({ "username": username, "password": password }))
            .send()
            .await
    }

    pub async fn token_of(&self, username: &str) -> String {
        let reply = self.login(username, PASSWORD).await;
        assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
        reply.body["token"].as_str().expect("jeton").to_owned()
    }
}

impl<'a> Call<'a> {
    pub fn token(mut self, token: &str) -> Self {
        self.token = Some(token.to_owned());
        self
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    pub fn key(self, key: &str) -> Self {
        self.header("idempotency-key", key)
    }

    pub fn json(mut self, body: &Value) -> Self {
        self.body = Some(body.to_string());
        self
    }

    pub fn raw_body(mut self, body: &str) -> Self {
        self.body = Some(body.to_owned());
        self
    }

    /// Version d'interface annoncée (`None` : en-tête absent).
    pub fn version(mut self, version: Option<&str>) -> Self {
        self.version = version.map(str::to_owned);
        self
    }

    pub async fn send(self) -> Reply {
        let mut builder = Request::builder().method(self.method).uri(self.path);
        if let Some(version) = &self.version {
            builder = builder.header("x-hearth-api", version);
        }
        if let Some(token) = &self.token {
            builder = builder.header("authorization", format!("Bearer {token}"));
        }
        if self.body.is_some() {
            builder = builder.header("content-type", "application/json");
        }
        for (name, value) in &self.headers {
            builder = builder.header(name, value);
        }
        let mut request = builder
            .body(Body::from(self.body.unwrap_or_default()))
            .expect("requête");
        request.extensions_mut().insert(ConnectInfo(self.api.addr));
        let response = self
            .api
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("réponse");
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .expect("corps");
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        Reply {
            status,
            headers,
            body,
            text: String::from_utf8_lossy(&bytes).into_owned(),
        }
    }
}

impl Env {
    /// Crée un compte et rend un jeton de session ouverte par la route de connexion.
    pub async fn account_with_token(&self, api: &Api, username: &str, role: Role) -> String {
        self.create(username, role).await;
        api.token_of(username).await
    }
}
