//! Le catalogue des fiches de serveurs de jeu : `GET /catalog` et `GET /catalog/{id}` (HRT-101, ADR-0050,
//! BR-FICHE-001 à 004).
//!
//! Deux consultations ouvertes à tout compte authentifié, sans effet et sans journal. Une fiche que cet agent ne
//! propose pas (inconnue, ou `preview` sur une construction de publication) répond `404 FICHE_UNKNOWN`, la même
//! réponse dans les deux cas. Rien de secret : une fiche n'en contient pas.

use axum::Json;
use axum::body::Body;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use hearth_proto::api::catalog::{CatalogView, FicheDetail};
use hearth_proto::api::worlds::{WorldInspection, WorldRef, WorldsView};
use hearth_proto::error::ErrorCode;

use super::{ApiError, AppState};
use crate::application::catalog::CatalogError;
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

/// `GET /api/v1/catalog/{id}/illustration` (HRT-238, ADR-0061, BR-FICHE-062) : les octets de l'illustration embarquée d'une
/// fiche LISTÉE. Ouverte à tout compte authentifié comme `/catalog`, sans journal. L'identifiant est validé avant toute
/// recherche (jamais répété) ; les octets sont ceux du binaire (aucune lecture de fichier) ; type fixe, `nosniff`,
/// `Content-Security-Policy: default-src 'none'`, `ETag` = empreinte, `304` si le client l'a déjà.
pub async fn illustration(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let unknown = || {
        ApiError::new(
            ErrorCode::FicheUnknown,
            "Cette fiche n'est pas listée par ce serveur.",
        )
    };
    if !is_stable_id(&id) {
        return Err(unknown());
    }
    let found = state
        .catalog
        .illustration(&id)
        .map_err(|error| match error {
            CatalogError::Unknown => unknown(),
            CatalogError::IllustrationAbsent => ApiError::new(
                ErrorCode::IllustrationAbsent,
                "Cette fiche n'a pas d'illustration.",
            ),
        })?;
    let etag = format!("\"sha256:{}\"", found.digest_hex);
    let mut out = Response::builder()
        .header(header::ETAG, &etag)
        .header(
            header::CACHE_CONTROL,
            "private, max-age=31536000, immutable",
        )
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::CONTENT_SECURITY_POLICY, "default-src 'none'");
    let not_modified = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.split(',').any(|candidate| candidate.trim() == etag));
    if not_modified {
        return out
            .status(StatusCode::NOT_MODIFIED)
            .body(Body::empty())
            .map_err(|_| unknown());
    }
    out = out
        .header(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"))
        .header(header::CONTENT_LENGTH, found.bytes.len());
    out.body(Body::from(found.bytes))
        .map(IntoResponse::into_response)
        .map_err(|_| unknown())
}

/// Un identifiant stable de fiche : minuscules, chiffres, tiret et souligné, 64 caractères au plus (même règle que
/// `hearth_link::is_stable_id`, que l'agent ne peut pas importer).
fn is_stable_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
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
