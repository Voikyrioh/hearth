#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]
//! Faux moteur de conteneurs : un script de conteneurs, des événements injectables, une sonde réglable.
//!
//! Sert à tous les tests de l'agent : le poste de développement n'a pas de moteur (le vrai n'existe
//! qu'en CI Linux). La même suite de contrat (`tests/support/engine_contract.rs`) passe contre ce
//! faux, contre un faux démon HTTP local et contre un vrai moteur, ce qui l'empêche de mentir.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use futures_util::StreamExt;
use futures_util::stream;
use sha2::{Digest, Sha256};
use tokio::sync::{broadcast, watch};

use hearth_agent::application::ports::{
    ContainerEngine, EngineError, EngineEvent, EngineEvents, EnvEntry, Listing, LogQuery,
    LogStream, Unreadable,
};
use hearth_agent::domain::secret::Secret;
use hearth_agent::domain::services::image::image_name;
use hearth_agent::domain::services::logs::{LineAssembler, LogLine, LogSource};
use hearth_agent::domain::services::{
    Announced, ApiVersion, EngineProbe, Hidden, Observation, ProbeOutcome, RestartPolicy, RunState,
};

#[derive(Debug, Clone)]
enum Feed {
    Event(EngineEvent),
    /// Coupe le flux d'événements : le moteur est à revérifier.
    End,
}

struct State {
    probe: EngineProbe,
    containers: BTreeMap<String, Observation>,
    list_error: Option<EngineError>,
    /// Conteneurs dont l'inspection échoue (la liste les nomme, l'inspection répond par l'erreur).
    inspect_errors: BTreeMap<String, EngineError>,
    /// Variables d'environnement (nom, valeur) par conteneur, pour `env`.
    envs: BTreeMap<String, Vec<(String, String)>>,
    /// Combien de fois `env` a été appelée.
    env_reads: u64,
    /// Le flux d'événements se ferme aussitôt ouvert (un moteur dont le flux ne tient pas).
    events_broken: bool,
    /// Temps que `list` met à répondre (un moteur lent).
    list_delay: std::time::Duration,
    /// Les morceaux écrits par chaque conteneur dans ses journaux (HRT-66).
    logs: BTreeMap<String, Vec<RawLog>>,
    /// Lectures de journaux qui échouent (None : rétablit).
    log_errors: BTreeMap<String, EngineError>,
    /// Génération des flux de journaux de chaque conteneur : l'augmenter ferme ceux déjà ouverts (le
    /// moteur ferme le suivi quand le conteneur s'arrête, même s'il est reparti aussitôt).
    log_epochs: BTreeMap<String, u64>,
}

/// Un morceau d'octets écrit par un conteneur sur l'une de ses sorties, à l'heure du moteur.
struct RawLog {
    source: LogSource,
    at_ms: i64,
    bytes: Vec<u8>,
}

pub struct FakeEngine {
    state: Arc<Mutex<State>>,
    feed: broadcast::Sender<Feed>,
    probes: AtomicU64,
    lists: AtomicU64,
    inspections: AtomicU64,
    /// Réveille les lectures de journaux en suivi : une ligne écrite, un conteneur qui change d'état.
    log_tick: watch::Sender<u64>,
    /// Combien de lectures de journaux ont été demandées, et combien sont encore ouvertes.
    log_reads: AtomicU64,
    log_open: Arc<AtomicUsize>,
    /// Combien de lignes le moteur a rendues aux lectures (le travail du moteur, pas du client).
    log_rendered: Arc<AtomicU64>,
    /// Les paramètres de chaque requête de journaux reçue, dans l'ordre.
    log_queries: Mutex<Vec<LogQuery>>,
}

/// La sonde d'un Docker récent qui répond.
pub fn ready_probe() -> EngineProbe {
    EngineProbe {
        outcome: ProbeOutcome::Answered(Announced {
            name: "Docker Engine - Community".into(),
            version: "24.0.7".into(),
            api_version: ApiVersion {
                major: 1,
                minor: 43,
            },
        }),
        installed: true,
    }
}

/// 2026-10-13 00:00:00 UTC plus `hours` heures : des dates fixes pour les conteneurs de test.
pub fn at(hours: i64) -> time::OffsetDateTime {
    time::OffsetDateTime::from_unix_timestamp(1_791_849_600 + hours * 3600)
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH)
}

/// Un conteneur plausible : identifiant de 64 caractères tiré du nom, image `busybox:1.36`.
pub fn container(name: &str, run_state: RunState) -> Observation {
    let id: String = Sha256::digest(name.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let started = matches!(
        run_state,
        RunState::Running | RunState::Paused | RunState::Restarting | RunState::Exited
    );
    Observation {
        id,
        name: name.to_owned(),
        created_at: Some(at(9)),
        run_state,
        exit_code: matches!(run_state, RunState::Exited).then_some(0),
        started_at: started.then_some(at(10)),
        finished_at: matches!(run_state, RunState::Exited).then_some(at(11)),
        image: image_name("busybox:1.36", "sha256:0123456789abcdef0123456789abcdef"),
        ports: vec![],
        ports_omitted: 0,
        host_network: false,
        mounts: vec![],
        mounts_omitted: 0,
        restart: RestartPolicy::None,
        auto_remove: false,
        compose_project: None,
        env_names: vec![],
        env_omitted: 0,
        command: None,
        stop_timeout_s: None,
        tty: false,
        log_driver: Some("json-file".into()),
        hearth_key: None,
    }
}

impl FakeEngine {
    /// Un moteur prêt, sans conteneur.
    pub fn new() -> Self {
        let (feed, _) = broadcast::channel(256);
        Self {
            state: Arc::new(Mutex::new(State {
                probe: ready_probe(),
                containers: BTreeMap::new(),
                list_error: None,
                inspect_errors: BTreeMap::new(),
                envs: BTreeMap::new(),
                env_reads: 0,
                events_broken: false,
                list_delay: std::time::Duration::ZERO,
                logs: BTreeMap::new(),
                log_errors: BTreeMap::new(),
                log_epochs: BTreeMap::new(),
            })),
            feed,
            probes: AtomicU64::new(0),
            lists: AtomicU64::new(0),
            inspections: AtomicU64::new(0),
            log_tick: watch::channel(0).0,
            log_reads: AtomicU64::new(0),
            log_open: Arc::new(AtomicUsize::new(0)),
            log_rendered: Arc::new(AtomicU64::new(0)),
            log_queries: Mutex::new(Vec::new()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Règle ce que la prochaine sonde verra.
    pub fn set_probe(&self, probe: EngineProbe) {
        self.lock().probe = probe;
    }

    pub fn set_outcome(&self, outcome: ProbeOutcome, installed: bool) {
        self.set_probe(EngineProbe { outcome, installed });
    }

    /// Le moteur répond de nouveau comme un Docker récent.
    pub fn set_ready(&self) {
        self.set_probe(ready_probe());
    }

    /// Ajoute ou remplace un conteneur, sans événement.
    pub fn put_silently(&self, observation: Observation) {
        self.lock()
            .containers
            .insert(observation.id.clone(), observation);
        self.wake_logs();
    }

    /// Les identifiants des conteneurs que le moteur connaît maintenant (pour adopter en base, HRT-79).
    pub fn container_ids(&self) -> Vec<String> {
        self.lock().containers.keys().cloned().collect()
    }

    /// Ajoute ou remplace un conteneur et émet l'événement correspondant.
    pub fn put(&self, observation: Observation, action: &str) {
        let id = observation.id.clone();
        self.put_silently(observation);
        self.emit(&id, action);
    }

    /// Modifie un conteneur, sans événement.
    pub fn update(&self, id: &str, change: impl FnOnce(&mut Observation)) {
        if let Some(observation) = self.lock().containers.get_mut(id) {
            change(observation);
        }
        self.wake_logs();
    }

    /// Supprime un conteneur et émet `destroy`.
    pub fn remove(&self, id: &str) {
        self.lock().containers.remove(id);
        self.wake_logs();
        self.emit(id, "destroy");
    }

    pub fn emit(&self, container_id: &str, action: &str) {
        // Sans abonné, `send` rend une erreur : personne n'écoute, ce n'est pas un échec.
        let _ = self.feed.send(Feed::Event(EngineEvent {
            container_id: container_id.to_owned(),
            action: action.to_owned(),
        }));
    }

    /// Coupe les flux d'événements ouverts.
    pub fn break_events(&self) {
        let _ = self.feed.send(Feed::End);
    }

    /// Fait échouer `list` (None : rétablit).
    pub fn fail_list(&self, error: Option<EngineError>) {
        self.lock().list_error = error;
    }

    /// Fait échouer l'inspection d'un conteneur (None : rétablit). La liste le rend « illisible ».
    pub fn fail_inspect(&self, id: &str, error: Option<EngineError>) {
        let mut state = self.lock();
        match error {
            Some(error) => state.inspect_errors.insert(id.to_owned(), error),
            None => state.inspect_errors.remove(id),
        };
    }

    /// Règle les variables d'un conteneur : la LISTE n'en montre que les noms (dans l'observation, à
    /// poser avec `Observation::env_names`), `env` rend noms et valeurs.
    pub fn set_env(&self, id: &str, variables: &[(&str, &str)]) {
        let pairs = variables
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        self.lock().envs.insert(id.to_owned(), pairs);
    }

    /// Combien de fois les valeurs ont été lues (jamais par la liste ni par les événements).
    pub fn env_read_count(&self) -> u64 {
        self.lock().env_reads
    }

    /// Les flux d'événements ouverts à partir de maintenant se ferment aussitôt (true) ou tiennent.
    pub fn set_events_broken(&self, broken: bool) {
        self.lock().events_broken = broken;
    }

    /// `list` met ce temps à répondre (`Duration::ZERO` : rétablit).
    pub fn set_list_delay(&self, delay: std::time::Duration) {
        self.lock().list_delay = delay;
    }

    fn wake_logs(&self) {
        self.log_tick.send_modify(|tick| *tick += 1);
    }

    /// Le conteneur écrit une ligne (le texte et une fin de ligne) sur cette sortie, à cette heure.
    pub fn push_log(&self, id: &str, source: LogSource, at_ms: i64, text: &str) {
        let mut bytes = text.as_bytes().to_vec();
        bytes.push(b'\n');
        self.push_log_raw(id, source, at_ms, &bytes);
    }

    /// Le conteneur écrit ces octets tels quels (sans fin de ligne ajoutée).
    pub fn push_log_raw(&self, id: &str, source: LogSource, at_ms: i64, bytes: &[u8]) {
        self.lock()
            .logs
            .entry(id.to_owned())
            .or_default()
            .push(RawLog {
                source,
                at_ms,
                bytes: bytes.to_vec(),
            });
        self.wake_logs();
    }

    /// Le moteur fait tourner ses fichiers : seuls les `keep` derniers morceaux restent.
    pub fn rotate_logs(&self, id: &str, keep: usize) {
        if let Some(entries) = self.lock().logs.get_mut(id) {
            let drop = entries.len().saturating_sub(keep);
            entries.drain(..drop);
        }
        self.wake_logs();
    }

    /// Fait échouer la lecture des journaux de ce conteneur (None : rétablit).
    pub fn fail_logs(&self, id: &str, error: Option<EngineError>) {
        let mut state = self.lock();
        match error {
            Some(error) => state.log_errors.insert(id.to_owned(), error),
            None => state.log_errors.remove(id),
        };
    }

    /// Le moteur ferme les suivis de journaux déjà ouverts sur ce conteneur (il s'est arrêté), sans que
    /// l'état publié change : un conteneur qui repart aussitôt garde ses lecteurs fermés derrière lui.
    pub fn end_log_streams(&self, id: &str) {
        *self.lock().log_epochs.entry(id.to_owned()).or_default() += 1;
        self.wake_logs();
    }

    /// Combien de lignes le moteur a rendues, en tout, aux lectures de journaux.
    /// Les requêtes de journaux reçues jusqu'ici, avec leurs paramètres.
    pub fn log_queries(&self) -> Vec<LogQuery> {
        self.log_queries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn rendered_log_lines(&self) -> u64 {
        self.log_rendered.load(Ordering::SeqCst)
    }

    /// Combien de lectures de journaux ont été demandées au moteur.
    pub fn log_read_count(&self) -> u64 {
        self.log_reads.load(Ordering::SeqCst)
    }

    /// Combien de lectures de journaux sont ouvertes en ce moment (un flux abandonné se ferme).
    pub fn open_log_reads(&self) -> usize {
        self.log_open.load(Ordering::SeqCst)
    }

    pub fn probe_count(&self) -> u64 {
        self.probes.load(Ordering::SeqCst)
    }

    pub fn list_count(&self) -> u64 {
        self.lists.load(Ordering::SeqCst)
    }

    pub fn inspect_count(&self) -> u64 {
        self.inspections.load(Ordering::SeqCst)
    }

    /// Le nombre d'abonnés actuels au flux d'événements.
    pub fn event_subscribers(&self) -> usize {
        self.feed.receiver_count()
    }
}

impl Default for FakeEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ContainerEngine for FakeEngine {
    async fn probe(&self) -> EngineProbe {
        self.probes.fetch_add(1, Ordering::SeqCst);
        self.lock().probe.clone()
    }

    async fn list(&self) -> Result<Listing, EngineError> {
        self.lists.fetch_add(1, Ordering::SeqCst);
        let delay = self.lock().list_delay;
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        let state = self.lock();
        if let Some(error) = state.list_error {
            return Err(error);
        }
        let mut listing = Listing::default();
        for (id, observation) in &state.containers {
            match state.inspect_errors.get(id) {
                Some(error) if error.engine_gone() => return Err(*error),
                Some(_) => listing.unreadable.push(Unreadable {
                    id: id.clone(),
                    name: observation.name.clone(),
                }),
                None => listing.observations.push(observation.clone()),
            }
        }
        Ok(listing)
    }

    async fn inspect(&self, id: &str) -> Result<Option<Observation>, EngineError> {
        self.inspections.fetch_add(1, Ordering::SeqCst);
        let state = self.lock();
        if let Some(error) = state
            .list_error
            .or_else(|| state.inspect_errors.get(id).copied())
        {
            return Err(error);
        }
        Ok(state.containers.get(id).cloned())
    }

    async fn env(&self, id: &str) -> Result<Option<Vec<EnvEntry>>, EngineError> {
        let mut state = self.lock();
        state.env_reads += 1;
        if let Some(error) = state
            .list_error
            .or_else(|| state.inspect_errors.get(id).copied())
        {
            return Err(error);
        }
        if !state.containers.contains_key(id) {
            return Ok(None);
        }
        Ok(Some(
            state
                .envs
                .get(id)
                .into_iter()
                .flatten()
                .map(|(name, value)| EnvEntry {
                    name: Hidden::new(name.clone()),
                    value: Secret::from(value.as_str()),
                })
                .collect(),
        ))
    }

    fn logs(&self, id: &str, query: LogQuery) -> LogStream {
        self.log_reads.fetch_add(1, Ordering::SeqCst);
        self.log_queries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(query);
        self.log_open.fetch_add(1, Ordering::SeqCst);
        let follower = Follower {
            state: self.state.clone(),
            tick: self.log_tick.subscribe(),
            id: id.to_owned(),
            query,
            epoch: self.lock().log_epochs.get(id).copied().unwrap_or_default(),
            rendered: self.log_rendered.clone(),
            started: false,
            finished: false,
            index: 0,
            assembler: LineAssembler::new(),
            ready: VecDeque::new(),
            _open: OpenGuard(self.log_open.clone()),
        };
        stream::unfold(follower, Follower::step).boxed()
    }

    fn events(&self) -> EngineEvents {
        if self.lock().events_broken {
            return stream::empty().boxed();
        }
        let receiver = self.feed.subscribe();
        stream::unfold(receiver, |mut receiver| async move {
            loop {
                match receiver.recv().await {
                    Ok(Feed::Event(event)) => return Some((Ok(event), receiver)),
                    Ok(Feed::End) | Err(broadcast::error::RecvError::Closed) => return None,
                    // Un lecteur lent a perdu des événements : le vrai moteur ne le dit pas, le
                    // registre se resynchronise au besoin.
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
        })
        .boxed()
    }
}

/// Une lecture de journaux ouverte : se ferme (et se décompte) quand le flux est abandonné.
struct OpenGuard(Arc<AtomicUsize>);

impl Drop for OpenGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// L'état d'une lecture de journaux du faux : mêmes règles que le vrai moteur (les morceaux sont
/// assemblés par le même `LineAssembler`, `since` filtre à la seconde, `tail` garde les dernières
/// lignes, le suivi finit quand le conteneur s'arrête).
struct Follower {
    state: Arc<Mutex<State>>,
    tick: watch::Receiver<u64>,
    id: String,
    query: LogQuery,
    /// La génération des flux de ce conteneur quand cette lecture s'est ouverte.
    epoch: u64,
    rendered: Arc<AtomicU64>,
    started: bool,
    finished: bool,
    /// Combien de morceaux du conteneur ont déjà été lus.
    index: usize,
    assembler: LineAssembler,
    ready: VecDeque<LogLine>,
    _open: OpenGuard,
}

impl Follower {
    async fn step(mut self) -> Option<(Result<LogLine, EngineError>, Self)> {
        loop {
            if let Some(line) = self.ready.pop_front() {
                return Some((Ok(line), self));
            }
            if self.finished {
                return None;
            }
            if let Err(error) = self.advance() {
                self.finished = true;
                return Some((Err(error), self));
            }
            if self.ready.is_empty() && !self.finished && self.tick.changed().await.is_err() {
                self.finished = true;
            }
        }
    }

    /// Un pas, sous le verrou, sans attendre.
    fn advance(&mut self) -> Result<(), EngineError> {
        let state = Arc::clone(&self.state);
        let state = state.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(container) = state.containers.get(&self.id) else {
            return Err(EngineError::NotFound);
        };
        if let Some(error) = state.log_errors.get(&self.id) {
            return Err(*error);
        }
        if container.log_driver.as_deref() == Some("none") {
            return Err(EngineError::Unsupported);
        }
        let alive = matches!(
            container.run_state,
            hearth_agent::domain::services::RunState::Running
                | hearth_agent::domain::services::RunState::Paused
                | hearth_agent::domain::services::RunState::Restarting
        );
        let entries = state
            .logs
            .get(&self.id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let mut lines = Vec::new();
        if !self.started {
            self.started = true;
            let since = self.query.since_ms.map(|ms| ms.div_euclid(1000) * 1000);
            let mut chosen: Vec<&RawLog> = entries
                .iter()
                .filter(|entry| since.is_none_or(|since| entry.at_ms >= since))
                .collect();
            // Comme le vrai moteur : `tail` compte des MESSAGES du moteur (une écriture), pas des lignes
            // (un message plein de retours chariot donne plusieurs lignes).
            if let Some(tail) = self.query.tail {
                let keep = usize::try_from(tail).unwrap_or(usize::MAX);
                let skip = chosen.len().saturating_sub(keep);
                chosen.drain(..skip);
            }
            for entry in chosen {
                self.assembler
                    .push(entry.source, entry.at_ms, &entry.bytes, &mut lines);
            }
            if !self.query.follow {
                self.assembler.finish(&mut lines);
                self.finished = true;
            }
        } else {
            for entry in entries.iter().skip(self.index) {
                self.assembler
                    .push(entry.source, entry.at_ms, &entry.bytes, &mut lines);
            }
            let cut = state.log_epochs.get(&self.id).copied().unwrap_or_default() > self.epoch;
            if !alive || cut {
                self.assembler.finish(&mut lines);
                self.finished = true;
            }
        }
        self.index = entries.len();
        self.rendered.fetch_add(
            u64::try_from(lines.len()).unwrap_or(u64::MAX),
            Ordering::SeqCst,
        );
        self.ready.extend(lines);
        Ok(())
    }
}
