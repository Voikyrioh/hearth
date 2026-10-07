//! `GET /security` : l'état de sécurité du compte connecté (HRT-24, BR-TRUST-008). Permis à tout
//! rôle : chacun voit l'alerte sur SON identifiant ; seul un administrateur voit en plus combien
//! d'autres comptes sont visés (jamais leurs noms).

use axum::Json;
use axum::extract::State;
use hearth_proto::api::security::{
    AlertInfo, AttackModeInfo, SecurityResponse, SecurityView, SessionDevice,
};

use super::auth::Caller;
use super::{ApiError, AppState, wire};
use crate::application::security::SecuritySnapshot;

/// L'alerte que le client voit, depuis ce que le service sait.
pub(crate) fn alert_info(snapshot: &SecuritySnapshot) -> Result<AlertInfo, ApiError> {
    Ok(AlertInfo {
        own: snapshot.alert.own,
        since: snapshot.alert.since.map(wire::date).transpose()?,
        others: snapshot.alert.others,
    })
}

/// Le contenu du message `security` du flux.
pub(crate) fn view(snapshot: &SecuritySnapshot) -> Result<SecurityView, ApiError> {
    Ok(SecurityView {
        alert: alert_info(snapshot)?,
        // Le mode attaque n'existe pas encore (HRT-25) : toujours éteint.
        attack_mode: AttackModeInfo::off(),
    })
}

/// `GET /api/v1/security`.
pub async fn state(
    State(state): State<AppState>,
    Caller(caller): Caller,
) -> Result<Json<SecurityResponse>, ApiError> {
    let snapshot = state
        .security
        .state_for(&caller.account, &caller.session_id)
        .await
        .map_err(|error| ApiError::internal(&error))?;
    let view = view(&snapshot)?;
    Ok(Json(SecurityResponse {
        alert: view.alert,
        attack_mode: view.attack_mode,
        device: if snapshot.device_proven {
            SessionDevice::Proven
        } else {
            SessionDevice::None
        },
    }))
}
