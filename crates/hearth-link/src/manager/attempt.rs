//! Une tentative de connexion, la reconnexion silencieuse et la relecture d'une opération.
//! Chacune tourne dans sa propre tâche, abandonnable ; aucune ne touche à la machine à états :
//! elles rendent un résultat, la tâche du serveur en fait des événements.

use std::time::Duration;

use hearth_proto::api::machine::MachineResponse;
use hearth_proto::api::metrics::Sample;
use hearth_proto::api::sessions::LoginRequest;
use hearth_proto::error::{ErrorCode, UpgradeTarget};
use hearth_proto::fingerprint::Fingerprint;
use hearth_proto::stream::{ClientMessage, ServerMessage, SessionNotice, Topic};
use tokio::time::timeout;
use zeroize::Zeroize;

use super::{Deps, Shared};
use crate::domain::pending_ops::{Lookup, OperationId};
use crate::domain::secret::Secret;
use crate::ports::transport::{Frame, StreamConn, TransportError};
use crate::ports::vault::SecretKind;

pub(crate) enum AttemptResult {
    /// Flux ouvert, authentifié, instantané reçu.
    Ready {
        stream: Box<dyn StreamConn>,
        machine: Box<MachineResponse>,
        history: Vec<Sample>,
    },
    /// Échec passager (réseau, délai, serveur occupé…) : on réessaiera.
    Failed,
    SessionExpired,
    Revoked,
    Fingerprint {
        presented: Fingerprint,
    },
    Incompatible(UpgradeTarget),
    /// La reconnexion silencieuse a rouvert une session.
    Reauthenticated,
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
            Some(ErrorCode::SessionRevoked | ErrorCode::InvalidCredentials) => {
                AttemptResult::Revoked
            }
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
    let token = match deps.vault.get(&id, SecretKind::Token) {
        Ok(Some(token)) => token,
        Ok(None) => return AttemptResult::SessionExpired,
        Err(error) => {
            tracing::warn!(server = %id, %error, "coffre illisible");
            return AttemptResult::Failed;
        }
    };
    let mut stream = match deps.transport.open_stream(&shared.target()).await {
        Ok(stream) => stream,
        Err(error) => return classify(&error),
    };
    // Les erreurs d'envoi sont lues par `recv` : un refus d'authentification arrive en message
    // `error` avant la fermeture, et c'est lui qu'on veut.
    let auth = ClientMessage::Auth {
        token: token.expose().to_owned(),
    };
    let _ = stream.send(&auth).await;
    // La copie du jeton faite pour le message est effacée dès qu'il est parti.
    if let ClientMessage::Auth { mut token } = auth {
        token.zeroize();
    }
    let mut topics = vec![Topic::Metrics, Topic::Session];
    if deps.config.subscribe_audit {
        topics.push(Topic::Audit);
    }
    let _ = stream.send(&ClientMessage::Subscribe { topics }).await;
    loop {
        match stream.recv().await {
            Ok(Frame::Message(message)) => match *message {
                ServerMessage::Snapshot { machine, history } => {
                    return AttemptResult::Ready {
                        stream,
                        machine: Box::new(machine),
                        history,
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
        reauthenticate_inner(deps, shared),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => AttemptResult::Failed,
    }
}

async fn reauthenticate_inner(deps: &Deps, shared: &Shared) -> AttemptResult {
    let id = shared.id();
    let password = match deps.vault.get(&id, SecretKind::Password) {
        Ok(Some(password)) => password,
        Ok(None) => return AttemptResult::SessionExpired,
        Err(error) => {
            tracing::warn!(server = %id, %error, "coffre illisible");
            return AttemptResult::Failed;
        }
    };
    let request = login_request(&shared.record().username, &password);
    let outcome = deps.transport.login(&shared.target(), &request).await;
    wipe(request);
    match outcome {
        Ok(mut response) => {
            // Le jeton passe dans un `Secret` sans copie : la réponse n'en garde rien.
            let token = Secret::new(std::mem::take(&mut response.token));
            match deps.vault.put(&id, SecretKind::Token, &token) {
                Ok(()) => AttemptResult::Reauthenticated,
                Err(error) => {
                    tracing::warn!(server = %id, %error, "jeton non mémorisé");
                    AttemptResult::Failed
                }
            }
        }
        // Un mot de passe refusé ne se réessaie pas : l'accès est révoqué.
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
