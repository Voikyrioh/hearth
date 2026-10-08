//! Routes de la mise à jour de l'agent à distance (BR-UPDATE-011 à 019, 024).
//!
//! `POST /agent/update` est réservée aux administrateurs : la table `ENDPOINTS` lui donne le
//! niveau `Admin`, et la couche d'accès du routeur refuse un compte en lecture seule (`403
//! FORBIDDEN_ROLE`, consigné au journal) avant la lecture du corps (BR-UPDATE-011). Les handlers ne
//! contrôlent rien : ils traduisent. Les lectures sont ouvertes à tout compte authentifié.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Extension, State};
use axum::http::StatusCode;
use hearth_proto::api::update::{
    AgentUpdateAccepted, AgentUpdateRequest, AgentUpdateStatus, LastUpdateResponse,
};
use hearth_proto::error::ErrorCode;

use super::auth::Requester;
use super::{ApiError, AppState};
use crate::application::update::UpdateError;
use crate::domain::update::{UpdateInput, UpdateRefusal};

/// `GET /api/v1/agent/update` : version, mode, mise à jour en cours, dernier résultat.
pub async fn status(State(state): State<AppState>) -> Json<AgentUpdateStatus> {
    Json(state.update.status())
}

/// `GET /api/v1/agent/update/last` : le dernier résultat, qui survit au redémarrage de l'agent
/// (BR-UPDATE-017 : le client qui revient après une coupure le lit ici).
pub async fn last(State(state): State<AppState>) -> Json<LastUpdateResponse> {
    Json(LastUpdateResponse {
        last: state.update.last(),
    })
}

/// `POST /api/v1/agent/update` : lance la mise à jour côté serveur et rend `202` tout de suite.
pub async fn start(
    State(state): State<AppState>,
    _confirmed: Extension<crate::application::sessions::Reauthenticated>,
    Requester(by): Requester,
    body: Result<Json<AgentUpdateRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<AgentUpdateAccepted>), ApiError> {
    let Json(request) = body?;
    let first = state
        .update
        .start(
            by,
            UpdateInput {
                version: &request.version,
                url: &request.url,
                signature: &request.signature,
                sha256: &request.sha256,
            },
        )
        .await
        .map_err(api_error)?;
    Ok((
        StatusCode::ACCEPTED,
        Json(AgentUpdateAccepted {
            version: first.version,
            step: first.step,
        }),
    ))
}

/// Le refus, au format de l'API. Les messages sont ceux de la spécification ; le client affiche
/// les siens, indexés par le code.
fn api_error(error: UpdateError) -> ApiError {
    match error {
        UpdateError::BadSignature => ApiError::new(ErrorCode::BadSignature, error.to_string()),
        UpdateError::Refused(refusal) => match &refusal {
            UpdateRefusal::Managed => ApiError::new(ErrorCode::ManagedInstall, refusal.to_string()),
            UpdateRefusal::InProgress => {
                ApiError::new(ErrorCode::OperationInProgress, refusal.to_string())
            }
            UpdateRefusal::Invalid { field, .. } => ApiError::invalid(field, refusal.to_string()),
            UpdateRefusal::NotNewer { .. } => ApiError::invalid("version", refusal.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_refusal_has_its_own_code_and_the_spec_message() {
        let in_progress = api_error(UpdateError::Refused(UpdateRefusal::InProgress))
            .0
            .error;
        assert_eq!(in_progress.code, ErrorCode::OperationInProgress);
        assert_eq!(
            in_progress.message,
            "Une mise à jour de l'agent est déjà en cours. Réessaye plus tard."
        );
        assert_eq!(
            api_error(UpdateError::Refused(UpdateRefusal::Managed))
                .0
                .error
                .code,
            ErrorCode::ManagedInstall
        );
        assert_eq!(
            api_error(UpdateError::BadSignature).0.error.code,
            ErrorCode::BadSignature
        );
        let invalid = api_error(UpdateError::Refused(UpdateRefusal::Invalid {
            field: "url",
            reason: "x",
        }))
        .0
        .error;
        assert_eq!(invalid.code, ErrorCode::ValidationError);
        assert_eq!(invalid.details["field"], "url");
    }
}
