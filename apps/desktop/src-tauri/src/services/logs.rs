//! Les lectures de journaux ouvertes par l'interface (HRT-67, ADR-0040) : une session par onglet Journaux.
//!
//! La coquille ne garde AUCUNE ligne : elle transmet les lots à la fenêtre qui a ouvert la lecture, un événement
//! par lot, et rien n'est écrit sur disque ni tracé (le texte d'une ligne est du contenu sensible, `BR-SLOG-019`).
//!
//! # Bornes
//!
//! - [`WINDOW`] lots sans accusé au plus : au-delà la coquille cesse de lire la liaison, donc le socket, et c'est
//!   l'agent qui compte la perte (`gap { slow }`) au lieu que la mémoire du client enfle ;
//! - [`MAX_SESSIONS_PER_SERVER`] lectures par serveur (le plafond d'un compte côté agent, `BR-SLOG-026`, repris
//!   de `hearth_proto`) et [`MAX_SESSIONS_TOTAL`] au total (garde) : plus, la coquille refuse (`LogReadsFull`)
//!   sans toucher au serveur.
//!
//! # Une session ne fuit pas, par construction
//!
//! Libérer une session arrête sa tâche et ferme son socket : la place est rendue au compte. Cinq mécanismes,
//! tous côté Rust, aucun ne compte sur le JavaScript :
//!
//! 1. **Génération de page** : chaque session est liée à la fenêtre qui l'a ouverte et à la génération de sa
//!    page. Quand une page démarre elle s'annonce ([`LogSessions::announce_page`]) et la coquille ferme tout
//!    ce qui restait de la génération précédente de CETTE fenêtre ; une ouverture d'une génération périmée est
//!    refusée (`LogPageUnknown`).
//! 2. **Fenêtre** : [`LogSessions::close_window`] à la fermeture (cachée dans la zone de notification) avec
//!    un message `closed` vers l'interface, [`LogSessions::close_server`] à la déconnexion, au changement ou à
//!    l'oubli d'un serveur, [`LogSessions::forget_window`] à la destruction de la fenêtre et [`LogSessions::close_all`] à la sortie.
//! 3. **Battement** : l'interface signale sa présence toutes les [`KEEP_ALIVE_PERIOD`]
//!    ([`LogSessions::keep_alive`]) ou par un accusé ; une session muette depuis [`IDLE_TIMEOUT`] est fermée
//!    d'office ([`LogSessions::reap`], appelée toutes les [`REAP_PERIOD`]).
//! 4. **Plafonds** par serveur et global (ci-dessus).
//! 5. **Fin de lecture** : la tâche qui finit seule retire sa session.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use hearth_link::domain::server::ServerId;
use hearth_link::{LinkManager, LogOutput, ServiceLogReader};
use hearth_proto::api::service_logs::MAX_READS_PER_ACCOUNT;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;

use super::logs_dto::{EVENT, LogClosedCauseDto, LogEndDto, ServiceLogBody, ServiceLogEvent};
use crate::link::UiSink;
use crate::link_dto::LinkFailure;

/// Lectures ouvertes en même temps pour UN serveur, au plus : le plafond d'un compte côté agent (repris de
/// `hearth_proto`, donc aligné par construction).
pub const MAX_SESSIONS_PER_SERVER: usize = MAX_READS_PER_ACCOUNT;
/// Lectures ouvertes en même temps, tous serveurs confondus : garde, au-dessus de quatre serveurs pleins.
pub const MAX_SESSIONS_TOTAL: usize = 12;
/// Lots sans accusé au plus.
pub const WINDOW: usize = 20;
/// Période à laquelle l'interface signale sa présence quand elle tient une lecture.
pub const KEEP_ALIVE_PERIOD: Duration = Duration::from_secs(10);
/// Une session dont l'interface n'a rien signalé ni accusé depuis ce délai est fermée d'office : un peu
/// plus de quatre battements manqués (une page très ralentie en rate un ou deux, jamais quatre).
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(45);
/// Période du contrôle des sessions muettes.
pub const REAP_PERIOD: Duration = Duration::from_secs(5);

/// Le temps, en durée depuis un départ fixe : injectable pour rejouer un battement expiré sans attendre.
pub type Clock = Arc<dyn Fn() -> Duration + Send + Sync>;

fn system_clock() -> Clock {
    let start = Instant::now();
    Arc::new(move || start.elapsed())
}

struct Session {
    window: Arc<Window>,
    task: JoinHandle<()>,
    /// La fenêtre (son étiquette) qui a ouvert la lecture : elle seule reçoit les événements.
    owner: String,
    /// La génération de la page de cette fenêtre au moment de l'ouverture.
    page: u32,
    server: String,
    sink: Arc<dyn UiSink>,
    last_seen: Duration,
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

pub struct LogSessions {
    inner: Mutex<Inner>,
    clock: Clock,
    per_server: usize,
    total: usize,
}

#[derive(Default)]
struct Inner {
    sessions: HashMap<String, Session>,
    /// La génération courante de la page de chaque fenêtre.
    pages: HashMap<String, u32>,
    next: u64,
    next_page: u32,
}

impl Default for LogSessions {
    fn default() -> Self {
        Self::with_clock(system_clock())
    }
}

impl LogSessions {
    /// Avec une horloge donnée (les tests rejouent un battement expiré sans attendre).
    pub fn with_clock(clock: Clock) -> Self {
        Self::with_limits(clock, MAX_SESSIONS_PER_SERVER, MAX_SESSIONS_TOTAL)
    }

    /// Avec une horloge et des plafonds donnés (les tests éprouvent la garde globale sans douze serveurs).
    pub fn with_limits(clock: Clock, per_server: usize, total: usize) -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            clock,
            per_server,
            total,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Une page démarre (ou redémarre) dans la fenêtre `owner` : elle reçoit une génération neuve, et tout ce qui
    /// restait ouvert pour cette fenêtre (une page rechargée ou plantée n'a pas pu le fermer) l'est maintenant.
    /// Rien n'est envoyé à la page : l'ancienne n'existe plus.
    pub fn announce_page(&self, owner: &str) -> u32 {
        let mut inner = self.lock();
        inner.next_page = inner.next_page.wrapping_add(1).max(1);
        let page = inner.next_page;
        inner.pages.insert(owner.to_owned(), page);
        let stale = drain_where(&mut inner, |found| found.owner == owner);
        drop(inner);
        stop(stale, None);
        page
    }

    /// Ouvre une lecture et rend l'identifiant de sa session. `page` : la génération rendue à cette page par
    /// [`Self::announce_page`], sinon `LogPageUnknown`. Hors « Connecté » : `NotConnected`, rien n'est envoyé.
    /// Au plafond d'un serveur ou global : `LogReadsFull`, sans toucher au serveur.
    pub fn open(
        self: &Arc<Self>,
        manager: &LinkManager,
        sink: Arc<dyn UiSink>,
        owner: &str,
        page: u32,
        server_id: &str,
        key: &str,
    ) -> Result<String, LinkFailure> {
        let id = ServerId::parse(server_id).map_err(|_| LinkFailure::UnknownServer)?;
        let mut inner = self.lock();
        if inner.pages.get(owner) != Some(&page) {
            return Err(LinkFailure::LogPageUnknown);
        }
        let on_server = inner
            .sessions
            .values()
            .filter(|found| found.server == id.as_str())
            .count();
        if on_server >= self.per_server || inner.sessions.len() >= self.total {
            return Err(LinkFailure::LogReadsFull);
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
            sink.clone(),
        ));
        let last_seen = (self.clock)();
        inner.sessions.insert(
            session.clone(),
            Session {
                window,
                task,
                owner: owner.to_owned(),
                page,
                server: id.as_str().to_owned(),
                sink,
                last_seen,
            },
        );
        Ok(session)
    }

    /// Ferme une session : la tâche s'arrête, le socket se ferme, plus aucun événement ne part. Une session
    /// inconnue (déjà finie) n'est pas une erreur.
    pub fn close(&self, session: &str) {
        if let Some(found) = self.lock().sessions.remove(session) {
            found.task.abort();
        }
    }

    /// La fenêtre a affiché `batches` lots de plus. Un accusé est aussi un signe de vie.
    pub fn acknowledge(&self, session: &str, batches: u32) {
        let now = (self.clock)();
        let window = self.lock().sessions.get_mut(session).map(|found| {
            found.last_seen = now;
            found.window.clone()
        });
        if let Some(window) = window {
            window.acknowledge(usize::try_from(batches).unwrap_or(usize::MAX));
        }
    }

    /// Signe de vie de la page `page` de la fenêtre `owner` : rend les sessions de cette page encore ouvertes (de
    /// quoi savoir lesquelles la coquille a fermées entre-temps, même si l'événement s'est perdu). Une page
    /// périmée n'a plus aucune session.
    pub fn keep_alive(&self, owner: &str, page: u32) -> Vec<String> {
        let now = (self.clock)();
        let mut inner = self.lock();
        if inner.pages.get(owner) != Some(&page) {
            return Vec::new();
        }
        let mut alive: Vec<String> = inner
            .sessions
            .iter_mut()
            .filter(|(_, found)| found.owner == owner && found.page == page)
            .map(|(id, found)| {
                found.last_seen = now;
                id.clone()
            })
            .collect();
        alive.sort();
        alive
    }

    /// Ferme les sessions dont l'interface n'a rien signalé depuis [`IDLE_TIMEOUT`] et le lui dit. Rend leur
    /// nombre.
    pub fn reap(&self) -> usize {
        let now = (self.clock)();
        let mut inner = self.lock();
        let expired = drain_where(&mut inner, |found| {
            now.saturating_sub(found.last_seen) > IDLE_TIMEOUT
        });
        drop(inner);
        let count = expired.len();
        stop(expired, Some(LogClosedCauseDto::Idle));
        count
    }

    /// La fenêtre `owner` n'est plus regardée (cachée dans la zone de notification) : ses lectures sont fermées
    /// et la page en est avertie, pour qu'elle EFFACE les lignes et propose de reprendre (`BR-SLOG-019`, `020`).
    pub fn close_window(&self, owner: &str, cause: LogClosedCauseDto) -> usize {
        let mut inner = self.lock();
        let closing = drain_where(&mut inner, |found| found.owner == owner);
        drop(inner);
        let count = closing.len();
        stop(closing, Some(cause));
        count
    }

    /// La fenêtre `owner` est détruite : ses lectures sont fermées sans rien dire (personne ne les lit plus) et sa
    /// génération de page est oubliée.
    pub fn forget_window(&self, owner: &str) -> usize {
        let mut inner = self.lock();
        inner.pages.remove(owner);
        let closing = drain_where(&mut inner, |found| found.owner == owner);
        drop(inner);
        let count = closing.len();
        stop(closing, None);
        count
    }

    /// Ce serveur est déconnecté, changé ou oublié : ses lectures sont fermées et leur page en est avertie.
    pub fn close_server(&self, server_id: &str, cause: LogClosedCauseDto) -> usize {
        let mut inner = self.lock();
        let closing = drain_where(&mut inner, |found| found.server == server_id);
        drop(inner);
        let count = closing.len();
        stop(closing, Some(cause));
        count
    }

    /// Sessions ouvertes (tests).
    pub fn open_count(&self) -> usize {
        self.lock().sessions.len()
    }

    /// Sessions ouvertes pour ce serveur (tests).
    pub fn open_on(&self, server_id: &str) -> usize {
        self.lock()
            .sessions
            .values()
            .filter(|found| found.server == server_id)
            .count()
    }

    /// Ferme tout, sans rien dire à personne : à la destruction de la fenêtre et à la sortie de l'application.
    pub fn close_all(&self) {
        let mut inner = self.lock();
        let all = drain_where(&mut inner, |_| true);
        inner.pages.clear();
        drop(inner);
        stop(all, None);
    }
}

/// Retire de la table les sessions qui répondent au critère.
fn drain_where(inner: &mut Inner, matches: impl Fn(&Session) -> bool) -> Vec<(String, Session)> {
    let ids: Vec<String> = inner
        .sessions
        .iter()
        .filter(|(_, found)| matches(found))
        .map(|(id, _)| id.clone())
        .collect();
    ids.into_iter()
        .filter_map(|id| inner.sessions.remove(&id).map(|found| (id, found)))
        .collect()
}

/// Arrête les tâches (le socket se ferme) ; `cause` : prévient la page de chaque session, une fois la tâche
/// arrêtée (plus aucun lot ne part après).
fn stop(sessions: Vec<(String, Session)>, cause: Option<LogClosedCauseDto>) {
    for (id, found) in sessions {
        found.task.abort();
        if let Some(cause) = cause {
            let event = ServiceLogEvent {
                session: id,
                body: ServiceLogBody::Ended {
                    end: LogEndDto::Closed { cause },
                },
            };
            match serde_json::to_value(&event) {
                Ok(value) => found.sink.emit(EVENT, value),
                Err(_) => tracing::error!("événement de journaux illisible, abandonné"),
            }
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
