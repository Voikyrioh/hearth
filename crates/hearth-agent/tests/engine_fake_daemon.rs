//! L'adaptateur `bollard` contre un faux démon HTTP local qui rejoue des réponses de la forme
//! documentée de l'API du moteur : la suite de contrat commune, puis les cas de sonde (huit états),
//! la négociation de version d'API et la lecture seule.
//!
//! Les réponses du faux démon sont FABRIQUÉES d'après la documentation de l'API (pas enregistrées
//! d'un vrai moteur) : la suite de contrat contre un vrai moteur (`engine_real.rs`, en CI) dit si
//! elles sont fidèles.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use hearth_agent::application::ports::ContainerEngine;
use hearth_agent::domain::services::logs::LogSource;
use hearth_agent::domain::services::{EngineState, ProbeOutcome, classify_probe};
use hearth_agent::infrastructure::engine::{BollardEngine, InstallMarkers, Target};
use support::engine_contract::{self, Harness, Run, Spec};
use support::fake_daemon::{DaemonContainer, FakeDaemon};

fn engine_of(daemon: &FakeDaemon) -> BollardEngine {
    BollardEngine::new(
        Target::Http(daemon.address()),
        InstallMarkers::at(Vec::<PathBuf>::new()),
    )
}

/// L'heure des lignes écrites par le décor : une milliseconde de plus à chaque ligne.
static NEXT_LOG_MS: AtomicI64 = AtomicI64::new(1_791_885_600_000);

struct DaemonHarness {
    daemon: Arc<FakeDaemon>,
    engine: Arc<BollardEngine>,
    runs: Mutex<HashMap<String, Run>>,
}

#[async_trait]
impl Harness for DaemonHarness {
    fn engine(&self) -> Arc<dyn ContainerEngine> {
        self.engine.clone()
    }

    async fn create(&self, spec: &Spec) -> String {
        let mut container = DaemonContainer::new(&spec.name, "created");
        container.publish = spec.publish;
        container.env = spec.env.clone();
        container.tty = spec.tty;
        if spec.log_driver_none {
            container.log_driver = "none".into();
        }
        // Un programme qui répond à l'ordre d'arrêt ne s'appelle pas `sleep` (le faux démon s'en sert pour
        // savoir qui ignore le signal).
        if spec.run == Run::Obedient {
            container.cmd = vec!["obedient".to_owned()];
        }
        let id = container.id.clone();
        self.runs.lock().unwrap().insert(id.clone(), spec.run);
        self.daemon.put(container);
        self.daemon.emit(&id, "create");
        id
    }

    async fn start(&self, id: &str) {
        let run = self.runs.lock().unwrap()[id];
        self.daemon.update(id, |container| {
            container.started_at = "2026-10-13T10:00:00.123456789Z".into();
            match run {
                Run::Forever | Run::Obedient => {
                    container.status = "running".into();
                    container.exit_code = 0;
                    container.finished_at = "0001-01-01T00:00:00Z".into();
                }
                Run::ExitsWith(code) => {
                    container.status = "exited".into();
                    container.exit_code = code;
                    container.finished_at = "2026-10-13T11:00:00.5Z".into();
                }
            }
        });
        self.daemon.emit(id, "start");
        if matches!(run, Run::ExitsWith(_)) {
            self.daemon.emit(id, "die");
        }
    }

    async fn stop(&self, id: &str) {
        self.daemon.update(id, |container| {
            container.status = "exited".into();
            container.exit_code = 0;
            container.finished_at = "2026-10-13T11:00:00.5Z".into();
        });
        self.daemon.emit(id, "die");
        self.daemon.emit(id, "stop");
    }

    async fn remove(&self, id: &str) {
        self.daemon.remove(id);
        self.daemon.emit(id, "destroy");
    }

    async fn print(&self, id: &str, source: LogSource, text: &str) {
        let at_ms = NEXT_LOG_MS.fetch_add(1, Ordering::SeqCst);
        let stream = if source == LogSource::Err { 2 } else { 1 };
        self.daemon.push_log(id, stream, at_ms, text);
    }
}

#[tokio::test]
async fn the_bollard_adapter_honours_the_engine_contract_against_a_fake_daemon() {
    let daemon = Arc::new(FakeDaemon::start().await);
    let harness = DaemonHarness {
        engine: Arc::new(engine_of(&daemon)),
        daemon: daemon.clone(),
        runs: Mutex::default(),
    };
    engine_contract::run(&harness, "contract").await;
    // Le démon n'a vu que des lectures, et les trois seules routes d'écriture du port (HRT-64) : démarrer,
    // arrêter, redémarrer un conteneur. Jamais créer, supprimer, tirer une image.
    let requests = daemon.requests();
    assert!(!requests.is_empty());
    for request in requests {
        let write = request.starts_with("POST ")
            && ["/start", "/stop", "/restart"]
                .iter()
                .any(|verb| request.ends_with(verb))
            && request.contains("/containers/");
        assert!(request.starts_with("GET ") || write, "{request}");
    }
}

async fn state_of(engine: &BollardEngine) -> EngineState {
    classify_probe(true, Some(&engine.probe().await))
}

#[tokio::test]
async fn a_recent_docker_is_ready_and_announces_its_name_and_version() {
    let daemon = FakeDaemon::start().await;
    let engine = engine_of(&daemon);
    let probe = engine.probe().await;
    let ProbeOutcome::Answered(announced) = &probe.outcome else {
        panic!("{probe:?}");
    };
    assert_eq!(announced.name, "Docker Engine - Community");
    assert_eq!(announced.version, "24.0.7");
    assert_eq!(classify_probe(true, Some(&probe)), EngineState::Ready);
}

#[tokio::test]
async fn an_old_docker_that_refuses_newer_api_versions_is_still_read() {
    // Docker 20.10 (API 1.41) refuse tout chemin versionné plus récent : l'adaptateur parle 1.41.
    let daemon = FakeDaemon::start().await;
    daemon.configure(|config| {
        config.api_version = (1, 41);
        config.engine_version = "20.10.24".into();
        config.platform_name = String::new();
    });
    daemon.put(DaemonContainer::new("old", "running"));
    let engine = engine_of(&daemon);
    let probe = engine.probe().await;
    let ProbeOutcome::Answered(announced) = &probe.outcome else {
        panic!("{probe:?}");
    };
    // Debian laisse le nom de plateforme vide.
    assert_eq!(announced.name, "Docker Engine");
    assert_eq!(classify_probe(true, Some(&probe)), EngineState::Ready);
    assert_eq!(engine.list().await.expect("liste").observations.len(), 1);
}

#[tokio::test]
async fn an_engine_older_than_the_minimum_is_an_unsupported_version() {
    let daemon = FakeDaemon::start().await;
    daemon.configure(|config| {
        config.api_version = (1, 40);
        config.engine_version = "19.03.15".into();
    });
    assert_eq!(
        state_of(&engine_of(&daemon)).await,
        EngineState::UnsupportedVersion
    );
}

#[tokio::test]
async fn a_podman_engine_is_an_unsupported_engine() {
    let daemon = FakeDaemon::start().await;
    daemon.configure(|config| {
        config.platform_name = "linux/amd64/fedora-40".into();
        config.component = "Podman Engine".into();
        config.engine_version = "5.2.0".into();
        config.api_version = (1, 41);
    });
    assert_eq!(
        state_of(&engine_of(&daemon)).await,
        EngineState::UnsupportedEngine
    );
}

#[tokio::test]
async fn a_refusing_socket_proxy_is_access_denied() {
    let daemon = FakeDaemon::start().await;
    daemon.configure(|config| config.ping_status = 403);
    assert_eq!(
        state_of(&engine_of(&daemon)).await,
        EngineState::AccessDenied
    );
}

#[tokio::test]
async fn a_daemon_that_never_answers_is_unresponsive_after_the_probe_delay() {
    let daemon = FakeDaemon::start().await;
    daemon.configure(|config| config.silent = true);
    let engine = engine_of(&daemon).with_probe_timeout(Duration::from_millis(200));
    assert_eq!(state_of(&engine).await, EngineState::Unresponsive);
}

#[tokio::test]
async fn the_whole_probe_is_bounded_by_one_delay_not_one_per_exchange() {
    // `/_ping` met 400 ms à répondre, `/version` ne répond jamais, le délai est de 800 ms. Un délai par
    // échange ferait 400 + 800 = 1200 ms ; un seul délai pour toute la sonde coupe à 800 ms (BR-ENGINE-009).
    // Marges de 100 ms en dessous et 300 ms au-dessus : un exécuteur chargé ne fait pas échouer le test.
    let daemon = FakeDaemon::start().await;
    daemon.configure(|config| {
        config.ping_delay = Duration::from_millis(400);
        config.version_silent = true;
    });
    let engine = engine_of(&daemon).with_probe_timeout(Duration::from_millis(800));
    let started = std::time::Instant::now();
    assert_eq!(state_of(&engine).await, EngineState::Unresponsive);
    let took = started.elapsed();
    assert!(took >= Duration::from_millis(700), "{took:?}");
    assert!(took < Duration::from_millis(1100), "{took:?}");
}

#[tokio::test]
async fn a_container_the_daemon_cannot_describe_is_unreadable_not_a_failed_list() {
    // Une inspection qui échoue pour un conteneur (ici, une réponse qui n'est pas du JSON attendu :
    // le démon répond 404 pour l'un des deux après la liste) ne fait pas échouer la liste.
    let daemon = FakeDaemon::start().await;
    daemon.put(DaemonContainer::new("good", "running"));
    let broken = DaemonContainer::new("broken", "running");
    let broken_id = broken.id.clone();
    daemon.put(broken);
    daemon.break_inspection(&broken_id);
    let engine = engine_of(&daemon);
    let listing = engine.list().await.expect("la liste aboutit");
    assert_eq!(listing.observations.len(), 1);
    assert_eq!(listing.observations[0].name, "good");
    assert_eq!(listing.unreadable.len(), 1);
    assert_eq!(listing.unreadable[0].id, broken_id);
    assert_eq!(listing.unreadable[0].name, "broken");
    // Un conteneur supprimé entre la liste et l'inspection n'est ni dans l'un ni dans l'autre.
    daemon.vanish_after_listing(&broken_id);
    let listing = engine.list().await.expect("la liste aboutit");
    assert_eq!(listing.observations.len(), 1);
    assert!(listing.unreadable.is_empty());
}

#[tokio::test]
async fn a_refusing_daemon_makes_the_list_denied() {
    let daemon = FakeDaemon::start().await;
    daemon.put(DaemonContainer::new("good", "running"));
    daemon.configure(|config| config.list_status = 403);
    let engine = engine_of(&daemon);
    assert_eq!(
        engine.list().await.expect_err("refusée"),
        hearth_agent::application::ports::EngineError::Denied
    );
}

#[tokio::test]
async fn a_refused_connection_is_a_stopped_engine() {
    // Un port qui ne reçoit personne : la connexion est refusée. Un socket resté sans moteur derrière
    // est celui d'un moteur éteint, avec ou sans repère d'installation.
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let address = format!("127.0.0.1:{port}");
    let installed = BollardEngine::new(
        Target::Http(address.clone()),
        InstallMarkers::at([std::env::current_exe().unwrap()]),
    );
    assert_eq!(state_of(&installed).await, EngineState::Stopped);
    let bare = BollardEngine::new(
        Target::Http(address),
        InstallMarkers::at(Vec::<PathBuf>::new()),
    );
    assert_eq!(state_of(&bare).await, EngineState::Stopped);
}

#[cfg(unix)]
#[tokio::test]
async fn a_missing_socket_file_is_absent_without_a_marker_and_stopped_with_one() {
    let missing = PathBuf::from("/nonexistent/hearth-test/docker.sock");
    let bare = BollardEngine::new(
        Target::Unix(missing.clone()),
        InstallMarkers::at(Vec::<PathBuf>::new()),
    );
    assert_eq!(state_of(&bare).await, EngineState::Absent);
    let installed = BollardEngine::new(
        Target::Unix(missing),
        InstallMarkers::at([std::env::current_exe().unwrap()]),
    );
    assert_eq!(state_of(&installed).await, EngineState::Stopped);
}

#[tokio::test]
async fn an_event_stream_that_the_daemon_cuts_ends_so_the_engine_is_rechecked() {
    let daemon = Arc::new(FakeDaemon::start().await);
    let engine = engine_of(&daemon);
    let mut events = engine.events();
    // Le flux se connecte à sa première lecture : on émet jusqu'à voir passer l'événement.
    let id = "ab".repeat(32);
    let mut seen = false;
    for _ in 0..30 {
        daemon.emit(&id, "start");
        if let Ok(Some(Ok(event))) =
            tokio::time::timeout(Duration::from_millis(200), events.next()).await
        {
            assert_eq!(event.container_id, id);
            seen = true;
            break;
        }
    }
    assert!(seen);
    // Le démon disparaît : le flux se termine (ou se rompt) au lieu de rester muet sans fin.
    drop(daemon);
    let end = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(item) = events.next().await {
            if item.is_err() {
                break;
            }
        }
    })
    .await;
    assert!(
        end.is_ok(),
        "le flux d'événements doit finir quand le démon tombe"
    );
}
