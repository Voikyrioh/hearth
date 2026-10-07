//! Une tentative de connexion, la reconnexion silencieuse et la relecture d'une opération.
//! Chacune tourne dans sa propre tâche, abandonnable ; aucune ne touche à la machine à états :
//! elles rendent un résultat, la tâche du serveur en fait des événements.

use std::time::Duration;

use hearth_proto::api::machine::MachineResponse;
use hearth_proto::api::metrics::Sample;
use hearth_proto::api::sessions::LoginRequest;
use hearth_proto::api::update::UpdateProgress;
use hearth_proto::error::{ErrorCode, UpgradeTarget};
use hearth_proto::fingerprint::Fingerprint;
use hearth_proto::stream::{ClientMessage, ServerMessage, SessionNotice, SignedAuth, Topic};
use tokio::time::timeout;
use zeroize::Zeroize;

use super::{Deps, Shared, device};
use crate::domain::pending_ops::{Lookup, OperationId};
use crate::domain::secret::Secret;
use crate::ports::transport::{Frame, StreamConn, TransportError};
use crate::ports::vault::SecretKind;

/// Messages de mise à jour retenus au plus pendant l'attente de l'instantané.
const MAX_EARLY_UPDATES: usize = 8;

pub(crate) enum AttemptResult {
    /// Flux ouvert, authentifié, instantané reçu.
    Ready {
        stream: Box<dyn StreamConn>,
        machine: Box<MachineResponse>,
        history: Vec<Sample>,
        /// État de la mise à jour de l'agent reçu AVANT l'instantané (l'ordre des sujets est celui
        /// de l'agent) : il n'est pas perdu.
        updates: Vec<UpdateProgress>,
    },
    /// Échec passager (réseau, délai, serveur occupé…) : on réessaiera.
    Failed,
    SessionExpired,
    Revoked,
    /// Le mot de passe mémorisé est refusé (reconnexion silencieuse).
    StoredPasswordRefused,
    /// Le serveur demande d'attendre avant de réessayer (`429`, `503`).
    RetryAfter(Duration),
    Fingerprint {
        presented: Fingerprint,
    },
    Incompatible(UpgradeTarget),
    /// La reconnexion silencieuse a rouvert une session ; `role` est celui que le serveur donne
    /// maintenant au compte (un administrateur rétrogradé le perd sans nouvelle saisie).
    Reauthenticated {
        role: hearth_proto::api::accounts::RoleName,
    },
}

/// Classe l'échec d'une ouverture de connexion ou d'une requête.
fn classify(error: &TransportError) -> AttemptResult {
    match error {
        TransportError::FingerprintMismatch { presented } => AttemptResult::Fingerprint {
            presented: *presented,
        },
        TransportError::Api(api) => match api.code {
            Some(ErrorCode::IncompatibleVersion) => {
                AttemptResult::Incompatible(upgrade_target(&api.details))
            }
            Some(ErrorCode::SessionExpired | ErrorCode::Unauthenticated) => {
                AttemptResult::SessionExpired
            }
            Some(ErrorCode::SessionRevoked) => AttemptResult::Revoked,
            Some(ErrorCode::InvalidCredentials) => AttemptResult::StoredPasswordRefused,
            Some(ErrorCode::TooManyAttempts | ErrorCode::Busy) => match api.retry_after_s {
                // Plafonné : un serveur détraqué ne nous endort pas pour des jours.
                Some(seconds) => AttemptResult::RetryAfter(Duration::from_secs(seconds.min(3_600))),
                None => AttemptResult::Failed,
            },
            _ if api.status == 426 => AttemptResult::Incompatible(upgrade_target(&api.details)),
            _ => AttemptResult::Failed,
        },
        _ => AttemptResult::Failed,
    }
}

fn upgrade_target(details: &serde_json::Value) -> UpgradeTarget {
    details
        .get("upgrade")
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or(UpgradeTarget::Client)
}

/// Ouvre le flux, s'authentifie avec le jeton du coffre et attend l'instantané.
pub(crate) async fn connect(deps: &Deps, shared: &Shared) -> AttemptResult {
    match timeout(deps.config.attempt_timeout, connect_inner(deps, shared)).await {
        Ok(result) => result,
        Err(_) => AttemptResult::Failed,
    }
}

async fn connect_inner(deps: &Deps, shared: &Shared) -> AttemptResult {
    let id = shared.id();
    // Client mis à jour, session déjà ouverte, mot de passe mémorisé : ce PC s'inscrit en silence
    // (une seule fois par exécution), avant d'ouvrir le flux avec le jeton qui en résulte.
    device::enroll_silently(deps, shared).await;
    let token = match deps.vault.get(&id, SecretKind::Token) {
        Ok(Some(token)) => token,
        Ok(None) => return AttemptResult::SessionExpired,
        Err(error) => {
            tracing::warn!(server = %id, %error, "coffre illisible");
            return AttemptResult::Failed;
        }
    };
    // Le défi AVANT l'ouverture du flux : l'agent n'attend le premier message que quelques
    // secondes, et le défi vaut 60 s. Sans clé, agent ancien ou défi en échec : jeton seul.
    let proof = device::session_proof(
        deps,
        &shared.target(),
        &id,
        &shared.record().username,
        &token,
    )
    .await;
    let mut stream = match deps.transport.open_stream(&shared.target()).await {
        Ok(stream) => stream,
        Err(error) => return classify(&error),
    };
    // Les erreurs d'envoi sont lues par `recv` : un refus d'authentification arrive en message
    // `error` avant la fermeture, et c'est lui qu'on veut.
    let auth = SignedAuth::Auth {
        token: token.expose().to_owned(),
        device: proof,
    };
    let _ = stream.send_auth(&auth).await;
    // La copie du jeton faite pour le message est effacée dès qu'il est parti.
    let SignedAuth::Auth { mut token, .. } = auth;
    token.zeroize();
    // Le sujet `update` est ouvert à tout compte : le serveur annonce ainsi son redémarrage de mise
    // à jour (coupure attendue, BR-UPDATE-014) et son avancement.
    let mut topics = vec![Topic::Metrics, Topic::Session, Topic::Update];
    if deps.config.subscribe_audit {
        topics.push(Topic::Audit);
    }
    let _ = stream.send(&ClientMessage::Subscribe { topics }).await;
    let mut updates = Vec::new();
    loop {
        match stream.recv().await {
            Ok(Frame::Message(message)) => match *message {
                ServerMessage::Snapshot { machine, history } => {
                    return AttemptResult::Ready {
                        stream,
                        machine: Box::new(machine),
                        history,
                        updates,
                    };
                }
                ServerMessage::Error(detail) => match detail.code {
                    ErrorCode::SessionExpired | ErrorCode::Unauthenticated => {
                        return AttemptResult::SessionExpired;
                    }
                    ErrorCode::SessionRevoked => return AttemptResult::Revoked,
                    // Le sujet `audit` refusé à un compte lecture seule : le reste est pris.
                    ErrorCode::ForbiddenRole => {}
                    _ => return AttemptResult::Failed,
                },
                ServerMessage::Session { kind } => {
                    return match kind {
                        SessionNotice::Expired => AttemptResult::SessionExpired,
                        SessionNotice::Revoked => AttemptResult::Revoked,
                    };
                }
                // Au plus quelques-uns : un agent bavard ne remplit pas la mémoire d'une tentative.
                // Les plus récents : seul le dernier dit l'état.
                ServerMessage::Update(progress) => {
                    if updates.len() >= MAX_EARLY_UPDATES {
                        updates.remove(0);
                    }
                    updates.push(progress);
                }
                _ => {}
            },
            Ok(Frame::Other) => {}
            Err(_) => return AttemptResult::Failed,
        }
    }
}

/// Requête de connexion dont le mot de passe est effacé après usage.
pub(crate) fn login_request(username: &str, password: &Secret) -> LoginRequest {
    LoginRequest {
        username: username.to_owned(),
        password: password.expose().to_owned(),
    }
}

pub(crate) fn wipe(mut request: LoginRequest) {
    request.password.zeroize();
}

/// Reconnexion silencieuse avec le mot de passe du coffre (BR-RESIL-013).
pub(crate) async fn reauthenticate(deps: &Deps, shared: &Shared) -> AttemptResult {
    match timeout(
        deps.config.attempt_timeout,
        reauthenticate_with_key(deps, shared),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => AttemptResult::Failed,
    }
}

pub(crate) async fn reauthenticate_with_key(deps: &Deps, shared: &Shared) -> AttemptResult {
    let id = shared.id();
    let password = match deps.vault.get(&id, SecretKind::Password) {
        Ok(Some(password)) => password,
        Ok(None) => return AttemptResult::SessionExpired,
        Err(error) => {
            tracing::warn!(server = %id, %error, "coffre illisible");
            return AttemptResult::Failed;
        }
    };
    // La reconnexion silencieuse passe elle aussi par la clé d'appareil : le poste est reconnu (ou
    // inscrit, si c'est sa première connexion par mot de passe depuis la mise à jour du client).
    let outcome = device::login(
        deps,
        &shared.target(),
        Some(&id),
        &shared.record().username,
        &password,
    )
    .await;
    match outcome {
        Ok(authenticated) => {
            let mut response = authenticated.response;
            // La clé d'abord, puis le jeton (BR-TRUST-003) ; un coffre en échec ne change rien ici.
            if let Some(key) = &authenticated.new_key {
                device::store_new_key(deps, &id, key);
            }
            // Le jeton passe dans un `Secret` sans copie : la réponse n'en garde rien.
            let token = Secret::new(std::mem::take(&mut response.token));
            match deps.vault.put(&id, SecretKind::Token, &token) {
                Ok(()) => AttemptResult::Reauthenticated {
                    role: response.account.role,
                },
                Err(error) => {
                    tracing::warn!(server = %id, %error, "jeton non mémorisé");
                    AttemptResult::Failed
                }
            }
        }
        // Mot de passe mémorisé refusé (`INVALID_CREDENTIALS`) : `StoredPasswordRefused`, pas de
        // nouvelle tentative (BR-CONN-017) ; un `429` donne `RetryAfter`.
        Err(error) => classify(&error),
    }
}

/// Demande à l'agent ce qu'est devenue une opération (BR-RESIL-010).
pub(crate) async fn lookup(deps: &Deps, shared: &Shared, id: &OperationId) -> Lookup {
    let token = match deps.vault.get(&shared.id(), SecretKind::Token) {
        Ok(Some(token)) => token,
        _ => return Lookup::Unreachable,
    };
    let limit = deps.config.request_timeout;
    match timeout(
        limit,
        deps.transport.operation(&shared.target(), &token, id),
    )
    .await
    {
        Ok(Ok(response)) => Lookup::Status {
            status: response.status,
            result: response.result,
        },
        Ok(Err(TransportError::Api(api))) if api.status == 404 => Lookup::NotFound,
        _ => Lookup::Unreachable,
    }
}

/// Exécute une tâche en capturant une éventuelle panique : elle devient un échec de transport.
pub(crate) async fn guarded<T>(
    future: impl Future<Output = Result<T, TransportError>>,
) -> Result<T, TransportError> {
    use futures_util::FutureExt as _;
    match std::panic::AssertUnwindSafe(future).catch_unwind().await {
        Ok(result) => result,
        Err(_) => Err(TransportError::Protocol("incident interne".into())),
    }
}

/// Durée d'attente entre deux relectures d'une opération « en cours ».
pub(crate) fn recheck_delay(deps: &Deps) -> Duration {
    deps.config.recheck_delay
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::ports::transport::ApiError;

    fn api(code: Option<ErrorCode>, status: u16, retry: Option<u64>) -> TransportError {
        TransportError::Api(ApiError {
            status,
            code,
            details: json!({ "upgrade": "agent" }),
            retry_after_s: retry,
        })
    }

    #[test]
    fn a_throttled_login_waits_for_the_delay_the_server_gave() {
        let result = classify(&api(Some(ErrorCode::TooManyAttempts), 429, Some(60)));
        assert!(matches!(result, AttemptResult::RetryAfter(d) if d == Duration::from_secs(60)));
        // Plafonné : un serveur détraqué ne nous endort pas pour des jours.
        let result = classify(&api(Some(ErrorCode::Busy), 503, Some(u64::MAX)));
        assert!(matches!(result, AttemptResult::RetryAfter(d) if d == Duration::from_secs(3_600)));
        // Sans délai donné : tentative ordinaire.
        assert!(matches!(
            classify(&api(Some(ErrorCode::Busy), 503, None)),
            AttemptResult::Failed
        ));
    }

    #[test]
    fn each_refusal_has_its_own_outcome() {
        assert!(matches!(
            classify(&api(Some(ErrorCode::InvalidCredentials), 401, None)),
            AttemptResult::StoredPasswordRefused
        ));
        assert!(matches!(
            classify(&api(Some(ErrorCode::SessionRevoked), 401, None)),
            AttemptResult::Revoked
        ));
        assert!(matches!(
            classify(&api(Some(ErrorCode::SessionExpired), 401, None)),
            AttemptResult::SessionExpired
        ));
        assert!(matches!(
            classify(&api(None, 426, None)),
            AttemptResult::Incompatible(UpgradeTarget::Agent)
        ));
        assert!(matches!(
            classify(&TransportError::Timeout),
            AttemptResult::Failed
        ));
        assert!(matches!(
            classify(&TransportError::FingerprintMismatch {
                presented: Fingerprint::from_bytes([1; 32])
            }),
            AttemptResult::Fingerprint { .. }
        ));
    }
}
