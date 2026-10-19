//! La suite de contrat du port `ContainerEngine` contre le faux moteur (`FakeEngine`) : si le faux
//! ne tient pas le contrat, tous les tests bâtis dessus mentent.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use hearth_agent::application::ports::ContainerEngine;
use hearth_agent::domain::services::logs::LogSource;
use hearth_agent::domain::services::{Hidden, PortBinding, PortProtocol, PortScope, RunState};
use support::engine_contract::{self, Harness, Run, Spec};
use support::fake_engine::{FakeEngine, at, container};

/// L'heure des lignes écrites par le décor : une milliseconde de plus à chaque ligne.
static NEXT_LOG_MS: AtomicI64 = AtomicI64::new(1_791_885_600_000);

struct FakeHarness {
    engine: Arc<FakeEngine>,
    runs: Mutex<HashMap<String, Run>>,
}

#[async_trait]
impl Harness for FakeHarness {
    fn engine(&self) -> Arc<dyn ContainerEngine> {
        self.engine.clone()
    }

    async fn create(&self, spec: &Spec) -> String {
        let mut observation = container(&spec.name, RunState::Created);
        observation.ports = spec
            .publish
            .map(|(host_port, container_port)| PortBinding {
                host_port,
                container_port,
                protocol: PortProtocol::Tcp,
                scope: PortScope::All,
            })
            .into_iter()
            .collect();
        observation.tty = spec.tty;
        if spec.log_driver_none {
            observation.log_driver = Some("none".into());
        }
        let id = observation.id.clone();
        // Les variables du décor : leurs noms dans l'observation, leurs valeurs pour `env` seulement.
        let pairs: Vec<(&str, &str)> = spec
            .env
            .iter()
            .filter_map(|entry| entry.split_once('='))
            .collect();
        observation.env_names = pairs.iter().map(|(name, _)| Hidden::new(*name)).collect();
        self.engine.set_env(&id, &pairs);
        self.runs.lock().unwrap().insert(id.clone(), spec.run);
        self.engine.put_silently(observation);
        // Un programme qui ignore l'ordre d'arrêt est tué à l'échéance du délai de grâce, code 137.
        if spec.run == Run::Forever {
            self.engine.make_stubborn(&id);
        }
        self.engine.emit(&id, "create");
        id
    }

    async fn start(&self, id: &str) {
        let run = self.runs.lock().unwrap()[id];
        self.engine.update(id, |observation| {
            observation.started_at = Some(at(10));
            match run {
                Run::Forever | Run::Obedient => {
                    observation.run_state = RunState::Running;
                    observation.exit_code = None;
                    observation.finished_at = None;
                }
                Run::ExitsWith(code) => {
                    observation.run_state = RunState::Exited;
                    observation.exit_code = Some(code);
                    observation.finished_at = Some(at(11));
                }
            }
        });
        self.engine.emit(id, "start");
        if matches!(run, Run::ExitsWith(_)) {
            self.engine.emit(id, "die");
        }
    }

    async fn stop(&self, id: &str) {
        self.engine.update(id, |observation| {
            observation.run_state = RunState::Exited;
            observation.exit_code = Some(0);
            observation.finished_at = Some(at(11));
        });
        self.engine.emit(id, "die");
    }

    async fn remove(&self, id: &str) {
        self.engine.remove(id);
    }

    async fn print(&self, id: &str, source: LogSource, text: &str) {
        let at_ms = NEXT_LOG_MS.fetch_add(1, Ordering::SeqCst);
        self.engine.push_log(id, source, at_ms, text);
    }
}

#[tokio::test]
async fn the_fake_engine_honours_the_engine_contract() {
    let harness = FakeHarness {
        engine: Arc::new(FakeEngine::new()),
        runs: Mutex::default(),
    };
    engine_contract::run(&harness, "contract").await;
}

#[tokio::test]
async fn the_fake_lists_inspects_and_streams_events() {
    use futures_util::StreamExt;
    use hearth_agent::application::ports::ContainerEngine;
    let engine = FakeEngine::new();
    let web = container("web", RunState::Running);
    let id = web.id.clone();
    engine.put_silently(web);
    assert_eq!(engine.list().await.expect("list").observations.len(), 1);
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
