//! Le catalogue des fiches de serveurs de jeu : `GET /catalog` et `GET /catalog/{id}` (HRT-101, ADR-0050,
//! BR-FICHE-001 à 004).
//!
//! Deux consultations ouvertes à tout compte authentifié, sans effet et sans journal. Une fiche que cet agent ne
//! propose pas (inconnue, ou `preview` sur une construction de publication) répond `404 FICHE_UNKNOWN`, la même
//! réponse dans les deux cas. Rien de secret : une fiche n'en contient pas.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use hearth_proto::api::catalog::{CatalogView, FicheDetail};
use hearth_proto::api::worlds::{WorldInspection, WorldRef, WorldsView};
use hearth_proto::error::ErrorCode;

use super::{ApiError, AppState};
use crate::application::world_import::WorldsError;

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

/// Une consultation de monde refusée : un code stable, jamais un chemin ni un nom de monde (`BR-MONDE-023`).
fn worlds_error(error: WorldsError) -> ApiError {
    match error {
        WorldsError::FicheUnknown => ApiError::new(
            ErrorCode::FicheUnknown,
            "Cette fiche n'est pas proposée par ce serveur.",
        ),
        other => {
            ApiError::invalid_because("world", other.reason(), "Ce monde ne peut pas être repris.")
        }
    }
}

/// `GET /api/v1/catalog/{id}/worlds` : les mondes que la fiche reconnaît dans les dossiers ouverts aux services
/// (HRT-105, `BR-MONDE-003`). Droit « gérer les services » (`BR-MONDE-024`) ; ni diffusée ni journalisée.
pub async fn worlds(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<WorldsView>, ApiError> {
    state
        .world_import
        .list(&id)
        .await
        .map(Json)
        .map_err(worlds_error)
}

/// `POST /api/v1/catalog/{id}/worlds/inspect` : ce que la reprise du monde choisi copierait et laisserait
/// (`BR-MONDE-006`, `BR-MONDE-012`). Une consultation : rien n'est écrit.
pub async fn inspect_world(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Result<Json<WorldRef>, JsonRejection>,
) -> Result<Json<WorldInspection>, ApiError> {
    let Json(pick) = body.map_err(|_| {
        ApiError::new(
            ErrorCode::ValidationError,
            "Le corps de la demande n'est pas valable.",
        )
    })?;
    state
        .world_import
        .inspect(&id, &pick)
        .await
        .map(Json)
        .map_err(worlds_error)
}
