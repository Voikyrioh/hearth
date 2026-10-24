//! Outils des tests du moteur de conteneurs : un faux moteur branché sur l'agent, des cadences
//! courtes, un hôte simulé. Aucun test ne touche le vrai socket d'un moteur.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use super::fake_engine::FakeEngine;
use super::fake_supervisor::FakeSupervisor;
use hearth_agent::app::EngineParts;
use hearth_agent::application::ports::{HostDescription, HostEnvironment};
use hearth_agent::application::service_lifecycle::LifecycleTimings;
use hearth_agent::application::services::EngineTimings;

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
    parts_supervised(engine, Arc::new(FakeSupervisor::new()), timings)
}

/// Comme `parts_with`, avec ce faux superviseur : le test règle propriétaires et états des unités.
pub fn parts_supervised(
    engine: Arc<FakeEngine>,
    supervisor: Arc<FakeSupervisor>,
    timings: EngineTimings,
) -> EngineParts {
    EngineParts {
        engine,
        supervisor,
        host: Arc::new(TestHost),
        timings,
        // Les tests qui ne portent pas sur le plafond par compte ni sur le frein les détendent.
        log_brake: hearth_agent::domain::services::BrakeSettings::relaxed(),
        lifecycle: fast_lifecycle(),
        host_ports: Arc::new(super::fake_host::FakeHostPorts::new()),
        free_space: Arc::new(super::fake_host::FakeFreeSpace::new()),
        deploy: fast_deploy(),
        safe_fs: Some(Arc::new(super::fake_safe_fs::FakeSafeFs::new())),
        catalog: hearth_agent::app::CatalogParts {
            include_preview: false,
            random: Arc::new(super::fake_random::FakeRandom::counting_from(0)),
            service: None,
        },
    }
}

/// La surveillance qui suit un déploiement : 10 s devient 300 ms (HRT-68).
pub fn fast_deploy() -> hearth_agent::application::deployments::DeployTimings {
    hearth_agent::application::deployments::DeployTimings {
        deadline: Duration::from_secs(60),
        watch: Duration::from_millis(300),
        poll: Duration::from_millis(20),
    }
}

/// Des surveillances courtes après une action (HRT-64) : 10 s et 30 s deviennent 300 ms et 600 ms.
pub fn fast_lifecycle() -> LifecycleTimings {
    LifecycleTimings {
        startup_watch: Duration::from_millis(300),
        restart_watch: Duration::from_millis(600),
        poll: Duration::from_millis(20),
        exemption_margin: Duration::from_secs(2),
        wait_margin: Duration::from_secs(1),
        wait_floor: Duration::from_secs(1),
        // Une « seconde de fiche » dure une milliseconde (HRT-103).
        game: hearth_agent::application::game_stop::GameStopTimings {
            unit: Duration::from_millis(1),
            poll: Duration::from_millis(10),
        },
    }
}
