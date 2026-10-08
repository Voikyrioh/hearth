//! `GET /security` : l'état de sécurité du compte connecté (HRT-24, BR-TRUST-008). Permis à tout
//! rôle : chacun voit l'alerte sur SON identifiant ; seul un administrateur voit en plus combien
//! d'autres comptes sont visés (jamais leurs noms).

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Extension, State};
use hearth_proto::api::reauth::{AdminReauthInfo, SetReauthRequest};
use hearth_proto::api::security::{
    AlertInfo, AttackModeEnd, AttackModeInfo, AttackModeState, SecurityResponse, SecurityView,
    SessionDevice, SetAttackModeRequest,
};
use hearth_proto::error::ErrorCode;

use super::auth::{Caller, Requester};
use super::sessions::ClientAddr;
use super::{ApiError, AppState, wire};
use crate::application::attack_mode::AttackStatus;
use crate::application::security::SecuritySnapshot;
use crate::application::sessions::{AttackModeError, Reauthenticated};
use crate::domain::trust::attack_mode::{Effective, EndHow};

/// L'alerte que le client voit, depuis ce que le service sait.
pub(crate) fn alert_info(snapshot: &SecuritySnapshot) -> Result<AlertInfo, ApiError> {
    Ok(AlertInfo {
        own: snapshot.alert.own,
        since: snapshot.alert.since.map(wire::date).transpose()?,
        others: snapshot.alert.others,
    })
}

/// Le mode attaque que le client voit : l'état, depuis quand, dans combien de secondes il reprend s'il
/// est suspendu, comment il s'est terminé s'il est éteint.
pub(crate) fn attack_info(status: &AttackStatus) -> Result<AttackModeInfo, ApiError> {
    let (state, resumes_in_s) = match status.state {
        Effective::Off => (AttackModeState::Off, None),
        Effective::Active => (AttackModeState::Active, None),
        Effective::Suspended { remaining } => (
            AttackModeState::Suspended,
            // Arrondi à la seconde supérieure : jamais « 0 s » tant que la fenêtre court.
            Some(
                u64::try_from(
                    remaining.whole_seconds() + i64::from(remaining.subsec_nanoseconds() > 0),
                )
                .unwrap_or(0),
            ),
        ),
    };
    Ok(AttackModeInfo {
        state,
        since: status.since.map(wire::date).transpose()?,
        resumes_in_s,
        last_end: status.last_end.map(|how| match how {
            EndHow::Manual => AttackModeEnd::Manual,
            EndHow::Auto => AttackModeEnd::Auto,
            EndHow::Cli => AttackModeEnd::Cli,
        }),
    })
}

/// Le contenu du message `security` du flux.
pub(crate) fn view(snapshot: &SecuritySnapshot) -> Result<SecurityView, ApiError> {
    Ok(SecurityView {
        alert: alert_info(snapshot)?,
        attack_mode: attack_info(&snapshot.attack)?,
    })
}

/// `GET /api/v1/security`.
pub async fn state(
    State(state): State<AppState>,
    ClientAddr(addr): ClientAddr,
    Caller(caller): Caller,
) -> Result<Json<SecurityResponse>, ApiError> {
    let snapshot = state
        .security
        .state_for(&caller.account, &caller.session_id)
        .await
        .map_err(|error| ApiError::internal(&error))?;
    let view = view(&snapshot)?;
    let admin_reauth = state
        .sessions
        .admin_reauth_info(&caller, &addr)
        .await
        .map_err(|error| ApiError::internal(&error))?;
    Ok(Json(SecurityResponse {
        alert: view.alert,
        attack_mode: view.attack_mode,
        device: if snapshot.device_proven {
            SessionDevice::Proven
        } else {
            SessionDevice::None
        },
        admin_reauth,
    }))
}

/// `PUT /api/v1/security/attack-mode` : active ou désactive le mode attaque (administrateur). **Un acte
/// d'administration** (Q14 point 3, Q16) : la couche `reauth` exige le membre `reauth` (mot de passe actuel
/// et preuve d'une clé inscrite, usage `0x05`) ; sans lui, `426`. Idempotente. Il n'existe plus de forme
/// à plat (usage `0x03`).
pub async fn set_attack_mode(
    State(state): State<AppState>,
    Caller(caller): Caller,
    Requester(by): Requester,
    _confirmed: Extension<Reauthenticated>,
    body: Result<Json<SetAttackModeRequest>, JsonRejection>,
) -> Result<Json<AttackModeInfo>, ApiError> {
    let Json(request) = body?;
    // Confirmé par la couche `reauth` (contrat `reauth`, usage `0x05`) : preuve et mot de passe sont déjà
    // vérifiés, il ne reste que le changement (HRT-28). Il n'existe plus de forme à plat (usage `0x03`).
    let status = state
        .sessions
        .change_attack_mode_confirmed(&caller, request.active, &by)
        .await
        .map_err(attack_mode_error)?;
    Ok(Json(attack_info(&status)?))
}

fn attack_mode_error(error: AttackModeError) -> ApiError {
    match error {
        AttackModeError::Forbidden => ApiError::new(
            ErrorCode::ForbiddenRole,
            "Tu n'as pas la permission d'activer le mode attaque. C'est réservé aux administrateurs.",
        ),
        AttackModeError::Unavailable => ApiError::new(ErrorCode::NotFound, "Route inconnue"),
        AttackModeError::Store(error) => ApiError::internal(&error),
    }
}

/// `PUT /api/v1/me/reauth` : le réglage de fréquence du mot de passe du compte de l'appelant (HRT-28,
/// Q19, BR-TRUST-042). Tout rôle. **Toujours confirmé** par la couche `reauth` (mot de passe et clé,
/// jamais l'élévation) : sans confirmation, `426` ; la couche pose `Reauthenticated` avant ce handler.
pub async fn set_reauth(
    State(state): State<AppState>,
    ClientAddr(addr): ClientAddr,
    Caller(caller): Caller,
    Requester(by): Requester,
    _confirmed: Extension<Reauthenticated>,
    body: Result<Json<SetReauthRequest>, JsonRejection>,
) -> Result<Json<AdminReauthInfo>, ApiError> {
    let Json(request) = body?;
    state
        .sessions
        .set_reauth_mode(&caller.account, request.password, &by)
        .await
        .map_err(|error| ApiError::internal(&error))?;
    let info = state
        .sessions
        .admin_reauth_info(&caller, &addr)
        .await
        .map_err(|error| ApiError::internal(&error))?
        .ok_or_else(|| ApiError::new(ErrorCode::NotFound, "Route inconnue"))?;
    Ok(Json(info))
}
