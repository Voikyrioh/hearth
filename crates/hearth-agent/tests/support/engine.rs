//! Outils des tests du moteur de conteneurs : un faux moteur branché sur l'agent, des cadences
//! courtes, un hôte simulé. Aucun test ne touche le vrai socket d'un moteur.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use hearth_agent::app::EngineParts;
use hearth_agent::application::ports::{HostDescription, HostEnvironment};
use hearth_agent::application::services::EngineTimings;
use hearth_agent::infrastructure::engine::FakeEngine;

pub struct TestHost;

impl HostEnvironment for TestHost {
    fn describe(&self) -> HostDescription {
        HostDescription {
            os_id: "debian".into(),
            os_like: vec![],
            systemd: true,
            declarative: false,
            agent_account: "root".into(),
            agent_is_admin: true,
        }
    }
}

/// Des cadences courtes : l'état du moteur change au plus une fois par 250 ms, les événements sont
/// regroupés sur 20 ms.
pub fn fast_timings() -> EngineTimings {
    EngineTimings {
        ready_period: Duration::from_millis(40),
        other_period: Duration::from_millis(40),
        debounce_window: Duration::from_millis(250),
        batch_window: Duration::from_millis(20),
        min_recheck: Duration::ZERO,
    }
}

pub fn parts(engine: Arc<FakeEngine>) -> EngineParts {
    parts_with(engine, fast_timings())
}

pub fn parts_with(engine: Arc<FakeEngine>, timings: EngineTimings) -> EngineParts {
    EngineParts {
        engine,
        host: Arc::new(TestHost),
        timings,
    }
}
