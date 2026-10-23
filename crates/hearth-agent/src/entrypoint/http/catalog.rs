//! Le catalogue des fiches de serveurs de jeu : `GET /catalog` et `GET /catalog/{id}` (HRT-101, ADR-0050,
//! BR-FICHE-001 à 004).
//!
//! Deux consultations ouvertes à tout compte authentifié, sans effet et sans journal. Une fiche que cet agent ne
//! propose pas (inconnue, ou `preview` sur une construction de publication) répond `404 FICHE_UNKNOWN`, la même
//! réponse dans les deux cas. Rien de secret : une fiche n'en contient pas.

use axum::Json;
use axum::extract::{Path, State};
use hearth_proto::api::catalog::{CatalogView, FicheDetail};
use hearth_proto::error::ErrorCode;

use super::{ApiError, AppState};

/// `GET /api/v1/catalog`.
pub async fn list(State(state): State<AppState>) -> Json<CatalogView> {
    Json(state.catalog.list())
}

/// `GET /api/v1/catalog/{id}`.
pub async fn detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<FicheDetail>, ApiError> {
    state.catalog.detail(&id).map(Json).map_err(|_| {
        ApiError::new(
            ErrorCode::FicheUnknown,
            "Cette fiche n'est pas proposée par ce serveur.",
        )
    })
}
