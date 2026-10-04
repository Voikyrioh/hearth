use axum::Json;
use axum::extract::State;
use hearth_proto::api::hello::HelloResponse;

use super::AppState;

/// `GET /api/v1/hello` : identité publique de l'agent, sans authentification.
pub async fn hello(State(state): State<AppState>) -> Json<HelloResponse> {
    Json(state.hello.describe())
}
