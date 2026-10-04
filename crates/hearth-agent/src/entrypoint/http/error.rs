use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use hearth_proto::error::{ErrorBody, ErrorCode};

/// Erreur d'API : toujours rendue au format `ErrorBody`, avec le statut du code.
#[derive(Debug)]
pub struct ApiError(pub ErrorBody);

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self(ErrorBody::new(code, message))
    }
}

impl From<ErrorBody> for ApiError {
    fn from(body: ErrorBody) -> Self {
        Self(body)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.0.error.code.http_status())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(self.0)).into_response()
    }
}

/// Réponse des routes inconnues.
pub async fn not_found() -> ApiError {
    ApiError::new(ErrorCode::NotFound, "route inconnue")
}
