//! Modifier un service : `GET /services/{key}/spec`, `PUT /services/{key}/spec` et `GET
//! /operations/services/{id}/tail` (HRT-87, ADR-0044, ADR-0045, BR-EDIT-*).
//!
//! - **`GET /services/{key}/spec`** est une consultation du droit « gérer les services » : la description du
//!   service, SANS valeur ni commande (les noms des variables sous `Concealed`), sa révision et l'écart avec le
//!   moteur. Elle n'écrit rien et n'est pas journalisée. Le moteur n'a pas besoin d'être prêt (`BR-EDIT-029`).
//! - **`PUT /services/{key}/spec`** est un acte d'administration confirmé (`ServiceUpdate` `0x13`, JAMAIS couvert
//!   par l'élévation, `BR-EDIT-017`), réservé au droit (niveau `Services`). Il répond `202 { applied: true,
//!   operation_id }` : la modification court en arrière-plan, son issue se lit dans l'opération ; ou `200 {
//!   applied: false }` quand rien ne change (`BR-EDIT-013`, aucune opération, aucune entrée au journal). Les refus
//!   connus d'avance sont synchrones et typés : `SERVICE_NOT_FOUND`, `SERVICE_ADOPTED` (journalisé),
//!   `SERVICE_BUSY`, `SERVICE_NOT_DRIVABLE`, `ENGINE_UNAVAILABLE`, `SPEC_CONFLICT` (la configuration a changé
//!   depuis l'ouverture du formulaire, jamais le nom d'un compte), `PLAN_SETTING_REFUSED`, `VALIDATION_ERROR`,
//!   `LIMITS_REQUIRED`, `FOLDER_REFUSED`, `PORT_IN_USE`.
//! - **`GET /operations/services/{id}/tail`** rend les 20 dernières lignes du nouveau service qui n'a pas démarré
//!   (`BR-EDIT-020`), gardées 24 heures en mémoire de l'agent : du contenu sensible, réservé au droit.
//!
//! Aucun message d'erreur ne répète une valeur reçue, un nom de variable ni un chemin : les textes sont fixes.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Extension, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use hearth_proto::api::service_update::{OperationTail, ServiceSpecView, SpecChange};
use hearth_proto::error::{ErrorBody, ErrorCode};

use super::auth::{Caller, Requester};
use super::deployments::deploy_error;
use super::error::TargetMark;
use super::service_ops::{access_error, checked_id, viewer_of};
use super::services::operation_key;
use super::{ApiError, AppState};
use crate::application::service_update::{UpdateError, UpdateFailed};
use crate::application::sessions::Reauthenticated;
use crate::domain::audit::{ClientName, Target};
use crate::domain::services::update::UpdateRefusal;
use crate::domain::services::{ServiceKey, ServiceStateKind};
use crate::entrypoint::services_wire;

/// Le refus d'une modification, avec le service visé quand il est publié (le journal d'activité l'écrit par son
/// nom).
pub struct UpdateFailure(ApiError, Option<Target>);

impl IntoResponse for UpdateFailure {
    fn into_response(self) -> Response {
        let mut response = self.0.into_response();
        if let Some(target) = self.1 {
            response.extensions_mut().insert(TargetMark(target));
        }
        response
    }
}

fn failure(failed: UpdateFailed) -> UpdateFailure {
    let target = failed
        .name
        .as_deref()
        .and_then(ClientName::parse)
        .map(Target::Service);
    UpdateFailure(update_error(failed.error), target)
}

fn update_error(error: UpdateError) -> ApiError {
    match error {
        UpdateError::EngineUnavailable(engine) => ApiError(ErrorBody::with_details(
            ErrorCode::EngineUnavailable,
            "Le moteur de conteneurs n'est pas prêt. Le service n'a pas été modifié.",
            serde_json::json!({ "engine": services_wire::engine_state(engine) }),
        )),
        UpdateError::NotFound => {
            ApiError::new(ErrorCode::ServiceNotFound, "Ce service n'existe plus.")
        }
        UpdateError::Adopted => ApiError::new(
            ErrorCode::ServiceAdopted,
            "Hearth ne modifie pas les services créés hors de Hearth.",
        ),
        UpdateError::Busy { by, kind } => ApiError(ErrorBody::with_details(
            ErrorCode::ServiceBusy,
            "Une action est déjà en cours sur ce service.",
            serde_json::json!({ "by": by, "kind": kind.to_wire() }),
        )),
        UpdateError::NotUpdatable(state) => ApiError(ErrorBody::with_details(
            ErrorCode::ServiceNotDrivable,
            match state {
                ServiceStateKind::Deploying
                | ServiceStateKind::Starting
                | ServiceStateKind::Stopping => "Une action est déjà en cours sur ce service.",
                _ => "Hearth ne pilote pas ce service dans son état actuel.",
            },
            serde_json::json!({ "state": services_wire::service_state(state) }),
        )),
        // Jamais le nom d'un compte : l'écart se détecte sans auteur, et une modification de la description par un
        // autre compte se dit pareil (BR-EDIT-024).
        UpdateError::Conflict {
            current_rev,
            outside,
        } => ApiError(ErrorBody::with_details(
            ErrorCode::SpecConflict,
            "Ce service a changé depuis que tu as ouvert le formulaire.",
            serde_json::json!({ "current_rev": current_rev, "outside": outside }),
        )),
        UpdateError::Refused(refusal) => match refusal {
            UpdateRefusal::Invalid(invalid) => ApiError::invalid_because(
                invalid.field,
                invalid.reason,
                "Une valeur du formulaire n'est pas valable.",
            ),
            UpdateRefusal::SettingNotAllowed { reason } => ApiError(ErrorBody::with_details(
                ErrorCode::PlanSettingRefused,
                match reason {
                    "frozen" => {
                        "Ce réglage est figé dans le monde : le changer demande de créer un autre monde."
                    }
                    "fiche" => {
                        "Ce changement n'est pas permis pour un serveur de jeu : la fiche décide."
                    }
                    _ => "Ce réglage n'est pas permis pour un service créé par Hearth.",
                },
                serde_json::json!({ "reason": reason }),
            )),
            UpdateRefusal::LimitsRequired => ApiError::new(
                ErrorCode::LimitsRequired,
                "Un service se crée avec une limite de mémoire et une limite de processeur.",
            ),
            UpdateRefusal::HostFolder { index } => ApiError(ErrorBody::with_details(
                ErrorCode::FolderRefused,
                "Dossiers de données : pas encore disponible.",
                serde_json::json!({ "field": "mounts", "index": index, "reason": "not_available" }),
            )),
        },
        UpdateError::Ports(error) => deploy_error(*error),
        UpdateError::SecretsUnreadable => ApiError::new(
            ErrorCode::InternalError,
            "Les secrets des services sont illisibles : rien n'a été changé.",
        ),
        UpdateError::Internal(message) => {
            tracing::error!(error = %message, "modification d'un service refusée : erreur interne");
            ApiError::new(ErrorCode::InternalError, "Une erreur interne est survenue.")
        }
    }
}

/// Une clé mal formée est un service inconnu, comme pour toute route de service.
fn unknown() -> UpdateFailure {
    failure(UpdateFailed::not_found())
}

/// `GET /api/v1/services/{key}/spec`.
pub async fn view(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<ServiceSpecView>, UpdateFailure> {
    let key = ServiceKey::parse(&key).ok_or_else(unknown)?;
    state
        .service_updates
        .view(&key)
        .await
        .map(Json)
        .map_err(failure)
}

/// `PUT /api/v1/services/{key}/spec` (acte `ServiceUpdate` `0x13`).
pub async fn update(
    State(state): State<AppState>,
    _confirmed: Extension<Reauthenticated>,
    Requester(actor): Requester,
    headers: HeaderMap,
    Path(key): Path<String>,
    body: Result<Json<SpecChange>, JsonRejection>,
) -> Result<Response, UpdateFailure> {
    let key = ServiceKey::parse(&key).ok_or_else(unknown)?;
    // Un texte fixe : le texte d'une erreur de lecture pourrait citer une valeur du corps.
    let Json(change) = body.map_err(|_| {
        UpdateFailure(
            ApiError::new(
                ErrorCode::ValidationError,
                "Le corps de la demande n'est pas valable.",
            ),
            None,
        )
    })?;
    let accepted = state
        .service_updates
        .update(&key, change, actor, &operation_key(&headers))
        .await
        .map_err(failure)?;
    let status = if accepted.applied {
        StatusCode::ACCEPTED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(accepted)).into_response())
}

/// `GET /api/v1/operations/services/{id}/tail`.
pub async fn tail(
    State(state): State<AppState>,
    Caller(session): Caller,
    Path(id): Path<String>,
) -> Result<Json<OperationTail>, ApiError> {
    let id = checked_id(&id)?;
    // Les lignes d'un journal sont sensibles : la même visibilité que `GET /operations/services/{id}` (une
    // opération d'un service que le compte ne voit pas est une opération inconnue).
    state
        .service_ops
        .read(id, viewer_of(&session))
        .await
        .map_err(access_error)?;
    let found = state
        .service_updates
        .tail(id)
        .or_else(|| state.mods.tail(id));
    found.map(Json).ok_or_else(|| {
        ApiError::new(
            ErrorCode::OperationNotFound,
            "Cette opération n'existe pas, ou plus.",
        )
    })
}
