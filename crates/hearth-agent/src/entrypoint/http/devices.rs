//! Postes de confiance de l'utilisateur : `GET /me/devices` et `DELETE /me/devices/{id}`
//! (HRT-22, BR-TRUST-004, 022). Permis à tout rôle : chacun ne voit et ne retire que les siens.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use hearth_proto::api::devices::{DeviceItem, DevicesResponse, MAX_DEVICES_PER_ACCOUNT};
use hearth_proto::error::ErrorCode;

use super::auth::{Caller, Requester};
use super::{ApiError, AppState, wire};
use crate::application::trust::{DeviceView, RemoveError, TrustService};

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
/// sessions. `404` si le poste n'existe pas ou n'est pas à l'appelant, `422` si c'est le poste
/// d'où part la requête.
pub async fn remove(
    State(state): State<AppState>,
    Caller(caller): Caller,
    Requester(by): Requester,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    trust(&state)?
        .remove(&caller.account.id, &caller.session_id, &id, &by)
        .await
        .map_err(|error| match error {
            RemoveError::NotFound => ApiError::new(ErrorCode::NotFound, "Poste introuvable."),
            RemoveError::IsCurrent => ApiError::invalid(
                "id",
                "C'est le poste que tu utilises : retire-le depuis un autre poste.",
            ),
            RemoveError::Store(error) => ApiError::internal(&error),
        })?;
    Ok(StatusCode::NO_CONTENT)
}
