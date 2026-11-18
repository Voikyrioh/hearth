//! Les mods d'un serveur de jeu installé (HRT-256, ADR-0062, BR-MOD-020 à 033) : `GET /services/{key}/mods`.
//!
//! Une consultation ouverte à tout compte authentifié (BR-MOD-031), sans effet ni journal : la route ne modifie
//! rien. Aucune erreur ne répète une clé, un nom de fichier, un chemin ni une valeur lue dans un jar.

use axum::Json;
use axum::extract::{Path, State};
use hearth_proto::api::mods::ModsView;
use hearth_proto::error::ErrorCode;

use super::{ApiError, AppState};
use crate::application::mods::ModsError;
use crate::domain::services::ServiceKey;

fn mods_error(error: ModsError) -> ApiError {
    match error {
        ModsError::NotAGameService => ApiError::new(
            ErrorCode::NotAGameService,
            "Ce service n'est pas un serveur de jeu créé depuis une fiche : il n'a pas de mods.",
        ),
        ModsError::FolderBlocked => ApiError::new(
            ErrorCode::ModsFolderBlocked,
            "Le dossier des mods est un lien ou n'est pas un dossier : Hearth ne le lit pas.",
        ),
        ModsError::Storage(message) => {
            tracing::error!(error = %message, "liste des mods en échec");
            ApiError::new(ErrorCode::InternalError, "Une erreur interne est survenue.")
        }
    }
}

/// `GET /api/v1/services/{key}/mods` : la liste des mods, leur origine, leur état et leur verdict.
pub async fn list(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<ModsView>, ApiError> {
    let key = ServiceKey::parse(&key)
        .ok_or_else(|| ApiError::new(ErrorCode::ServiceNotFound, "Ce service n'existe pas."))?;
    state.mods.list(&key).await.map(Json).map_err(mods_error)
}
