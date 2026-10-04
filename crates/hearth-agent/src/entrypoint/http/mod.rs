//! API HTTPS `/api/v1`.
//!
//! Convention : un handler lit l'état partagé (cas d'usage), puis convertit la structure
//! applicative en type du fil de `hearth-proto`. Toute erreur de routage, d'extraction ou de
//! méthode sort au format `ErrorBody` (voir `error.rs`).

mod error;
mod hello;
mod server;

use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use tower_http::LatencyUnit;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::application::hello::HelloService;

pub use error::ApiError;
pub use server::{ServerError, ServerHandle, spawn};

/// État partagé des routes : les cas d'usage, jamais d'infrastructure directe.
#[derive(Clone)]
pub struct AppState {
    pub hello: Arc<HelloService>,
}

/// Routeur complet : routes sous `/api/v1`, erreurs de routage au format `ErrorBody`,
/// une ligne de journal par requête servie (méthode, chemin, statut, durée).
pub fn router(state: AppState) -> Router {
    let v1 = Router::new().route("/hello", get(hello::hello));
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
    use axum::Json;
    use axum::body::Body;
    use axum::extract::rejection::JsonRejection;
    use axum::http::{Method, Request, StatusCode};
    use axum::routing::post;
    use hearth_proto::api::hello::HelloResponse;
    use hearth_proto::error::{ErrorBody, ErrorCode};
    use tower::ServiceExt;

    use super::*;
    use crate::application::ports::MachineInfo;
    use crate::domain::install_id::InstallId;

    struct Fake;
    impl MachineInfo for Fake {
        fn machine_name(&self) -> String {
            "forge".into()
        }
        fn mac_addresses(&self) -> Vec<String> {
            Vec::new()
        }
    }

    fn app() -> Router {
        let hello = HelloService::new(InstallId::from_bytes([3; 16]), false, &Fake);
        router(AppState {
            hello: Arc::new(hello),
        })
    }

    async fn send<T: serde::de::DeserializeOwned>(
        router: Router,
        method: Method,
        path: &str,
        body: &str,
    ) -> (StatusCode, T) {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json")
            .body(Body::from(body.to_owned()))
            .expect("request");
        let response = router.oneshot(request).await.expect("réponse");
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .expect("body");
        (status, serde_json::from_slice(&bytes).expect("json"))
    }

    #[tokio::test]
    async fn hello_answers_with_the_agent_identity() {
        let (status, hello): (_, HelloResponse) =
            send(app(), Method::GET, "/api/v1/hello", "").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(hello.product, "hearth");
        assert_eq!(hello.machine_name, "forge");
    }

    #[tokio::test]
    async fn unknown_routes_answer_404_in_the_error_format() {
        for path in ["/", "/api/v1/nope", "/hello"] {
            let (status, body): (_, ErrorBody) = send(app(), Method::GET, path, "").await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
            assert_eq!(body.error.code, ErrorCode::NotFound);
        }
    }

    #[tokio::test]
    async fn wrong_method_answers_405_in_the_error_format() {
        for method in [Method::POST, Method::PUT, Method::DELETE] {
            let (status, body): (_, ErrorBody) = send(app(), method, "/api/v1/hello", "{}").await;
            assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
            assert_eq!(body.error.code, ErrorCode::MethodNotAllowed);
        }
    }

    #[tokio::test]
    async fn extraction_errors_answer_in_the_error_format() {
        async fn echo(
            body: Result<Json<serde_json::Value>, JsonRejection>,
        ) -> Result<(), ApiError> {
            let _json = body?;
            Ok(())
        }
        let router = with_error_fallbacks(Router::new().route("/echo", post(echo)));
        let (status, body): (_, ErrorBody) =
            send(router, Method::POST, "/echo", "pas du json").await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body.error.code, ErrorCode::ValidationError);
    }
}
