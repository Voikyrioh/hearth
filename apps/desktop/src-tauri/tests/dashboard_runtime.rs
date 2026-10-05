//! Le tableau de bord contre un vrai agent (TLS 1.3, WebSocket) : l'identité et l'historique
//! arrivent par `link://snapshot`, un échantillon par `link://metrics`, avec des niveaux déjà
//! décidés ; la commande `get_dashboard` rend la dernière vue connue, avant tout abonné et après
//! un redémarrage du client (BR-DASH-001, 002, 005, 006, 009).
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "../../../../crates/hearth-link/tests/support/agent.rs"]
mod agent;
#[path = "../../../../crates/hearth-link/tests/support/proxy.rs"]
mod proxy;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent::{PASSWORD, TestAgent};
use hearth_agent::domain::accounts::Role;
use hearth_desktop_lib::link::{LinkRuntime, UiSink};
use hearth_desktop_lib::link_dto::LinkFailure;
use hearth_desktop_lib::vault::{CredentialBackend, CredentialVault};
use hearth_link::LinkConfig;
use serde_json::Value;

#[derive(Default)]
struct Memory(Mutex<HashMap<String, Vec<u8>>>);

#[derive(Clone)]
struct Shared(Arc<Memory>);

impl CredentialBackend for Shared {
    fn read(&self, target: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.0.0.lock().unwrap().get(target).cloned())
    }
    fn write(&self, target: &str, secret: &[u8]) -> Result<(), String> {
        self.0
            .0
            .lock()
            .unwrap()
            .insert(target.into(), secret.into());
        Ok(())
    }
    fn remove(&self, target: &str) -> Result<(), String> {
        self.0.0.lock().unwrap().remove(target);
        Ok(())
    }
}

#[derive(Default)]
struct Recorder(Mutex<Vec<(String, Value)>>);

impl UiSink for Recorder {
    fn emit(&self, event: &str, payload: Value) {
        self.0.lock().unwrap().push((event.to_owned(), payload));
    }
}

impl Recorder {
    fn of(&self, event: &str) -> Vec<Value> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(name, _)| name == event)
            .map(|(_, payload)| payload.clone())
            .collect()
    }
}

async fn eventually(what: &str, mut condition: impl FnMut() -> bool) {
    let started = Instant::now();
    while !condition() {
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "attendu : {what}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn open(dir: &std::path::Path, secrets: &Arc<Memory>) -> (Arc<LinkRuntime>, Arc<Recorder>) {
    let vault = Arc::new(CredentialVault::new(Shared(secrets.clone())));
    let runtime = Arc::new(
        LinkRuntime::open_with(dir, vault, "poste-test/0.1", LinkConfig::default())
            .await
            .unwrap(),
    );
    let sink = Arc::new(Recorder::default());
    let stream = runtime.manager().subscribe();
    let (forwarded, to) = (runtime.clone(), sink.clone());
    tokio::spawn(async move { forwarded.forward(stream, &*to).await });
    (runtime, sink)
}

/// Un serveur enregistré et connecté (l'assistant en entier).
async fn connected() -> (
    TestAgent,
    Arc<LinkRuntime>,
    Arc<Recorder>,
    Arc<Memory>,
    tempfile::TempDir,
    String,
) {
    let agent = TestAgent::install().await;
    agent.create_account("marie", Role::Admin).await;
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(Memory::default());
    let (runtime, sink) = open(dir.path(), &secrets).await;
    let port = agent.addr.port();
    let probe = runtime.probe("127.0.0.1", Some(port)).await.unwrap();
    let server = runtime
        .add_and_login(
            "forge".into(),
            2,
            "127.0.0.1".into(),
            Some(port),
            &probe.fingerprint,
            probe.mac_addresses,
            "marie",
            PASSWORD.into(),
            true,
            &*sink,
        )
        .await
        .unwrap();
    (agent, runtime, sink, secrets, dir, server.id)
}

#[tokio::test]
async fn the_live_stream_reaches_the_interface_with_levels_and_the_machine_identity() {
    let (_agent, runtime, sink, _secrets, _dir, id) = connected().await;
    eventually("l'identité et au moins trois échantillons", || {
        !sink.of("link://snapshot").is_empty() && sink.of("link://metrics").len() >= 3
    })
    .await;

    let snapshot = sink.of("link://snapshot").remove(0);
    assert_eq!(snapshot["serverId"], id.as_str());
    assert_eq!(snapshot["machine"]["name"], "forge-test");
    // Une machine sans carte graphique ni sonde : capacités fausses, listes vides (BR-DASH-005, 006).
    assert_eq!(snapshot["machine"]["capabilities"]["gpu"], false);
    assert_eq!(snapshot["machine"]["capabilities"]["temps"], false);
    assert_eq!(snapshot["machine"]["gpus"].as_array().unwrap().len(), 0);
    assert_eq!(snapshot["machine"]["disks"][0]["mount"], "/");

    let metrics = sink.of("link://metrics");
    let times: Vec<f64> = metrics
        .iter()
        .map(|event| event["sample"]["at"].as_f64().unwrap())
        .collect();
    assert!(times.windows(2).all(|pair| pair[0] < pair[1]), "{times:?}");
    let last = metrics.last().unwrap();
    assert_eq!(last["serverId"], id.as_str());
    // 4 Gio sur 16 Gio : 25 %, normal ; le disque à 40 % : normal.
    assert_eq!(last["levels"]["mem"], "normal");
    assert_eq!(last["levels"]["disks"][0], "normal");
    assert_eq!(last["sample"]["cores"].as_array().unwrap().len(), 4);
    assert!(last["sample"]["net"].is_null());
    assert_eq!(last["sample"]["gpus"].as_array().unwrap().len(), 0);

    // Un abonné qui arrive après coup relit la dernière vue.
    let view = runtime.dashboard(&id).await.unwrap().unwrap();
    assert_eq!(view.machine.name, "forge-test");
    assert!(!view.history.is_empty());
    assert!(view.levels.is_some());
}

#[tokio::test]
async fn the_last_view_survives_a_restart_of_the_client_and_is_read_without_any_link() {
    let (agent, runtime, sink, secrets, dir, id) = connected().await;
    eventually("une vue enregistrée", || {
        sink.of("link://metrics").len() >= 2
    })
    .await;
    // Le serveur s'éteint puis le client est relancé : la dernière vue, périmée, reste lisible.
    drop(agent);
    runtime.manager().shutdown().await;
    drop(runtime);
    let (reopened, _sink) = open(dir.path(), &secrets).await;
    let view = reopened.dashboard(&id).await.unwrap().unwrap();
    assert_eq!(view.machine.name, "forge-test");
    assert!(!view.history.is_empty());
}

#[tokio::test]
async fn an_unknown_server_has_no_dashboard() {
    let (_agent, runtime, _sink, _secrets, _dir, _id) = connected().await;
    let unknown = runtime.dashboard("01J9ZY0G3Q8M2K6W4T7V5N1B9D").await;
    assert!(matches!(unknown, Err(LinkFailure::UnknownServer)));
    assert!(matches!(
        runtime.dashboard("../mal").await,
        Err(LinkFailure::UnknownServer)
    ));
}

#[tokio::test]
async fn after_a_lag_the_last_view_of_every_server_is_announced_again() {
    let (_agent, runtime, sink, _secrets, _dir, id) = connected().await;
    eventually("une vue enregistrée", || {
        !sink.of("link://snapshot").is_empty() && sink.of("link://metrics").len() >= 2
    })
    .await;
    let before = sink.of("link://snapshot").len();
    runtime.resync_dashboards(&*sink).await;
    let snapshots = sink.of("link://snapshot");
    assert!(snapshots.len() > before);
    let last = snapshots.last().unwrap();
    assert_eq!(last["serverId"], id.as_str());
    assert_eq!(last["machine"]["name"], "forge-test");
}
