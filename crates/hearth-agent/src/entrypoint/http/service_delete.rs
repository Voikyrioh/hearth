//! Supprimer un service : `GET /services/{key}/delete-preview` et `DELETE /services/{key}` (HRT-85, ADR-0045,
//! ADR-0046, BR-DEL-*).
//!
//! - **`DELETE /services/{key}`** est un acte d'administration confirmé (`ServiceDelete` `0x14`, JAMAIS couvert
//!   par l'élévation : le mot de passe à chaque fois, même sans case cochée, `BR-DEL-012`), réservé aux comptes
//!   qui ont le droit « gérer les services » (niveau `Services`). Il répond `202 { operation_id }` : la
//!   suppression court en arrière-plan, son issue se lit dans l'opération (`GET /operations/services/{id}`) et
//!   l'effacement des données se suit dans la bande après la fermeture de la fenêtre (`BR-DEL-031`). Les refus
//!   connus d'avance sont synchrones et typés : `SERVICE_ADOPTED` (journalisé, `BR-DEL-002`), `SERVICE_NOT_FOUND`,
//!   `SERVICE_BUSY`, `SERVICE_NOT_DRIVABLE`, `ENGINE_UNAVAILABLE`, `VALIDATION_ERROR` (nom retapé faux, case
//!   « sauvegardes » cochée alors que Hearth n'en fait pas encore).
//! - **`GET /services/{key}/delete-preview`** est une consultation du même droit : les dossiers gérés par Hearth,
//!   leur taille mesurée en 10 secondes au plus, leur sort. Elle n'écrit rien et n'est pas journalisée.
//!
//! Aucun message d'erreur ne répète une valeur reçue ni un chemin : les textes sont fixes.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Extension, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use hearth_proto::api::service_delete::{DeleteAccepted, DeletePreview, DeleteRequest};
use hearth_proto::error::{ErrorBody, ErrorCode};

use super::auth::Requester;
use super::error::TargetMark;
use super::services::operation_key;
use super::{ApiError, AppState};
use crate::application::delete::{DeleteError, DeleteFailed};
use crate::application::sessions::Reauthenticated;
use crate::domain::audit::{ClientName, Target};
use crate::domain::services::{ServiceKey, ServiceStateKind};
use crate::entrypoint::services_wire;

/// Le refus d'une suppression, avec le service visé quand il est publié (le journal d'activité l'écrit par son nom).
pub struct DeleteFailure(ApiError, Option<Target>);

impl IntoResponse for DeleteFailure {
    fn into_response(self) -> Response {
        let mut response = self.0.into_response();
        if let Some(target) = self.1 {
            response.extensions_mut().insert(TargetMark(target));
        }
        response
    }
}

fn failure(failed: DeleteFailed) -> DeleteFailure {
    let target = failed
        .name
        .as_deref()
        .and_then(ClientName::parse)
        .map(Target::Service);
    DeleteFailure(delete_error(failed.error), target)
}

fn delete_error(error: DeleteError) -> ApiError {
    match error {
        DeleteError::EngineUnavailable(engine) => ApiError(ErrorBody::with_details(
            ErrorCode::EngineUnavailable,
            "Le moteur de conteneurs n'est pas prêt. Le service n'a pas été supprimé.",
            serde_json::json!({ "engine": services_wire::engine_state(engine) }),
        )),
        DeleteError::NotFound => {
            ApiError::new(ErrorCode::ServiceNotFound, "Ce service n'existe plus.")
        }
        DeleteError::Adopted => ApiError::new(
            ErrorCode::ServiceAdopted,
            "Hearth ne supprime pas les services créés hors de Hearth. Tu peux seulement ne plus le gérer.",
        ),
        DeleteError::Busy { by, kind } => ApiError(ErrorBody::with_details(
            ErrorCode::ServiceBusy,
            "Une action est déjà en cours sur ce service.",
            serde_json::json!({ "by": by, "kind": kind.to_wire() }),
        )),
        DeleteError::NotDeletable(state) => ApiError(ErrorBody::with_details(
            ErrorCode::ServiceNotDrivable,
            match state {
                ServiceStateKind::Deploying => {
                    "Un déploiement est en cours pour ce service. Annule-le d'abord."
                }
                ServiceStateKind::Paused | ServiceStateKind::Unknown => {
                    "Hearth ne pilote pas ce service dans son état actuel."
                }
                _ => "Une action est déjà en cours sur ce service.",
            },
            serde_json::json!({ "state": services_wire::service_state(state) }),
        )),
        DeleteError::NameMismatch => ApiError::invalid_because(
            "confirm_name",
            "mismatch",
            "Le nom saisi ne correspond pas.",
        ),
        DeleteError::BackupsUnavailable => ApiError::invalid_because(
            "erase_backups",
            "not_available",
            "Ce serveur ne fait pas encore de sauvegardes : il n'y en a aucune à supprimer.",
        ),
        DeleteError::Internal(message) => {
            tracing::error!(error = %message, "suppression d'un service refusée : erreur interne");
            ApiError::new(ErrorCode::InternalError, "Une erreur interne est survenue.")
        }
    }
}

/// Une clé mal formée est un service inconnu, comme pour toute route de service.
fn unknown() -> DeleteFailure {
    failure(DeleteFailed::not_found())
}

/// `GET /api/v1/services/{key}/delete-preview`.
pub async fn preview(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<DeletePreview>, DeleteFailure> {
    let key = ServiceKey::parse(&key).ok_or_else(unknown)?;
    state
        .service_deletion
        .preview(&key)
        .await
        .map(Json)
        .map_err(failure)
}

/// `DELETE /api/v1/services/{key}` (acte `ServiceDelete` `0x14`).
pub async fn delete(
    State(state): State<AppState>,
    _confirmed: Extension<Reauthenticated>,
    Requester(actor): Requester,
    headers: HeaderMap,
    Path(key): Path<String>,
    body: Result<Json<DeleteRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<DeleteAccepted>), DeleteFailure> {
    let key = ServiceKey::parse(&key).ok_or_else(unknown)?;
    let Json(request) = body.map_err(|rejection| DeleteFailure(rejection.into(), None))?;
    let accepted = state
        .service_deletion
        .delete(&key, request, actor, &operation_key(&headers))
        .await
        .map_err(failure)?;
    Ok((StatusCode::ACCEPTED, Json(accepted)))
}
