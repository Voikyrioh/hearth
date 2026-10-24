//! Déployer une image : `POST /deployments/check`, `POST /deployments` et `GET /engine/capacity` (HRT-68,
//! ADR-0039, BR-DEPLOY-*, BR-DEPLOY-038).
//!
//! - **`POST /deployments`** est un acte d'administration confirmé (`ServiceDeploy` `0x0F`, JAMAIS couvert par
//!   l'élévation : le mot de passe à chaque fois), réservé aux comptes qui ont le droit « gérer les services »
//!   (niveau `Services`). Il répond `202 { operation_id, service_key }` : le déploiement court en arrière-plan,
//!   son issue se lit dans l'opération (`GET /operations/services/{id}`), qui s'annule aussi
//!   (`DELETE /operations/services/{id}`, acte `0x10`) tant que l'image se récupère. Les refus connus d'avance
//!   sont SYNCHRONES et typés (`NAME_TAKEN`, `PORT_IN_USE`, `DEPLOY_LIMIT`, `LIMITS_REQUIRED`,
//!   `PLAN_SETTING_REFUSED`, `FOLDER_REFUSED`, `ENGINE_UNAVAILABLE`, `VALIDATION_ERROR`) : le formulaire les
//!   montre sans attendre.
//! - **`POST /deployments/check`** est une consultation (un `POST` qui ne modifie rien) : elle reçoit la partie
//!   NON secrète de la demande (jamais une variable, jamais la commande), rend ses constats par champ, n'écrit
//!   rien et n'est pas journalisée (`BR-DEPLOY-033`).
//! - **`GET /engine/capacity`** rend les bornes de la machine (`BR-DEPLOY-038`) et ce qui est déjà engagé, sans
//!   aucun nom.
//!
//! Aucun message d'erreur ne répète une valeur reçue (nom de variable, valeur, commande, chemin) : les textes
//! sont fixes, le champ et la raison sont des codes stables.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Extension, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use hearth_proto::api::deploy::{
    DeployAccepted, DeployCheck, DeployCheckRequest, DeploySource, DeploySpec, EngineCapacity,
    PortStatus,
};
use hearth_proto::error::{ErrorBody, ErrorCode};

use super::auth::Requester;
use super::error::TargetMark;
use super::services::operation_key;
use super::{ApiError, AppState};
use crate::application::deployments::DeployError;
use crate::application::sessions::Reauthenticated;
use crate::domain::audit::{ClientName, Target};
use crate::entrypoint::services_wire;

fn deploy_error(error: DeployError) -> ApiError {
    match error {
        DeployError::EngineUnavailable(engine) => ApiError(ErrorBody::with_details(
            ErrorCode::EngineUnavailable,
            "Le moteur de conteneurs n'est pas prêt. Rien n'a été créé.",
            serde_json::json!({ "engine": services_wire::engine_state(engine) }),
        )),
        DeployError::Invalid(invalid) => ApiError::invalid_because(
            invalid.field,
            invalid.reason,
            "Une valeur du formulaire n'est pas valable.",
        ),
        DeployError::SettingNotAllowed => ApiError::new(
            ErrorCode::PlanSettingRefused,
            "Ce réglage n'est pas permis pour un service créé par Hearth.",
        ),
        DeployError::LimitsRequired => ApiError::new(
            ErrorCode::LimitsRequired,
            "Un service se crée avec une limite de mémoire et une limite de processeur.",
        ),
        DeployError::FolderRefused { index, reason } => ApiError(ErrorBody::with_details(
            ErrorCode::FolderRefused,
            match reason {
                "protected" => {
                    "Ce dossier est protégé : il ne peut pas être monté dans un service."
                }
                "not_a_path" => "Ce chemin n'est pas un chemin de dossier.",
                _ => "Dossiers de données : pas encore disponible.",
            },
            serde_json::json!({ "field": "mounts", "index": index, "reason": reason }),
        )),
        DeployError::NameTaken => ApiError::new(ErrorCode::NameTaken, "Ce nom est déjà pris."),
        DeployError::NameBeingDeleted => ApiError::new(
            ErrorCode::NameBeingDeleted,
            "Ce nom est celui d'un service en cours de suppression. Attends la fin de l'effacement.",
        ),
        DeployError::PortInUse {
            host_port,
            protocol,
            status,
            holder,
            suggested,
        } => ApiError(ErrorBody::with_details(
            ErrorCode::PortInUse,
            match status {
                PortStatus::AgentPort => "Ce port est utilisé par Hearth lui-même.",
                PortStatus::HeldByUnmanaged => "Port tenu par un conteneur créé hors de Hearth.",
                PortStatus::ReservedByStopped => "Ce port est réservé par un service arrêté.",
                PortStatus::HeldByService => "Ce port est utilisé par un autre service.",
                PortStatus::HeldByProgram => "Ce port est utilisé par un programme du serveur.",
                _ => "Ce port n'est pas disponible.",
            },
            serde_json::json!({
                "port": host_port,
                "protocol": protocol,
                "status": status,
                "holder": holder,
                "suggested": suggested,
            }),
        )),
        DeployError::DeployLimit => ApiError::new(
            ErrorCode::DeployLimit,
            "Deux déploiements sont déjà en cours sur ce serveur. Attends la fin de l'un d'eux.",
        ),
        DeployError::LowDisk { free_bytes } => ApiError(ErrorBody::with_details(
            ErrorCode::NoSpace,
            "Il reste peu de place sur le disque des images. Coche « Continuer malgré le manque de place » pour lancer quand même.",
            serde_json::json!({ "reason": "low_disk", "free_bytes": free_bytes }),
        )),
        // Installer depuis une fiche (HRT-101) : des refus typés, sans jamais citer une valeur reçue.
        DeployError::FicheUnknown => ApiError::new(
            ErrorCode::FicheUnknown,
            "Cette fiche n'est pas proposée par ce serveur.",
        ),
        DeployError::FicheChanged => ApiError::new(
            ErrorCode::FicheChanged,
            "Cette fiche a changé depuis son affichage. Relis-la avant d'installer.",
        ),
        DeployError::AgreementRequired(ids) => ApiError(ErrorBody::with_details(
            ErrorCode::AgreementRequired,
            "Ce serveur ne peut pas être installé sans que le contrat de licence soit accepté.",
            serde_json::json!({ "ids": ids }),
        )),
        DeployError::SettingRefused {
            id,
            reason,
            character,
        } => ApiError(ErrorBody::with_details(
            ErrorCode::SettingRefused,
            "Une réponse n'est pas valable.",
            serde_json::json!({ "id": id, "reason": reason, "char": character }),
        )),
        DeployError::HostTooSmall { lack, needed, has } => ApiError(ErrorBody::with_details(
            ErrorCode::HostTooSmall,
            "Ce serveur est trop petit pour ce jeu.",
            serde_json::json!({ "lack": lack, "needed": needed, "has": has }),
        )),
        DeployError::NotEnoughSpace {
            needed,
            free,
            missing,
        } => ApiError(ErrorBody::with_details(
            ErrorCode::NoSpace,
            "Il n'y a pas assez de place pour installer ce jeu.",
            serde_json::json!({
                "reason": "install",
                "needed_bytes": needed,
                "free_bytes": free,
                "missing_bytes": missing,
            }),
        )),
        DeployError::Internal(message) => {
            tracing::error!(error = %message, "déploiement refusé : erreur interne");
            ApiError::new(ErrorCode::InternalError, "Une erreur interne est survenue.")
        }
    }
}

/// `POST /api/v1/deployments/check`.
pub async fn check(
    State(state): State<AppState>,
    body: Result<Json<DeployCheckRequest>, JsonRejection>,
) -> Result<Json<DeployCheck>, ApiError> {
    let Json(request) = body?;
    state
        .deployments
        .check(&request)
        .await
        .map(Json)
        .map_err(deploy_error)
}

/// Une erreur de `POST /deployments` qui désigne, pour le journal d'activité, la fiche visée (id et version, jamais
/// une réponse) quand la demande vient d'une fiche (BR-FICHE-032).
pub struct DeployFailure(ApiError, Option<Target>);

impl IntoResponse for DeployFailure {
    fn into_response(self) -> Response {
        let mut response = self.0.into_response();
        if let Some(target) = self.1 {
            response.extensions_mut().insert(TargetMark(target));
        }
        response
    }
}

/// La fiche visée par la demande : son identifiant (nettoyé et borné comme un nom) et la version vue par le client.
fn fiche_target(spec: &DeploySpec) -> Option<Target> {
    let Some(DeploySource::Fiche { id, version, .. }) = &spec.source else {
        return None;
    };
    ClientName::parse(&format!("{id} (version {version})")).map(Target::Fiche)
}

/// `POST /api/v1/deployments` (acte `ServiceDeploy` `0x0F`).
pub async fn create(
    State(state): State<AppState>,
    _confirmed: Extension<Reauthenticated>,
    Requester(actor): Requester,
    headers: HeaderMap,
    body: Result<Json<DeploySpec>, JsonRejection>,
) -> Result<(StatusCode, Json<DeployAccepted>), DeployFailure> {
    let Json(spec) = body.map_err(|rejection| DeployFailure(rejection.into(), None))?;
    let fiche = fiche_target(&spec);
    let accepted = state
        .deployments
        .deploy(spec, actor, &operation_key(&headers))
        .await
        .map_err(|error| DeployFailure(deploy_error(error), fiche))?;
    Ok((StatusCode::ACCEPTED, Json(accepted)))
}

/// `GET /api/v1/engine/capacity`.
pub async fn capacity(State(state): State<AppState>) -> Result<Json<EngineCapacity>, ApiError> {
    state
        .deployments
        .capacity()
        .await
        .map(Json)
        .map_err(deploy_error)
}
