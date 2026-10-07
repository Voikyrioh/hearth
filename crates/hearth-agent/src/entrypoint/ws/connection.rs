//! Une connexion du flux : authentification par le premier message, abonnements, envoi des
//! échantillons, battement, surveillance de la session, fermeture.
//!
//! Ne bloque jamais personne : chaque connexion est sa propre tâche, un abonné lent perd des
//! échantillons (canal de diffusion borné), un client qui ne lit plus est abandonné après
//! `send_timeout`.

use std::sync::Arc;

use axum::extract::ws::{CloseFrame, Message, WebSocket, close_code};
use hearth_proto::api::update::UpdateProgress;
use hearth_proto::error::{ErrorCode, ErrorDetail};
use hearth_proto::stream::{
    ClientMessage, ServerMessage, SessionNotice, SignedAuth, Topic, UpdateMessage,
};
use serde_json::Value;
use tokio::sync::broadcast::{self, error::RecvError};
use tokio::time::{Instant, MissedTickBehavior, interval_at, sleep, timeout};

use super::{Permit, StreamSettings};
use crate::application::audit::AuditService;
use crate::application::sessions::{AuthError, CurrentSession};
use crate::domain::audit::AuditRecord;
use crate::domain::metrics::Sample;
use crate::domain::secret::Secret;
use crate::domain::sessions::SessionEnd;
use crate::domain::stream::{SNAPSHOT_WINDOW, is_new, may_subscribe};
use crate::entrypoint::http::{self, ApiError, AppState};
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
pub async fn run(mut socket: WebSocket, state: AppState, permit: Permit, addr: String) {
    // La place de flux est rendue à la fin de la connexion, quelle qu'en soit l'issue.
    let closing = serve(&mut socket, &state, permit, &addr).await;
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
        // FIX:01M47PCYX3BY3YV84R9WW3KAQ3 — fermeture ÉLÉGANTE : on attend la réponse du client à la
        // trame de fermeture avant de lâcher la connexion. Fermer tout de suite avec des données du
        // client encore non lues (ses battements) fait réinitialiser la connexion TCP, et la
        // réinitialisation efface côté client l'avis de fin de session et la trame de fermeture non lus :
        // le client croit à une perte de lien (docs/bugs/FIX-01M47PCYX3BY3YV84R9WW3KAQ3.md).
        // Pas à l'arrêt de l'agent (1001) : un arrêt ou une mise à jour n'attend aucun client.
        let drain = if closing.code == close_code::AWAY {
            std::time::Duration::ZERO
        } else {
            state.stream.settings.send_timeout
        };
        let _ = timeout(drain, async {
            while let Some(Ok(message)) = socket.recv().await {
                if matches!(message, Message::Close(_)) {
                    break;
                }
            }
        })
        .await;
    }
}

/// Envoie un message ; `false` si le client ne le reçoit plus (coupé ou trop lent).
async fn send(
    socket: &mut WebSocket,
    settings: &StreamSettings,
    message: &impl serde::Serialize,
) -> bool {
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
    audit: Option<broadcast::Receiver<AuditRecord>>,
    /// Progression de la mise à jour de l'agent (tout compte authentifié).
    update: Option<broadcast::Receiver<UpdateProgress>>,
    /// Instant monotone du dernier échantillon envoyé (snapshot compris) : pas de doublon
    /// (BR-DASH-011), même si l'horloge murale recule.
    last_sent: Option<time::Duration>,
    /// Dernier `subscribe` accepté : un par seconde au plus.
    last_subscribe: Option<Instant>,
    /// Cartes graphiques de l'identité envoyée dans le dernier `snapshot` : un nouveau `snapshot`
    /// part quand la liste des cartes change.
    snapshot_gpus: Vec<crate::domain::machine::GpuIdentity>,
}

/// Reçoit le prochain élément d'un abonnement, ou attend sans fin s'il n'y en a pas.
async fn next<T: Clone>(rx: &mut Option<broadcast::Receiver<T>>) -> Result<T, RecvError> {
    match rx {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

async fn serve(
    socket: &mut WebSocket,
    state: &AppState,
    mut permit: Permit,
    addr: &str,
) -> Option<Closing> {
    let settings = state.stream.settings;
    let mut shutdown = state.stream.shutdown_signal();

    // 1. Authentification : le premier message, dans le délai imparti.
    let (mut session, token) = tokio::select! {
        outcome = timeout(settings.auth_timeout, authenticate(socket, state, addr)) => match outcome {
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
        () = stopped(&mut shutdown) => return going_away(),
    };

    // Plafond de flux par compte, une fois le compte connu.
    if !permit.authenticate(
        session.account.id.as_str(),
        settings.max_total,
        settings.max_per_account,
    ) {
        send(
            socket,
            &settings,
            &error_message(
                ErrorCode::Busy,
                "Trop de flux ouverts (sur l'agent ou pour ce compte) : ferme-en un avant d'en ouvrir un autre",
            ),
        )
        .await;
        return policy("trop de flux ouverts");
    }

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
            () = stopped(&mut shutdown) => return going_away(),
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
                    if is_new(subscriptions.last_sent, sample.mono) {
                        subscriptions.last_sent = Some(sample.mono);
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
                Ok(record) => {
                    // L'événement du domaine devient le type du fil ici, comme les mesures.
                    match http::audit_item(&record) {
                        Ok(event) => {
                            if !send(socket, &settings, &ServerMessage::Audit { event }).await {
                                return None;
                            }
                        }
                        Err(_) => tracing::warn!("événement du journal non convertible, ignoré"),
                    }
                }
                Err(RecvError::Lagged(missed)) => tracing::debug!(missed, "abonné lent : événements d'audit perdus"),
                Err(RecvError::Closed) => subscriptions.audit = None,
            },
            progress = next(&mut subscriptions.update) => match progress {
                Ok(progress) => {
                    if !send(socket, &settings, &UpdateMessage::Update(progress)).await {
                        return None;
                    }
                }
                Err(RecvError::Lagged(missed)) => tracing::debug!(missed, "abonné lent : étapes de mise à jour perdues"),
                Err(RecvError::Closed) => subscriptions.update = None,
            },
            _ = check.tick() => {
                match state.sessions.authenticate_at(token.expose(), addr).await {
                    Ok(current) => {
                        // Une carte graphique apparue depuis le dernier snapshot : l'identité a
                        // changé, le client en reçoit un nouveau.
                        if subscriptions.metrics.is_some()
                            && state.metrics.identity().await.is_ok_and(|identity| identity.gpus != subscriptions.snapshot_gpus)
                        {
                            let snapshot = metrics_snapshot(state, &mut subscriptions).await;
                            if !send(socket, &settings, &snapshot).await {
                                return None;
                            }
                        }
                        // Le rôle a pu changer pendant le flux (`change_role` ne ferme pas les
                        // sessions) : le sujet `audit` se perd avec le droit de le lire.
                        session = current;
                        if subscriptions.audit.is_some()
                            && AuditService::ensure_reader(session.account.role).is_err()
                        {
                            subscriptions.audit = None;
                            let notice = error_message(
                                ErrorCode::ForbiddenRole,
                                "Le journal d'activité n'est plus accessible avec ton rôle",
                            );
                            if !send(socket, &settings, &notice).await {
                                return None;
                            }
                        }
                    }
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

/// Se termine quand l'arrêt de l'agent est demandé, y compris s'il l'était déjà à la création du
/// récepteur.
async fn stopped(shutdown: &mut tokio::sync::watch::Receiver<bool>) {
    let _ = shutdown.wait_for(|stopped| *stopped).await;
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
    addr: &str,
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
    // Le jeton, et en option la preuve de la clé d'appareil (HRT-22) : un client sans clé envoie le
    // même message qu'avant.
    let Ok(SignedAuth::Auth { token, device }) = serde_json::from_str::<SignedAuth>(text.as_str())
    else {
        return Err(refuse(socket, &settings).await);
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
        Ok(ClientMessage::Subscribe { .. })
            if !may_subscribe(
                subscriptions.last_subscribe.map(|at| at.elapsed()),
                settings.min_subscribe_interval,
            ) =>
        {
            vec![error_message(
                ErrorCode::Busy,
                "Un abonnement par seconde au plus : attends avant de t'abonner de nouveau",
            )]
        }
        Ok(ClientMessage::Subscribe { topics }) => {
            subscriptions.last_subscribe = Some(Instant::now());
            let replies = subscribe(state, session, subscriptions, &topics).await;
            // La mise à jour : ouverte à tout compte authentifié. L'état courant part d'abord, un
            // client qui se reconnecte en plein redémarrage voit où elle en est.
            subscriptions.update = None;
            if topics.contains(&Topic::Update) {
                let (receiver, current) = state.update.subscribe();
                subscriptions.update = Some(receiver);
                if let Some(progress) = current
                    && !send(socket, &settings, &UpdateMessage::Update(progress)).await
                {
                    return Handled::Stop;
                }
            }
            replies
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

/// Abonne la connexion aux mesures et rend le `snapshot` (ou l'erreur) à envoyer. S'abonner AVANT
/// de lire l'historique : aucun échantillon n'échappe, ceux que le snapshot contient déjà sont
/// écartés à l'envoi (`is_new`).
async fn metrics_snapshot(state: &AppState, subscriptions: &mut Subscriptions) -> ServerMessage {
    let receiver = state.metrics.subscribe();
    let history = state.metrics.history(SNAPSHOT_WINDOW);
    match state.metrics.identity().await {
        Ok(identity) => {
            subscriptions.last_sent = history.last().map(|sample| sample.mono);
            subscriptions.metrics = Some(receiver);
            subscriptions.snapshot_gpus = identity.gpus.clone();
            ServerMessage::Snapshot {
                machine: metrics_wire::machine(&identity),
                history: history
                    .iter()
                    .map(|sample| metrics_wire::sample(sample))
                    .collect(),
            }
        }
        Err(error) => from_api(ApiError::internal(&error)),
    }
}

/// Remplace les abonnements par les sujets demandés ; rend les messages à envoyer. S'abonner à
/// `metrics` répond par un `snapshot` (identité et historique des 5 dernières minutes) ;
/// s'abonner à `audit` est réservé aux administrateurs (`AuditService::subscribe`, qui porte
/// la règle du journal, `domain::audit::can_read_journal`).
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
        out.push(metrics_snapshot(state, subscriptions).await);
    }
    if topics.contains(&Topic::Audit) {
        // Le seul chemin d'abonnement : `AuditService::subscribe` contrôle le rôle (BR-AUDIT-001).
        match state.audit.subscribe(session.account.role) {
            Ok(receiver) => subscriptions.audit = Some(receiver),
            Err(_) => out.push(error_message(
                ErrorCode::ForbiddenRole,
                "Le journal d'activité est réservé aux administrateurs",
            )),
        }
    }
    // `session` est toujours reçu : s'y abonner est sans effet.
    out
}
