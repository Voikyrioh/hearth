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
//! Il n'existe aucun régime « accepte sans exiger », même en test : la couche n'a qu'une issue pour un acte
//! sans `reauth`, le `426`, et chaque handler d'acte exige `Extension<Reauthenticated>` (BR-TRUST-045).

use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use hearth_proto::admin_act::{ActKind, AdminAct, RouteAct, decode_segment};
use hearth_proto::api::accounts::{
    ChangeOwnPasswordRequest, ChangeRoleRequest, CreateAccountRequest, RoleName,
};
use hearth_proto::api::deploy::DeploySpec;
use hearth_proto::api::game_settings::{
    ChangeAction, DriftResolution, SaveOrigin, SaveSettingsRequest, SettingEffect,
};
use hearth_proto::api::reauth::{
    Reauth, ReauthMode, SetReauthRequest, lenient_reauth, reauth_refusal,
};
use hearth_proto::api::security::SetAttackModeRequest;
use hearth_proto::api::service_delete::DeleteRequest;
use hearth_proto::api::service_ops::{OperationChoice, OperationDecision};
use hearth_proto::api::services::AdoptRequest;
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
    /// Adopter : le corps porte les clés (HRT-80), dont le nombre et l'empreinte sont signés.
    Adopt(AdoptRequest),
    /// Décider d'une opération (HRT-144) : le choix du corps est signé avec l'identifiant du chemin.
    Decide(OperationDecision),
    /// Déployer (HRT-68) : l'acte est construit UNE fois depuis le corps (`AdminAct::service_deploy`, la même
    /// fonction que la liaison cliente) : le nom, `dépôt:version`, l'empreinte de la partie non secrète, le
    /// nombre de variables, la présence d'une commande. Le corps ne reste pas en mémoire ici.
    Deploy(Box<DeployAct>),
    /// Enregistrer des réglages (HRT-108) : la révision de base, le nombre de changements, l'empreinte de leurs
    /// identifiants, l'origine et l'effet sont signés (`AdminAct::game_settings_save`, la fonction de la liaison
    /// cliente) ; jamais une valeur.
    Settings(Box<SaveSettingsRequest>),
    /// Supprimer un service (HRT-85) : la clé du chemin est la cible, les deux cases du corps sont les paramètres
    /// signés (`BR-DEL-012`). Le nom retapé n'est pas signé : le cas d'usage le compare à celui qu'il connaît.
    Delete(DeleteRequest),
    /// Acte dont la cible est dans le chemin et dont aucun paramètre ne vient du corps.
    TargetOnly,
}

/// L'acte d'un déploiement, rendu possédé : ni valeur, ni nom de variable, ni commande n'y sont gardés.
struct DeployAct {
    name: String,
    image: String,
    digest: [u8; hearth_proto::admin_act::FINGERPRINT_LEN],
    env_count: u32,
    command: bool,
}

impl Parsed {
    /// `None` : corps illisible pour ce que la route attend.
    fn read(kind: ActKind, bytes: &[u8]) -> Option<Self> {
        // FIX:01M4VR6Y4Q4AVQVT9FCME7Z2SB : une valeur d'énumération que cette version ne connaît pas (client
        // plus récent) rend le corps illisible pour la route : refus de validation avant toute confirmation.
        match kind {
            ActKind::AccountCreate => serde_json::from_slice(bytes)
                .ok()
                .filter(|request: &CreateAccountRequest| request.role != RoleName::Unknown)
                .map(Self::Create),
            ActKind::AccountRole => serde_json::from_slice(bytes)
                .ok()
                .filter(|request: &ChangeRoleRequest| request.role != RoleName::Unknown)
                .map(Self::Role),
            ActKind::AgentUpdate => serde_json::from_slice(bytes).ok().map(Self::Update),
            ActKind::AttackModeEnable | ActKind::AttackModeDisable => {
                serde_json::from_slice(bytes).ok().map(Self::Attack)
            }
            ActKind::AccountPasswordOwn => serde_json::from_slice(bytes).ok().map(Self::Own),
            ActKind::ReauthSetting => serde_json::from_slice(bytes)
                .ok()
                .filter(|request: &SetReauthRequest| request.password != ReauthMode::Unknown)
                .map(Self::Setting),
            ActKind::ServicesAdopt => serde_json::from_slice(bytes).ok().map(Self::Adopt),
            // Un choix inconnu (client plus récent, ou texte inventé) n'est pas un acte : refus de validation
            // avant toute confirmation, jamais un acte signé avec un choix par défaut (BR-COMPAT-001 point 5).
            ActKind::OperationDecide => serde_json::from_slice(bytes)
                .ok()
                .filter(|request: &OperationDecision| request.choice != OperationChoice::Unknown)
                .map(Self::Decide),
            ActKind::AccountPassword
            | ActKind::AccountDelete
            | ActKind::SessionsRevoke
            | ActKind::ServiceRelease
            | ActKind::ServiceStart
            | ActKind::ServiceStop
            | ActKind::ServiceRestart
            | ActKind::OperationCancel => Some(Self::TargetOnly),
            ActKind::ServiceDeploy => {
                serde_json::from_slice::<DeploySpec>(bytes)
                    .ok()
                    .and_then(|spec| match AdminAct::service_deploy(&spec) {
                        AdminAct::ServiceDeploy {
                            name,
                            image,
                            digest,
                            env_count,
                            command,
                        } => Some(Self::Deploy(Box::new(DeployAct {
                            name: name.into_owned(),
                            image: image.into_owned(),
                            digest,
                            env_count,
                            command,
                        }))),
                        _ => None,
                    })
            }
            // Enregistrer des réglages : un mot que cette version ne connaît pas (origine, effet, geste, choix) rend le
            // corps illisible, jamais un acte signé avec un repli (BR-COMPAT-001 point 5).
            ActKind::GameSettingsSave => serde_json::from_slice::<SaveSettingsRequest>(bytes)
                .ok()
                .filter(|request| {
                    request.origin != SaveOrigin::Unknown
                        && request.effect != SettingEffect::Unknown
                        && request
                            .changes
                            .iter()
                            .all(|change| change.action != ChangeAction::Unknown)
                        && request
                            .drift_choices
                            .iter()
                            .all(|choice| choice.choice != DriftResolution::Unknown)
                })
                .map(|request| Self::Settings(Box::new(request))),
            // Supprimer : un corps dont une case n'est pas un booléen est illisible, jamais un acte signé avec une
            // case décochée par défaut.
            ActKind::ServiceDelete => {
                if bytes.iter().all(u8::is_ascii_whitespace) {
                    return Some(Self::Delete(DeleteRequest::default()));
                }
                serde_json::from_slice(bytes).ok().map(Self::Delete)
            }
            // Déclarés pour les tranches suivantes : aucune route ne les porte, donc aucun corps à lire.
            ActKind::BackupCreate | ActKind::BackupDelete => None,
        }
    }

    /// L'acte reconstruit : sa cible vient du chemin, ses paramètres du corps lu. `None` : ce corps ne
    /// correspond pas à ce genre d'acte (jamais atteint par `read`, mais jamais rattrapé par un autre acte :
    /// pas de repli, par exemple vers la révocation de sessions).
    fn act<'a>(&'a self, kind: ActKind, id: &'a str) -> Option<AdminAct<'a>> {
        Some(match (self, kind) {
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
            (Self::Adopt(request), _) => {
                AdminAct::services_adopt(request.keys.iter().map(String::as_str))
            }
            (Self::TargetOnly, ActKind::ServiceRelease) => {
                AdminAct::ServiceRelease { target: id.into() }
            }
            (Self::TargetOnly, ActKind::ServiceStart) => {
                AdminAct::ServiceStart { target: id.into() }
            }
            (Self::TargetOnly, ActKind::ServiceStop) => AdminAct::ServiceStop { target: id.into() },
            (Self::TargetOnly, ActKind::ServiceRestart) => {
                AdminAct::ServiceRestart { target: id.into() }
            }
            (Self::TargetOnly, ActKind::OperationCancel) => {
                AdminAct::OperationCancel { target: id }
            }
            (Self::Deploy(deploy), _) => AdminAct::ServiceDeploy {
                name: deploy.name.as_str().into(),
                image: deploy.image.as_str().into(),
                digest: deploy.digest,
                env_count: deploy.env_count,
                command: deploy.command,
            },
            (Self::Settings(request), _) => {
                AdminAct::game_settings_save(std::borrow::Cow::Borrowed(id), request)
            }
            (Self::Delete(request), _) => AdminAct::ServiceDelete {
                target: id.into(),
                data: request.erase_data,
                backups: request.erase_backups,
            },
            (Self::Decide(request), _) => AdminAct::OperationDecide {
                target: id,
                choice: request.choice,
            },
            (Self::TargetOnly, ActKind::AccountPassword) => {
                AdminAct::AccountPassword { target: id }
            }
            (Self::TargetOnly, ActKind::AccountDelete) => AdminAct::AccountDelete { target: id },
            (Self::TargetOnly, ActKind::SessionsRevoke) => AdminAct::SessionsRevoke { target: id },
            // Les autres genres lisent un corps (autre variante) ou n'ont aucune route : jamais « cible seule ».
            // Liste exhaustive, sans `_` : un genre ajouté à `ActKind` oblige à choisir ici.
            (
                Self::TargetOnly,
                ActKind::AccountCreate
                | ActKind::AccountRole
                | ActKind::AccountPasswordOwn
                | ActKind::AgentUpdate
                | ActKind::AttackModeEnable
                | ActKind::AttackModeDisable
                | ActKind::ReauthSetting
                | ActKind::ServicesAdopt
                | ActKind::OperationDecide
                | ActKind::ServiceDeploy
                | ActKind::ServiceDelete
                | ActKind::BackupCreate
                | ActKind::BackupDelete
                | ActKind::GameSettingsSave,
            ) => return None,
        })
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
    let client = ClientInfo {
        name: client_name(&parts.headers),
        addr: origin_of(&parts).addr().unwrap_or_default().to_owned(),
    };
    // La cible vient du chemin : l'identifiant d'un compte (`{id}`) ou la clé d'un service (`{key}`, décodée
    // comme la lit le routeur ; un encodage illisible laisse la cible vide, donc une preuve qui ne colle pas).
    let id = path_param(state.route.pattern, parts.uri.path(), "id")
        .or_else(|| {
            path_param(state.route.pattern, parts.uri.path(), "key")
                .and_then(|raw| decode_segment(&raw))
        })
        .unwrap_or_default();
    // Le corps est lu avec le type du handler : illisible, il n'y a pas d'acte à confirmer.
    let Some(parsed) = Parsed::read(kind, &bytes) else {
        return ApiError::invalid("body", "Corps de requête illisible").into_response();
    };
    // Le corps lu doit correspondre au genre d'acte de la route : sinon il n'y a pas d'acte à confirmer.
    if parsed.act(kind, &id).is_none() {
        return ApiError::invalid("body", "Corps de requête illisible").into_response();
    }
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
                    let Some(act) = parsed.act(kind, &id) else {
                        return Err(ReauthError::Unavailable);
                    };
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn body(value: serde_json::Value) -> Vec<u8> {
        serde_json::to_vec(&value).expect("json")
    }

    /// FIX:01M4VR6Y4Q4AVQVT9FCME7Z2SB : un client plus récent envoie une valeur que cette version ne
    /// connaît pas dans une commande. Le corps est illisible pour la route (refus de validation typé,
    /// avant toute confirmation), jamais un acte reconstruit avec une valeur par défaut.
    #[test]
    fn a_command_carrying_a_value_this_version_does_not_know_is_unreadable_for_its_route() {
        let create = body(json!({ "username": "marie", "password": "x", "role": "futur" }));
        assert!(Parsed::read(ActKind::AccountCreate, &create).is_none());
        let role = body(json!({ "role": "futur" }));
        assert!(Parsed::read(ActKind::AccountRole, &role).is_none());
        let decision = body(json!({ "choice": "futur" }));
        assert!(Parsed::read(ActKind::OperationDecide, &decision).is_none());
        let unknown_word = body(json!({ "choice": "unknown" }));
        assert!(Parsed::read(ActKind::OperationDecide, &unknown_word).is_none());
        let setting = body(json!({ "password": "futur" }));
        assert!(Parsed::read(ActKind::ReauthSetting, &setting).is_none());
        // Les valeurs connues et un champ en trop restent lus.
        let known =
            body(json!({ "username": "marie", "password": "x", "role": "admin", "futur": 1 }));
        assert!(Parsed::read(ActKind::AccountCreate, &known).is_some());
        assert!(
            Parsed::read(ActKind::ReauthSetting, &body(json!({ "password": "each" }))).is_some()
        );
        assert!(
            Parsed::read(ActKind::OperationDecide, &body(json!({ "choice": "keep" }))).is_some()
        );
    }

    /// Un genre d'acte ne se reconstruit jamais d'un corps d'un autre genre, et « cible seule » ne retombe
    /// jamais sur la révocation de sessions (revue de la PR 94).
    #[test]
    fn a_target_only_body_never_stands_for_an_act_that_reads_a_body() {
        for kind in ActKind::ALL {
            let act = Parsed::TargetOnly.act(kind, "x");
            let target_only = matches!(
                kind,
                ActKind::AccountPassword
                    | ActKind::AccountDelete
                    | ActKind::SessionsRevoke
                    | ActKind::ServiceRelease
                    | ActKind::ServiceStart
                    | ActKind::ServiceStop
                    | ActKind::ServiceRestart
                    | ActKind::OperationCancel
            );
            assert_eq!(act.is_some(), target_only, "{kind:?}");
            if let Some(act) = act {
                assert_eq!(act.kind(), kind, "{kind:?}");
            }
        }
    }
}
