#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]
//! Faux moteur de conteneurs : un script de conteneurs, des événements injectables, une sonde réglable.
//!
//! Sert à tous les tests de l'agent : le poste de développement n'a pas de moteur (le vrai n'existe
//! qu'en CI Linux). La même suite de contrat (`tests/support/engine_contract.rs`) passe contre ce
//! faux, contre un faux démon HTTP local et contre un vrai moteur, ce qui l'empêche de mentir.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use async_trait::async_trait;
use futures_util::StreamExt;
use futures_util::stream;
use sha2::{Digest, Sha256};
use tokio::sync::broadcast;

use hearth_agent::application::ports::{
    ContainerEngine, EngineError, EngineEvent, EngineEvents, EnvEntry, Listing, Unreadable,
};
use hearth_agent::domain::secret::Secret;
use hearth_agent::domain::services::image::image_name;
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
}

pub struct FakeEngine {
    state: Mutex<State>,
    feed: broadcast::Sender<Feed>,
    probes: AtomicU64,
    lists: AtomicU64,
    inspections: AtomicU64,
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
    }
}

impl FakeEngine {
    /// Un moteur prêt, sans conteneur.
    pub fn new() -> Self {
        let (feed, _) = broadcast::channel(256);
        Self {
            state: Mutex::new(State {
                probe: ready_probe(),
                containers: BTreeMap::new(),
                list_error: None,
                inspect_errors: BTreeMap::new(),
                envs: BTreeMap::new(),
                env_reads: 0,
                events_broken: false,
                list_delay: std::time::Duration::ZERO,
            }),
            feed,
            probes: AtomicU64::new(0),
            lists: AtomicU64::new(0),
            inspections: AtomicU64::new(0),
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
    }

    /// Supprime un conteneur et émet `destroy`.
    pub fn remove(&self, id: &str) {
        self.lock().containers.remove(id);
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
