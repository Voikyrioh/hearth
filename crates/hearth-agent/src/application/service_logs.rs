//! Lecture des journaux d'un service en direct (HRT-66, ADR-0038, BR-SLOG-*).
//!
//! Une lecture = une [`LogSession`]. Deux côtés qui ne s'attendent jamais :
//! - une tâche LIT le moteur sans jamais s'arrêter et pousse dans un anneau borné (`LogRing`) ; si le
//!   client ne suit pas, l'anneau perd ses lignes les plus anciennes, les compte, et un `gap { slow }`
//!   précède le lot suivant : le moteur n'est jamais bloqué par un client lent (BR-SLOG-007) ;
//! - l'appelant (le socket) VIDE l'anneau par [`LogSession::next_message`], par lots de 100 ms,
//!   500 lignes ou 256 Kio.
//!
//! La lecture suit la CLÉ du service (`u_...`, `c_...`), jamais l'identifiant d'un conteneur : un service
//! géré par le système change de conteneur à chaque démarrage (BR-SVC-016). À chaque changement de
//! conteneur la tâche pose un repère « redémarré » puis lit le nouveau depuis son début (BR-SLOG-009) ;
//! elle relit ce que le service désigne maintenant à chaque tour, par [`LogAccess`], l'UNIQUE point de
//! décision « ce service est-il lisible par ce compte » (`may_read` : créé par Hearth, tous les comptes ; adopté, le
//! droit de gérer les services ; non adopté, personne, comme une clé inconnue, HRT-79). La
//! décision est rejouée à chaque tour ET pendant une lecture longue (période [`LogSettings::access_recheck`],
//! et tout de suite quand la session change le compte vu, [`LogSession::set_viewer`]) : un droit retiré
//! ferme la lecture avec `MISSING_RIGHT` (service adopté, droit retiré) ou `SERVICE_NOT_FOUND` (service qui
//! disparaît, adoption relâchée, ou jamais publié).
//!
//! Reprise (BR-SLOG-014) : le curseur vient du client et n'est jamais cru sur parole
//! ([`check_resume`] : fenêtre de temps, rang plausible, pas dans le futur) ; la relecture qui vise un
//! curseur est plafonnée en lignes ([`RESUME_MAX_LINES`]) ; un conteneur lu depuis son début l'est au plus
//! sur [`FROM_START_LINES`] lignes. Hors de ces bornes : `gap { resume_lost }` puis les dernières lignes.
//!
//! Première page : tant que la tâche lit l'historique (un fichier déjà écrit), elle attend la place dans
//! l'anneau au lieu de le déborder (le moteur n'est ralenti que pour un fichier fini, jamais pour le
//! direct) ; si le client ne lit rien pendant [`LogSettings::history_wait`], elle ne l'attend plus et les
//! lignes évincées sont comptées en `gap { first_page }`, jamais en `slow`.
//!
//! Dix lectures au plus pour tout l'agent ([`ServiceLogs::open`], BR-SLOG-020) ; la place revient quand la
//! [`LogSession`] est lâchée, quelle que soit la raison (socket fermé, coupure, erreur) : la lâcher
//! abandonne aussi la tâche et ferme la lecture du moteur.
//!
//! Rien ici ne garde ni ne trace une ligne (BR-SLOG-019, BR-SLOG-021) : l'anneau d'une lecture est le seul
//! endroit où des lignes attendent, et il meurt avec elle.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use futures_util::StreamExt;
use hearth_proto::api::service_logs::{
    BATCH_BYTES, BATCH_LINES, BATCH_WINDOW_MS, GapReason, LogCursor, LogGap, LogItem, LogLines,
    LogMarker, LogState, LogsError, LogsErrorCode, LogsServerMessage, LogsStatus, MAX_READS,
    MarkerKind, RING_BYTES, RING_LINES, TAIL_LINES,
};
use tokio::sync::{Notify, broadcast, watch};
use tokio::task::JoinHandle;
use tokio::time::{Instant, MissedTickBehavior, interval_at, sleep, timeout, timeout_at};

use super::ports::{Clock, ContainerEngine, EngineError, LogQuery};
use super::services::{ServicesChange, ServicesRegistry};
use crate::domain::services::logs::{
    Entry, FROM_START_LINES, LogContainer, LogEvent, LogLine, LogRing, LogTarget, RESUME_MAX_LINES,
    RESUME_WINDOW_LINES, RankTracker, ReadRefusal, Relation, ResumeCheck, ResumeFilter, Step,
    Verdict, Viewer, check_resume, flush_due, may_read,
};
use crate::domain::services::{EngineState, RunState, ServiceKey};

/// Réglages d'une lecture. Les valeurs par défaut sont celles du protocole ; les tests les resserrent.
#[derive(Debug, Clone, Copy)]
pub struct LogSettings {
    /// Lectures simultanées au plus.
    pub max_reads: usize,
    pub ring_lines: usize,
    pub ring_bytes: usize,
    /// Un service arrêté ou sans conteneur est relu à cette cadence, même sans événement du moteur.
    pub recheck: Duration,
    /// Attente avant de rouvrir une lecture du moteur dont le conteneur a disparu entre la liste et la
    /// lecture (évite de tourner à vide).
    pub reattach_pause: Duration,
    /// Quand le flux d'un conteneur qui tournait finit, la tâche attend au plus cela que le registre
    /// publie du nouveau sur ce service (état, démarrage, identifiant) avant de relire : le registre
    /// regroupe les événements du moteur sur 250 ms, relire plus tôt ferait lire une nouvelle exécution
    /// sans repère de redémarrage (BR-SLOG-009).
    pub restart_grace: Duration,
    /// Pendant une lecture, la décision de lecture est rejouée à cette cadence (en plus de chaque
    /// changement du compte vu par la session et de chaque changement de conteneur).
    pub access_recheck: Duration,
    /// Combien de temps la lecture de la première page attend que le client vide l'anneau avant de
    /// renoncer à l'attendre (BR-SLOG-001).
    pub history_wait: Duration,
}

impl Default for LogSettings {
    fn default() -> Self {
        Self {
            max_reads: MAX_READS,
            ring_lines: RING_LINES,
            ring_bytes: RING_BYTES,
            recheck: Duration::from_secs(2),
            reattach_pause: Duration::from_millis(150),
            restart_grace: Duration::from_secs(2),
            access_recheck: Duration::from_secs(2),
            history_wait: Duration::from_secs(5),
        }
    }
}

/// Ce que le client demande à l'ouverture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenRequest {
    /// Nombre de dernières lignes (plafonné à `TAIL_LINES`).
    pub tail: u32,
    /// Reprise après une coupure : à la place des dernières lignes.
    pub since: Option<LogCursor>,
}

/// La décision de lecture : `may_read` du domaine. Une autre ne s'injecte que pour tester qu'un refus en
/// cours de lecture ferme la lecture ([`ServiceLogs::with_policy`]) ; l'agent n'en met jamais d'autre.
pub type ReadPolicy = Arc<dyn Fn(&Viewer, &LogTarget) -> Result<(), ReadRefusal> + Send + Sync>;

/// L'unique décision « ce service est-il lisible par ce compte » (voir le module). La lecture du moteur
/// ne la prend jamais elle-même : `engine.logs` ne connaît ni rôle ni compte. Le compte est celui que la
/// session montre À CE MOMENT (une cellule que le socket met à jour), pas celui de l'ouverture.
pub struct LogAccess {
    registry: Arc<ServicesRegistry>,
    viewer: watch::Receiver<Viewer>,
    policy: ReadPolicy,
}

impl LogAccess {
    /// Ce que la clé désigne maintenant, ou pourquoi ce compte ne le lit pas. Un service qui n'existe
    /// pas et un service que ce compte ne peut pas lire donnent la même réponse (aucun oracle).
    pub fn target(&self, key: &ServiceKey) -> Result<LogTarget, LogsErrorCode> {
        let target = self.registry.log_target(key);
        let verdict = {
            let viewer = self.viewer.borrow();
            (self.policy)(&viewer, &target)
        };
        match verdict {
            // Un conteneur non adopté : la réponse d'une clé inconnue, avant tout appel au moteur.
            Err(ReadRefusal::NotEnabled) => Err(LogsErrorCode::ServiceNotFound),
            // Un service adopté, donc dans la liste de ce compte : refus explicite (BR-SLOG-018). Le
            // domaine ne le rend que pour une origine adoptée, jamais pour un caché ni un inconnu.
            Err(ReadRefusal::MissingRight) => Err(LogsErrorCode::MissingRight),
            Ok(()) if target == LogTarget::Missing => Err(LogsErrorCode::ServiceNotFound),
            Ok(()) => Ok(target),
        }
    }

    /// Ce que le registre publie pour cette clé, SANS décision de droit : sert seulement à voir si
    /// quelque chose a changé.
    fn peek(&self, key: &ServiceKey) -> LogTarget {
        self.registry.log_target(key)
    }
}

/// Le service des lectures de journaux : le registre (qui désigne le conteneur), le moteur (qui rend
/// les lignes), et le compte des lectures ouvertes.
pub struct ServiceLogs {
    registry: Arc<ServicesRegistry>,
    engine: Arc<dyn ContainerEngine>,
    clock: Arc<dyn Clock>,
    settings: LogSettings,
    reads: Arc<AtomicUsize>,
    policy: ReadPolicy,
}

/// Une place de lecture, rendue quand elle est lâchée.
struct Permit {
    reads: Arc<AtomicUsize>,
}

impl Drop for Permit {
    fn drop(&mut self) {
        self.reads.fetch_sub(1, Ordering::SeqCst);
    }
}

impl ServiceLogs {
    pub fn new(
        registry: Arc<ServicesRegistry>,
        engine: Arc<dyn ContainerEngine>,
        clock: Arc<dyn Clock>,
        settings: LogSettings,
    ) -> Self {
        Self {
            registry,
            engine,
            clock,
            settings,
            reads: Arc::new(AtomicUsize::new(0)),
            policy: Arc::new(may_read),
        }
    }

    /// Remplace la décision de lecture. Réservé aux tests du contrôle d'accès : l'agent de production
    /// n'appelle jamais ceci, la décision y est `may_read`.
    #[must_use]
    pub fn with_policy(mut self, policy: ReadPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Lectures ouvertes en ce moment.
    pub fn open_reads(&self) -> usize {
        self.reads.load(Ordering::SeqCst)
    }

    fn acquire(&self) -> Result<Permit, LogsErrorCode> {
        let mut current = self.reads.load(Ordering::SeqCst);
        loop {
            if current >= self.settings.max_reads {
                return Err(LogsErrorCode::LogsLimit);
            }
            match self.reads.compare_exchange(
                current,
                current + 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => {
                    return Ok(Permit {
                        reads: self.reads.clone(),
                    });
                }
                Err(actual) => current = actual,
            }
        }
    }

    /// Ouvre une lecture. Les refus connus d'avance sont rendus tout de suite : limite de lectures,
    /// moteur pas prêt, service inconnu ou non lisible, journaux non relisibles.
    pub fn open(
        &self,
        viewer: Viewer,
        key: ServiceKey,
        request: OpenRequest,
    ) -> Result<LogSession, LogsErrorCode> {
        let permit = self.acquire()?;
        if self.registry.engine_status().state != EngineState::Ready {
            return Err(LogsErrorCode::EngineUnavailable);
        }
        let (viewer_tx, viewer_rx) = watch::channel(viewer);
        let access = LogAccess {
            registry: self.registry.clone(),
            viewer: viewer_rx,
            policy: self.policy.clone(),
        };
        if let LogTarget::Container(container) = access.target(&key)?
            && !container.readable
        {
            return Err(LogsErrorCode::LogsUnreadable);
        }
        let shared = Arc::new(Shared {
            ring: Mutex::new(LogRing::new(
                self.settings.ring_lines,
                self.settings.ring_bytes,
            )),
            wake: Notify::new(),
            space: Notify::new(),
            finished: AtomicBool::new(false),
        });
        let (changes, _) = self.registry.subscribe();
        let reader = Reader {
            shared: shared.clone(),
            engine: self.engine.clone(),
            access,
            key,
            clock: self.clock.clone(),
            changes,
            settings: self.settings,
            tail: request.tail.min(TAIL_LINES),
            since: request.since,
            last: None,
            last_status: None,
            attached: None,
            read_done: None,
            owed_marker: None,
            clock_rewound: false,
        };
        let task = tokio::spawn(reader.run());
        Ok(LogSession {
            shared,
            task,
            viewer: viewer_tx,
            _permit: permit,
            window_started: None,
            seq: 0,
        })
    }
}

/// Ce que partagent la tâche qui lit le moteur et le côté qui écrit sur le socket.
struct Shared {
    ring: Mutex<LogRing>,
    /// Réveille le côté socket : une ligne, un état, une fin.
    wake: Notify,
    /// Réveille la tâche qui attend de la place dans l'anneau : le socket vient d'en vider une partie.
    space: Notify,
    /// La tâche de lecture est finie (après une erreur terminale).
    finished: AtomicBool,
}

impl Shared {
    fn ring(&self) -> MutexGuard<'_, LogRing> {
        self.ring.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Une lecture ouverte. La lâcher libère sa place, abandonne la tâche et ferme la lecture du moteur.
pub struct LogSession {
    shared: Arc<Shared>,
    task: JoinHandle<()>,
    viewer: watch::Sender<Viewer>,
    _permit: Permit,
    /// Depuis quand des lignes attendent dans l'anneau (la fenêtre de 100 ms).
    window_started: Option<Instant>,
    seq: u64,
}

impl Drop for LogSession {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl LogSession {
    /// Le prochain message à envoyer au client, ou `None` quand la lecture est finie (une erreur
    /// terminale a été rendue). Annulable : l'état de la fenêtre est dans la session, pas dans l'appel.
    pub async fn next_message(&mut self) -> Option<LogsServerMessage> {
        loop {
            let wait = {
                let mut ring = self.shared.ring();
                if ring.is_empty() {
                    self.window_started = None;
                    if self.shared.finished.load(Ordering::SeqCst) {
                        return None;
                    }
                    None
                } else {
                    let started = *self.window_started.get_or_insert_with(Instant::now);
                    if flush_due(
                        ring.head_is_line(),
                        ring.lines(),
                        ring.bytes(),
                        started.elapsed(),
                    ) && let Some(step) = ring.pop_step(BATCH_LINES, BATCH_BYTES)
                    {
                        self.window_started = None;
                        drop(ring);
                        self.shared.space.notify_one();
                        return Some(self.message_of(step));
                    }
                    Some(started + Duration::from_millis(BATCH_WINDOW_MS))
                }
            };
            match wait {
                Some(until) => {
                    let _ = timeout_at(until, self.shared.wake.notified()).await;
                }
                None => self.shared.wake.notified().await,
            }
        }
    }

    /// Le compte tel que la session le montre maintenant : la lecture rejoue sa décision de lecture
    /// tout de suite si quelque chose a changé (rôle, droit de gérer les services).
    pub fn set_viewer(&self, viewer: Viewer) {
        self.viewer.send_if_modified(|current| {
            if *current == viewer {
                false
            } else {
                *current = viewer;
                true
            }
        });
    }

    /// Lignes perdues par cet anneau depuis l'ouverture (la trace de fermeture les compte).
    pub fn lost(&self) -> u64 {
        self.shared.ring().lost()
    }

    fn message_of(&mut self, step: Step) -> LogsServerMessage {
        match step {
            Step::Lines(entries) => {
                self.seq += 1;
                let cursor = entries
                    .last()
                    .map(|entry| entry.cursor)
                    .unwrap_or(LogCursor { ts: 0, rank: 0 });
                LogsServerMessage::Lines(LogLines {
                    seq: self.seq,
                    items: entries
                        .into_iter()
                        .map(|entry| LogItem {
                            t: entry.line.at_ms,
                            s: entry.line.source,
                            x: entry.line.text,
                            cut: entry.line.cut,
                        })
                        .collect(),
                    cursor,
                })
            }
            Step::Gap(gap) => LogsServerMessage::Gap(gap),
            Step::Event(LogEvent::Status(status)) => LogsServerMessage::Status(status),
            Step::Event(LogEvent::Marker(marker)) => LogsServerMessage::Marker(marker),
            Step::Event(LogEvent::Error(code)) => LogsServerMessage::Error(LogsError { code }),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// La tâche qui lit le moteur
// ---------------------------------------------------------------------------------------------

/// Le conteneur et le démarrage dont les lignes ont déjà été lues.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Run {
    id: String,
    started_ms: Option<i64>,
}

impl Run {
    fn of(container: &LogContainer) -> Self {
        Self {
            id: container.id.clone(),
            started_ms: container.started_ms,
        }
    }
}

/// Ce qui finit une tentative de lecture d'un conteneur.
enum Outcome {
    /// Le flux du moteur a fini : on relit ce que le service désigne maintenant.
    Ended,
    /// Erreur terminale pour le client.
    Fail(LogsErrorCode),
}

struct Reader {
    shared: Arc<Shared>,
    engine: Arc<dyn ContainerEngine>,
    access: LogAccess,
    key: ServiceKey,
    clock: Arc<dyn Clock>,
    changes: broadcast::Receiver<Arc<ServicesChange>>,
    settings: LogSettings,
    tail: u32,
    /// Le curseur de l'ouverture, tant qu'il n'a pas servi.
    since: Option<LogCursor>,
    /// Curseur de la dernière ligne poussée.
    last: Option<LogCursor>,
    last_status: Option<LogsStatus>,
    /// Le conteneur et le démarrage auxquels les lignes déjà poussées appartiennent.
    attached: Option<Run>,
    /// Une exécution arrêtée dont toutes les lignes sont lues : inutile de la relire.
    read_done: Option<Run>,
    /// Un repère « redémarré » dû, à poser juste avant la première ligne écrite à partir de cette heure
    /// (millisecondes) : les dernières lignes de l'exécution précédente, que le client n'a pas encore, le
    /// précèdent.
    owed_marker: Option<i64>,
    /// L'heure du moteur a reculé pendant une lecture : le curseur de la dernière ligne ne retrouve plus
    /// ses lignes par `since`, la prochaine lecture de ce service repart des dernières lignes.
    clock_rewound: bool,
}

/// Marque la fin de la tâche, même abandonnée : le côté socket voit `None` une fois l'anneau vide.
struct FinishGuard(Arc<Shared>);

impl Drop for FinishGuard {
    fn drop(&mut self) {
        self.0.finished.store(true, Ordering::SeqCst);
        self.0.wake.notify_one();
    }
}

impl Reader {
    async fn run(mut self) {
        let _finish = FinishGuard(self.shared.clone());
        loop {
            let code = match self.access.target(&self.key) {
                Err(code) => code,
                Ok(LogTarget::Missing) => LogsErrorCode::ServiceNotFound,
                Ok(LogTarget::NoContainer { .. }) => {
                    // Un service géré par le système dont le conteneur est supprimé à l'arrêt : le moteur
                    // n'a rien gardé. La lecture repart quand le système le relance (BR-SLOG-008).
                    self.status(LogsStatus {
                        state: LogState::Waiting,
                        container: None,
                        mixed: false,
                        exit_code: None,
                        ended_at: None,
                        not_kept: true,
                    });
                    self.wait_for_change().await;
                    continue;
                }
                Ok(LogTarget::Container(container)) => {
                    if !container.readable {
                        LogsErrorCode::LogsUnreadable
                    } else {
                        match self.follow_container(&container).await {
                            Outcome::Ended => continue,
                            Outcome::Fail(code) => code,
                        }
                    }
                }
            };
            self.push_event(LogEvent::Error(code));
            return;
        }
    }

    /// Une tentative de lecture de ce conteneur : repère éventuel, état, ouverture du flux, lignes.
    async fn follow_container(&mut self, container: &LogContainer) -> Outcome {
        let run = Run::of(container);
        let live = container.is_live();
        if !live && self.read_done.as_ref() == Some(&run) {
            self.status(status_of(container));
            self.wait_for_change().await;
            return Outcome::Ended;
        }
        // Créé, jamais démarré : rien à lire, le client attend son démarrage (BR-SLOG-023).
        if container.run_state == RunState::Created {
            self.status(LogsStatus {
                state: LogState::Waiting,
                container: Some(container.short_id()),
                mixed: container.tty,
                exit_code: None,
                ended_at: None,
                not_kept: false,
            });
            self.wait_for_change().await;
            return Outcome::Ended;
        }
        let plan = self.plan(container);
        let started = container
            .started_ms
            .or(container.created_ms)
            .unwrap_or_else(|| millis(self.clock.now()));
        match plan.marker {
            Marker::None => {}
            // Un autre conteneur : rien de l'exécution précédente ne le suit, le repère passe d'abord.
            Marker::Now => self.push_marker(started),
            // Le même conteneur, démarré de nouveau : le repère se pose entre les deux exécutions.
            Marker::AtStart => self.owed_marker = Some(started),
        }
        self.status(status_of(container));
        if plan.lost {
            // La reprise demandée n'est pas acceptée (curseur trop ancien, futur, illisible, horloge
            // reculée) : un trou dont on ne sait pas la taille, puis les dernières lignes (BR-SLOG-014).
            self.push_gap(LogGap {
                count: None,
                reason: GapReason::ResumeLost,
            });
        }
        let mut query = plan.query;
        query.follow = live;
        query.tty = container.tty;
        // La première page : tant que ces lignes arrivent, la tâche attend la place dans l'anneau.
        let mut history_left = if plan.resume.is_none() {
            usize::try_from(query.tail.unwrap_or(0)).unwrap_or(0)
        } else {
            0
        };
        let mut waits_for_room = true;
        let opened_ms = millis(self.clock.now());
        let mut filter = plan.resume.map(ResumeFilter::new);
        let mut skipped = 0usize;
        let mut tracker = RankTracker::new();
        let mut lost = false;
        let mut window_checked = false;
        let probe = LogQuery {
            follow: false,
            ..query
        };
        let mut stream = self.engine.logs(&container.id, query);
        let mut access_tick = interval_at(
            Instant::now() + self.settings.access_recheck,
            self.settings.access_recheck,
        );
        access_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut viewer_open = true;
        loop {
            let item = tokio::select! {
                item = stream.next() => match item {
                    Some(item) => item,
                    None => break,
                },
                _ = access_tick.tick() => {
                    if let Some(code) = self.refused_now() {
                        return Outcome::Fail(code);
                    }
                    continue;
                }
                changed = self.access.viewer.changed(), if viewer_open => {
                    viewer_open = changed.is_ok();
                    if let Some(code) = self.refused_now() {
                        return Outcome::Fail(code);
                    }
                    continue;
                }
            };
            match item {
                Ok(line) => {
                    let cursor = tracker.next(line.at_ms);
                    if let Some(filter) = filter.as_mut() {
                        // La fenêtre demandée (`tail`) peut avoir été coupée au milieu de la milliseconde du
                        // curseur : les rangs recalculés seraient sous-estimés et des lignes non reçues
                        // sautées sans trou. Une fenêtre qui n'est pas pleine n'a rien perdu.
                        if !window_checked {
                            window_checked = true;
                            if plan.resume.is_some_and(|target| target.ts == cursor.ts)
                                && self.window_is_full(&container.id, probe).await
                            {
                                lost = true;
                                break;
                            }
                        }
                        match filter.judge(cursor) {
                            Verdict::Skip => {
                                // La ligne du curseur est vue : tout ce qui suit est nouveau, même si rien
                                // ne vient (une relecture ne renvoie jamais ce que le client a déjà).
                                if filter.passed() && self.last.is_none() {
                                    self.last = plan.resume;
                                }
                                // Un curseur qu'on n'atteint pas en un nombre raisonnable de lignes est
                                // tenu pour perdu : le moteur ne relit pas tout un journal pour lui.
                                skipped += 1;
                                if !filter.passed() && skipped > RESUME_MAX_LINES {
                                    lost = true;
                                    break;
                                }
                                continue;
                            }
                            Verdict::Take => {}
                            Verdict::Lost => {
                                lost = true;
                                break;
                            }
                        }
                    }
                    // Une ligne de l'historique : déjà écrite quand la lecture s'est ouverte (tout ce que rend
                    // un conteneur arrêté ; pour un conteneur qui tourne, ce qui précède l'ouverture, dans la
                    // limite de ce qui a été demandé). Une ligne écrite depuis est du direct.
                    let history = history_left > 0 && (!live || line.at_ms <= opened_ms);
                    if history {
                        history_left -= 1;
                    }
                    if history && waits_for_room {
                        waits_for_room = self.wait_for_room(line.weight()).await;
                    }
                    self.push_line(line, cursor, history);
                }
                Err(EngineError::Unsupported) => {
                    return Outcome::Fail(LogsErrorCode::LogsUnreadable);
                }
                // Supprimé entre la liste et la lecture : le registre va le dire, on relit.
                Err(EngineError::NotFound) => {
                    drop(stream);
                    self.attached = Some(run);
                    sleep(self.settings.reattach_pause).await;
                    return Outcome::Ended;
                }
                Err(_) => return Outcome::Fail(LogsErrorCode::EngineUnavailable),
            }
        }
        drop(stream);
        if tracker.rewound() {
            self.clock_rewound = true;
        }
        if filter.as_ref().is_some_and(|filter| !filter.passed()) {
            lost = true;
        }
        if lost {
            // La ligne de reprise n'existe plus : un trou dont on ne sait pas la taille, puis les
            // dernières lignes (BR-SLOG-014). Rien n'a été poussé de cette lecture.
            self.push_gap(LogGap {
                count: None,
                reason: GapReason::ResumeLost,
            });
            self.since = None;
            self.last = None;
            self.attached = Some(run);
            self.tail = TAIL_LINES;
            return Outcome::Ended;
        }
        self.since = None;
        // Le repère dû, si l'exécution suivante n'a encore écrit aucune ligne.
        if let Some(at) = self.owed_marker.take() {
            self.push_marker(at);
        }
        self.attached = Some(run.clone());
        if !live {
            self.read_done = Some(run);
        } else {
            // Le conteneur qui tournait ne répond plus : il s'est arrêté ou a redémarré. Le registre
            // l'apprend avec un peu de retard (il regroupe les événements) : relire avant qu'il ait publié
            // le nouveau démarrage ferait lire la nouvelle exécution sans repère (BR-SLOG-009).
            if let Some(code) = self.wait_registry_moves(container).await {
                return Outcome::Fail(code);
            }
        }
        Outcome::Ended
    }

    /// La fenêtre de reprise rendue par le moteur est-elle pleine (`tail` atteint) ? Relit la même fenêtre
    /// sans suivre, pour la compter ; une erreur compte comme pleine (la reprise est alors perdue).
    async fn window_is_full(&self, id: &str, probe: LogQuery) -> bool {
        let window = usize::try_from(probe.tail.unwrap_or(0)).unwrap_or(usize::MAX);
        let mut stream = self.engine.logs(id, probe);
        let mut seen = 0usize;
        while seen < window {
            match stream.next().await {
                Some(Ok(_)) => seen += 1,
                Some(Err(_)) => return true,
                None => return false,
            }
        }
        true
    }

    /// La décision de lecture, rejouée maintenant : `Some(code)` si la lecture doit se fermer.
    fn refused_now(&self) -> Option<LogsErrorCode> {
        self.access.target(&self.key).err()
    }

    /// Attend de la place dans l'anneau pour une ligne de la première page. `false` : le client n'a rien
    /// vidé pendant `history_wait`, on ne l'attend plus.
    async fn wait_for_room(&self, weight: usize) -> bool {
        let deadline = Instant::now() + self.settings.history_wait;
        loop {
            if self.shared.ring().has_room_for(weight) {
                return true;
            }
            if timeout_at(deadline, self.shared.space.notified())
                .await
                .is_err()
            {
                return false;
            }
        }
    }

    /// Attend, au plus `restart_grace`, que le registre publie du nouveau pour ce service (autre état,
    /// autre démarrage, autre conteneur, service disparu). `Some(code)` si la décision de lecture a
    /// changé entre-temps.
    async fn wait_registry_moves(&mut self, container: &LogContainer) -> Option<LogsErrorCode> {
        let deadline = Instant::now() + self.settings.restart_grace;
        let followed = LogTarget::Container(container.clone());
        loop {
            if let Some(code) = self.refused_now() {
                return Some(code);
            }
            if self.access.peek(&self.key) != followed {
                return None;
            }
            match timeout_at(deadline, self.changes.recv()).await {
                Err(_) => return None,
                Ok(Err(broadcast::error::RecvError::Closed)) => {
                    sleep(deadline.saturating_duration_since(Instant::now())).await;
                    return None;
                }
                Ok(_) => {}
            }
        }
    }

    /// Ce qu'il faut lire de ce conteneur, et si un repère « redémarré » précède.
    fn plan(&mut self, container: &LogContainer) -> Plan {
        let run = Run::of(container);
        let fresh = LogQuery {
            follow: false,
            tail: None,
            since_ms: None,
            tty: false,
        };
        // Un conteneur lu « depuis son début » ne l'est jamais sans borne : au plus un anneau de lignes.
        let from_start = LogQuery {
            tail: Some(FROM_START_LINES),
            ..fresh
        };
        // Les dernières lignes, sans reprise.
        let last_lines = LogQuery {
            tail: Some(self.tail),
            ..fresh
        };
        // Reprise de l'ouverture : le curseur dit où le client s'est arrêté, s'il est plausible.
        if self.attached.is_none() {
            return match self.since.take() {
                None => Plan {
                    marker: Marker::None,
                    resume: None,
                    lost: false,
                    query: last_lines,
                },
                Some(cursor)
                    if check_resume(cursor, millis(self.clock.now())) == ResumeCheck::Lost =>
                {
                    Plan {
                        marker: Marker::None,
                        resume: None,
                        lost: true,
                        query: last_lines,
                    }
                }
                Some(cursor) => match container.relation_to(cursor) {
                    Relation::NewContainer => Plan {
                        marker: Marker::Now,
                        resume: None,
                        lost: false,
                        query: from_start,
                    },
                    relation => Plan {
                        marker: if relation == Relation::Restarted {
                            Marker::AtStart
                        } else {
                            Marker::None
                        },
                        resume: Some(cursor),
                        lost: false,
                        query: resume_query(cursor),
                    },
                },
            };
        }
        let previous = self.attached.clone();
        // Un autre conteneur (recréé par le système) : ses lignes depuis son début.
        if previous.as_ref().map(|p| &p.id) != Some(&run.id) {
            self.last = None;
            return Plan {
                marker: Marker::Now,
                resume: None,
                lost: false,
                query: from_start,
            };
        }
        // Le même conteneur, démarré de nouveau : un repère, puis la suite de ce qu'on a lu.
        let marker = if previous.as_ref().map(|p| p.started_ms) != Some(run.started_ms) {
            Marker::AtStart
        } else {
            Marker::None
        };
        // L'heure du moteur a reculé : le curseur de la dernière ligne ne retrouverait plus ses lignes par
        // `since`. Pas de reprise par curseur : un trou, puis les dernières lignes (BR-SLOG-014).
        if self.clock_rewound {
            self.clock_rewound = false;
            if self.last.take().is_some() {
                return Plan {
                    marker,
                    resume: None,
                    lost: true,
                    query: last_lines,
                };
            }
        }
        match self.last {
            Some(cursor) => Plan {
                marker,
                resume: Some(cursor),
                lost: false,
                query: resume_query(cursor),
            },
            // Rien n'a encore été lu de ce conteneur (ou la reprise a été perdue) : ses dernières lignes.
            None => Plan {
                marker,
                resume: None,
                lost: false,
                query: last_lines,
            },
        }
    }

    /// Attend qu'un service change (événement du moteur) ou que la période de relecture passe.
    async fn wait_for_change(&mut self) {
        if let Ok(Err(broadcast::error::RecvError::Closed)) =
            timeout(self.settings.recheck, self.changes.recv()).await
        {
            sleep(self.settings.recheck).await;
        }
    }

    fn status(&mut self, status: LogsStatus) {
        if self.last_status.as_ref() != Some(&status) {
            self.last_status = Some(status.clone());
            self.push_event(LogEvent::Status(status));
        }
    }

    fn push_marker(&self, at: i64) {
        self.push_event(LogEvent::Marker(LogMarker {
            kind: MarkerKind::Restarted,
            at,
        }));
    }

    fn push_line(&mut self, line: LogLine, cursor: LogCursor, history: bool) {
        if let Some(at) = self.owed_marker
            && line.at_ms >= at
        {
            self.owed_marker = None;
            self.push_marker(at);
        }
        self.last = Some(cursor);
        self.shared.ring().push_line(Entry {
            line,
            cursor,
            history,
        });
        self.shared.wake.notify_one();
    }

    fn push_gap(&self, gap: LogGap) {
        self.shared.ring().push_gap(gap);
        self.shared.wake.notify_one();
    }

    fn push_event(&self, event: LogEvent) {
        self.shared.ring().push_event(event);
        self.shared.wake.notify_one();
    }
}

/// Où le repère « redémarré » se pose.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Marker {
    None,
    /// Tout de suite, avant l'état et les lignes.
    Now,
    /// Juste avant la première ligne du nouveau démarrage.
    AtStart,
}

struct Plan {
    marker: Marker,
    resume: Option<LogCursor>,
    /// La reprise n'est pas acceptée : `gap { resume_lost }` avant les dernières lignes.
    lost: bool,
    query: LogQuery,
}

/// La requête qui reprend après un curseur : toujours bornée par `tail` (jamais « tout le journal gardé »).
fn resume_query(cursor: LogCursor) -> LogQuery {
    LogQuery {
        follow: false,
        tail: Some(RESUME_WINDOW_LINES),
        since_ms: Some(cursor.ts),
        tty: false,
    }
}

fn status_of(container: &LogContainer) -> LogsStatus {
    let live = container.is_live();
    LogsStatus {
        state: if live {
            LogState::Live
        } else {
            LogState::Ended
        },
        container: Some(container.short_id()),
        mixed: container.tty,
        exit_code: if live { None } else { container.exit_code },
        ended_at: if live { None } else { container.finished_ms },
        not_kept: false,
    }
}

fn millis(at: time::OffsetDateTime) -> i64 {
    i64::try_from(at.unix_timestamp_nanos() / 1_000_000).unwrap_or(i64::MAX)
}
