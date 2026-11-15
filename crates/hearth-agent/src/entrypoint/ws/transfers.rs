//! Le socket d'UN envoi, `GET /api/v1/transfers/{id}` (HRT-95, ADR-0048, BR-FILES-019, BR-MOD-015).
//!
//! Un socket par transfert, jamais le flux commun : un client lent ne peut faire fermer que son propre transfert.
//! Déroulement : `auth` (même forme que `/stream`), `open`, puis des trames binaires `[index][empreinte][données]`
//! de 4 Mio au plus, chacune vérifiée, écrite et rendue durable AVANT son `ack` ; `finish` avec l'empreinte de
//! l'ensemble ferme l'envoi. L'agent ne lit une trame qu'une fois la précédente écrite : sa mémoire est UN morceau,
//! quel que soit le débit du client, et rien du tout pendant qu'il se tait (60 secondes de silence ferment le socket,
//! l'envoi reste reprenable 24 heures).
//!
//! Le compte doit être celui qui a ouvert le dépôt et garder le droit de gérer les services pendant tout l'envoi
//! (revérifié toutes les `session_check_period`). Aucun nom de fichier, aucun chemin, aucun octet du contenu dans une
//! trace : seuls des nombres sont tracés.

use axum::extract::ws::{Message, WebSocket, close_code};
use hearth_proto::api::transfers::{
    MAX_TEXT_FRAME_BYTES, TransferError, TransferErrorCode, UploadClientMessage,
    UploadServerMessage, decode_frame, digest_from_hex, hex_of,
};
use hearth_proto::stream::SignedAuth;
use tokio::time::{Instant, MissedTickBehavior, interval_at, sleep, timeout};

use super::connection::{Closing, close, going_away, policy, send, stopped};
use super::{Permit, StreamSettings};
use crate::application::sessions::{AuthError, CurrentSession};
use crate::application::transfers::{Attached, ChunkError, FinishError, TransferError as Refusal};
use crate::domain::audit::{Actor, AuditAction, Origin, Outcome, Target};
use crate::domain::secret::Secret;
use crate::domain::transfers::{Limit, TransferId};
use crate::entrypoint::http::AppState;

/// Sert le transfert jusqu'à sa fin, puis envoie la trame de fermeture.
pub async fn run(
    mut socket: WebSocket,
    state: AppState,
    permit: Permit,
    addr: String,
    id: TransferId,
) {
    let closing = serve(&mut socket, &state, permit, &addr, id).await;
    close(&mut socket, &state, closing).await;
}

fn normal() -> Option<Closing> {
    Some(Closing {
        code: close_code::NORMAL,
        reason: "envoi terminé",
    })
}

fn error(code: TransferErrorCode) -> UploadServerMessage {
    UploadServerMessage::Error(TransferError::new(code))
}

fn refusal_message(refusal: &Refusal) -> UploadServerMessage {
    match refusal {
        Refusal::NotFound | Refusal::Invalid { .. } => error(TransferErrorCode::TransferNotFound),
        Refusal::Expired => error(TransferErrorCode::TransferExpired),
        Refusal::Limit(Limit::Total | Limit::Account | Limit::Open) => {
            error(TransferErrorCode::TransferLimit)
        }
        Refusal::NoSpace(shortfall) => UploadServerMessage::Error(TransferError {
            missing_bytes: Some(shortfall.missing),
            free_after_bytes: Some(shortfall.free_after),
            ..TransferError::new(TransferErrorCode::NoSpace)
        }),
        Refusal::Unavailable | Refusal::Storage => error(TransferErrorCode::WriteFailed),
    }
}

async fn serve(
    socket: &mut WebSocket,
    state: &AppState,
    permit: Permit,
    addr: &str,
    id: TransferId,
) -> Option<Closing> {
    let settings = state.stream.settings;
    let mut shutdown = state.stream.shutdown_signal();

    // 1. Authentification : le premier message, dans le délai imparti.
    let (session, token) = tokio::select! {
        outcome = timeout(settings.auth_timeout, authenticate(socket, state, addr)) => match outcome {
            Ok(Ok(authenticated)) => authenticated,
            Ok(Err(closing)) => return closing,
            Err(_) => {
                send(socket, &settings, &error(TransferErrorCode::Unauthenticated)).await;
                return policy("authentification attendue");
            }
        },
        () = stopped(&mut shutdown) => return going_away(),
    };

    // 2. `open`, dans le même délai.
    let () = tokio::select! {
        outcome = timeout(settings.auth_timeout, read_open(socket, state)) => match outcome {
            Ok(Ok(())) => (),
            Ok(Err(closing)) => return closing,
            Err(_) => {
                send(socket, &settings, &error(TransferErrorCode::BadRequest)).await;
                return policy("open attendu");
            }
        },
        () = stopped(&mut shutdown) => return going_away(),
    };
    // La place d'attente d'authentification est rendue ici : un transfert se compte par ses propres places.
    drop(permit);

    // 3. Le droit de gérer les services, au moment d'ouvrir (BR-GRANT-003).
    if !session.account.may_manage_services() {
        send(socket, &settings, &error(TransferErrorCode::MissingRight)).await;
        return normal();
    }

    // 4. L'attache : relit le disque, annonce l'offset durable.
    let account = session.account.id.as_str().to_owned();
    let mut upload = match state.transfers.attach(&id, &account).await {
        Ok(Attached::Receiving(upload)) => upload,
        Ok(Attached::Complete { size, tree_digest }) => {
            send(
                socket,
                &settings,
                &UploadServerMessage::Done {
                    size,
                    verified: true,
                    tree_digest: hex_of(&tree_digest),
                },
            )
            .await;
            return normal();
        }
        Err(refused) => {
            send(socket, &settings, &refusal_message(&refused)).await;
            return normal();
        }
    };
    tracing::info!(
        size = upload.size(),
        offset = upload.offset(),
        "transfert ouvert"
    );
    if !send(
        socket,
        &settings,
        &UploadServerMessage::Ready {
            offset: upload.offset(),
            window: upload.window(),
        },
    )
    .await
    {
        return None;
    }

    let idle = sleep(settings.transfer_idle_timeout);
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
                match message {
                    Message::Close(_) => break None,
                    // Ni un `ping` ni un battement ne prouvent un envoi : seul un morceau remet le silence à zéro, sinon
                    // un client qui ne fait que pinger garderait une place de transfert indéfiniment.
                    Message::Ping(_) | Message::Pong(_) => {}
                    Message::Binary(bytes) => {
                        idle.as_mut()
                            .reset(Instant::now() + settings.transfer_idle_timeout);
                        let Ok(frame) = decode_frame(&bytes) else {
                            send(socket, &settings, &error(TransferErrorCode::BadRequest)).await;
                            break normal();
                        };
                        let (index, digest) = (frame.index, frame.digest);
                        let data = bytes.slice(hearth_proto::api::transfers::FRAME_HEADER_BYTES..);
                        match upload.put_chunk(index, digest, data).await {
                            Ok(offset) => {
                                if !send(socket, &settings, &UploadServerMessage::Ack {
                                    offset,
                                    window: upload.window(),
                                }).await {
                                    break None;
                                }
                            }
                            Err(refused) => {
                                send(socket, &settings, &chunk_message(&refused)).await;
                                break normal();
                            }
                        }
                    }
                    Message::Text(text) => {
                        if text.len() > MAX_TEXT_FRAME_BYTES {
                            send(socket, &settings, &error(TransferErrorCode::BadRequest)).await;
                            break normal();
                        }
                        match serde_json::from_str::<UploadClientMessage>(text.as_str()) {
                            Ok(UploadClientMessage::Ping { n }) => {
                                if !send(socket, &settings, &UploadServerMessage::Pong { n }).await {
                                    break None;
                                }
                            }
                            Ok(UploadClientMessage::Finish { tree_digest }) => {
                                let Some(claimed) = digest_from_hex(&tree_digest) else {
                                    send(socket, &settings, &error(TransferErrorCode::BadRequest)).await;
                                    break normal();
                                };
                                match upload.finish(claimed).await {
                                    Ok(done) => {
                                        journal_complete(state, &session, addr).await;
                                        tracing::info!(size = done.size, "transfert complet");
                                        send(socket, &settings, &UploadServerMessage::Done {
                                            size: done.size,
                                            verified: true,
                                            tree_digest: hex_of(&done.tree_digest),
                                        }).await;
                                        break normal();
                                    }
                                    Err(refused) => {
                                        send(socket, &settings, &finish_message(&refused)).await;
                                        break normal();
                                    }
                                }
                            }
                            // Une deuxième ouverture, ou un message illisible : l'envoi est mal piloté.
                            Ok(UploadClientMessage::Open {}) | Err(_) => {
                                send(socket, &settings, &error(TransferErrorCode::BadRequest)).await;
                                break normal();
                            }
                        }
                    }
                }
            }
            _ = check.tick() => {
                match state.sessions.authenticate_at(token.expose(), addr).await {
                    // Le droit retiré pendant l'envoi ferme l'envoi (la partie reste reprenable par un compte qui l'a).
                    Ok(current) if !current.account.may_manage_services() => {
                        send(socket, &settings, &error(TransferErrorCode::MissingRight)).await;
                        break normal();
                    }
                    Ok(_) => {}
                    Err(AuthError::Ended(_) | AuthError::NotRecognized) => {
                        send(socket, &settings, &error(TransferErrorCode::Unauthenticated)).await;
                        break policy("session terminée");
                    }
                    Err(error) => tracing::warn!(%error, "vérification de la session du transfert impossible"),
                }
            }
        }
    };
    tracing::info!(offset = upload.offset(), "transfert fermé");
    closing
}

fn chunk_message(refused: &ChunkError) -> UploadServerMessage {
    match refused {
        ChunkError::Sequence { expected } => UploadServerMessage::Error(TransferError {
            expected_index: Some(*expected),
            ..TransferError::new(TransferErrorCode::ChunkSequence)
        }),
        ChunkError::Corrupt { expected } => UploadServerMessage::Error(TransferError {
            expected_index: Some(*expected),
            ..TransferError::new(TransferErrorCode::ChunkCorrupt)
        }),
        ChunkError::NoSpace(shortfall) => UploadServerMessage::Error(TransferError {
            missing_bytes: Some(shortfall.missing),
            free_after_bytes: Some(shortfall.free_after),
            ..TransferError::new(TransferErrorCode::NoSpace)
        }),
        // Un autre socket a repris ce transfert, ou le dépôt a disparu : plus rien à reprendre ici.
        ChunkError::Superseded | ChunkError::Gone => error(TransferErrorCode::TransferNotFound),
        ChunkError::Write => error(TransferErrorCode::WriteFailed),
    }
}

fn finish_message(refused: &FinishError) -> UploadServerMessage {
    match refused {
        FinishError::Incomplete | FinishError::Mismatch => error(TransferErrorCode::TreeMismatch),
        FinishError::Superseded | FinishError::Gone => error(TransferErrorCode::TransferNotFound),
        FinishError::Write => error(TransferErrorCode::WriteFailed),
    }
}

/// Inscrit au journal d'activité qu'un fichier est arrivé en entier dans la zone d'attente (`service.file.write`,
/// cible `/transfers/{id}`, à la fin ; l'ouverture du dépôt a la sienne, cible `/deposits`) : qui et d'où, jamais un
/// nom de fichier ni un chemin. Un échec d'écriture est tracé par le puits, jamais subi.
async fn journal_complete(state: &AppState, session: &CurrentSession, addr: &str) {
    let actor = Actor::new(
        Some(session.account.username.clone()),
        Origin::client(None, addr),
    );
    state
        .sink
        .record(
            actor,
            AuditAction::ServiceFileWrite,
            Target::Route("/transfers/{id}"),
            Outcome::Succeeded,
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
            Some(Ok(Message::Text(text))) if text.len() <= MAX_TEXT_FRAME_BYTES => break text,
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
            Some(Ok(Message::Text(_) | Message::Binary(_))) => {
                return Err(refuse(socket, &settings).await);
            }
            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return Err(None),
        }
    };
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
        Err(_) => {
            send(
                socket,
                &settings,
                &error(TransferErrorCode::Unauthenticated),
            )
            .await;
            Err(policy("authentification refusée"))
        }
    }
}

/// Premier message qui n'est pas `auth`.
async fn refuse(socket: &mut WebSocket, settings: &StreamSettings) -> Option<Closing> {
    send(socket, settings, &error(TransferErrorCode::Unauthenticated)).await;
    policy("authentification attendue")
}

/// Lit `open` (un `ping` avant lui reçoit son `pong`). Tout autre message est une mauvaise demande.
async fn read_open(socket: &mut WebSocket, state: &AppState) -> Result<(), Option<Closing>> {
    let settings = state.stream.settings;
    loop {
        match socket.recv().await {
            Some(Ok(Message::Text(text))) if text.len() <= MAX_TEXT_FRAME_BYTES => {
                match serde_json::from_str::<UploadClientMessage>(text.as_str()) {
                    Ok(UploadClientMessage::Open {}) => return Ok(()),
                    Ok(UploadClientMessage::Ping { n }) => {
                        if !send(socket, &settings, &UploadServerMessage::Pong { n }).await {
                            return Err(None);
                        }
                    }
                    Ok(UploadClientMessage::Finish { .. }) | Err(_) => {
                        send(socket, &settings, &error(TransferErrorCode::BadRequest)).await;
                        return Err(normal());
                    }
                }
            }
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
            Some(Ok(Message::Text(_) | Message::Binary(_))) => {
                send(socket, &settings, &error(TransferErrorCode::BadRequest)).await;
                return Err(normal());
            }
            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return Err(None),
        }
    }
}
