//! Une connexion du flux : authentification par le premier message, abonnements, envoi des
//! échantillons, battement, surveillance de la session, fermeture.
//!
//! Ne bloque jamais personne : chaque connexion est sa propre tâche, un abonné lent perd des
//! échantillons (canal de diffusion borné), un client qui ne lit plus est abandonné après
//! `send_timeout`.

use std::sync::Arc;

use axum::extract::ws::{CloseFrame, Message, WebSocket, close_code};
use hearth_proto::error::{ErrorCode, ErrorDetail};
use hearth_proto::stream::{ClientMessage, ServerMessage, SessionNotice, Topic};
use serde_json::Value;
use time::OffsetDateTime;
use tokio::sync::broadcast::{self, error::RecvError};
use tokio::time::{Instant, MissedTickBehavior, interval_at, sleep, timeout};

use super::StreamSettings;
use crate::application::sessions::{AuthError, CurrentSession};
use crate::domain::metrics::Sample;
use crate::domain::secret::Secret;
use crate::domain::sessions::SessionEnd;
use crate::domain::stream::{SNAPSHOT_WINDOW, is_new};
use crate::entrypoint::http::{ApiError, AppState};
use crate::entrypoint::metrics_wire;

/// Comment la connexion se termine : un code de fermeture et une raison courte, ou rien si le
/// client est déjà parti.
struct Closing {
    code: u16,
    reason: &'static str,
}

fn policy(reason: &'static str) -> Option<Closing> {
    Some(Closing {
        code: close_code::POLICY,
        reason,
    })
}

/// Sert la connexion jusqu'à sa fin, puis envoie la trame de fermeture.
pub async fn run(mut socket: WebSocket, state: AppState) {
    let closing = serve(&mut socket, &state).await;
    if let Some(closing) = closing {
        let frame = CloseFrame {
            code: closing.code,
            reason: closing.reason.into(),
        };
        // Le client a peut-être déjà coupé : rien à faire de l'échec.
        let _ = timeout(
            state.stream.settings.send_timeout,
            socket.send(Message::Close(Some(frame))),
        )
        .await;
    }
}

/// Envoie un message ; `false` si le client ne le reçoit plus (coupé ou trop lent).
async fn send(socket: &mut WebSocket, settings: &StreamSettings, message: &ServerMessage) -> bool {
    let Ok(text) = serde_json::to_string(message) else {
        tracing::error!("message du flux impossible à sérialiser");
        return true;
    };
    matches!(
        timeout(
            settings.send_timeout,
            socket.send(Message::Text(text.into()))
        )
        .await,
        Ok(Ok(()))
    )
}

fn error_message(code: ErrorCode, message: &str) -> ServerMessage {
    ServerMessage::Error(ErrorDetail {
        code,
        message: message.to_owned(),
        details: Value::Object(Default::default()),
    })
}

fn from_api(error: ApiError) -> ServerMessage {
    ServerMessage::Error(error.0.error)
}

/// Les abonnements de la connexion et ce qui a déjà été envoyé.
#[derive(Default)]
struct Subscriptions {
    metrics: Option<broadcast::Receiver<Arc<Sample>>>,
    audit: Option<broadcast::Receiver<Arc<Value>>>,
    /// Instant du dernier échantillon envoyé (snapshot compris) : pas de doublon (BR-DASH-011).
    last_sent: Option<OffsetDateTime>,
}

/// Reçoit le prochain élément d'un abonnement, ou attend sans fin s'il n'y en a pas.
async fn next<T: Clone>(rx: &mut Option<broadcast::Receiver<T>>) -> Result<T, RecvError> {
    match rx {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

async fn serve(socket: &mut WebSocket, state: &AppState) -> Option<Closing> {
    let settings = state.stream.settings;
    let mut shutdown = state.stream.shutdown_signal();

    // 1. Authentification : le premier message, dans le délai imparti.
    let (session, token) = tokio::select! {
        outcome = timeout(settings.auth_timeout, authenticate(socket, state)) => match outcome {
            Ok(Ok(authenticated)) => authenticated,
            Ok(Err(closing)) => return closing,
            Err(_) => {
                send(
                    socket,
                    &settings,
                    &error_message(
                        ErrorCode::Unauthenticated,
                        "Le message auth est attendu dans les 5 secondes de l'ouverture",
                    ),
                )
                .await;
                return policy("authentification attendue");
            }
        },
        _ = shutdown.changed() => return going_away(),
    };

    // 2. Flux : abonnements, échantillons, battement, session.
    let mut subscriptions = Subscriptions::default();
    let idle = sleep(settings.idle_timeout);
    tokio::pin!(idle);
    let mut check = interval_at(
        Instant::now() + settings.session_check_period,
        settings.session_check_period,
    );
    check.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = shutdown.changed() => return going_away(),
            () = &mut idle => return policy("silence du client"),
            received = socket.recv() => {
                let message = match received {
                    Some(Ok(message)) => message,
                    // Coupé, ou message refusé par la couche WebSocket (trop gros, mal formé).
                    Some(Err(error)) => {
                        tracing::debug!(%error, "flux fermé sur erreur de réception");
                        return Some(Closing { code: close_code::POLICY, reason: "message refusé" });
                    }
                    None => return None,
                };
                idle.as_mut().reset(Instant::now() + settings.idle_timeout);
                match message {
                    Message::Close(_) => return None,
                    Message::Ping(_) | Message::Pong(_) => {}
                    Message::Binary(_) => {
                        let reply = error_message(ErrorCode::ValidationError, "Les messages sont du JSON en texte");
                        if !send(socket, &settings, &reply).await {
                            return None;
                        }
                    }
                    Message::Text(text) => {
                        match handle_text(socket, state, &session, &mut subscriptions, text.as_str()).await {
                            Handled::Continue => {}
                            Handled::Stop => return None,
                        }
                    }
                }
            }
            sample = next(&mut subscriptions.metrics) => match sample {
                Ok(sample) => {
                    if is_new(subscriptions.last_sent, sample.at) {
                        subscriptions.last_sent = Some(sample.at);
                        let message = ServerMessage::Metrics(metrics_wire::sample(&sample));
                        if !send(socket, &settings, &message).await {
                            return None;
                        }
                    }
                }
                // Un abonné lent perd les échantillons les plus anciens : le suivant arrive.
                Err(RecvError::Lagged(missed)) => tracing::debug!(missed, "abonné lent : échantillons perdus"),
                Err(RecvError::Closed) => return going_away(),
            },
            event = next(&mut subscriptions.audit) => match event {
                Ok(event) => {
                    let message = ServerMessage::Audit { event: Value::clone(&event) };
                    if !send(socket, &settings, &message).await {
                        return None;
                    }
                }
                Err(RecvError::Lagged(missed)) => tracing::debug!(missed, "abonné lent : événements d'audit perdus"),
                Err(RecvError::Closed) => subscriptions.audit = None,
            },
            _ = check.tick() => {
                match state.sessions.authenticate(token.expose()).await {
                    Ok(_) => {}
                    Err(AuthError::Ended(end)) => {
                        let kind = match end {
                            SessionEnd::Expired => SessionNotice::Expired,
                            SessionEnd::Revoked => SessionNotice::Revoked,
                        };
                        send(socket, &settings, &ServerMessage::Session { kind }).await;
                        return policy("session terminée");
                    }
                    Err(error) => tracing::warn!(%error, "vérification de la session du flux impossible"),
                }
            }
        }
    }
}

fn going_away() -> Option<Closing> {
    Some(Closing {
        code: close_code::AWAY,
        reason: "arrêt de l'agent",
    })
}

/// Lit le premier message : `auth` avec un jeton valable. Tout autre cas : une erreur au format
/// de l'API, puis la fermeture.
async fn authenticate(
    socket: &mut WebSocket,
    state: &AppState,
) -> Result<(CurrentSession, Secret), Option<Closing>> {
    let settings = state.stream.settings;
    let text = loop {
        match socket.recv().await {
            Some(Ok(Message::Text(text))) => break text,
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
            Some(Ok(Message::Binary(_))) => return Err(refuse(socket, &settings).await),
            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return Err(None),
        }
    };
    let Ok(ClientMessage::Auth { token }) = serde_json::from_str::<ClientMessage>(text.as_str())
    else {
        return Err(refuse(socket, &settings).await);
    };
    let token = Secret::new(token);
    match state.sessions.authenticate(token.expose()).await {
        Ok(session) => Ok((session, token)),
        Err(error) => {
            send(socket, &settings, &from_api(error.into())).await;
            Err(policy("authentification refusée"))
        }
    }
}

/// Premier message qui n'est pas `auth`.
async fn refuse(socket: &mut WebSocket, settings: &StreamSettings) -> Option<Closing> {
    send(
        socket,
        settings,
        &error_message(
            ErrorCode::Unauthenticated,
            "Le premier message doit être auth avec le jeton de session",
        ),
    )
    .await;
    policy("authentification attendue")
}

enum Handled {
    Continue,
    Stop,
}

/// Un message texte du client, une fois authentifié.
async fn handle_text(
    socket: &mut WebSocket,
    state: &AppState,
    session: &CurrentSession,
    subscriptions: &mut Subscriptions,
    text: &str,
) -> Handled {
    let settings = state.stream.settings;
    let reply = match serde_json::from_str::<ClientMessage>(text) {
        Ok(ClientMessage::Ping { n }) => vec![ServerMessage::Pong { n }],
        Ok(ClientMessage::Subscribe { topics }) => {
            subscribe(state, session, subscriptions, &topics).await
        }
        Ok(ClientMessage::Auth { .. }) => vec![error_message(
            ErrorCode::ValidationError,
            "Déjà authentifié : auth n'est attendu qu'en premier message",
        )],
        Err(_) => vec![error_message(
            ErrorCode::ValidationError,
            "Message illisible : un objet JSON avec un champ type (subscribe, ping) est attendu",
        )],
    };
    for message in reply {
        if !send(socket, &settings, &message).await {
            return Handled::Stop;
        }
    }
    Handled::Continue
}

/// Le journal est réservé aux administrateurs ; la règle de ce qu'est un administrateur est celle
/// de `Role` (HRT-05 pourra la préciser).
fn may_read_audit(session: &CurrentSession) -> bool {
    session.account.role.can_manage_accounts()
}

/// Remplace les abonnements par les sujets demandés ; rend les messages à envoyer. S'abonner à
/// `metrics` répond par un `snapshot` (identité et historique des 5 dernières minutes).
async fn subscribe(
    state: &AppState,
    session: &CurrentSession,
    subscriptions: &mut Subscriptions,
    topics: &[Topic],
) -> Vec<ServerMessage> {
    let mut out = Vec::new();
    subscriptions.metrics = None;
    subscriptions.audit = None;
    subscriptions.last_sent = None;

    if topics.contains(&Topic::Metrics) {
        // S'abonner AVANT de lire l'historique : aucun échantillon n'échappe, ceux que le
        // snapshot contient déjà sont écartés à l'envoi (`is_new`).
        let receiver = state.metrics.subscribe();
        let history = state.metrics.history(SNAPSHOT_WINDOW);
        match state.metrics.identity().await {
            Ok(identity) => {
                subscriptions.last_sent = history.last().map(|sample| sample.at);
                subscriptions.metrics = Some(receiver);
                out.push(ServerMessage::Snapshot {
                    machine: metrics_wire::machine(&identity),
                    history: history.iter().map(metrics_wire::sample).collect(),
                });
            }
            Err(error) => out.push(from_api(ApiError::internal(&error))),
        }
    }
    if topics.contains(&Topic::Audit) {
        if may_read_audit(session) {
            subscriptions.audit = state.stream.audit.subscribe();
        } else {
            out.push(error_message(
                ErrorCode::ForbiddenRole,
                "Le journal d'activité est réservé aux administrateurs",
            ));
        }
    }
    // `session` est toujours reçu : s'y abonner est sans effet.
    out
}
