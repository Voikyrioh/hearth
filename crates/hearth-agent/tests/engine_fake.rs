//! La suite de contrat du port `ContainerEngine` contre le faux moteur (`FakeEngine`) : si le faux
//! ne tient pas le contrat, tous les tests bâtis dessus mentent.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use hearth_agent::application::ports::ContainerEngine;
use hearth_agent::domain::services::{PortBinding, PortProtocol, PortScope, RunState};
use hearth_agent::infrastructure::engine::FakeEngine;
use hearth_agent::infrastructure::engine::fake::{at, container};
use support::engine_contract::{self, Harness, Run, Spec};

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
        let id = observation.id.clone();
        self.runs.lock().unwrap().insert(id.clone(), spec.run);
        self.engine.put_silently(observation);
        self.engine.emit(&id, "create");
        id
    }

    async fn start(&self, id: &str) {
        let run = self.runs.lock().unwrap()[id];
        self.engine.update(id, |observation| {
            observation.started_at = Some(at(10));
            match run {
                Run::Forever => {
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
}

#[tokio::test]
async fn the_fake_engine_honours_the_engine_contract() {
    let harness = FakeHarness {
        engine: Arc::new(FakeEngine::new()),
        runs: Mutex::default(),
    };
    engine_contract::run(&harness, "contract").await;
}
