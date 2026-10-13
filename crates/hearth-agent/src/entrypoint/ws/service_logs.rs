//! Le socket d'UNE lecture de journaux, `GET /api/v1/services/{key}/logs` (HRT-66, ADR-0038).
//!
//! Un socket par lecture, jamais le flux commun : un client lent ne peut ainsi faire fermer que sa propre
//! lecture (et seulement s'il ne reçoit plus rien pendant `send_timeout`), jamais `/stream`. Fermer le
//! socket abandonne la `LogSession`, donc la tâche qui lit le moteur, et rend la place (dix lectures).
//!
//! Déroulement : `auth` (même forme que `/stream`), `open`, puis les messages de `hearth_proto::api::service_logs`.
//! Le texte d'une ligne ne passe par aucune trace d'ici : seuls des nombres sont tracés.
//! Lire n'est pas journalisé (BR-SLOG-022) : la route n'a pas d'action d'audit. Seul un REFUS faute du droit
//! sur un service adopté est inscrit (BR-SLOG-024, `journal_refusal`), une fois par lecture refusée.

use axum::extract::ws::{Message, WebSocket, close_code};
use hearth_proto::api::service_logs::{
    LogsClientMessage, LogsError, LogsErrorCode, LogsServerMessage,
};
use hearth_proto::stream::SignedAuth;
use tokio::time::{Instant, MissedTickBehavior, interval_at, sleep, timeout};

use super::connection::{Closing, close, going_away, policy, send, stopped};
use super::{Permit, StreamSettings};
use crate::application::service_logs::OpenRequest;
use crate::application::sessions::{AuthError, CurrentSession};
use crate::domain::audit::{Actor, AuditAction, ClientName, Origin, Outcome, Reason, Target};
use crate::domain::secret::Secret;
use crate::domain::services::ServiceKey;
use crate::domain::services::logs::Viewer;
use crate::entrypoint::http::AppState;

/// Sert la lecture jusqu'à sa fin, puis envoie la trame de fermeture.
pub async fn run(
    mut socket: WebSocket,
    state: AppState,
    permit: Permit,
    addr: String,
    key: ServiceKey,
) {
    let closing = serve(&mut socket, &state, permit, &addr, key).await;
    close(&mut socket, &state, closing).await;
}

fn normal() -> Option<Closing> {
    Some(Closing {
        code: close_code::NORMAL,
        reason: "lecture terminée",
    })
}

/// Le compte tel que la session le montre (la décision de lecture ne se fonde jamais sur un rôle copié).
fn viewer_of(session: &CurrentSession) -> Viewer {
    Viewer {
        account: session.account.id.clone(),
        role: session.account.role,
        services_right: session.account.services_right,
    }
}

fn error(code: LogsErrorCode) -> LogsServerMessage {
    LogsServerMessage::Error(LogsError { code })
}

async fn serve(
    socket: &mut WebSocket,
    state: &AppState,
    permit: Permit,
    addr: &str,
    key: ServiceKey,
) -> Option<Closing> {
    let settings = state.stream.settings;
    let mut shutdown = state.stream.shutdown_signal();

    // 1. Authentification : le premier message, dans le délai imparti.
    let (session, token) = tokio::select! {
        outcome = timeout(settings.auth_timeout, authenticate(socket, state, addr)) => match outcome {
            Ok(Ok(authenticated)) => authenticated,
            Ok(Err(closing)) => return closing,
            Err(_) => {
                send(socket, &settings, &error(LogsErrorCode::Unauthenticated)).await;
                return policy("authentification attendue");
            }
        },
        () = stopped(&mut shutdown) => return going_away(),
    };

    // 2. `open`, dans le même délai.
    let request = tokio::select! {
        outcome = timeout(settings.auth_timeout, read_open(socket, state)) => match outcome {
            Ok(Ok(request)) => request,
            Ok(Err(closing)) => return closing,
            Err(_) => {
                send(socket, &settings, &error(LogsErrorCode::BadRequest)).await;
                return policy("open attendu");
            }
        },
        () = stopped(&mut shutdown) => return going_away(),
    };

    // La place d'attente d'authentification est gardée jusqu'ici (un compte authentifié qui n'ouvre rien ne
    // multiplie pas ses sockets pendant le délai), puis rendue : une lecture est comptée par ses propres
    // dix places.
    drop(permit);

    // 3. La lecture. Les refus connus d'avance sont des erreurs du socket, pas de la mise à niveau.
    let mut reading = match state
        .service_logs
        .open(viewer_of(&session), key.clone(), request)
    {
        Ok(reading) => reading,
        Err(code) => {
            if code == LogsErrorCode::MissingRight {
                journal_refusal(state, &session, addr, &key).await;
            }
            send(socket, &settings, &error(code)).await;
            return normal();
        }
    };
    tracing::info!("lecture de journaux ouverte");

    let idle = sleep(settings.idle_timeout);
    tokio::pin!(idle);
    let mut check = interval_at(
        Instant::now() + settings.session_check_period,
        settings.session_check_period,
    );
    check.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let closing = loop {
        tokio::select! {
            () = stopped(&mut shutdown) => break going_away(),
            () = &mut idle => break policy("silence du client"),
            received = socket.recv() => {
                let message = match received {
                    Some(Ok(message)) => message,
                    Some(Err(_)) => break policy("message refusé"),
                    None => break None,
                };
                idle.as_mut().reset(Instant::now() + settings.idle_timeout);
                match message {
                    Message::Close(_) => break None,
                    Message::Ping(_) | Message::Pong(_) => {}
                    Message::Binary(_) => {
                        send(socket, &settings, &error(LogsErrorCode::BadRequest)).await;
                        break normal();
                    }
                    Message::Text(text) => match serde_json::from_str::<LogsClientMessage>(text.as_str()) {
                        Ok(LogsClientMessage::Ping { n }) => {
                            if !send(socket, &settings, &LogsServerMessage::Pong { n }).await {
                                break None;
                            }
                        }
                        // Une deuxième ouverture, ou un message illisible : la lecture est mal pilotée.
                        Ok(LogsClientMessage::Open { .. }) | Err(_) => {
                            send(socket, &settings, &error(LogsErrorCode::BadRequest)).await;
                            break normal();
                        }
                    },
                }
            }
            message = reading.next_message() => match message {
                Some(message) => {
                    let terminal = matches!(message, LogsServerMessage::Error(_));
                    // Le droit retiré pendant la lecture est un refus comme un autre (BR-SLOG-024).
                    if matches!(&message, LogsServerMessage::Error(LogsError { code: LogsErrorCode::MissingRight })) {
                        journal_refusal(state, &session, addr, &key).await;
                    }
                    // Un client qui ne reçoit plus rien pendant `send_timeout` perd SA lecture, pas le lien.
                    if !send(socket, &settings, &message).await {
                        break None;
                    }
                    if terminal {
                        break normal();
                    }
                }
                None => break normal(),
            },
            _ = check.tick() => {
                match state.sessions.authenticate_at(token.expose(), addr).await {
                    // Le compte tel que la session le montre maintenant : la lecture rejoue sa décision
                    // de lecture tout de suite s'il a changé (rôle changé pendant la lecture).
                    Ok(current) => reading.set_viewer(viewer_of(&current)),
                    // Session expirée, révoquée, compte supprimé ou désactivé, ou mode attaque : la
                    // lecture se ferme sur l'erreur d'authentification (comme `/stream`, qui annonce la
                    // fin de session avant de fermer).
                    Err(AuthError::Ended(_) | AuthError::NotRecognized) => {
                        send(socket, &settings, &error(LogsErrorCode::Unauthenticated)).await;
                        break policy("session terminée");
                    }
                    Err(error) => tracing::warn!(%error, "vérification de la session des journaux impossible"),
                }
            }
        }
    };
    tracing::info!(lost = reading.lost(), "lecture de journaux fermée");
    closing
}

/// Inscrit au journal d'activité le refus de lire les journaux d'un service adopté faute du droit
/// (BR-SLOG-024, BR-GRANT-004) : qui, quel service (nom et unité, déjà dans sa liste), d'où, résultat
/// « Refusé : droits insuffisants ». Jamais pour un conteneur non adopté ni une clé inconnue, qui n'ont
/// pas de refus explicite. Un échec d'écriture est tracé par le puits, jamais subi par l'appelant.
async fn journal_refusal(state: &AppState, session: &CurrentSession, addr: &str, key: &ServiceKey) {
    let target = match state.services.published_label(key) {
        Some((name, unit)) => {
            ClientName::parse(&name).map_or(Target::Route("/services/{key}/logs"), |name| {
                Target::ServiceUnit(name, unit.and_then(|unit| ClientName::parse(unit.as_str())))
            })
        }
        None => Target::Route("/services/{key}/logs"),
    };
    let actor = Actor::new(
        Some(session.account.username.clone()),
        Origin::client(None, addr),
    );
    state
        .sink
        .record(
            actor,
            AuditAction::ServiceLogsRead,
            target,
            Outcome::Denied(Reason::InsufficientRights),
        )
        .await;
}

/// Lit le premier message : `auth` avec un jeton valable. Sinon une erreur du socket, puis la fermeture.
async fn authenticate(
    socket: &mut WebSocket,
    state: &AppState,
    addr: &str,
) -> Result<(CurrentSession, Secret), Option<Closing>> {
    let settings = state.stream.settings;
    let text = loop {
        match socket.recv().await {
            Some(Ok(Message::Text(text))) => break text,
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
            Some(Ok(Message::Binary(_))) => {
                return Err(refuse(socket, &state.stream.settings).await);
            }
            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return Err(None),
        }
    };
    let Ok(SignedAuth::Auth { token, device }) = serde_json::from_str::<SignedAuth>(text.as_str())
    else {
        return Err(refuse(socket, &state.stream.settings).await);
    };
    let token = Secret::new(token);
    let authenticated = match &device {
        Some(proof) => {
            state
                .sessions
                .authenticate_proved(token.expose(), addr, proof)
                .await
        }
        None => state.sessions.authenticate_at(token.expose(), addr).await,
    };
    match authenticated {
        Ok(session) => Ok((session, token)),
        Err(_) => {
            send(socket, &settings, &error(LogsErrorCode::Unauthenticated)).await;
            Err(policy("authentification refusée"))
        }
    }
}

/// Premier message qui n'est pas `auth`.
async fn refuse(socket: &mut WebSocket, settings: &StreamSettings) -> Option<Closing> {
    send(socket, settings, &error(LogsErrorCode::Unauthenticated)).await;
    policy("authentification attendue")
}

/// Lit `open` (un `ping` avant lui reçoit son `pong`). Tout autre message est une mauvaise demande.
async fn read_open(
    socket: &mut WebSocket,
    state: &AppState,
) -> Result<OpenRequest, Option<Closing>> {
    let settings = state.stream.settings;
    loop {
        match socket.recv().await {
            Some(Ok(Message::Text(text))) => {
                match serde_json::from_str::<LogsClientMessage>(text.as_str()) {
                    Ok(LogsClientMessage::Open { tail, since }) => {
                        return Ok(OpenRequest { tail, since });
                    }
                    Ok(LogsClientMessage::Ping { n }) => {
                        if !send(socket, &settings, &LogsServerMessage::Pong { n }).await {
                            return Err(None);
                        }
                    }
                    Err(_) => {
                        send(socket, &settings, &error(LogsErrorCode::BadRequest)).await;
                        return Err(normal());
                    }
                }
            }
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
            Some(Ok(Message::Binary(_))) => {
                send(socket, &settings, &error(LogsErrorCode::BadRequest)).await;
                return Err(normal());
            }
            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return Err(None),
        }
    }
}
