//! Lecture des journaux d'un service (HRT-67, ADR-0038 et ADR-0040, `BR-SLOG-*`).
//!
//! UNE lecture = un [`ServiceLogReader`] = un socket vers l'agent (`GET /services/{key}/logs`), jamais le flux
//! commun. Le lecteur est une tâche qui :
//!
//! 1. s'authentifie comme le flux (jeton du coffre, preuve de la clé d'appareil) puis envoie `open` ;
//! 2. rend, dans l'ordre, ce que l'agent envoie (`status`, `lines`, `gap`, `marker`) par un canal BORNÉ : un
//!    consommateur lent fait attendre la tâche, donc le socket, et l'agent compte alors la perte lui-même
//!    (`gap { slow }`, `BR-SLOG-007`) ; le moteur n'est jamais ralenti ;
//! 3. bat le rythme (`ping`) pendant qu'elle attend le consommateur ;
//! 4. se REPREND seule après une coupure (socket tombé, lien coupé puis revenu) avec le curseur de la dernière
//!    ligne reçue, avec un délai croissant et un nombre d'essais borné (`BR-SLOG-014`). Un `503 BUSY` attend
//!    son `Retry-After` ; un refus de l'agent est TERMINAL, jamais retenté (le frein `LOGS_RATE_LIMITED` porte
//!    son délai, `LOGS_ACCOUNT_LIMIT` demande à l'utilisateur de fermer une lecture) ;
//! 5. s'arrête quand le lecteur est libéré : le socket est alors fermé et la place rendue au compte.
//!
//! Le texte d'une ligne est du contenu sensible : ni trace, ni `Debug`, ni disque, ni message d'erreur
//! (`BR-SLOG-019`). Cette tâche ne le lit jamais : elle le transmet.

use std::sync::Arc;
use std::time::Duration;

use hearth_proto::admin_act::is_service_key;
use hearth_proto::api::service_logs::{
    LogCursor, LogGap, LogLines, LogMarker, LogsErrorCode, LogsServerMessage, LogsStatus,
};
use hearth_proto::error::ErrorCode;
use hearth_proto::stream::SignedAuth;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::{MissedTickBehavior, interval, sleep, timeout};
use zeroize::Zeroize as _;

use super::{LinkManager, Shared};
use crate::domain::event::StateInfo;
use crate::domain::server::ServerId;
use crate::domain::state::LinkState;
use crate::error::{InputField, LinkError};
use crate::ports::transport::{LogsConn, LogsFrame, TransportError};
use crate::ports::vault::SecretKind;

/// Messages en attente entre la tâche et son consommateur. Petit à dessein : au-delà, c'est l'agent qui compte
/// la perte, pas la mémoire du client.
const CHANNEL_CAPACITY: usize = 32;
/// Essais consécutifs sans rien recevoir avant de dire « injoignable ».
const MAX_FAILURES: u32 = 6;
/// Plafond de l'attente d'un `503 BUSY` ou d'un délai croissant.
const MAX_WAIT: Duration = Duration::from_secs(30);

/// Ce qu'un lecteur rend, dans l'ordre. Aucun `Debug` ne montre le texte d'une ligne.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogOutput {
    Status(LogsStatus),
    Lines(LogLines),
    Gap(LogGap),
    Marker(LogMarker),
    /// Le socket est tombé (ou le lien est coupé) : le lecteur reprend seul, par le curseur de la dernière
    /// ligne reçue. Le message suivant est un `Status` de la reprise.
    Interrupted,
    /// La lecture est finie et ne repart pas seule.
    Ended(LogEnd),
}

/// Pourquoi une lecture est finie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogEnd {
    /// L'agent a refusé ou fermé la lecture avec ce code (`LOGS_ACCOUNT_LIMIT`, `LOGS_RATE_LIMITED` et son
    /// délai, `MISSING_RIGHT`, `SERVICE_NOT_FOUND`...). Un code inconnu est `Unknown` : une fin générique.
    Refused {
        code: LogsErrorCode,
        retry_after_s: Option<u32>,
    },
    /// La session du compte n'existe plus (jeton absent du coffre ou refusé).
    SessionEnded,
    /// L'agent ne sert pas les journaux (agent ancien).
    Unsupported,
    /// Plusieurs essais de reprise de suite sans réponse.
    Unreachable,
}

/// Une lecture ouverte. La libérer (`Drop`) arrête la tâche et ferme le socket : la place est rendue au compte.
pub struct ServiceLogReader {
    rx: mpsc::Receiver<LogOutput>,
    task: JoinHandle<()>,
}

impl ServiceLogReader {
    /// Le message suivant, dans l'ordre ; `None` quand la tâche est finie (après un `Ended`).
    pub async fn next(&mut self) -> Option<LogOutput> {
        self.rx.recv().await
    }
}

impl Drop for ServiceLogReader {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl std::fmt::Debug for ServiceLogReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ServiceLogReader")
    }
}

impl LinkManager {
    /// Ouvre la lecture des journaux de ce service. La clé est vérifiée (forme d'une clé de service) puis
    /// encodée par le code ; hors « Connecté » : `NotConnected`, rien n'est envoyé. Tout le reste (refus de
    /// l'agent, coupure, reprise) arrive comme messages du lecteur. Une seule lecture par appel : l'appelant
    /// la libère quand l'écran la quitte (elle compte dans le plafond du compte, `BR-SLOG-026`).
    pub fn open_service_logs(
        &self,
        id: &ServerId,
        key: &str,
    ) -> Result<ServiceLogReader, LinkError> {
        if !is_service_key(key) {
            return Err(LinkError::InvalidInput(InputField::Filter));
        }
        // Hors « Connecté » : NotConnected, avant toute tâche.
        self.credentials(id)?;
        let (_, shared) = self.handle(id)?;
        let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);
        let task = tokio::spawn(run(
            self.clone(),
            shared,
            id.clone(),
            super::services::key_segment(key),
            tx,
        ));
        Ok(ServiceLogReader { rx, task })
    }
}

/// Résultat d'une ouverture de socket, de la lecture jusqu'à sa fin.
enum Read {
    /// Fin définitive.
    End(LogEnd),
    /// Le socket est tombé ; `progressed` : au moins un message reçu.
    Cut { progressed: bool },
    /// L'agent demande d'attendre (`503 BUSY`).
    Busy(Duration),
    /// Plus personne ne lit : la tâche s'arrête.
    Gone,
}

async fn run(
    manager: LinkManager,
    shared: Arc<Shared>,
    id: ServerId,
    segment: String,
    tx: mpsc::Sender<LogOutput>,
) {
    let mut state = shared.state.subscribe();
    let mut cursor: Option<LogCursor> = None;
    let mut failures = 0u32;
    let mut streaming = false;
    loop {
        if !wait_connected(&mut state, &tx).await {
            return;
        }
        let outcome = read_once(
            &manager,
            &shared,
            &id,
            &segment,
            &mut cursor,
            &mut state,
            &tx,
            &mut streaming,
        )
        .await;
        match outcome {
            Read::Gone => return,
            Read::End(end) => {
                let _ = tx.send(LogOutput::Ended(end)).await;
                return;
            }
            Read::Cut { progressed } => {
                failures = if progressed { 1 } else { failures + 1 };
                if streaming {
                    streaming = false;
                    if tx.send(LogOutput::Interrupted).await.is_err() {
                        return;
                    }
                }
                if failures > MAX_FAILURES {
                    let _ = tx.send(LogOutput::Ended(LogEnd::Unreachable)).await;
                    return;
                }
                let base = manager.inner.deps.config.restart_delay;
                let wait = base.saturating_mul(1 << failures.saturating_sub(1).min(4));
                tokio::select! {
                    () = sleep(wait.min(MAX_WAIT)) => {}
                    () = tx.closed() => return,
                }
            }
            Read::Busy(wait) => {
                failures += 1;
                if failures > MAX_FAILURES {
                    let _ = tx.send(LogOutput::Ended(LogEnd::Unreachable)).await;
                    return;
                }
                tokio::select! {
                    () = sleep(wait.min(MAX_WAIT)) => {}
                    () = tx.closed() => return,
                }
            }
        }
    }
}

/// Attend que le lien soit « Connecté ». Faux : plus personne ne lit, ou le serveur est retiré.
async fn wait_connected(
    state: &mut watch::Receiver<StateInfo>,
    tx: &mpsc::Sender<LogOutput>,
) -> bool {
    loop {
        if state.borrow_and_update().state == LinkState::Connected {
            return true;
        }
        tokio::select! {
            changed = state.changed() => if changed.is_err() { return false; },
            () = tx.closed() => return false,
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn read_once(
    manager: &LinkManager,
    shared: &Shared,
    id: &ServerId,
    segment: &str,
    cursor: &mut Option<LogCursor>,
    state: &mut watch::Receiver<StateInfo>,
    tx: &mpsc::Sender<LogOutput>,
    streaming: &mut bool,
) -> Read {
    let deps = &manager.inner.deps;
    let token = match deps.vault.get(id, SecretKind::Token) {
        Ok(Some(token)) => token,
        Ok(None) => return Read::End(LogEnd::SessionEnded),
        Err(_) => return Read::Cut { progressed: false },
    };
    let target = shared.target();
    let proof =
        match super::device::session_proof(deps, &target, id, &shared.record().username, &token)
            .await
        {
            Ok(proof) => proof,
            Err(_) => return Read::Cut { progressed: false },
        };
    let opened = timeout(
        deps.config.attempt_timeout,
        deps.transport.open_service_logs(&target, segment),
    )
    .await;
    let mut conn = match opened {
        Err(_) => return Read::Cut { progressed: false },
        Ok(Ok(conn)) => conn,
        Ok(Err(TransportError::Api(api))) => {
            return match (api.status, api.code) {
                (503, _) | (_, Some(ErrorCode::Busy)) => Read::Busy(Duration::from_secs(
                    api.retry_after_s.unwrap_or(1).clamp(1, 30),
                )),
                (404, _) | (405, _) => Read::End(LogEnd::Unsupported),
                (426, _) => Read::End(LogEnd::Unsupported),
                (401, _) => Read::End(LogEnd::SessionEnded),
                _ => Read::Cut { progressed: false },
            };
        }
        Ok(Err(_)) => return Read::Cut { progressed: false },
    };
    let auth = SignedAuth::Auth {
        token: token.expose().to_owned(),
        device: proof,
    };
    let sent = conn.send_auth(&auth).await;
    // La copie du jeton faite pour le message est effacée dès qu'il est parti.
    let SignedAuth::Auth {
        token: mut copy, ..
    } = auth;
    copy.zeroize();
    if sent.is_err() || conn.send_open(*cursor).await.is_err() {
        return Read::Cut { progressed: false };
    }

    let mut progressed = false;
    let mut ping = interval(deps.config.heartbeat_period.min(Duration::from_secs(2)));
    ping.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut n = 0u64;
    loop {
        tokio::select! {
            frame = conn.recv() => {
                let message = match frame {
                    Ok(LogsFrame::Message(message)) => *message,
                    Ok(LogsFrame::Other) => continue,
                    Err(_) => return Read::Cut { progressed },
                };
                progressed = true;
                let output = match message {
                    LogsServerMessage::Status(status) => {
                        *streaming = true;
                        LogOutput::Status(status)
                    }
                    LogsServerMessage::Lines(lines) => {
                        *cursor = Some(lines.cursor);
                        LogOutput::Lines(lines)
                    }
                    LogsServerMessage::Gap(gap) => LogOutput::Gap(gap),
                    LogsServerMessage::Marker(marker) => LogOutput::Marker(marker),
                    LogsServerMessage::Error(error) => {
                        return Read::End(match error.code {
                            LogsErrorCode::Unauthenticated => LogEnd::SessionEnded,
                            code => LogEnd::Refused { code, retry_after_s: error.retry_after_s },
                        });
                    }
                    LogsServerMessage::Pong { .. } | LogsServerMessage::Unknown => continue,
                };
                if !forward(&mut conn, tx, output, &mut ping, &mut n).await {
                    return Read::Gone;
                }
            }
            _ = ping.tick() => {
                n += 1;
                if conn.send_ping(n).await.is_err() {
                    return Read::Cut { progressed };
                }
            }
            changed = state.changed() => {
                if changed.is_err() {
                    return Read::Gone;
                }
                if state.borrow_and_update().state != LinkState::Connected {
                    return Read::Cut { progressed };
                }
            }
            () = tx.closed() => return Read::Gone,
        }
    }
}

/// Remet un message au consommateur en gardant le battement : attendre un consommateur lent ne doit pas faire
/// croire à l'agent que le client est muet. Faux : plus personne ne lit.
async fn forward(
    conn: &mut Box<dyn LogsConn>,
    tx: &mpsc::Sender<LogOutput>,
    output: LogOutput,
    ping: &mut tokio::time::Interval,
    n: &mut u64,
) -> bool {
    let send = tx.send(output);
    tokio::pin!(send);
    loop {
        tokio::select! {
            result = &mut send => return result.is_ok(),
            _ = ping.tick() => {
                *n += 1;
                let _ = conn.send_ping(*n).await;
            }
        }
    }
}
