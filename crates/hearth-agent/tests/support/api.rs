//! Client de test du routeur `/api/v1` en processus (sans TLS) : requêtes de bout en bout sur
//! une vraie base, avec adresse du client et version d'interface par défaut.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Method, Request, StatusCode};
use hearth_agent::application::hello::HelloService;
use hearth_agent::application::metrics::MetricsService;
use hearth_agent::application::ports::MachineInfo;
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::install_id::InstallId;
use hearth_agent::entrypoint::http::{AppState, router};
use hearth_agent::entrypoint::ws::{StreamContext, StreamSettings};
use hearth_agent::infrastructure::audit_feed::NoAuditFeed;
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
    AppState {
        hello: Arc::new(HelloService::new(
            InstallId::from_bytes([3; 16]),
            false,
            &FakeMachine,
        )),
        accounts: env.service.clone(),
        sessions: env.sessions.clone(),
        operations: env.operations.clone(),
        metrics: Arc::new(MetricsService::new(
            Arc::new(FakeSystem::default()),
            Arc::new(FakeGpu),
            env.clock.clone(),
            Arc::new(SystemMonotonic::new()),
        )),
        stream: StreamContext::new(Arc::new(NoAuditFeed), StreamSettings::default()),
    }
}

pub struct Reply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Value,
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
        Self {
            router: router(state(env)),
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
            serde_json::from_slice(&bytes).expect("JSON")
        };
        Reply {
            status,
            headers,
            body,
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
