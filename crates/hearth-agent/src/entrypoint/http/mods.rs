//! Les mods d'un serveur de jeu installé (HRT-256, ADR-0062, BR-MOD-020 à 033) : `GET /services/{key}/mods`.
//!
//! Une consultation ouverte à tout compte authentifié (BR-MOD-031), sans effet ni journal : la route ne modifie
//! rien. Aucune erreur ne répète une clé, un nom de fichier, un chemin ni une valeur lue dans un jar.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Extension, Path, State};
use axum::http::{HeaderMap, StatusCode};
use hearth_proto::api::mods::{
    ModsApplyAccepted, ModsApplyRequest, ModsCheckRequest, ModsCheckView, ModsView,
};
use hearth_proto::error::{ErrorBody, ErrorCode};

use super::auth::Requester;
use super::backups::{BackupFailure, failure};
use super::services::operation_key;
use super::{ApiError, AppState};
use crate::application::backups::BackupError;
use crate::application::mods::{ApplyError, ModsError};
use crate::application::sessions::Reauthenticated;
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
        ModsError::BatchInvalid => ApiError::new(
            ErrorCode::ModsBatchInvalid,
            "Le contrôle porte sur 1 à 8 dépôts, chacun une seule fois.",
        ),
        ModsError::DepositUnknown => ApiError::new(
            ErrorCode::DepositUnknown,
            "Ce dépôt n'existe pas ou n'est plus dans la zone d'attente.",
        ),
        ModsError::NotApplicable => ApiError::new(
            ErrorCode::ModsNotApplicable,
            "Le moteur de ce serveur ne charge pas de mods.",
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

/// `POST /api/v1/services/{key}/mods/check` (HRT-257) : juge des `.jar` déposés par ce compte, sans rien écrire sur
/// le service. Un dépôt refusé est retiré de la zone d'attente aussitôt.
pub async fn check(
    State(state): State<AppState>,
    Requester(actor): Requester,
    Path(key): Path<String>,
    body: Result<Json<ModsCheckRequest>, JsonRejection>,
) -> Result<Json<ModsCheckView>, ApiError> {
    let key = ServiceKey::parse(&key)
        .ok_or_else(|| ApiError::new(ErrorCode::ServiceNotFound, "Ce service n'existe pas."))?;
    let Json(request) = body?;
    let account = actor
        .account
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_default();
    state
        .mods
        .check(&key, &account, &request.deposits)
        .await
        .map(Json)
        .map_err(mods_error)
}

fn refused(code: ErrorCode, message: &'static str) -> BackupFailure {
    BackupFailure(ApiError::new(code, message), None)
}

/// Le refus d'un lot : des textes FIXES, jamais un nom de fichier, un chemin ni une valeur lue dans un jar.
fn apply_failure(error: ApplyError) -> BackupFailure {
    match error {
        ApplyError::Mods(error) => BackupFailure(mods_error(error), None),
        ApplyError::Backup(failed) => match failed.error {
            // Une autre opération tient le service : le code propre aux mods (HRT-258).
            BackupError::Busy { by, kind } => BackupFailure(
                ApiError(ErrorBody::with_details(
                    ErrorCode::OperationInProgress,
                    "Une opération est déjà en cours sur ce serveur.",
                    serde_json::json!({ "by": by, "kind": kind.to_wire() }),
                )),
                None,
            ),
            other => failure(failed_with(other, failed.name)),
        },
        ApplyError::Batch => refused(
            ErrorCode::ModsBatchInvalid,
            "Le lot compte 1 à 64 changements, dont 8 ajouts au plus, chaque fichier une seule fois.",
        ),
        ApplyError::Refused { deposit, refusal } => BackupFailure(
            ApiError(ErrorBody::with_details(
                ErrorCode::ModRefused,
                "Un des fichiers déposés est refusé.",
                serde_json::json!({ "deposit": deposit, "refusal": refusal }),
            )),
            None,
        ),
        ApplyError::FileMissing => refused(
            ErrorCode::ModFileMissing,
            "Un des fichiers du lot n'est pas (ou plus) dans le bon dossier.",
        ),
        ApplyError::InModrinthPack => refused(
            ErrorCode::ModInModrinthPack,
            "Un mod du pack Modrinth ne se retire, ne se remplace et ne se désactive pas : il reviendrait au démarrage.",
        ),
        ApplyError::AlreadyActive => refused(
            ErrorCode::ModAlreadyActive,
            "Un autre fichier actif porte déjà ce mod.",
        ),
        ApplyError::NameTaken => refused(
            ErrorCode::ModNameTaken,
            "Le nom d'arrivée d'un des fichiers est déjà pris.",
        ),
    }
}

fn failed_with(
    error: BackupError,
    name: Option<String>,
) -> crate::application::backups::BackupFailed {
    crate::application::backups::BackupFailed { error, name }
}

/// `POST /api/v1/services/{key}/mods/apply` (HRT-258, acte `ModsApply` `0x20`, jamais couvert par l'élévation) :
/// `202 { operation_id, backup_id }`. Le client n'est jamais cru : l'agent rejuge tout.
pub async fn apply(
    State(state): State<AppState>,
    _confirmed: Extension<Reauthenticated>,
    Requester(actor): Requester,
    headers: HeaderMap,
    Path(key): Path<String>,
    body: Result<Json<ModsApplyRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<ModsApplyAccepted>), BackupFailure> {
    let key = ServiceKey::parse(&key)
        .ok_or_else(|| refused(ErrorCode::ServiceNotFound, "Ce service n'existe pas."))?;
    let Json(request) = body.map_err(|rejection| BackupFailure(rejection.into(), None))?;
    let accepted = state
        .mods
        .apply(&key, actor, request, &operation_key(&headers))
        .await
        .map_err(apply_failure)?;
    Ok((StatusCode::ACCEPTED, Json(accepted)))
}
