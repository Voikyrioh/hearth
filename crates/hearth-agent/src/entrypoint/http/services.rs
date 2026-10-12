//! `GET /services` et `POST /engine/recheck` : l'état du moteur et la liste des services, pour les
//! deux rôles, en lecture seule (HRT-61, BR-ENGINE-016, BR-ENGINE-020, BR-SVC-020, BR-SVC-024).
//!
//! Les deux sont des consultations : aucune entrée au journal. `POST /engine/recheck` est un `POST`
//! qui ne modifie rien (il relance une détection, sans effet sur le moteur) : il est dans
//! `READ_ONLY_POSTS`, ni acte d'administration ni exception nommée. Aucune route n'écrit dans le
//! moteur ni dans la base.

use axum::Json;
use axum::extract::State;
use hearth_proto::api::services::{EngineView, ServicesState};

use super::AppState;
use crate::entrypoint::services_wire;

/// `GET /api/v1/services` : le même contenu que `services_snapshot` du flux (repli, tests).
pub async fn list(State(state): State<AppState>) -> Json<ServicesState> {
    Json(services_wire::snapshot(&state.services.snapshot()))
}

/// `POST /api/v1/engine/recheck` : relance la détection du moteur tout de suite et rend son état.
pub async fn recheck(State(state): State<AppState>) -> Json<EngineView> {
    let status = state.services.recheck().await;
    Json(services_wire::engine_view(&status, state.services.host()))
}
