//! Postes de confiance de l'utilisateur : `GET /me/devices` et `DELETE /me/devices/{id}`
//! (HRT-22, BR-TRUST-004, 022). Permis à tout rôle : chacun ne voit et ne retire que les siens.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use hearth_proto::api::devices::{
    DeviceItem, DevicesResponse, MAX_DEVICES_PER_ACCOUNT, RemoveDeviceRequest, removal_refusal,
};
use hearth_proto::error::ErrorCode;
use tracing::Instrument;

use super::auth::{Caller, Requester, bearer_token};
use super::sessions::ClientAddr;
use super::{ApiError, AppState, wire};
use crate::application::sessions::{ClientInfo, LoginError};
use crate::application::trust::{DeviceView, RemoveError, TrustService};
use crate::domain::secret::Secret;

fn trust(state: &AppState) -> Result<&std::sync::Arc<TrustService>, ApiError> {
    state
        .sessions
        .trust()
        .ok_or_else(|| ApiError::new(ErrorCode::NotFound, "Route inconnue"))
}

fn item(device: DeviceView) -> Result<DeviceItem, ApiError> {
    Ok(DeviceItem {
        id: device.id.to_string(),
        name: device.name,
        created_at: wire::date(device.created_at)?,
        last_proved_at: wire::date(device.last_proved_at)?,
        last_addr: device.last_addr,
        current: device.current,
    })
}

/// `GET /api/v1/me/devices` : les postes de confiance de l'appelant (jamais ceux d'un autre).
pub async fn list(
    State(state): State<AppState>,
    Caller(caller): Caller,
) -> Result<Json<DevicesResponse>, ApiError> {
    let devices = trust(&state)?
        .list(&caller.account.id, &caller.session_id)
        .await?
        .into_iter()
        .map(item)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(DevicesResponse {
        devices,
        max: MAX_DEVICES_PER_ACCOUNT,
    }))
}

/// `DELETE /api/v1/me/devices/{id}` : retire un poste de l'appelant, son adresse retenue et ses
/// sessions. **Un acte d'administration** (Q16) : le corps porte le mot de passe actuel et la preuve de
/// possession de la clé du poste courant (`purpose: "device_removal"`). `404` si le poste n'existe pas
/// ou n'est pas à l'appelant, `422` si c'est le poste d'où part la requête, `422` typé
/// (`details.reason`) sans clé inscrite ou sans preuve valable, `422 WRONG_PASSWORD` ou `429` pour le
/// mot de passe (mêmes compteurs que la connexion).
pub async fn remove(
    State(state): State<AppState>,
    ClientAddr(addr): ClientAddr,
    Caller(caller): Caller,
    Requester(by): Requester,
    Path(id): Path<String>,
    request_headers: HeaderMap,
    body: Result<Json<RemoveDeviceRequest>, JsonRejection>,
) -> Result<StatusCode, ApiError> {
    trust(&state)?;
    let Json(request) = body?;
    let token = bearer_token(&request_headers).ok_or_else(|| {
        ApiError::new(
            ErrorCode::Unauthenticated,
            "Jeton de session absent ou illisible",
        )
    })?;
    let client = ClientInfo {
        name: super::sessions::client_name(&request_headers),
        addr,
    };
    // La tentative va jusqu'au bout même si le client coupe (comme la connexion) : un échec de mot de
    // passe est toujours compté.
    let sessions = state.sessions.clone();
    let work = tokio::spawn(
        async move {
            sessions
                .remove_device(
                    &caller,
                    &token,
                    &id,
                    Secret::from(request.password),
                    request.device.as_ref(),
                    &client,
                    &by,
                )
                .await
        }
        .in_current_span(),
    );
    work.await
        .map_err(|error| ApiError::internal(&error))?
        .map_err(|error| match error {
            RemoveError::NotFound => ApiError::new(ErrorCode::NotFound, "Poste introuvable."),
            RemoveError::IsCurrent => ApiError::invalid(
                "id",
                "C'est le poste que tu utilises : retire-le depuis un autre poste.",
            ),
            RemoveError::DeviceRequired => refusal(
                removal_refusal::DEVICE_REQUIRED,
                "Ce poste n'a pas de clé enregistrée : retire un poste depuis un poste qui en a une, ou demande à un administrateur de changer ton mot de passe.",
            ),
            RemoveError::ProofInvalid => refusal(
                removal_refusal::PROOF_INVALID,
                "La preuve de la clé de ce poste est absente ou invalide : redemande un défi et signe-le.",
            ),
            RemoveError::Password(error) => match *error {
                // Un utilisateur authentifié : « mot de passe actuel incorrect », pas un 401.
                LoginError::InvalidCredentials => ApiError::new(
                    ErrorCode::WrongPassword,
                    "Mot de passe actuel incorrect.",
                ),
                other => ApiError::from(other),
            },
            RemoveError::Store(error) => ApiError::internal(&error),
        })?;
    Ok(StatusCode::NO_CONTENT)
}

fn refusal(reason: &str, message: &str) -> ApiError {
    ApiError(hearth_proto::error::ErrorBody::with_details(
        ErrorCode::ValidationError,
        message,
        serde_json::json!({ "field": "device", "reason": reason }),
    ))
}
