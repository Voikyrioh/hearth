//! Le registre des services et l'état du moteur de conteneurs, tenus à jour en direct (HRT-61,
//! `BR-ENGINE-009` à `BR-ENGINE-013`, `BR-SVC-001`, `BR-SVC-015`).
//!
//! Le registre vit en mémoire et se reconstruit depuis le moteur à chaque démarrage de l'agent : il
//! n'écrit rien dans le moteur ni dans la base (lecture seule). Une tâche (`run`) :
//! 1. sonde le moteur (`/_ping` toutes les 2 s quand il est prêt, toutes les 5 s sinon) ;
//! 2. quand il est prêt, relit tous les conteneurs puis suit les événements du moteur : les
//!    événements d'un lot de 250 ms sont regroupés, chaque conteneur touché est relu (un seul) ;
//! 3. si le flux d'événements se rompt, sonde tout de suite puis relit tout.
//!
//! Un conteneur dont l'inspection échoue (disparu, réponse illisible, délai) ne gèle pas la liste :
//! il garde sa dernière observation, ou apparaît « non reconnu » s'il est nouveau, et la relecture
//! est retentée en reculant (2 s, 4 s, 8 s… jusqu'à 60 s). Une liste refusée (401, 403) alors que la
//! sonde répond donne « accès refusé », pas « prêt » avec une liste vide.
//!
//! L'état du moteur passe par `Debounce` : au plus un changement affiché toutes les 5 secondes.
//! Chaque changement est un lot (`ServicesChange`) diffusé aux abonnés ; un abonné trop lent
//! (`RecvError::Lagged`) relit un instantané complet. `rev` croît à chaque changement et repart de
//! zéro au redémarrage.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use futures_util::{StreamExt, stream};
use time::OffsetDateTime;
use tokio::sync::{Notify, broadcast};
use tokio::time::{Instant, MissedTickBehavior, interval, sleep, sleep_until};

use crate::application::ports::{
    Clock, ContainerEngine, EngineError, HostDescription, HostEnvironment, MonotonicClock,
};
use crate::domain::services::{
    Announced, Debounce, Derived, EngineProbe, EngineState, Observation, ProbeOutcome, RunState,
    ServiceKey, StartWindow, classify_probe, compare_names, derive_state,
};

/// Capacité de la diffusion : un abonné qui prend plus de 64 lots de retard relit un instantané.
const CHANNEL_CAPACITY: usize = 64;
/// Inspections simultanées au plus lors de la relecture des conteneurs touchés par un lot d'événements.
const REFRESH_CONCURRENCY: usize = 8;
/// Délai de la relecture des conteneurs touchés : au-delà, le moteur est à revérifier.
const REFRESH_TIMEOUT: Duration = Duration::from_secs(30);
/// La relecture qui échoue ou laisse des conteneurs illisibles est retentée au plus toutes les
/// `ready_period` x 30 (60 s en production).
const MAX_BACKOFF_FACTOR: u32 = 30;

/// Les cadences du registre. Celles de production sont celles de la conception ; les tests les
/// raccourcissent.
#[derive(Debug, Clone, Copy)]
pub struct EngineTimings {
    /// Entre deux sondes quand le moteur est prêt (`/_ping` toutes les 2 s).
    pub ready_period: Duration,
    /// Entre deux sondes sinon.
    pub other_period: Duration,
    /// Un changement d'état du moteur affiché au plus une fois par fenêtre (`BR-ENGINE-013`).
    pub debounce_window: Duration,
    /// Les événements du moteur reçus pendant cette fenêtre sont regroupés en un lot.
    pub batch_window: Duration,
    /// « Vérifier à nouveau » plus rapproché que cela rend l'état courant sans sonder.
    pub min_recheck: Duration,
}

impl EngineTimings {
    pub const fn production() -> Self {
        Self {
            ready_period: Duration::from_secs(2),
            other_period: Duration::from_secs(5),
            debounce_window: Duration::from_secs(5),
            batch_window: Duration::from_millis(250),
            min_recheck: Duration::from_secs(1),
        }
    }
}

/// L'état du moteur tel que l'agent le publie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineStatus {
    pub state: EngineState,
    /// Nom et version annoncés par le moteur, seulement s'il a répondu.
    pub name: Option<String>,
    pub version: Option<String>,
    pub last_ready_at: Option<OffsetDateTime>,
    pub checked_at: OffsetDateTime,
    /// Dernière lecture COMPLÈTE réussie de la liste (`checked_at` date la sonde, pas la liste).
    pub list_read_at: Option<OffsetDateTime>,
    /// La liste affichée est peut-être périmée : lecture en échec, conteneurs illisibles ou suivi en
    /// direct suspendu. Le client dit « Impossible de lire les services » et date la liste.
    pub list_degraded: bool,
}

/// Un service tel que la liste le montre : le conteneur observé et ce qu'on en dérive.
#[derive(Debug, Clone, PartialEq)]
pub struct ServiceView {
    pub key: ServiceKey,
    pub observation: Observation,
    pub derived: Derived,
    /// Démarrages non demandés des 10 dernières minutes.
    pub restarts_10m: u32,
}

/// L'état complet : moteur et services (triés par nom sans tenir compte de la casse).
#[derive(Debug, Clone, PartialEq)]
pub struct ServicesSnapshot {
    pub rev: u64,
    pub engine: EngineStatus,
    pub host: HostDescription,
    pub services: Vec<ServiceView>,
}

/// Un lot de changements.
#[derive(Debug, Clone, PartialEq)]
pub struct ServicesChange {
    pub rev: u64,
    /// Présent seulement si l'état du moteur (ou son nom, sa version) a changé.
    pub engine: Option<EngineStatus>,
    pub upserts: Vec<ServiceView>,
    pub removed: Vec<ServiceKey>,
}

/// Ce qui change l'état publié du moteur (hors dates de contrôle, qui changent à chaque sonde).
type EngineKey = (EngineState, Option<String>, Option<String>, bool);

struct Inner {
    rev: u64,
    debounce: Debounce,
    /// Ce que la dernière sonde a classé (l'état publié peut être en retard de la fenêtre).
    observed: EngineState,
    /// Nom et version annoncés par le moteur pour l'état publié.
    shown: Option<Announced>,
    /// Ce que la dernière sonde a annoncé (publié avec l'état qui la suit, pas avant).
    last_announced: Option<Announced>,
    last_ready_at: Option<OffsetDateTime>,
    checked_at: OffsetDateTime,
    /// Instant monotone de la dernière sonde lancée.
    last_probe: Option<Duration>,
    containers: HashMap<String, Observation>,
    windows: HashMap<ServiceKey, StartWindow>,
    published_engine: EngineKey,
    published: HashMap<ServiceKey, ServiceView>,
    /// Conteneurs vus dans la liste mais illisibles à la dernière relecture complète.
    degraded: usize,
    /// La liste a été refusée (401, 403) alors que la sonde répond : l'accès est refusé.
    list_denied: bool,
    /// Sondes en cours (un compteur : la sonde de la boucle et celle d'une relance peuvent se
    /// chevaucher) ; une relance demandée pendant ce temps n'en lance pas une autre.
    probing: u32,
    /// Date de la dernière relecture complète réussie (sans conteneur illisible).
    list_read_at: Option<OffsetDateTime>,
    /// La dernière relecture de la liste a échoué (tant qu'une relecture n'a pas réussi).
    read_failing: bool,
    /// Le suivi en direct est dégradé : flux d'événements qui casse en boucle, ou conteneur touché
    /// par un événement et illisible. Une relecture complète propre le rétablit.
    tracking_lost: bool,
}

/// Une sonde en cours : retirée du compte quand elle finit OU qu'elle est abandonnée en route.
struct ProbingFlag<'a>(&'a ServicesRegistry);

impl Drop for ProbingFlag<'_> {
    fn drop(&mut self) {
        let mut inner = self.0.lock();
        inner.probing = inner.probing.saturating_sub(1);
    }
}

pub struct ServicesRegistry {
    engine: Arc<dyn ContainerEngine>,
    clock: Arc<dyn Clock>,
    monotonic: Arc<dyn MonotonicClock>,
    host: HostDescription,
    /// `engine.enabled` : à `false`, l'agent n'ouvre jamais le socket.
    enabled: bool,
    timings: EngineTimings,
    state: Mutex<Inner>,
    changes: broadcast::Sender<Arc<ServicesChange>>,
    /// Réveille la tâche (« Vérifier à nouveau »).
    wake: Notify,
}

impl ServicesRegistry {
    pub fn new(
        engine: Arc<dyn ContainerEngine>,
        host: &dyn HostEnvironment,
        clock: Arc<dyn Clock>,
        monotonic: Arc<dyn MonotonicClock>,
        enabled: bool,
        timings: EngineTimings,
    ) -> Arc<Self> {
        let now = monotonic.elapsed().unsigned_abs();
        let mut debounce = Debounce::new(timings.debounce_window, EngineState::Checking);
        // Désactivé : l'état est connu tout de suite, sans sonde, et ne changera plus.
        let observed = if enabled {
            EngineState::Checking
        } else {
            EngineState::Disabled
        };
        if !enabled {
            debounce.observe(now, EngineState::Disabled);
        }
        let published = debounce.published();
        let (changes, _) = broadcast::channel(CHANNEL_CAPACITY);
        Arc::new(Self {
            engine,
            host: host.describe(),
            enabled,
            timings,
            state: Mutex::new(Inner {
                rev: 0,
                debounce,
                observed,
                shown: None,
                last_announced: None,
                last_ready_at: None,
                checked_at: clock.now(),
                last_probe: None,
                containers: HashMap::new(),
                windows: HashMap::new(),
                published_engine: (published, None, None, false),
                published: HashMap::new(),
                degraded: 0,
                list_denied: false,
                probing: 0,
                list_read_at: None,
                read_failing: false,
                tracking_lost: false,
            }),
            clock,
            monotonic,
            changes,
            wake: Notify::new(),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn mono(&self) -> Duration {
        self.monotonic.elapsed().unsigned_abs()
    }

    /// L'instantané et un récepteur des changements suivants, pris sous le même verrou : aucun
    /// changement ne s'intercale, aucun ne se perd.
    pub fn subscribe(&self) -> (broadcast::Receiver<Arc<ServicesChange>>, ServicesSnapshot) {
        let inner = self.lock();
        let receiver = self.changes.subscribe();
        (receiver, self.snapshot_of(&inner))
    }

    /// L'état complet maintenant (`GET /services`, abonné en retard).
    pub fn snapshot(&self) -> ServicesSnapshot {
        self.snapshot_of(&self.lock())
    }

    /// Ce que l'agent sait du serveur (distribution, init, compte de l'agent).
    pub fn host(&self) -> &HostDescription {
        &self.host
    }

    pub fn engine_status(&self) -> EngineStatus {
        self.status_of(&self.lock())
    }

    fn snapshot_of(&self, inner: &Inner) -> ServicesSnapshot {
        let mut services: Vec<ServiceView> = inner.published.values().cloned().collect();
        sort_views(&mut services);
        ServicesSnapshot {
            rev: inner.rev,
            engine: self.status_of(inner),
            host: self.host.clone(),
            services,
        }
    }

    fn status_of(&self, inner: &Inner) -> EngineStatus {
        EngineStatus {
            state: inner.debounce.published(),
            name: inner.shown.as_ref().map(|shown| shown.name.clone()),
            version: inner.shown.as_ref().map(|shown| shown.version.clone()),
            last_ready_at: inner.last_ready_at,
            checked_at: inner.checked_at,
            list_read_at: inner.list_read_at,
            list_degraded: Self::list_degraded(inner),
        }
    }

    fn list_degraded(inner: &Inner) -> bool {
        inner.read_failing || inner.tracking_lost || inner.degraded > 0
    }

    fn engine_key(inner: &Inner) -> EngineKey {
        (
            inner.debounce.published(),
            inner.shown.as_ref().map(|shown| shown.name.clone()),
            inner.shown.as_ref().map(|shown| shown.version.clone()),
            Self::list_degraded(inner),
        )
    }

    /// « Vérifier à nouveau » : relance la détection tout de suite et rend l'état. Sans effet sur le
    /// moteur ; trop rapproché de la sonde précédente, ou pendant qu'une sonde est déjà en cours,
    /// l'état courant est rendu sans sonder. La demande est ouverte aux deux rôles (spec : « tous ») ;
    /// le résultat est publié tout de suite, hors anti-clignotement : l'état affiché ne change que si
    /// l'état réel du moteur a changé, et au plus une fois par `min_recheck`.
    pub async fn recheck(&self) -> EngineStatus {
        if let Some(flag) = self.enabled.then(|| self.claim_probe()).flatten() {
            self.probe_with(true, flag).await;
            self.wake.notify_one();
        }
        self.engine_status()
    }

    /// Réserve la sonde d'une relance, d'un seul geste : deux demandes simultanées n'en lancent pas
    /// deux.
    fn claim_probe(&self) -> Option<ProbingFlag<'_>> {
        let mut inner = self.lock();
        let now = self.mono();
        let too_soon = inner
            .last_probe
            .is_some_and(|at| now.saturating_sub(at) < self.timings.min_recheck);
        if inner.probing > 0 || too_soon {
            return None;
        }
        inner.last_probe = Some(now);
        inner.probing += 1;
        Some(ProbingFlag(self))
    }

    /// Une sonde a-t-elle eu lieu depuis moins de `within` ?
    fn probed_within(&self, within: Duration) -> bool {
        self.lock()
            .last_probe
            .is_some_and(|at| self.mono().saturating_sub(at) < within)
    }

    /// Sonde (sauf désactivé) puis applique le résultat. Rend l'état observé.
    async fn probe_and_apply(&self, force: bool) -> EngineState {
        let flag = self.enabled.then(|| {
            let mut inner = self.lock();
            inner.last_probe = Some(self.mono());
            inner.probing += 1;
            ProbingFlag(self)
        });
        match flag {
            Some(flag) => self.probe_with(force, flag).await,
            None => self.apply_probe(None, force),
        }
    }

    /// Comme `probe_and_apply` pour une sonde déjà comptée (`flag`).
    async fn probe_with(&self, force: bool, flag: ProbingFlag<'_>) -> EngineState {
        let probe = if self.enabled {
            let probe = self.engine.probe().await;
            drop(flag);
            Some(probe)
        } else {
            None
        };
        self.apply_probe(probe, force)
    }

    fn apply_probe(&self, probe: Option<EngineProbe>, force: bool) -> EngineState {
        // La sonde répond mais la liste est refusée : pour l'utilisateur, l'accès est refusé.
        let probe = match probe {
            Some(EngineProbe {
                outcome: ProbeOutcome::Answered(_),
                installed,
            }) if self.lock().list_denied => Some(EngineProbe {
                outcome: ProbeOutcome::PermissionDenied,
                installed,
            }),
            other => other,
        };
        // Le moteur a disparu (socket absent, refusé, muet) : le refus de la liste ne vaut plus, il ne
        // doit pas s'afficher à tort quand le moteur reviendra.
        if matches!(
            probe.as_ref().map(|probe| &probe.outcome),
            Some(ProbeOutcome::SocketMissing | ProbeOutcome::Refused | ProbeOutcome::NoAnswer)
        ) {
            self.lock().list_denied = false;
        }
        let observed = classify_probe(self.enabled, probe.as_ref());
        let announced = match probe.map(|probe| probe.outcome) {
            Some(ProbeOutcome::Answered(announced)) => Some(announced),
            _ => None,
        };
        let now = self.mono();
        let wall = self.clock.now();
        let mut inner = self.lock();
        inner.checked_at = wall;
        inner.observed = observed;
        if observed == EngineState::Ready {
            inner.last_ready_at = Some(wall);
        }
        let published = if force {
            inner.debounce.force(now, observed)
        } else {
            inner
                .debounce
                .observe(now, observed)
                .or_else(|| inner.debounce.flush(now))
        };
        if let Some(state) = published {
            tracing::info!(?state, "état du moteur de conteneurs");
        }
        // Le nom et la version suivent l'état publié : ils changent avec lui, ou quand le moteur
        // publié change de version.
        inner.last_announced = announced;
        if published.is_some() || inner.debounce.published() == observed {
            inner.shown = inner.last_announced.clone();
        }
        self.commit(&mut inner);
        observed
    }

    /// Publie l'échéance de l'anti-clignotement si elle est arrivée.
    fn flush_debounce(&self) {
        let now = self.mono();
        let mut inner = self.lock();
        if let Some(state) = inner.debounce.flush(now) {
            tracing::info!(?state, "état du moteur de conteneurs");
            // L'état retenu est celui de la dernière sonde : il part avec ce qu'elle a annoncé.
            inner.shown = inner.last_announced.clone();
            self.commit(&mut inner);
        }
    }

    /// Dans combien de temps un changement retenu pourra partir.
    fn debounce_wait(&self) -> Option<Duration> {
        let deadline = self.lock().debounce.deadline()?;
        Some(deadline.saturating_sub(self.mono()))
    }

    /// Relit tous les conteneurs. Une erreur laisse le registre inchangé. Rend le nombre de
    /// conteneurs illisibles : ils gardent leur dernière observation, ou apparaissent « non
    /// reconnus » s'ils sont nouveaux (`Observation::unreadable`).
    pub async fn resync(&self) -> Result<usize, EngineError> {
        let listing = match self.engine.list().await {
            Ok(listing) => listing,
            Err(error) => {
                {
                    let mut inner = self.lock();
                    inner.read_failing = true;
                    if error != EngineError::Denied {
                        self.commit(&mut inner);
                    }
                }
                if error == EngineError::Denied {
                    self.lock().list_denied = true;
                    self.apply_probe(
                        Some(EngineProbe {
                            outcome: ProbeOutcome::PermissionDenied,
                            installed: true,
                        }),
                        false,
                    );
                }
                return Err(error);
            }
        };
        let unreadable = listing.unreadable.len();
        let recovered = {
            let mut inner = self.lock();
            let previous = std::mem::take(&mut inner.containers);
            for observation in listing.observations {
                note_start(&mut inner.windows, &observation);
                inner.containers.insert(observation.id.clone(), observation);
            }
            for missing in listing.unreadable {
                let kept = previous
                    .get(&missing.id)
                    .cloned()
                    .unwrap_or_else(|| Observation::unreadable(&missing.id, &missing.name));
                inner.containers.insert(missing.id, kept);
            }
            if unreadable > 0 && inner.degraded == 0 {
                // Une fois par épisode : la relecture suivante ne répète pas.
                tracing::warn!(
                    unreadable,
                    "conteneurs illisibles : leur dernière description est gardée"
                );
            } else if unreadable == 0 && inner.degraded > 0 {
                tracing::info!("tous les conteneurs sont de nouveau lisibles");
            }
            inner.degraded = unreadable;
            inner.read_failing = false;
            if unreadable == 0 {
                inner.list_read_at = Some(self.clock.now());
                inner.tracking_lost = false;
            }
            let recovered = std::mem::take(&mut inner.list_denied);
            self.commit(&mut inner);
            recovered
        };
        if recovered {
            // La liste est lisible de nouveau : l'état « prêt » revient avec la prochaine sonde.
            self.probe_and_apply(false).await;
        }
        Ok(unreadable)
    }

    /// Relit les conteneurs touchés par un lot d'événements (un seul échange par conteneur, quelques
    /// uns à la fois). Rend le nombre de conteneurs dont l'inspection a échoué : ils gardent leur
    /// dernière observation. Seule la perte du moteur (ou le dépassement du délai) est une erreur.
    async fn refresh(&self, ids: Vec<String>) -> Result<usize, EngineError> {
        let inspections = stream::iter(ids)
            .map(|id| async move {
                let result = self.engine.inspect(&id).await;
                (id, result)
            })
            .buffer_unordered(REFRESH_CONCURRENCY)
            .collect::<Vec<_>>();
        let read = tokio::time::timeout(REFRESH_TIMEOUT, inspections)
            .await
            .map_err(|_| EngineError::Timeout)?;
        let mut unreadable = 0;
        let mut inner = self.lock();
        for (id, result) in read {
            match result {
                Ok(Some(observation)) => {
                    note_start(&mut inner.windows, &observation);
                    inner.containers.insert(observation.id.clone(), observation);
                }
                Ok(None) => {
                    inner.containers.remove(&id);
                }
                Err(error) if error.engine_gone() => return Err(error),
                Err(_) => unreadable += 1,
            }
        }
        if unreadable > 0 {
            inner.tracking_lost = true;
        }
        self.commit(&mut inner);
        Ok(unreadable)
    }

    /// Recalcule la vue de chaque service et diffuse ce qui a changé, moteur compris. Aucun lot si
    /// rien n'a changé.
    fn commit(&self, inner: &mut Inner) {
        let now = self.clock.now();
        let mut target: HashMap<ServiceKey, ServiceView> = HashMap::new();
        // Dans ces états (moteur absent, non pris en charge, désactivé), aucune liste ancienne n'est
        // gardée (`BR-SVC-018`) ; dans les états inaccessibles la dernière liste reste.
        if !inner.debounce.published().keeps_no_list() {
            let present: HashSet<ServiceKey> = inner
                .containers
                .values()
                .filter_map(|observation| ServiceKey::for_container(&observation.id))
                .collect();
            inner.windows.retain(|key, _| present.contains(key));
            for observation in inner.containers.values() {
                let Some(key) = ServiceKey::for_container(&observation.id) else {
                    continue;
                };
                let restarts_10m = inner.windows.entry(key.clone()).or_default().count(now);
                // Aucune action de Hearth dans cette tranche : l'arrêt n'est jamais « demandé ».
                let derived = derive_state(observation, restarts_10m, false);
                target.insert(
                    key.clone(),
                    ServiceView {
                        key,
                        observation: observation.clone(),
                        derived,
                        restarts_10m,
                    },
                );
            }
        }
        let engine_key = Self::engine_key(inner);
        let engine_changed = engine_key != inner.published_engine;
        let mut upserts: Vec<ServiceView> = target
            .values()
            .filter(|view| inner.published.get(&view.key) != Some(*view))
            .cloned()
            .collect();
        let mut removed: Vec<ServiceKey> = inner
            .published
            .keys()
            .filter(|key| !target.contains_key(*key))
            .cloned()
            .collect();
        if !engine_changed && upserts.is_empty() && removed.is_empty() {
            return;
        }
        sort_views(&mut upserts);
        removed.sort();
        inner.rev += 1;
        inner.published = target;
        inner.published_engine = engine_key;
        let change = ServicesChange {
            rev: inner.rev,
            engine: engine_changed.then(|| self.status_of(inner)),
            upserts,
            removed,
        };
        // Sans abonné, `send` rend une erreur : personne n'écoute, ce n'est pas un échec.
        let _ = self.changes.send(Arc::new(change));
    }

    /// Combien attendre avant de retenter une relecture après `failures` échecs de suite : une
    /// période de sonde au premier, doublée à chaque échec suivant, plafonnée à 30 périodes.
    fn backoff(&self, failures: u32) -> Duration {
        let factor = 1u32
            .checked_shl(failures.saturating_sub(1))
            .unwrap_or(u32::MAX);
        self.timings
            .ready_period
            .saturating_mul(factor.min(MAX_BACKOFF_FACTOR))
    }

    fn list_denied(&self) -> bool {
        self.lock().list_denied
    }

    /// La tâche de fond : détection du moteur, relecture, suivi des événements. Ne rend jamais la
    /// main (on l'arrête en abandonnant la tâche qui la porte). Sans moteur, elle sonde toutes les
    /// 5 secondes et ne dit rien de plus que le changement d'état : aucune erreur bruyante.
    pub async fn run(self: Arc<Self>) {
        if !self.enabled {
            // Pilotage désactivé : le socket n'est jamais ouvert.
            std::future::pending::<()>().await;
        }
        // Relectures complètes échouées de suite, et quand retenter.
        let mut failures: u32 = 0;
        let mut next_read = Instant::now();
        // Phases « prêt » finies en moins d'une période de suite (flux qui casse aussitôt).
        let mut quick_breaks: u32 = 0;
        loop {
            let started = Instant::now();
            // Une relance de l'utilisateur vient de sonder : on réutilise son résultat.
            let observed = if self.probed_within(self.timings.min_recheck) {
                self.lock().observed
            } else {
                self.probe_and_apply(false).await
            };
            let readable = observed == EngineState::Ready
                || (observed == EngineState::AccessDenied && self.list_denied());
            if !readable {
                failures = 0;
                self.wait_for(self.timings.other_period).await;
            } else if Instant::now() < next_read {
                // Relecture en recul : la sonde garde sa cadence, la liste attend.
                self.wait_for(self.timings.ready_period).await;
            } else {
                let Some(read) = self.while_probing(self.resync()).await else {
                    // Le moteur a disparu pendant la lecture : la boucle le resonde aussitôt.
                    continue;
                };
                match read {
                    Ok(unreadable) => {
                        if failures > 0 {
                            tracing::info!("relecture des services rétablie");
                        }
                        failures = 0;
                        self.ready_phase(unreadable).await;
                        // Une rupture du flux est reprise tout de suite (sonde puis relecture complète) ;
                        // si elle se répète sans que la phase dure une période, on recule : jamais de
                        // boucle serrée sur un moteur dont le flux ne tient pas.
                        if started.elapsed() < self.timings.ready_period {
                            quick_breaks = quick_breaks.saturating_add(1);
                        } else {
                            quick_breaks = 0;
                        }
                        if quick_breaks >= 2 {
                            next_read = Instant::now() + self.backoff(quick_breaks - 1);
                        }
                        // Le suivi en direct est suspendu tant que le flux casse en boucle.
                        self.set_tracking_lost(quick_breaks >= 2);
                    }
                    Err(error) => {
                        if failures == 0 {
                            // Une fois par série d'échecs ; les suivants restent au niveau debug.
                            tracing::warn!(%error, "relecture des services impossible");
                        } else {
                            tracing::debug!(%error, "relecture des services impossible");
                        }
                        failures = failures.saturating_add(1);
                        next_read = Instant::now() + self.backoff(failures);
                    }
                }
            }
        }
    }

    fn set_tracking_lost(&self, lost: bool) {
        let mut inner = self.lock();
        if inner.tracking_lost != lost {
            inner.tracking_lost = lost;
            self.commit(&mut inner);
        }
    }

    /// Mène `work` (une lecture du moteur, qui peut durer jusqu'à 30 s) SANS aveugler la tâche : la
    /// sonde garde sa cadence et l'anti-clignotement ses échéances pendant ce temps. Si le moteur
    /// n'est plus prêt, la lecture est abandonnée (`None`) : il n'y a plus rien à lire.
    async fn while_probing<T>(&self, work: impl Future<Output = T>) -> Option<T> {
        let mut ping = interval(self.timings.ready_period);
        ping.set_missed_tick_behavior(MissedTickBehavior::Delay);
        ping.tick().await;
        let probes = async {
            loop {
                ping.tick().await;
                let state = self.probe_and_apply(false).await;
                // Une liste refusée qu'on est justement en train de retenter reste « lisible ».
                let readable = state == EngineState::Ready
                    || (state == EngineState::AccessDenied && self.list_denied());
                if !readable {
                    return;
                }
            }
        };
        tokio::pin!(work, probes);
        loop {
            let debounce_wait = self.debounce_wait();
            tokio::select! {
                out = &mut work => return Some(out),
                () = &mut probes => return None,
                () = sleep(debounce_wait.unwrap_or_default()), if debounce_wait.is_some() => {
                    self.flush_debounce();
                }
            }
        }
    }

    /// Attend la prochaine sonde (`period`) ; publie au passage les changements retenus par
    /// l'anti-clignotement.
    async fn wait_for(&self, period: Duration) {
        let deadline = Instant::now() + period;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return;
            }
            let slice = self
                .debounce_wait()
                .map_or(remaining, |wait| wait.min(remaining));
            tokio::select! {
                () = sleep(slice) => {}
                () = self.wake.notified() => return,
            }
            self.flush_debounce();
        }
    }

    /// Le moteur est prêt et la liste lue : suit les événements jusqu'à une rupture. `unreadable` :
    /// conteneurs que la relecture qui vient d'avoir lieu n'a pas pu lire.
    async fn ready_phase(&self, unreadable: usize) {
        let mut events = self.engine.events();
        let mut touched: HashSet<String> = HashSet::new();
        let mut batch_at: Option<Instant> = None;
        // Le flux d'événements ne se connecte qu'à sa première lecture : une dernière relecture
        // juste après ferme l'intervalle entre la liste et cette connexion.
        let mut settle_at = Some(Instant::now() + self.timings.batch_window * 4);
        // Relectures de suite qui ont laissé des conteneurs illisibles (pour reculer).
        let mut retries: u32 = u32::from(unreadable > 0);
        let mut ping = interval(self.timings.ready_period);
        ping.set_missed_tick_behavior(MissedTickBehavior::Delay);
        ping.tick().await;
        loop {
            let debounce_wait = self.debounce_wait();
            tokio::select! {
                event = events.next() => match event {
                    Some(Ok(event)) => {
                        tracing::trace!(action = %event.action, "événement du moteur");
                        touched.insert(event.container_id);
                        batch_at.get_or_insert_with(|| Instant::now() + self.timings.batch_window);
                    }
                    // Flux rompu : le moteur est à revérifier (sonde immédiate, relecture complète).
                    Some(Err(_)) | None => return,
                },
                () = sleep_until(batch_at.unwrap_or_else(Instant::now)), if batch_at.is_some() => {
                    batch_at = None;
                    let ids: Vec<String> = touched.drain().collect();
                    let Some(refreshed) = self.while_probing(self.refresh(ids)).await else {
                        return;
                    };
                    match refreshed {
                        Ok(0) => {}
                        // Des conteneurs illisibles : une relecture complète les reprendra, en reculant.
                        Ok(_) => {
                            settle_at.get_or_insert_with(|| Instant::now() + self.backoff(retries));
                        }
                        Err(_) => return,
                    }
                }
                () = sleep_until(settle_at.unwrap_or_else(Instant::now)), if settle_at.is_some() => {
                    settle_at = None;
                    let Some(read) = self.while_probing(self.resync()).await else {
                        return;
                    };
                    match read {
                        Ok(0) => retries = 0,
                        Ok(_) => {
                            retries = retries.saturating_add(1);
                            settle_at = Some(Instant::now() + self.backoff(retries));
                        }
                        Err(_) => return,
                    }
                }
                _ = ping.tick() => {
                    if self.probe_and_apply(false).await != EngineState::Ready {
                        return;
                    }
                    // Les démarrages de plus de 10 minutes sortent de la fenêtre sans événement.
                    let mut inner = self.lock();
                    self.commit(&mut inner);
                }
                () = sleep(debounce_wait.unwrap_or_default()), if debounce_wait.is_some() => {
                    self.flush_debounce();
                }
                () = self.wake.notified() => {}
            }
        }
    }
}

/// Note le début d'exécution d'un conteneur qui tourne dans la fenêtre des démarrages.
fn note_start(windows: &mut HashMap<ServiceKey, StartWindow>, observation: &Observation) {
    if observation.run_state != RunState::Running {
        return;
    }
    let (Some(key), Some(started_at)) = (
        ServiceKey::for_container(&observation.id),
        observation.started_at,
    ) else {
        return;
    };
    windows.entry(key).or_default().observe(started_at);
}

fn sort_views(views: &mut [ServiceView]) {
    views.sort_by(|a, b| {
        compare_names(&a.observation.name, &b.observation.name).then_with(|| a.key.cmp(&b.key))
    });
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::domain::services::ServiceStateKind;
    use crate::infrastructure::engine::fake::{FakeEngine, container, ready_probe};

    struct Wall(Mutex<OffsetDateTime>);

    impl Clock for Wall {
        fn now(&self) -> OffsetDateTime {
            *self.0.lock().unwrap_or_else(PoisonError::into_inner)
        }
    }

    #[derive(Default)]
    struct Mono(AtomicU64);

    impl Mono {
        fn advance(&self, by: Duration) {
            self.0.fetch_add(by.as_millis() as u64, Ordering::SeqCst);
        }
    }

    impl MonotonicClock for Mono {
        fn elapsed(&self) -> time::Duration {
            time::Duration::milliseconds(self.0.load(Ordering::SeqCst) as i64)
        }
    }

    struct Linux;

    impl HostEnvironment for Linux {
        fn describe(&self) -> HostDescription {
            HostDescription {
                os_id: "debian".into(),
                systemd: true,
                agent_account: "root".into(),
                agent_is_admin: true,
                ..HostDescription::default()
            }
        }
    }

    fn registry(enabled: bool) -> (Arc<ServicesRegistry>, Arc<FakeEngine>, Arc<Mono>) {
        registry_at(enabled, crate::infrastructure::engine::fake::at(12))
    }

    fn registry_at(
        enabled: bool,
        wall: OffsetDateTime,
    ) -> (Arc<ServicesRegistry>, Arc<FakeEngine>, Arc<Mono>) {
        let engine = Arc::new(FakeEngine::new());
        let mono = Arc::new(Mono::default());
        let registry = ServicesRegistry::new(
            engine.clone(),
            &Linux,
            Arc::new(Wall(Mutex::new(wall))),
            mono.clone(),
            enabled,
            EngineTimings::production(),
        );
        (registry, engine, mono)
    }

    #[tokio::test]
    async fn the_first_state_is_checking_and_the_first_probe_publishes_at_once() {
        let (registry, _engine, _mono) = registry(true);
        assert_eq!(registry.engine_status().state, EngineState::Checking);
        registry.probe_and_apply(false).await;
        let status = registry.engine_status();
        assert_eq!(status.state, EngineState::Ready);
        assert_eq!(status.name.as_deref(), Some("Docker Engine - Community"));
        assert_eq!(status.version.as_deref(), Some("24.0.7"));
        assert!(status.last_ready_at.is_some());
    }

    #[tokio::test]
    async fn a_disabled_engine_is_never_probed() {
        let (registry, engine, _mono) = registry(false);
        assert_eq!(registry.engine_status().state, EngineState::Disabled);
        registry.recheck().await;
        assert_eq!(engine.probe_count(), 0, "le socket n'est jamais ouvert");
        assert!(registry.snapshot().services.is_empty());
    }

    #[tokio::test]
    async fn a_resync_lists_every_container_stopped_ones_included_sorted_by_name() {
        let (registry, engine, _mono) = registry(true);
        engine.put_silently(container("web", RunState::Running));
        engine.put_silently(container("Api", RunState::Exited));
        engine.put_silently(container("db", RunState::Created));
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        let snapshot = registry.snapshot();
        let names: Vec<_> = snapshot
            .services
            .iter()
            .map(|view| view.observation.name.as_str())
            .collect();
        assert_eq!(names, vec!["Api", "db", "web"]);
        assert_eq!(
            snapshot.services[1].derived.state,
            ServiceStateKind::Stopped
        );
        assert!(snapshot.services[1].derived.never_started);
        assert_eq!(snapshot.rev, 2, "l'état du moteur, puis la liste");
    }

    #[tokio::test]
    async fn a_change_is_one_delta_and_nothing_changed_is_no_delta() {
        let (registry, engine, _mono) = registry(true);
        registry.probe_and_apply(false).await;
        let (mut receiver, before) = registry.subscribe();
        // Rien n'a changé : relire ne diffuse rien.
        registry.resync().await.expect("resync");
        assert!(receiver.try_recv().is_err());
        engine.put_silently(container("web", RunState::Running));
        registry
            .refresh(vec![container("web", RunState::Running).id])
            .await
            .expect("refresh");
        let change = receiver.try_recv().expect("un lot");
        assert_eq!(change.rev, before.rev + 1);
        assert_eq!(change.upserts.len(), 1);
        assert!(change.removed.is_empty() && change.engine.is_none());
        // Le conteneur disparaît : un lot `removed`.
        let id = container("web", RunState::Running).id;
        engine.remove(&id);
        registry.refresh(vec![id.clone()]).await.expect("refresh");
        let change = receiver.try_recv().expect("un lot");
        assert_eq!(change.removed.len(), 1);
        assert!(change.upserts.is_empty());
    }

    #[tokio::test]
    async fn the_engine_state_changes_at_most_once_per_window_and_never_flashes() {
        let (registry, engine, mono) = registry(true);
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::Ready);
        // Le moteur tombe à 1 s : retenu.
        mono.advance(Duration::from_secs(1));
        engine.set_outcome(ProbeOutcome::Refused, true);
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::Ready);
        // Il revient avant la fin de la fenêtre : rien n'est jamais publié.
        mono.advance(Duration::from_secs(1));
        engine.set_probe(ready_probe());
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::Ready);
        // Il retombe, et cette fois la fenêtre se termine : publié à son échéance, avec l'état réel.
        engine.set_outcome(ProbeOutcome::Refused, true);
        mono.advance(Duration::from_secs(1));
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::Ready);
        mono.advance(Duration::from_secs(3));
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::Stopped);
    }

    #[tokio::test]
    async fn an_inaccessible_engine_keeps_the_last_list_and_a_missing_one_drops_it() {
        let (registry, engine, mono) = registry(true);
        engine.put_silently(container("web", RunState::Running));
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        // Arrêté : la liste reste (grisée par le client).
        mono.advance(Duration::from_secs(6));
        engine.set_outcome(ProbeOutcome::Refused, true);
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::Stopped);
        assert_eq!(registry.snapshot().services.len(), 1);
        // Absent : aucune liste ancienne n'est gardée.
        mono.advance(Duration::from_secs(6));
        engine.set_outcome(ProbeOutcome::SocketMissing, false);
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::Absent);
        assert!(registry.snapshot().services.is_empty());
    }

    #[tokio::test]
    async fn a_recheck_reflects_the_result_at_once_and_is_rate_limited() {
        let (registry, engine, mono) = registry(true);
        registry.probe_and_apply(false).await;
        mono.advance(Duration::from_secs(2));
        engine.set_outcome(ProbeOutcome::NoAnswer, true);
        let status = registry.recheck().await;
        assert_eq!(status.state, EngineState::Unresponsive);
        // Une seconde demande tout de suite : l'état courant, sans nouvelle sonde.
        let probes = engine.probe_count();
        registry.recheck().await;
        assert_eq!(engine.probe_count(), probes);
        mono.advance(Duration::from_secs(2));
        registry.recheck().await;
        assert_eq!(engine.probe_count(), probes + 1);
    }

    #[tokio::test]
    async fn three_starts_in_ten_minutes_make_a_restart_loop_and_old_ones_do_not_count() {
        let start = crate::infrastructure::engine::fake::at(10);
        let (registry, engine, _mono) = registry_at(true, start + time::Duration::minutes(5));
        registry.probe_and_apply(false).await;
        let mut web = container("web", RunState::Running);
        for minute in [0i64, 2, 4] {
            web.started_at = Some(start + time::Duration::minutes(minute));
            engine.put_silently(web.clone());
            registry
                .refresh(vec![web.id.clone()])
                .await
                .expect("refresh");
        }
        let view = registry.snapshot().services.remove(0);
        assert_eq!(view.restarts_10m, 3);
        assert_eq!(view.derived.state, ServiceStateKind::RestartLoop);

        // Les mêmes démarrages, vus une heure plus tard : hors fenêtre, le service tourne.
        let (late, engine, _mono) = registry_at(true, start + time::Duration::hours(1));
        late.probe_and_apply(false).await;
        engine.put_silently(web.clone());
        late.refresh(vec![web.id.clone()]).await.expect("refresh");
        let view = late.snapshot().services.remove(0);
        assert_eq!(view.restarts_10m, 0);
        assert_eq!(view.derived.state, ServiceStateKind::Running);
    }

    fn fast(ready_ms: u64) -> EngineTimings {
        EngineTimings {
            ready_period: Duration::from_millis(ready_ms),
            other_period: Duration::from_millis(ready_ms),
            debounce_window: Duration::from_millis(10),
            batch_window: Duration::from_millis(5),
            min_recheck: Duration::ZERO,
        }
    }

    fn registry_timed(timings: EngineTimings) -> (Arc<ServicesRegistry>, Arc<FakeEngine>) {
        let engine = Arc::new(FakeEngine::new());
        let registry = ServicesRegistry::new(
            engine.clone(),
            &Linux,
            Arc::new(Wall(Mutex::new(crate::infrastructure::engine::fake::at(
                12,
            )))),
            Arc::new(Mono::default()),
            true,
            timings,
        );
        (registry, engine)
    }

    #[tokio::test]
    async fn a_ready_engine_without_a_container_gives_an_empty_list() {
        let (registry, _engine, _mono) = registry(true);
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        let snapshot = registry.snapshot();
        assert_eq!(snapshot.engine.state, EngineState::Ready);
        assert!(snapshot.services.is_empty());
    }

    #[tokio::test]
    async fn one_unreadable_container_does_not_freeze_the_list() {
        let (registry, engine, _mono) = registry(true);
        let web = container("web", RunState::Running);
        engine.put_silently(web.clone());
        engine.put_silently(container("db", RunState::Running));
        registry.probe_and_apply(false).await;
        assert_eq!(registry.resync().await.expect("resync"), 0);
        // `db` devient illisible (délai, réponse illisible) : sa ligne garde sa dernière description,
        // le reste de la liste continue de suivre le moteur.
        let db_id = container("db", RunState::Running).id;
        engine.fail_inspect(&db_id, Some(EngineError::Timeout));
        engine.update(&web.id, |observation| {
            observation.run_state = RunState::Exited
        });
        assert_eq!(registry.resync().await.expect("resync"), 1);
        let snapshot = registry.snapshot();
        let state_of = |name: &str| {
            snapshot
                .services
                .iter()
                .find(|view| view.observation.name == name)
                .map(|view| view.derived.state)
        };
        assert_eq!(state_of("db"), Some(ServiceStateKind::Running), "gardée");
        assert_eq!(state_of("web"), Some(ServiceStateKind::Stopped), "suivie");
        // Un conteneur NOUVEAU et illisible apparaît « non reconnu », sous son nom.
        engine.put_silently(container("new", RunState::Running));
        engine.fail_inspect(
            &container("new", RunState::Running).id,
            Some(EngineError::Refused),
        );
        assert_eq!(registry.resync().await.expect("resync"), 2);
        let view = registry
            .snapshot()
            .services
            .into_iter()
            .find(|view| view.observation.name == "new")
            .expect("la ligne existe");
        assert_eq!(view.derived.state, ServiceStateKind::Unknown);
        // Rétabli : tout est relu.
        engine.fail_inspect(&db_id, None);
        engine.fail_inspect(&container("new", RunState::Running).id, None);
        assert_eq!(registry.resync().await.expect("resync"), 0);
        assert!(
            registry
                .snapshot()
                .services
                .iter()
                .all(|view| view.derived.state != ServiceStateKind::Unknown)
        );
    }

    #[tokio::test]
    async fn an_event_for_an_unreadable_container_keeps_it_and_says_so() {
        let (registry, engine, _mono) = registry(true);
        let web = container("web", RunState::Running);
        engine.put_silently(web.clone());
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        engine.update(&web.id, |observation| {
            observation.run_state = RunState::Exited
        });
        engine.fail_inspect(&web.id, Some(EngineError::Timeout));
        assert_eq!(registry.refresh(vec![web.id.clone()]).await, Ok(1));
        assert_eq!(
            registry.snapshot().services[0].derived.state,
            ServiceStateKind::Running
        );
        // Le moteur lui-même est perdu : c'est une erreur, pas un conteneur illisible.
        engine.fail_inspect(&web.id, Some(EngineError::Unreachable));
        assert_eq!(
            registry.refresh(vec![web.id.clone()]).await,
            Err(EngineError::Unreachable)
        );
    }

    #[tokio::test]
    async fn a_refused_list_while_the_probe_answers_is_access_denied_not_ready() {
        let (registry, engine, mono) = registry(true);
        engine.put_silently(container("web", RunState::Running));
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::Ready);
        engine.fail_list(Some(EngineError::Denied));
        assert_eq!(registry.resync().await, Err(EngineError::Denied));
        // La fenêtre de l'anti-clignotement passée, l'état publié est « accès refusé » et le reste
        // tant que la liste est refusée, même si la sonde répond.
        mono.advance(Duration::from_secs(6));
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::AccessDenied);
        mono.advance(Duration::from_secs(6));
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::AccessDenied);
        // La liste est de nouveau permise : l'état « prêt » revient.
        engine.fail_list(None);
        mono.advance(Duration::from_secs(6));
        registry.resync().await.expect("resync");
        assert_eq!(registry.engine_status().state, EngineState::Ready);
    }

    #[tokio::test]
    async fn a_refusal_does_not_outlive_the_engine_that_vanished() {
        let (registry, engine, mono) = registry(true);
        engine.put_silently(container("web", RunState::Running));
        registry.probe_and_apply(false).await;
        engine.fail_list(Some(EngineError::Denied));
        assert_eq!(registry.resync().await, Err(EngineError::Denied));
        assert!(registry.list_denied());
        // Le moteur disparaît, puis revient avec une liste permise : plus aucun « accès refusé ».
        engine.set_outcome(ProbeOutcome::SocketMissing, true);
        registry.probe_and_apply(false).await;
        assert!(!registry.list_denied());
        engine.set_ready();
        engine.fail_list(None);
        mono.advance(Duration::from_secs(6));
        registry.probe_and_apply(false).await;
        mono.advance(Duration::from_secs(6));
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::Ready);
    }

    #[tokio::test]
    async fn a_listing_that_keeps_failing_backs_off_instead_of_looping() {
        let (registry, engine) = registry_timed(fast(50));
        engine.fail_list(Some(EngineError::Refused));
        let task = tokio::spawn(registry.clone().run());
        tokio::time::sleep(Duration::from_secs(1)).await;
        task.abort();
        let reads = engine.list_count();
        assert!((2..=8).contains(&reads), "{reads} relectures en 1 s");
        // Et la sonde, elle, garde sa cadence : l'état reste suivi.
        assert!(engine.probe_count() >= 8, "{} sondes", engine.probe_count());
    }

    #[tokio::test]
    async fn a_slow_listing_does_not_blind_the_probe_nor_the_debounce() {
        let (registry, engine) = registry_timed(fast(50));
        engine.put_silently(container("web", RunState::Running));
        engine.set_list_delay(Duration::from_secs(3));
        let task = tokio::spawn(registry.clone().run());
        // La première lecture dure 3 s ; pendant ce temps, la sonde garde sa cadence (50 ms)...
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let probes = engine.probe_count();
        assert!(probes >= 8, "{probes} sondes pendant une lecture lente");
        // ... et un moteur qui tombe est vu SANS attendre la fin de la lecture (l'horloge de ce test est
        // figée : l'état publié attend l'anti-clignotement, l'état observé non).
        engine.set_outcome(ProbeOutcome::SocketMissing, false);
        let started = Instant::now();
        while registry.lock().observed == EngineState::Ready {
            assert!(
                started.elapsed() < Duration::from_secs(1),
                "perte du moteur non vue pendant la lecture lente"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        task.abort();
    }

    #[tokio::test]
    async fn a_failed_or_partial_read_is_told_with_the_date_of_the_last_good_one() {
        let (registry, engine, mono) = registry(true);
        let web = container("web", RunState::Running);
        engine.put_silently(web.clone());
        registry.probe_and_apply(false).await;
        // Avant toute lecture : ni date ni drapeau.
        assert_eq!(registry.engine_status().list_read_at, None);
        assert!(!registry.engine_status().list_degraded);
        registry.resync().await.expect("resync");
        let first = registry.engine_status();
        assert_eq!(first.list_read_at, Some(registry.clock.now()));
        assert!(!first.list_degraded);
        // Lecture en échec alors que la sonde répond : la liste d'avant reste, datée, marquée.
        engine.fail_list(Some(EngineError::Refused));
        assert_eq!(registry.resync().await, Err(EngineError::Refused));
        mono.advance(Duration::from_secs(6));
        let failed = registry.engine_status();
        assert!(failed.list_degraded);
        assert_eq!(
            failed.list_read_at, first.list_read_at,
            "date de la dernière bonne lecture"
        );
        assert_eq!(
            registry.snapshot().services.len(),
            1,
            "la liste d'avant reste"
        );
        // Rétablie : le drapeau tombe.
        engine.fail_list(None);
        registry.resync().await.expect("resync");
        assert!(!registry.engine_status().list_degraded);
        // Lecture partielle : un conteneur illisible marque la liste, sans changer la date.
        engine.fail_inspect(&web.id, Some(EngineError::Timeout));
        let before = registry.engine_status().list_read_at;
        assert_eq!(registry.resync().await, Ok(1));
        assert!(registry.engine_status().list_degraded);
        assert_eq!(registry.engine_status().list_read_at, before);
        // Un conteneur touché par un événement et illisible marque aussi la liste.
        engine.fail_inspect(&web.id, None);
        registry.resync().await.expect("resync");
        assert!(!registry.engine_status().list_degraded);
        engine.fail_inspect(&web.id, Some(EngineError::Timeout));
        assert_eq!(registry.refresh(vec![web.id.clone()]).await, Ok(1));
        assert!(registry.engine_status().list_degraded);
    }

    #[tokio::test]
    async fn a_change_of_list_freshness_is_published_to_subscribers() {
        let (registry, engine, _mono) = registry(true);
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        let (mut changes, _) = registry.subscribe();
        engine.fail_list(Some(EngineError::Refused));
        let _ = registry.resync().await;
        let change = changes.try_recv().expect("un lot");
        assert!(change.engine.as_ref().expect("moteur").list_degraded);
    }

    #[tokio::test]
    async fn an_event_stream_that_breaks_at_once_is_not_retried_in_a_tight_loop() {
        let (registry, engine) = registry_timed(fast(50));
        engine.set_events_broken(true);
        let task = tokio::spawn(registry.clone().run());
        tokio::time::sleep(Duration::from_secs(1)).await;
        task.abort();
        let reads = engine.list_count();
        // La première rupture est reprise tout de suite ; les suivantes reculent.
        assert!((2..=14).contains(&reads), "{reads} relectures en 1 s");
    }
}
