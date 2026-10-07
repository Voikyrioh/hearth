//! `GET /security` : l'état de sécurité du compte connecté (HRT-24, BR-TRUST-008). Permis à tout
//! rôle : chacun voit l'alerte sur SON identifiant ; seul un administrateur voit en plus combien
//! d'autres comptes sont visés (jamais leurs noms).

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Extension, State};
use axum::http::HeaderMap;
use hearth_proto::api::reauth::{AdminReauthInfo, SetReauthRequest};
use hearth_proto::api::security::{
    AlertInfo, AttackModeEnd, AttackModeInfo, AttackModeState, SecurityResponse, SecurityView,
    SessionDevice, SetAttackModeRequest, attack_mode_refusal,
};
use hearth_proto::error::{ErrorBody, ErrorCode};
use tracing::Instrument;

use super::auth::{Caller, Requester, bearer_token};
use super::sessions::ClientAddr;
use super::{ApiError, AppState, wire};
use crate::application::attack_mode::AttackStatus;
use crate::application::security::SecuritySnapshot;
use crate::application::sessions::{AttackModeError, ClientInfo, LoginError, Reauthenticated};
use crate::domain::secret::Secret;
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
        .admin_reauth_info(&caller, &addr, state.sessions.reauth_required())
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
/// d'administration** (Q14 point 3, Q16) : le corps porte le mot de passe actuel et la preuve de
/// possession d'une clé inscrite pour le compte (`purpose: "attack_mode"`, liée au jeton et à la valeur
/// demandée). Sans preuve valide : `409 POST_NOT_RECOGNIZED` (`details.reason`), rien n'est écrit.
/// Mot de passe faux : `422 WRONG_PASSWORD` ou `429` (mêmes compteurs que la connexion). Idempotente.
pub async fn set_attack_mode(
    State(state): State<AppState>,
    ClientAddr(addr): ClientAddr,
    Caller(caller): Caller,
    Requester(by): Requester,
    confirmed: Option<Extension<Reauthenticated>>,
    request_headers: HeaderMap,
    body: Result<Json<SetAttackModeRequest>, JsonRejection>,
) -> Result<Json<AttackModeInfo>, ApiError> {
    let Json(request) = body?;
    // Confirmé par la couche `reauth` (contrat `reauth`, usage `0x05`) : preuve et mot de passe sont déjà
    // vérifiés, il ne reste que le changement (HRT-28).
    if confirmed.is_some() {
        let status = state
            .sessions
            .change_attack_mode_confirmed(&caller, request.active, &by)
            .await
            .map_err(attack_mode_error)?;
        return Ok(Json(attack_info(&status)?));
    }
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
    // Le mot de passe est compté même si le client coupe (comme la connexion) : tâche détachée.
    let sessions = state.sessions.clone();
    let work = tokio::spawn(
        async move {
            sessions
                .set_attack_mode(
                    &caller,
                    &token,
                    request.active,
                    Secret::from(request.password),
                    request.device.as_ref(),
                    &client,
                    &by,
                )
                .await
        }
        .in_current_span(),
    );
    let status = work
        .await
        .map_err(|error| ApiError::internal(&error))?
        .map_err(attack_mode_error)?;
    Ok(Json(attack_info(&status)?))
}

fn attack_mode_error(error: AttackModeError) -> ApiError {
    match error {
        AttackModeError::Forbidden => ApiError::new(
            ErrorCode::ForbiddenRole,
            "Tu n'as pas la permission d'activer le mode attaque. C'est réservé aux administrateurs.",
        ),
        AttackModeError::ProofMissing => refusal(
            attack_mode_refusal::PROOF_MISSING,
            "Pour changer le mode attaque, ce poste doit prouver sa clé. Utilise un poste dont la clé est enregistrée, ou la commande sur le serveur.",
        ),
        AttackModeError::ProofInvalid => refusal(
            attack_mode_refusal::PROOF_INVALID,
            "La preuve de la clé de ce poste est absente ou invalide : redemande un défi et signe-le.",
        ),
        AttackModeError::Password(error) => match *error {
            // Un administrateur authentifié : « mot de passe actuel incorrect », pas un 401.
            LoginError::InvalidCredentials => {
                ApiError::new(ErrorCode::WrongPassword, "Mot de passe actuel incorrect.")
            }
            other => ApiError::from(other),
        },
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
    confirmed: Option<Extension<Reauthenticated>>,
    body: Result<Json<SetReauthRequest>, JsonRejection>,
) -> Result<Json<AdminReauthInfo>, ApiError> {
    let Json(request) = body?;
    if confirmed.is_none() {
        return Err(ApiError::internal(&"réglage sans confirmation"));
    }
    state
        .sessions
        .set_reauth_mode(&caller.account, request.password, &by)
        .await
        .map_err(|error| ApiError::internal(&error))?;
    let info = state
        .sessions
        .admin_reauth_info(&caller, &addr, state.sessions.reauth_required())
        .await
        .map_err(|error| ApiError::internal(&error))?
        .ok_or_else(|| ApiError::new(ErrorCode::NotFound, "Route inconnue"))?;
    Ok(Json(info))
}

fn refusal(reason: &str, message: &str) -> ApiError {
    ApiError(ErrorBody::with_details(
        ErrorCode::PostNotRecognized,
        message,
        serde_json::json!({ "field": "device", "reason": reason }),
    ))
}
