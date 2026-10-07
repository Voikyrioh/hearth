//! La couche de confirmation des actes d'administration (HRT-28, BR-TRUST-036, 037, 039, 040, 045, 046).
//!
//! **Posée par le routeur**, depuis la table des actes de `hearth-proto` (`admin_act::ROUTES`), sur toute
//! route dont le contrat est `Reauth` : un handler ne peut pas l'oublier (même discipline que
//! `auth::guard` et le suivi des opérations). Ordre des couches : `guard` (session, rôle), suivi des
//! opérations, **cette couche**, handler. La confirmation se fait donc *dans* l'exécution suivie : une
//! requête rejouée depuis sa clé d'opération rend le premier résultat sans revérifier une preuve déjà
//! consommée.
//!
//! Ordre des vérifications : l'acte est **reconstruit depuis la requête** (jamais lu dans la preuve), la
//! preuve de clé est vérifiée **avant** tout mot de passe, puis le mot de passe passe par le chemin de la
//! connexion (ou l'élévation couvre l'acte), puis le handler, puis le défi n'est consommé que si la
//! réponse est un succès.
//!
//! L'agent **exige** la confirmation dès la construction du service (`SessionService::new`) : une requête
//! sans membre `reauth` reçoit `426` « client trop ancien », jamais un repli vers « la session suffit ».
//! Le régime « accepte sans exiger » n'existe plus que pour les bancs d'essai qui envoient des actes bruts
//! (`accept_unconfirmed_acts_for_tests`) ; aucun code de production ne l'active.

use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use hearth_proto::admin_act::{ActKind, AdminAct, RouteAct};
use hearth_proto::api::accounts::{
    ChangeOwnPasswordRequest, ChangeRoleRequest, CreateAccountRequest,
};
use hearth_proto::api::reauth::{Reauth, SetReauthRequest, lenient_reauth, reauth_refusal};
use hearth_proto::api::security::SetAttackModeRequest;
use hearth_proto::api::update::AgentUpdateRequest;
use hearth_proto::error::{ErrorBody, ErrorCode, UpgradeTarget};
use serde::Deserialize;
use tracing::Instrument;

use super::auth::{Caller, Requester, bearer_token, origin_of, path_param};
use super::error::OutcomeMark;
use super::sessions::client_name;
use super::{ApiError, AppState};
use crate::application::sessions::{ClientInfo, LoginError, ReauthError};
use crate::domain::audit::{Outcome, Reason};
use crate::domain::lockout::retry_after_seconds;
use crate::domain::secret::Secret;

/// Posé par cette couche sur `PUT /me/password` une fois l'ancien mot de passe vérifié par le chemin de la
/// connexion (avec ou sans `reauth`) : le handler refuse de changer le mot de passe sans lui.
#[derive(Debug, Clone)]
pub struct PasswordConfirmed(pub Arc<Secret>);

/// Taille maximale d'un corps lu par la couche (celle de l'API).
const MAX_BYTES: usize = 1 << 20;

/// Ce que la couche sait d'une route d'acte.
#[derive(Clone)]
pub struct ReauthState {
    pub app: AppState,
    /// La ligne de la table des actes de `hearth-proto`.
    pub route: &'static RouteAct,
}

/// Le corps de la requête lu avec le type que le handler lira : même type, donc même lecture (un corps
/// que la couche lit autrement que le handler serait une porte dérobée).
enum Parsed {
    Create(CreateAccountRequest),
    Role(ChangeRoleRequest),
    Update(AgentUpdateRequest),
    Attack(SetAttackModeRequest),
    Own(ChangeOwnPasswordRequest),
    Setting(SetReauthRequest),
    /// Acte dont la cible est dans le chemin et dont aucun paramètre ne vient du corps.
    TargetOnly,
}

impl Parsed {
    /// `None` : corps illisible pour ce que la route attend.
    fn read(kind: ActKind, bytes: &[u8]) -> Option<Self> {
        match kind {
            ActKind::AccountCreate => serde_json::from_slice(bytes).ok().map(Self::Create),
            ActKind::AccountRole => serde_json::from_slice(bytes).ok().map(Self::Role),
            ActKind::AgentUpdate => serde_json::from_slice(bytes).ok().map(Self::Update),
            ActKind::AttackModeEnable | ActKind::AttackModeDisable => {
                serde_json::from_slice(bytes).ok().map(Self::Attack)
            }
            ActKind::AccountPasswordOwn => serde_json::from_slice(bytes).ok().map(Self::Own),
            ActKind::ReauthSetting => serde_json::from_slice(bytes).ok().map(Self::Setting),
            ActKind::AccountPassword | ActKind::AccountDelete | ActKind::SessionsRevoke => {
                Some(Self::TargetOnly)
            }
        }
    }

    /// L'acte reconstruit : sa cible vient du chemin, ses paramètres du corps lu.
    fn act<'a>(&'a self, kind: ActKind, id: &'a str) -> AdminAct<'a> {
        match (self, kind) {
            (Self::Create(request), _) => AdminAct::AccountCreate {
                username: &request.username,
                role: request.role,
            },
            (Self::Role(request), _) => AdminAct::AccountRole {
                target: id,
                role: request.role,
            },
            (Self::Update(request), _) => AdminAct::AgentUpdate {
                version: &request.version,
                sha256: &request.sha256,
            },
            (Self::Attack(request), _) => AdminAct::AttackMode {
                enable: request.active,
            },
            (Self::Own(_), _) => AdminAct::AccountPasswordOwn,
            (Self::Setting(request), _) => AdminAct::ReauthSetting {
                mode: request.password,
            },
            (Self::TargetOnly, ActKind::AccountPassword) => {
                AdminAct::AccountPassword { target: id }
            }
            (Self::TargetOnly, ActKind::AccountDelete) => AdminAct::AccountDelete { target: id },
            (Self::TargetOnly, _) => AdminAct::SessionsRevoke { target: id },
        }
    }
}

/// Le membre `reauth` d'un corps JSON : absent (`None`), ou présent et lu avec tolérance.
/// Un corps que l'enveloppe ne sait pas lire (clé `reauth` en double, par exemple) est une erreur de
/// lecture, jamais une absence : `Err`.
fn reauth_member(bytes: &[u8]) -> Result<Option<Reauth>, ()> {
    #[derive(Deserialize)]
    struct Envelope {
        #[serde(default, deserialize_with = "lenient_reauth")]
        reauth: Option<Reauth>,
    }
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(None);
    }
    serde_json::from_slice::<Envelope>(bytes)
        .map(|envelope| envelope.reauth)
        .map_err(|_| ())
}

fn marked(error: ApiError, outcome: Outcome) -> Response {
    let mut response = error.into_response();
    response.extensions_mut().insert(OutcomeMark(outcome));
    response
}

fn refusal(field: &str, reason: &str, message: &str, outcome: Outcome) -> Response {
    marked(
        ApiError(ErrorBody::with_details(
            ErrorCode::PostNotRecognized,
            message,
            serde_json::json!({ "field": field, "reason": reason }),
        )),
        outcome,
    )
}

/// Le refus d'une confirmation, au format de l'API, avec ce que le journal en retient.
fn refused(error: ReauthError) -> Response {
    match error {
        ReauthError::ProofMissing => refusal(
            "reauth.device",
            reauth_refusal::PROOF_MISSING,
            "Cette action exige la preuve de la clé de ce poste. Utilise un poste dont la clé est enregistrée, ou la commande sur le serveur.",
            Outcome::Denied(Reason::ProofMissing),
        ),
        ReauthError::ProofInvalid => refusal(
            "reauth.device",
            reauth_refusal::PROOF_INVALID,
            "La preuve de la clé de ce poste est absente ou invalide : redemande un défi et signe-le.",
            Outcome::Denied(Reason::ProofInvalid),
        ),
        ReauthError::PasswordRequired => refusal(
            "reauth.password",
            reauth_refusal::PASSWORD_REQUIRED,
            "Cette action demande ton mot de passe.",
            Outcome::Denied(Reason::PasswordRequired),
        ),
        ReauthError::Password(error) => password_refusal(*error),
        ReauthError::Unavailable => {
            ApiError::new(ErrorCode::NotFound, "Route inconnue").into_response()
        }
        ReauthError::Store(error) => ApiError::internal(&error).into_response(),
    }
}

/// Le mot de passe de confirmation est refusé par le chemin de la connexion : un utilisateur déjà
/// authentifié, donc « mot de passe actuel incorrect » et non un `401`. L'attente imposée est consignée
/// sous l'acte (constat C5).
fn password_refusal(error: LoginError) -> Response {
    match error {
        LoginError::InvalidCredentials => {
            ApiError::new(ErrorCode::WrongPassword, "Mot de passe actuel incorrect.")
                .into_response()
        }
        LoginError::TooManyAttempts { retry_after } => {
            let seconds = retry_after_seconds(retry_after);
            marked(
                ApiError::from(LoginError::TooManyAttempts { retry_after }),
                Outcome::Denied(Reason::TooManyAttempts {
                    retry_after_s: seconds,
                }),
            )
        }
        LoginError::Busy => marked(
            ApiError::from(LoginError::Busy),
            Outcome::Failed(Reason::Busy),
        ),
        other => ApiError::from(other).into_response(),
    }
}

/// Le client est trop ancien : l'acte arrive sans `reauth` à un agent qui l'exige.
fn too_old() -> Response {
    marked(
        ApiError(ErrorBody::with_details(
            ErrorCode::IncompatibleVersion,
            "Le client est trop ancien pour cette action. Mets à jour le client sur ce PC.",
            serde_json::json!({
                "upgrade": UpgradeTarget::Client,
                "reason": reauth_refusal::REAUTH_REQUIRED,
            }),
        )),
        Outcome::Denied(Reason::ReauthMissing),
    )
}

/// La couche. `request` porte déjà `Caller` et `Requester` (posés par `auth::guard`).
pub async fn layer(State(state): State<ReauthState>, request: Request, next: Next) -> Response {
    let (mut parts, body) = request.into_parts();
    let (Some(caller), Some(_requester)) = (
        parts.extensions.get::<Caller>().cloned(),
        parts.extensions.get::<Requester>().cloned(),
    ) else {
        return ApiError::internal(&"route d'acte sans couche d'accès").into_response();
    };
    let Some(kind) = state.route.kinds.first().copied() else {
        return next.run(Request::from_parts(parts, body)).await;
    };
    let bytes = match to_bytes(body, MAX_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => return ApiError::invalid("body", "Corps de requête illisible").into_response(),
    };
    let sessions = state.app.sessions.clone();
    let required = sessions.reauth_required() || kind == ActKind::ReauthSetting;
    let client = ClientInfo {
        name: client_name(&parts.headers),
        addr: origin_of(&parts).addr().unwrap_or_default().to_owned(),
    };
    let id = path_param(state.route.pattern, parts.uri.path(), "id").unwrap_or_default();
    // Le corps est lu avec le type du handler : illisible, il n'y a pas d'acte à confirmer.
    let Some(parsed) = Parsed::read(kind, &bytes) else {
        return ApiError::invalid("body", "Corps de requête illisible").into_response();
    };
    // Le mode attaque a deux gestes sur une route : le bon `kind` vient de `active`.
    let kind = match &parsed {
        Parsed::Attack(request) if request.active => ActKind::AttackModeEnable,
        Parsed::Attack(_) => ActKind::AttackModeDisable,
        _ => kind,
    };
    let Ok(reauth) = reauth_member(&bytes) else {
        return ApiError::invalid("body", "Corps de requête illisible").into_response();
    };
    match reauth {
        None if kind == ActKind::AccountPasswordOwn && !required => {
            // Client actuel : l'ancien mot de passe passe tout de même par les compteurs de la connexion
            // (constat C3), puis le handler fait comme avant.
            let Parsed::Own(request) = parsed else {
                return next
                    .run(Request::from_parts(parts, Body::from(bytes)))
                    .await;
            };
            let work = tokio::spawn(
                async move {
                    sessions
                        .confirm_password(
                            caller.0.account.username.as_str(),
                            Secret::from(request.current),
                            &client,
                            crate::domain::audit::AuditAction::OwnPassword,
                        )
                        .await
                }
                .in_current_span(),
            );
            let verified = match work.await {
                Ok(Ok(verified)) => verified,
                Ok(Err(error)) => return password_refusal(error),
                Err(error) => return ApiError::internal(&error).into_response(),
            };
            parts
                .extensions
                .insert(PasswordConfirmed(Arc::new(verified)));
            next.run(Request::from_parts(parts, Body::from(bytes)))
                .await
        }
        // Le mode attaque garde sa forme à plat (usage `0x03`) : le handler vérifie preuve et mot de
        // passe lui-même, aussi strictement.
        None if matches!(kind, ActKind::AttackModeEnable | ActKind::AttackModeDisable) => {
            next.run(Request::from_parts(parts, Body::from(bytes)))
                .await
        }
        None if !required => {
            next.run(Request::from_parts(parts, Body::from(bytes)))
                .await
        }
        None => too_old(),
        Some(reauth) => {
            // Changer son mot de passe : l'ancien mot de passe du corps est celui de la confirmation.
            if let Parsed::Own(request) = &parsed
                && !reauth.password.is_empty()
                && request.current != reauth.password
            {
                return ApiError::invalid("current", "L'ancien mot de passe ne correspond pas")
                    .into_response();
            }
            let Some(token) = bearer_token(&parts.headers) else {
                return ApiError::new(
                    ErrorCode::Unauthenticated,
                    "Jeton de session absent ou illisible",
                )
                .into_response();
            };
            // La confirmation va jusqu'au bout même si le client coupe : un mot de passe faux est
            // toujours compté. Tâche détachée, dans le span de la requête.
            let session = caller.0.clone();
            let work = tokio::spawn(
                async move {
                    let act = parsed.act(kind, &id);
                    sessions
                        .reauthenticate(&session, &token, &act, &reauth, &client)
                        .await
                }
                .in_current_span(),
            );
            let confirmed = match work.await {
                Ok(Ok(confirmed)) => confirmed,
                Ok(Err(error)) => return refused(error),
                Err(error) => return ApiError::internal(&error).into_response(),
            };
            parts.extensions.insert(confirmed.clone());
            if kind == ActKind::AccountPasswordOwn
                && let Some(verified) = confirmed.verified_hash()
            {
                // Jamais couvert par l'élévation : le mot de passe vient d'être vérifié.
                parts
                    .extensions
                    .insert(PasswordConfirmed(Arc::new(Secret::new(
                        verified.expose().to_owned(),
                    ))));
            }
            next.run(Request::from_parts(parts, Body::from(bytes)))
                .await
        }
    }
}
