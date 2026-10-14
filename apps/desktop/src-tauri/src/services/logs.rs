//! Les lectures de journaux ouvertes par l'interface (HRT-67, ADR-0040) : une session par onglet Journaux.
//!
//! La coquille ne garde AUCUNE ligne : elle transmet les lots à la fenêtre, un événement par lot, et rien n'est
//! écrit sur disque ni tracé (le texte d'une ligne est du contenu sensible, `BR-SLOG-019`). Deux bornes :
//!
//! - au plus [`MAX_SESSIONS`] lectures à la fois (le plafond d'un compte côté agent, `BR-SLOG-026`) : une
//!   interface qui en ouvrirait plus est refusée ici, sans toucher au serveur ;
//! - au plus [`WINDOW`] lots sans accusé de la fenêtre : au-delà la coquille cesse de lire la liaison, donc le
//!   socket, et c'est l'agent qui compte la perte (`gap { slow }`) au lieu que la mémoire du client enfle.
//!
//! Libérer une session (fermeture demandée, onglet quitté, fin de lecture) arrête la tâche et ferme le socket :
//! la place est rendue au compte.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use hearth_link::domain::server::ServerId;
use hearth_link::{LinkManager, LogOutput, ServiceLogReader};
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;

use super::logs_dto::{EVENT, ServiceLogBody, ServiceLogEvent};
use crate::link::UiSink;
use crate::link_dto::LinkFailure;

/// Lectures ouvertes en même temps, au plus (le plafond d'un compte côté agent).
pub const MAX_SESSIONS: usize = 3;
/// Lots sans accusé au plus.
pub const WINDOW: usize = 20;

struct Session {
    window: Arc<Window>,
    task: JoinHandle<()>,
}

/// Les lots envoyés à la fenêtre et pas encore acquittés.
struct Window {
    permits: Semaphore,
    in_flight: AtomicUsize,
}

impl Window {
    fn new() -> Self {
        Self {
            permits: Semaphore::new(WINDOW),
            in_flight: AtomicUsize::new(0),
        }
    }

    /// Rend `n` lots (jamais plus que ceux réellement en vol : un accusé de trop ne gonfle pas la fenêtre).
    fn acknowledge(&self, n: usize) {
        let mut released = 0;
        let _ = self
            .in_flight
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |flying| {
                released = n.min(flying);
                Some(flying - released)
            });
        self.permits.add_permits(released);
    }
}

#[derive(Default)]
pub struct LogSessions {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    sessions: HashMap<String, Session>,
    next: u64,
}

impl LogSessions {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Ouvre une lecture et rend l'identifiant de sa session. Hors « Connecté » : `NotConnected`, rien n'est
    /// envoyé. Au plafond : `Internal` (l'interface ne doit pas en ouvrir plus), sans toucher au serveur.
    pub fn open(
        self: &Arc<Self>,
        manager: &LinkManager,
        sink: Arc<dyn UiSink>,
        server_id: &str,
        key: &str,
    ) -> Result<String, LinkFailure> {
        let id = ServerId::parse(server_id).map_err(|_| LinkFailure::UnknownServer)?;
        let mut inner = self.lock();
        if inner.sessions.len() >= MAX_SESSIONS {
            return Err(LinkFailure::Internal);
        }
        let reader = manager.open_service_logs(&id, key)?;
        inner.next += 1;
        let session = format!("log-{}", inner.next);
        let window = Arc::new(Window::new());
        let task = tokio::spawn(forward(
            self.clone(),
            session.clone(),
            reader,
            window.clone(),
            sink,
        ));
        inner
            .sessions
            .insert(session.clone(), Session { window, task });
        Ok(session)
    }

    /// Ferme une session : la tâche s'arrête, le socket se ferme, plus aucun événement ne part. Une session
    /// inconnue (déjà finie) n'est pas une erreur.
    pub fn close(&self, session: &str) {
        if let Some(found) = self.lock().sessions.remove(session) {
            found.task.abort();
        }
    }

    /// La fenêtre a affiché `batches` lots de plus.
    pub fn acknowledge(&self, session: &str, batches: u32) {
        let window = self
            .lock()
            .sessions
            .get(session)
            .map(|found| found.window.clone());
        if let Some(window) = window {
            window.acknowledge(usize::try_from(batches).unwrap_or(usize::MAX));
        }
    }

    /// Sessions ouvertes (tests).
    pub fn open_count(&self) -> usize {
        self.lock().sessions.len()
    }

    /// Ferme tout : à la sortie de l'application.
    pub fn close_all(&self) {
        for (_, found) in self.lock().sessions.drain() {
            found.task.abort();
        }
    }
}

async fn forward(
    sessions: Arc<LogSessions>,
    session: String,
    mut reader: ServiceLogReader,
    window: Arc<Window>,
    sink: Arc<dyn UiSink>,
) {
    while let Some(output) = reader.next().await {
        let ended = matches!(output, LogOutput::Ended(_));
        let batch = matches!(output, LogOutput::Lines(_));
        if batch {
            // Un lot de plus en vol : on attend l'accusé de la fenêtre au-delà de la borne.
            match window.permits.acquire().await {
                Ok(permit) => permit.forget(),
                Err(_) => return,
            }
            window.in_flight.fetch_add(1, Ordering::SeqCst);
        }
        let event = ServiceLogEvent {
            session: session.clone(),
            body: ServiceLogBody::from(output),
        };
        match serde_json::to_value(&event) {
            Ok(value) => sink.emit(EVENT, value),
            // Jamais l'erreur de `serde` ni la charge : un texte fixe.
            Err(_) => tracing::error!("événement de journaux illisible, abandonné"),
        }
        if ended {
            break;
        }
    }
    // Fin de lecture : la session n'existe plus (sans arrêter cette tâche, qui finit ici).
    sessions.lock().sessions.remove(&session);
}
