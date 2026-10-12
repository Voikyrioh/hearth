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
//! L'état du moteur passe par `Debounce` : au plus un changement affiché toutes les 5 secondes.
//! Chaque changement est un lot (`ServicesChange`) diffusé aux abonnés ; un abonné trop lent
//! (`RecvError::Lagged`) relit un instantané complet. `rev` croît à chaque changement et repart de
//! zéro au redémarrage.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use futures_util::StreamExt;
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
type EngineKey = (EngineState, Option<String>, Option<String>);

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
                published_engine: (published, None, None),
                published: HashMap::new(),
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
        }
    }

    fn engine_key(inner: &Inner) -> EngineKey {
        (
            inner.debounce.published(),
            inner.shown.as_ref().map(|shown| shown.name.clone()),
            inner.shown.as_ref().map(|shown| shown.version.clone()),
        )
    }

    /// « Vérifier à nouveau » : relance la détection tout de suite et rend l'état. Sans effet sur le
    /// moteur ; trop rapproché de la sonde précédente, l'état courant est rendu sans sonder.
    pub async fn recheck(&self) -> EngineStatus {
        if self.enabled {
            let recent = {
                let inner = self.lock();
                inner
                    .last_probe
                    .is_some_and(|at| self.mono().saturating_sub(at) < self.timings.min_recheck)
            };
            if !recent {
                self.probe_and_apply(true).await;
                self.wake.notify_one();
            }
        }
        self.engine_status()
    }

    /// Sonde (sauf désactivé) puis applique le résultat. Rend l'état observé.
    async fn probe_and_apply(&self, force: bool) -> EngineState {
        let probe = if self.enabled {
            self.lock().last_probe = Some(self.mono());
            Some(self.engine.probe().await)
        } else {
            None
        };
        self.apply_probe(probe, force)
    }

    fn apply_probe(&self, probe: Option<EngineProbe>, force: bool) -> EngineState {
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

    /// Relit tous les conteneurs. Une erreur laisse le registre inchangé.
    pub async fn resync(&self) -> Result<(), EngineError> {
        let list = self.engine.list().await?;
        let mut inner = self.lock();
        inner.containers.clear();
        for observation in list {
            note_start(&mut inner.windows, &observation);
            inner.containers.insert(observation.id.clone(), observation);
        }
        self.commit(&mut inner);
        Ok(())
    }

    /// Relit les conteneurs touchés par un lot d'événements (un seul échange par conteneur).
    async fn refresh(&self, ids: Vec<String>) -> Result<(), EngineError> {
        let mut read = Vec::with_capacity(ids.len());
        for id in ids {
            read.push((id.clone(), self.engine.inspect(&id).await?));
        }
        let mut inner = self.lock();
        for (id, observation) in read {
            match observation {
                Some(observation) => {
                    note_start(&mut inner.windows, &observation);
                    inner.containers.insert(observation.id.clone(), observation);
                }
                None => {
                    inner.containers.remove(&id);
                }
            }
        }
        self.commit(&mut inner);
        Ok(())
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

    /// La tâche de fond : détection du moteur, relecture, suivi des événements. Ne rend jamais la
    /// main (on l'arrête en abandonnant la tâche qui la porte). Sans moteur, elle sonde toutes les
    /// 5 secondes et ne dit rien de plus que le changement d'état : aucune erreur bruyante.
    pub async fn run(self: Arc<Self>) {
        if !self.enabled {
            // Pilotage désactivé : le socket n'est jamais ouvert.
            std::future::pending::<()>().await;
        }
        loop {
            let started = Instant::now();
            let observed = self.probe_and_apply(false).await;
            if observed == EngineState::Ready {
                match self.resync().await {
                    Ok(()) => self.ready_phase().await,
                    Err(error) => tracing::debug!(%error, "relecture des services impossible"),
                }
                // Aucune relance serrée : un moteur dont le flux casse aussitôt est sondé au rythme
                // normal, pas en boucle.
                let ran = started.elapsed();
                if ran < self.timings.ready_period {
                    sleep(self.timings.ready_period - ran).await;
                }
            } else {
                self.wait_while_not_ready().await;
            }
        }
    }

    /// Attend la prochaine sonde ; publie au passage les changements retenus par l'anti-clignotement.
    async fn wait_while_not_ready(&self) {
        let deadline = Instant::now() + self.timings.other_period;
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

    /// Le moteur est prêt et la liste lue : suit les événements jusqu'à une rupture.
    async fn ready_phase(&self) {
        let mut events = self.engine.events();
        let mut touched: HashSet<String> = HashSet::new();
        let mut batch_at: Option<Instant> = None;
        // Le flux d'événements ne se connecte qu'à sa première lecture : une dernière relecture
        // juste après ferme l'intervalle entre la liste et cette connexion.
        let mut settle_at = Some(Instant::now() + self.timings.batch_window * 4);
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
                    if self.refresh(ids).await.is_err() {
                        return;
                    }
                }
                () = sleep_until(settle_at.unwrap_or_else(Instant::now)), if settle_at.is_some() => {
                    settle_at = None;
                    if self.resync().await.is_err() {
                        return;
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
}
