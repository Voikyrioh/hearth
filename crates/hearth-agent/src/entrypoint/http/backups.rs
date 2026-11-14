//! Sauvegarder les données d'un service (HRT-97, ADR-0047, BR-BACKUP-*).
//!
//! - `GET /services/{key}/backups` : la liste d'un service créé par Hearth, visible de tout compte
//!   authentifié (`BR-BACKUP-001`) ; une consultation, sans journal.
//! - `POST /services/{key}/backups/preview` : l'estimation de la fenêtre de création, du droit « gérer les
//!   services » ; une consultation (`READ_ONLY_POSTS`), sans journal.
//! - `POST /services/{key}/backups` (acte `BackupCreate` `0x19`, couvert par l'élévation) : `202
//!   { operation_id, backup_id }` ; la copie court en arrière-plan, son issue se lit dans l'opération.
//! - `POST /backups/{id}/verify` : la vérification à la demande (`READ_ONLY_POSTS` : elle n'écrit que l'état
//!   d'intégrité), sans journal.
//! - `DELETE /backups/{id}` (acte `BackupDelete` `0x1B`, jamais couvert par l'élévation) : une sauvegarde de
//!   Hearth, jamais un autre fichier.
//! - `PUT /services/{key}/backup-settings` (acte `BackupSettings` `0x1C`) : les éléments ignorés.
//!
//! Aucun message d'erreur ne répète une valeur reçue ni un chemin du serveur : les textes sont fixes.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Extension, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use hearth_proto::api::backups::{
    BackupAccepted, BackupDeleted, BackupPreview, BackupRequest, BackupSettings, BackupsView,
    VerifyResult,
};
use hearth_proto::error::{ErrorBody, ErrorCode};

use super::auth::Requester;
use super::error::TargetMark;
use super::services::operation_key;
use super::{ApiError, AppState};
use crate::application::backups::{BackupError, BackupFailed};
use crate::application::sessions::Reauthenticated;
use crate::domain::audit::{ClientName, Target};
use crate::domain::backups::BackupId;
use crate::domain::services::{ServiceKey, ServiceStateKind};
use crate::entrypoint::services_wire;

/// Le refus d'un geste de sauvegarde, avec le service visé quand il est publié.
pub struct BackupFailure(ApiError, Option<Target>);

impl IntoResponse for BackupFailure {
    fn into_response(self) -> Response {
        let mut response = self.0.into_response();
        if let Some(target) = self.1 {
            response.extensions_mut().insert(TargetMark(target));
        }
        response
    }
}

fn failure(failed: BackupFailed) -> BackupFailure {
    let target = failed
        .name
        .as_deref()
        .and_then(ClientName::parse)
        .map(Target::Service);
    BackupFailure(backup_error(failed.error), target)
}

fn backup_error(error: BackupError) -> ApiError {
    match error {
        BackupError::EngineUnavailable(engine) => ApiError(ErrorBody::with_details(
            ErrorCode::EngineUnavailable,
            "Le moteur de conteneurs n'est pas prêt.",
            serde_json::json!({ "engine": services_wire::engine_state(engine) }),
        )),
        BackupError::NotFound => {
            ApiError::new(ErrorCode::ServiceNotFound, "Ce service n'existe plus.")
        }
        BackupError::Adopted => ApiError::new(
            ErrorCode::ServiceAdopted,
            "Les sauvegardes d'un service créé hors de Hearth ne sont pas encore prises en charge.",
        ),
        BackupError::Busy { by, kind } => ApiError(ErrorBody::with_details(
            ErrorCode::ServiceBusy,
            "Une action est déjà en cours sur ce service.",
            serde_json::json!({ "by": by, "kind": kind.to_wire() }),
        )),
        BackupError::HeavyBusy { service } => ApiError(ErrorBody::with_details(
            ErrorCode::HeavyIoBusy,
            "Une sauvegarde ou une restauration est déjà en cours sur ce serveur. Attends sa fin.",
            serde_json::json!({ "service": service }),
        )),
        BackupError::NotBackable(state) => ApiError(ErrorBody::with_details(
            ErrorCode::ServiceNotDrivable,
            match state {
                ServiceStateKind::Deploying => {
                    "Un déploiement est en cours pour ce service. Attends sa fin."
                }
                ServiceStateKind::Paused | ServiceStateKind::Unknown => {
                    "Hearth ne sauvegarde pas ce service dans son état actuel."
                }
                _ => "Une action est déjà en cours sur ce service.",
            },
            serde_json::json!({ "state": services_wire::service_state(state) }),
        )),
        BackupError::ModeRefused => ApiError::invalid_because(
            "mode",
            "not_allowed",
            "Ce mode de sauvegarde n'est pas permis pour ce service.",
        ),
        BackupError::Invalid { field, reason } => {
            ApiError::invalid_because(field, reason, "La demande n'est pas valable.")
        }
        BackupError::NothingToSave => ApiError::invalid_because(
            "service",
            "nothing_to_save",
            "Rien à sauvegarder : ce service n'a aucun dossier de données.",
        ),
        BackupError::NoSpace {
            needed,
            free,
            reserve,
        } => ApiError(ErrorBody::with_details(
            ErrorCode::NoSpace,
            "Il n'y a pas assez de place sur le disque des sauvegardes.",
            serde_json::json!({
                "needed_bytes": needed,
                "free_bytes": free,
                "reserve_bytes": reserve,
                "missing_bytes": needed.saturating_sub(free.saturating_sub(reserve)),
                "free_after_bytes": free.saturating_sub(needed),
            }),
        )),
        BackupError::RootOverlap => ApiError(ErrorBody::with_details(
            ErrorCode::PathRefused,
            "La racine des sauvegardes chevauche un dossier de données.",
            serde_json::json!({ "reason": "backups_root_overlap" }),
        )),
        BackupError::Unavailable => ApiError(ErrorBody::with_details(
            ErrorCode::PathRefused,
            "Les sauvegardes ne sont pas disponibles sur ce serveur.",
            serde_json::json!({ "reason": "backups_root_unavailable" }),
        )),
        BackupError::BackupNotFound => {
            ApiError::new(ErrorCode::BackupNotFound, "Cette sauvegarde n'existe pas.")
        }
        BackupError::InUse => ApiError::new(
            ErrorCode::BackupInUse,
            "Cette sauvegarde est en cours d'utilisation. Attends la fin de l'opération.",
        ),
        BackupError::KeyUnavailable => ApiError::invalid_because(
            "note",
            "key_unavailable",
            "La clé des secrets de ce serveur est illisible : la note ne peut pas être enregistrée.",
        ),
        BackupError::Internal(message) => {
            tracing::error!(error = %message, "geste de sauvegarde refusé : erreur interne");
            ApiError::new(ErrorCode::InternalError, "Une erreur interne est survenue.")
        }
    }
}

/// Une clé mal formée est un service inconnu, comme pour toute route de service.
fn unknown_service() -> BackupFailure {
    failure(BackupFailed::not_found())
}

/// Un identifiant mal formé est une sauvegarde inconnue : jamais un nom de fichier.
fn unknown_backup() -> BackupFailure {
    BackupFailure(backup_error(BackupError::BackupNotFound), None)
}

/// `GET /api/v1/services/{key}/backups`.
pub async fn list(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<BackupsView>, BackupFailure> {
    let key = ServiceKey::parse(&key).ok_or_else(unknown_service)?;
    state.backups.list(&key).await.map(Json).map_err(failure)
}

/// `POST /api/v1/services/{key}/backups/preview`.
pub async fn preview(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<BackupPreview>, BackupFailure> {
    let key = ServiceKey::parse(&key).ok_or_else(unknown_service)?;
    state.backups.preview(&key).await.map(Json).map_err(failure)
}

/// `POST /api/v1/services/{key}/backups` (acte `BackupCreate` `0x19`).
pub async fn create(
    State(state): State<AppState>,
    _confirmed: Extension<Reauthenticated>,
    Requester(actor): Requester,
    headers: HeaderMap,
    Path(key): Path<String>,
    body: Result<Json<BackupRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<BackupAccepted>), BackupFailure> {
    let key = ServiceKey::parse(&key).ok_or_else(unknown_service)?;
    let Json(request) = body.map_err(|rejection| BackupFailure(rejection.into(), None))?;
    let accepted = state
        .backups
        .create(&key, request, actor, &operation_key(&headers))
        .await
        .map_err(failure)?;
    Ok((StatusCode::ACCEPTED, Json(accepted)))
}

/// `POST /api/v1/backups/{id}/verify`.
pub async fn verify(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<VerifyResult>, BackupFailure> {
    let id = BackupId::parse(&id).ok_or_else(unknown_backup)?;
    state.backups.verify(&id).await.map(Json).map_err(failure)
}

/// `DELETE /api/v1/backups/{id}` (acte `BackupDelete` `0x1B`).
pub async fn delete(
    State(state): State<AppState>,
    _confirmed: Extension<Reauthenticated>,
    Requester(actor): Requester,
    Path(id): Path<String>,
) -> Result<Json<BackupDeleted>, BackupFailure> {
    let id = BackupId::parse(&id).ok_or_else(unknown_backup)?;
    state
        .backups
        .delete(&id, actor)
        .await
        .map(Json)
        .map_err(failure)
}

/// `PUT /api/v1/services/{key}/backup-settings` (acte `BackupSettings` `0x1C`).
pub async fn settings(
    State(state): State<AppState>,
    _confirmed: Extension<Reauthenticated>,
    Requester(actor): Requester,
    Path(key): Path<String>,
    body: Result<Json<BackupSettings>, JsonRejection>,
) -> Result<Json<BackupSettings>, BackupFailure> {
    let key = ServiceKey::parse(&key).ok_or_else(unknown_service)?;
    let Json(settings) = body.map_err(|rejection| BackupFailure(rejection.into(), None))?;
    state
        .backups
        .set_settings(&key, settings, actor)
        .await
        .map(Json)
        .map_err(failure)
}
