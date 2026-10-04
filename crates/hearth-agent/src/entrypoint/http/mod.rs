//! API HTTPS `/api/v1`.

mod error;
mod hello;
mod server;

use std::sync::Arc;

use axum::Router;
use axum::routing::get;

use crate::application::hello::HelloService;

pub use error::ApiError;
pub use server::{ServerError, ServerHandle, spawn};

/// État partagé des routes : les cas d'usage, jamais d'infrastructure directe.
#[derive(Clone)]
pub struct AppState {
    pub hello: Arc<HelloService>,
}

/// Routeur complet : routes sous `/api/v1`, 404 au format `ErrorBody` partout ailleurs.
pub fn router(state: AppState) -> Router {
    let v1 = Router::new().route("/hello", get(hello::hello));
    Router::new()
        .nest("/api/v1", v1)
        .fallback(error::not_found)
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
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
        let hello = HelloService::new(InstallId::from_bytes([3; 16]), false, Arc::new(Fake));
        router(AppState {
            hello: Arc::new(hello),
        })
    }

    async fn get_json<T: serde::de::DeserializeOwned>(path: &str) -> (StatusCode, T) {
        let request = Request::builder()
            .uri(path)
            .body(Body::empty())
            .expect("request");
        let response = app().oneshot(request).await.expect("réponse");
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .expect("body");
        (status, serde_json::from_slice(&bytes).expect("json"))
    }

    #[tokio::test]
    async fn hello_answers_with_the_agent_identity() {
        let (status, hello): (_, HelloResponse) = get_json("/api/v1/hello").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(hello.product, "hearth");
        assert_eq!(hello.machine_name, "forge");
    }

    #[tokio::test]
    async fn unknown_routes_answer_404_in_the_error_format() {
        for path in ["/", "/api/v1/nope", "/hello"] {
            let (status, body): (_, ErrorBody) = get_json(path).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
            assert_eq!(body.error.code, ErrorCode::NotFound);
        }
    }
}
