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

use crate::application::ports::{ContainerEngine, EngineError, EngineEvent, EngineEvents};
use crate::domain::services::image::image_name;
use crate::domain::services::{
    Announced, ApiVersion, EngineProbe, Observation, ProbeOutcome, RestartPolicy, RunState,
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
        host_network: false,
        mounts: vec![],
        restart: RestartPolicy::None,
        auto_remove: false,
        compose_project: None,
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

    async fn list(&self) -> Result<Vec<Observation>, EngineError> {
        self.lists.fetch_add(1, Ordering::SeqCst);
        let state = self.lock();
        if let Some(error) = state.list_error {
            return Err(error);
        }
        Ok(state.containers.values().cloned().collect())
    }

    async fn inspect(&self, id: &str) -> Result<Option<Observation>, EngineError> {
        self.inspections.fetch_add(1, Ordering::SeqCst);
        let state = self.lock();
        if let Some(error) = state.list_error {
            return Err(error);
        }
        Ok(state.containers.get(id).cloned())
    }

    fn events(&self) -> EngineEvents {
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn the_fake_lists_inspects_and_streams_events() {
        let engine = FakeEngine::new();
        let web = container("web", RunState::Running);
        let id = web.id.clone();
        engine.put_silently(web);
        assert_eq!(engine.list().await.expect("list").len(), 1);
        assert!(engine.inspect(&id).await.expect("inspect").is_some());
        assert!(engine.inspect("absent").await.expect("inspect").is_none());

        let mut events = engine.events();
        engine.emit(&id, "die");
        let event = tokio::time::timeout(Duration::from_secs(1), events.next())
            .await
            .expect("événement")
            .expect("flux ouvert")
            .expect("pas d'erreur");
        assert_eq!(event.container_id, id);
        engine.break_events();
        let end = tokio::time::timeout(Duration::from_secs(1), events.next())
            .await
            .expect("fin");
        assert!(end.is_none());
    }

    #[test]
    fn the_sample_container_is_stable_and_carries_no_environment() {
        let a = container("web", RunState::Running);
        assert_eq!(a.id, container("web", RunState::Exited).id);
        assert_eq!(a.id.len(), 64);
        assert_ne!(a.id, container("db", RunState::Running).id);
    }
}
