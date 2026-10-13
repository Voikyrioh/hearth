//! `GET /services`, `POST /engine/recheck` et `GET /services/{key}/env` (HRT-61, HRT-63,
//! BR-ENGINE-016, BR-ENGINE-020, BR-SVC-009, BR-SVC-020, BR-SVC-024).
//!
//! `GET /services` et `POST /engine/recheck` sont des consultations ouvertes aux deux rôles : aucune
//! entrée au journal. `POST /engine/recheck` est un `POST` qui ne modifie rien (il relance une
//! détection, sans effet sur le moteur) : il est dans `READ_ONLY_POSTS`, ni acte d'administration ni
//! exception nommée. Aucune route n'écrit dans le moteur ni dans les services.
//!
//! `GET /services/{key}/env` révèle les VALEURS des variables d'un service : réservée aux comptes qui
//! ont le droit « gérer les services », quel que soit leur rôle (niveau `Services`, ADR-0041 : un
//! administrateur sans le droit est refusé, un compte en lecture seule qui l'a est servi ; la couche
//! d'accès refuse et consigne le refus), la réussite est consignée au journal par le cas d'usage sans nom ni valeur, la réponse n'est jamais
//! stockable. Aucune valeur n'entre dans une trace ni dans un message d'erreur.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::header::{CACHE_CONTROL, PRAGMA};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use hearth_proto::api::services::{EngineView, ServicesState};
use hearth_proto::error::ErrorCode;

use super::auth::Requester;
use super::{ApiError, AppState};
use crate::application::services::RevealError;
use crate::domain::services::ServiceKey;
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

/// `GET /api/v1/services/{key}/env` : les variables du service avec leurs valeurs (comptes qui ont le
/// droit « gérer les services » seulement, quel que soit le rôle ; BR-SVC-009, BR-GRANT-003). La clé du chemin n'est jamais répétée dans une réponse d'erreur.
pub async fn env(
    State(state): State<AppState>,
    Requester(actor): Requester,
    Path(key): Path<String>,
) -> Result<Response, ApiError> {
    let key = ServiceKey::parse(&key)
        .ok_or_else(|| ApiError::new(ErrorCode::NotFound, "Ce service n'existe pas."))?;
    let entries = state
        .services
        .reveal_env(&key, actor, state.sink.as_ref())
        .await
        .map_err(|error| match error {
            RevealError::NotFound => ApiError::new(ErrorCode::NotFound, "Ce service n'existe pas."),
            RevealError::NoContainer => ApiError::new(
                ErrorCode::NotFound,
                "Les variables de ce service ne sont pas lisibles : son conteneur n'existe pas.",
            ),
            RevealError::EngineUnavailable => ApiError::new(
                ErrorCode::Busy,
                "Le moteur de conteneurs ne répond pas. Réessaie dans un instant.",
            ),
            RevealError::JournalUnavailable => ApiError::new(
                ErrorCode::Busy,
                "La révélation n'a pas pu être consignée au journal : aucune valeur n'est rendue. Réessaie dans un instant.",
            ),
        })?;
    let mut response = (StatusCode::OK, Json(services_wire::env_reveal(&entries))).into_response();
    // Des valeurs secrètes : aucun cache, ni du client ni d'un intermédiaire.
    let headers = response.headers_mut();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(PRAGMA, HeaderValue::from_static("no-cache"));
    Ok(response)
}
