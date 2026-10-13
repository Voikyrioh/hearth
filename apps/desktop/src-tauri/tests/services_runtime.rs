//! HRT-62 : le moteur de conteneurs et les services dans la coquille. Deux preuves.
//!
//! 1. La forme de ce que reçoit l'interface : la conversion du protocole (`hearth_proto::api::services`)
//!    est comparée, champ par champ et valeur par valeur, au fichier `tests/fixtures/services-event.json`,
//!    que les tests de l'interface (Vitest) relisent aussi pour contrôler le pont simulé. Aucun nom ni
//!    valeur de variable, aucune commande.
//! 2. La chaîne réelle jusqu'à la sortie de la coquille : un VRAI agent (TLS 1.3, SQLite, faux moteur de
//!    conteneurs) envoie le sujet `services`, `hearth-link` le reçoit et le fusionne, le relais de la
//!    coquille l'écrit dans la fenêtre sous le nom `link://services`, et une fenêtre qui arrive après coup
//!    retrouve le dernier état (`list_services_states`).
//!
//! Ce que ces tests ne prouvent PAS : l'émission réelle vers la WebView (`window.emit`), l'écoute
//! (`listen`) et l'appel des commandes par `invoke` : le relais écrit dans un enregistreur à la place de la
//! fenêtre.
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "../../../../crates/hearth-link/tests/support/agent.rs"]
mod agent;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent::fake_engine::FakeEngine;
use agent::fake_engine::container;
use agent::{PASSWORD, TestAgent};
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::services::RunState;
use hearth_desktop_lib::link::{LinkRuntime, UiSink};
use hearth_desktop_lib::services::dto::{ServicesEvent, ServicesViewDto};
use hearth_desktop_lib::vault::{CredentialBackend, CredentialVault};
use hearth_link::LinkConfig;
use hearth_link::domain::event::Event;
use hearth_link::domain::server::ServerId;
use hearth_link::domain::state::{LinkState, Thresholds};
use hearth_proto::api::services::{
    Concealed, EngineState, EngineView, HostInfo, ImageInfo, InitSystem, MIN_ENGINE_VERSION,
    MountKind, PortProtocol, PortScope, RestartMode, ServiceDriver, ServiceItem, ServiceMount,
    ServiceOrigin, ServicePort, ServiceState, ServicesState,
};
use serde_json::Value;

const GUARD: Duration = Duration::from_secs(60);

// ── La forme : la conversion contre la fiche de référence ──────────────────────────────────────

fn fixture() -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/services-event.json"
    ))
    .unwrap();
    serde_json::from_str(&text).unwrap()
}

/// Un état du protocole où TOUT champ facultatif est rempli : de quoi comparer chaque champ.
fn full_state() -> ServicesState {
    ServicesState {
        rev: 41,
        engine: EngineView {
            state: EngineState::Ready,
            name: Some("Docker Engine - Community".into()),
            version: Some("24.0.7".into()),
            min_version: MIN_ENGINE_VERSION.into(),
            agent_account: "root".into(),
            agent_is_admin: true,
            host: HostInfo {
                os_id: "debian".into(),
                os_like: vec!["ubuntu".into()],
                init: InitSystem::Systemd,
                declarative: false,
            },
            last_ready_at: Some("2026-10-13T10:00:00Z".into()),
            checked_at: "2026-10-13T10:00:02Z".into(),
            list_read_at: Some("2026-10-13T09:59:30Z".into()),
            list_degraded: true,
            at: Some("2026-10-13T10:00:02Z".into()),
            list_current_at: Some("2026-10-13T09:59:30Z".into()),
            external_count: 0,
        },
        services: vec![
            ServiceItem {
                key: "c_9f0a".into(),
                name: "web".into(),
                origin: ServiceOrigin::External,
                driver: Some(ServiceDriver::Supervisor),
                unit: Some("docker-web.service".into()),
                removed_on_stop: true,
                compose_project: Some("shop".into()),
                state: ServiceState::RestartLoop,
                since: Some("2026-10-13T09:00:00Z".into()),
                created_at: Some("2026-10-12T09:00:00Z".into()),
                never_started: false,
                exit_code: Some(137),
                restarts_10m: 4,
                image: ImageInfo {
                    reference: Some("nginx".into()),
                    tag: Some("1.27".into()),
                    short_id: "0123456789ab".into(),
                },
                ports: vec![
                    ServicePort {
                        host_port: 8080,
                        container_port: 80,
                        protocol: PortProtocol::Tcp,
                        scope: PortScope::All,
                    },
                    ServicePort {
                        host_port: 5353,
                        container_port: 53,
                        protocol: PortProtocol::Udp,
                        scope: PortScope::Local,
                    },
                ],
                ports_omitted: 3,
                host_network: false,
                // Ignorés par la copie de la coquille (détail d'un service, tranche suivante).
                mounts: vec![ServiceMount {
                    kind: MountKind::Folder,
                    source: "/srv/data".into(),
                    target: "/data".into(),
                    read_only: true,
                }],
                mounts_omitted: 5,
                restart: RestartMode::UnlessStopped,
                // Ignorés aussi (HRT-63 côté client) : la sentinelle ne doit JAMAIS sortir de la coquille.
                env_names: vec![Concealed::new("SENTINEL_ENV_NAME")],
                env_names_omitted: 2,
                command: Some(vec![Concealed::new("SENTINEL_COMMAND")]),
                grace_s: Some(30),
                busy: None,
                last_action: None,
                adopted: None,
                details_reserved: false,
            },
            ServiceItem {
                key: "c_77e1".into(),
                name: "sans-nom".into(),
                origin: ServiceOrigin::Hearth,
                driver: None,
                unit: None,
                removed_on_stop: false,
                compose_project: None,
                state: ServiceState::Stopped,
                since: None,
                created_at: None,
                never_started: true,
                exit_code: None,
                restarts_10m: 0,
                image: ImageInfo {
                    reference: None,
                    tag: None,
                    short_id: "abcdef012345".into(),
                },
                ports: vec![],
                ports_omitted: 0,
                host_network: true,
                mounts: vec![],
                mounts_omitted: 0,
                restart: RestartMode::None,
                env_names: vec![],
                env_names_omitted: 0,
                command: None,
                grace_s: None,
                busy: None,
                last_action: None,
                adopted: None,
                details_reserved: false,
            },
        ],
    }
}

#[test]
fn the_event_the_window_receives_is_the_reference_file_field_by_field() {
    let mut book = hearth_desktop_lib::services::book::ServicesBook::default();
    let received = hearth_link::domain::time::WallTime::from_millis(1_790_000_000_000);
    let event = book.on_state("forge", &full_state(), Some(received));
    assert_eq!(
        serde_json::to_value(&event).unwrap(),
        fixture()["supported"]
    );

    // Rejoué plus tard (fenêtre rechargée, état relu du disque), l'événement garde la date de RÉCEPTION
    // que la liaison a donnée : la coquille n'en pose jamais une autre à l'heure du rejeu.
    let replayed = book.current("forge").expect("l'état est rangé");
    assert_eq!(replayed.engine_received_at, Some(1_790_000_000_000.0));
    assert_eq!(
        serde_json::to_value(&replayed).unwrap(),
        fixture()["supported"]
    );

    let unsupported = book.on_unsupported("salon", "0.1.0");
    assert_eq!(
        serde_json::to_value(&unsupported).unwrap(),
        fixture()["unsupported"]
    );
}

/// Un agent plus récent : chaque valeur que ce client ne connaît pas traverse la liaison (lecture serde du
/// message de l'agent) puis la copie de la coquille, et arrive à l'interface comme « inconnue », sans perdre
/// l'instantané. La fiche `unknown` est le dernier maillon que lit le test Vitest du pont.
#[test]
fn a_value_from_a_newer_agent_reaches_the_window_as_unknown() {
    let wire = serde_json::json!({
        "rev": 9,
        "engine": {
            "state": "hibernating", "min_version": "20.10", "agent_account": "root",
            "agent_is_admin": true, "checked_at": "2026-10-13T10:00:02Z",
            "host": { "os_id": "debian", "os_like": [], "init": "runit", "declarative": false }
        },
        "services": [{
            "key": "c_aa01", "name": "futur", "origin": "imported", "driver": "swarm",
            "removed_on_stop": false, "state": "migrating", "never_started": false,
            "restarts_10m": 0, "image": { "short_id": "0123456789ab", "ref": "nginx", "tag": "1.27" },
            "ports": [{ "host_port": 4433, "container_port": 443, "protocol": "quic", "scope": "vpn" }],
            "host_network": false,
            "mounts": [{ "kind": "nfs", "source": "s", "target": "t", "read_only": true }],
            "restart": "never-heard-of"
        }]
    });
    let state: ServicesState = serde_json::from_value(wire).expect("l'instantané est lu");
    let mut book = hearth_desktop_lib::services::book::ServicesBook::default();
    let event = book.on_state("futur", &state, None);
    assert_eq!(serde_json::to_value(&event).unwrap(), fixture()["unknown"]);
}

/// Aucune clé de l'événement ne désigne une variable d'environnement ni une commande de lancement.
#[test]
fn the_event_carries_no_variable_name_value_nor_command() {
    fn keys(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                for (key, inner) in map {
                    out.push(key.to_lowercase());
                    keys(inner, out);
                }
            }
            Value::Array(items) => items.iter().for_each(|item| keys(item, out)),
            _ => {}
        }
    }
    let mut book = hearth_desktop_lib::services::book::ServicesBook::default();
    let event = serde_json::to_value(book.on_state("forge", &full_state(), None)).unwrap();
    assert!(!event.to_string().contains("SENTINEL"), "{event}");
    let mut found = Vec::new();
    keys(&event, &mut found);
    for forbidden in [
        "env",
        "envnames",
        "command",
        "cmd",
        "entrypoint",
        "value",
        "secret",
    ] {
        assert!(
            !found.contains(&forbidden.to_owned()),
            "{forbidden} dans {found:?}"
        );
    }
}

// ── La chaîne réelle ────────────────────────────────────────────────────────────────────────────

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

struct Nowhere;

impl UiSink for Nowhere {
    fn emit(&self, _: &str, _: serde_json::Value) {}
}

/// Garde ce que le relais envoie à la fenêtre.
#[derive(Default)]
struct Recorder(Mutex<Vec<(String, serde_json::Value)>>);

impl UiSink for Recorder {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        self.0.lock().unwrap().push((event.to_owned(), payload));
    }
}

impl Recorder {
    fn services(&self) -> Vec<Value> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(event, _)| event == "link://services")
            .map(|(_, payload)| payload.clone())
            .collect()
    }
}

fn config() -> LinkConfig {
    let hour = Duration::from_secs(3_600);
    LinkConfig {
        thresholds: Thresholds {
            silence: hour,
            reconnecting_after: hour,
            offline_after: hour * 2,
            ..Thresholds::scaled(6)
        },
        attempt_timeout: Duration::from_secs(30),
        request_timeout: Duration::from_secs(30),
        persist_timeout: Duration::from_secs(120),
        ..LinkConfig::default()
    }
}

fn names(event: &Value) -> Vec<String> {
    event["view"]["services"]
        .as_array()
        .map(|list| {
            list.iter()
                .map(|service| service["name"].as_str().unwrap().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

async fn wait_for(sink: &Recorder, what: &str, predicate: impl Fn(&Value) -> bool) -> Value {
    tokio::time::timeout(GUARD, async {
        loop {
            if let Some(found) = sink.services().into_iter().find(|event| predicate(event)) {
                return found;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("délai dépassé en attendant : {what}"))
}

#[tokio::test]
async fn a_real_agent_reaches_the_window_through_the_link_and_the_shell_relay() {
    let engine = Arc::new(FakeEngine::new());
    engine.put_silently(container("zeta", RunState::Running));
    engine.put_silently(container("Alpha", RunState::Exited));
    let agent = TestAgent::install_engined(engine.clone()).await;
    agent.create_account("marie", Role::Admin).await;

    let dir = agent::tmp::tempdir().unwrap();
    let vault = Arc::new(CredentialVault::new(Shared(Arc::new(Memory::default()))));
    let runtime = Arc::new(
        LinkRuntime::open_with(dir.path(), vault, "poste-test/0.1", config())
            .await
            .unwrap(),
    );
    // Le relais de la coquille, branché AVANT la connexion : tout ce que la liaison annonce part à la fenêtre.
    let sink = Arc::new(Recorder::default());
    let relay = {
        let runtime = runtime.clone();
        let sink = sink.clone();
        let stream = runtime.manager().subscribe();
        tokio::spawn(async move { runtime.forward(stream, &*sink).await })
    };
    let probe = runtime
        .probe("127.0.0.1", Some(agent.addr.port()))
        .await
        .unwrap();
    let server = runtime
        .add_and_login(
            "forge".into(),
            2,
            "127.0.0.1".into(),
            Some(agent.addr.port()),
            &probe.fingerprint,
            probe.mac_addresses,
            "marie",
            PASSWORD.into(),
            false,
            &Nowhere,
        )
        .await
        .unwrap();
    let id = ServerId::parse(&server.id).unwrap();

    // 1. L'instantané arrive à la fenêtre : moteur prêt, deux services triés sans tenir compte de la casse.
    let first = wait_for(&sink, "l'instantané des services", |event| {
        event["view"]["kind"] == "supported" && names(event).len() == 2
    })
    .await;
    assert_eq!(first["serverId"], id.as_str());
    assert_eq!(first["view"]["engine"]["state"], "ready");
    assert_eq!(first["view"]["engine"]["host"]["osId"], "debian");
    assert_eq!(names(&first), ["Alpha", "zeta"]);
    assert_eq!(first["view"]["services"][0]["state"], "stopped");
    assert_eq!(first["view"]["services"][1]["state"], "running");
    assert_eq!(first["view"]["services"][1]["origin"], "external");
    let seq = first["seq"].as_u64().unwrap();

    // 2. Un changement du moteur arrive à la fenêtre, avec un numéro plus grand.
    engine.put(container("beta", RunState::Running), "create");
    let second = wait_for(&sink, "un service créé", |event| names(event).len() == 3).await;
    assert_eq!(names(&second), ["Alpha", "beta", "zeta"]);
    assert!(second["seq"].as_u64().unwrap() > seq);

    // 3. Une fenêtre qui arrive après coup retrouve le dernier état, au même numéro.
    let replay = runtime.services_states().await;
    assert_eq!(replay.len(), 1);
    let ServicesEvent {
        server_id,
        seq: replayed,
        engine_received_at,
        view: ServicesViewDto::Supported { services, .. },
    } = &replay[0]
    else {
        panic!("un état complet était attendu");
    };
    assert_eq!(server_id, id.as_str());
    assert_eq!(services.len(), 3);
    // La date de réception vient de la liaison : présente, et jamais postérieure à maintenant.
    let received = engine_received_at.expect("la liaison date la vue du moteur");
    assert!(received > 1_700_000_000_000.0);
    assert!(*replayed >= u32::try_from(second["seq"].as_u64().unwrap()).unwrap());

    // 4. « Vérifier à nouveau » : l'agent sonde, l'état du moteur revient.
    let view = runtime.manager().engine_recheck(&id).await.unwrap();
    assert_eq!(view.state, EngineState::Ready);

    // 5. Les mesures, elles, n'ont pas été remplacées : la liaison est « Connectée ».
    assert_eq!(
        runtime.manager().state(&id).unwrap().state,
        LinkState::Connected
    );

    // 6. Le relais s'arrête avec la liaison.
    runtime.manager().shutdown().await;
    relay.abort();
}

#[tokio::test]
async fn an_agent_without_the_capability_is_relayed_as_unsupported_with_its_version() {
    // Le relais traduit l'événement de la liaison tel quel ; le cas réel (agent sans la capacité) est
    // prouvé dans `hearth-link` (`tests/services.rs`). Ici : la traduction et le rejeu.
    let dir = agent::tmp::tempdir().unwrap();
    let vault = Arc::new(CredentialVault::new(Shared(Arc::new(Memory::default()))));
    let runtime = LinkRuntime::open_with(dir.path(), vault, "poste-test/0.1", config())
        .await
        .unwrap();
    let sink = Recorder::default();
    runtime.relay(
        Event::ServicesUnsupported {
            server: ServerId::parse("salon").unwrap(),
            agent_version: "0.1.0".into(),
        },
        &sink,
    );
    let sent = sink.services();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["view"]["kind"], "unsupported");
    assert_eq!(sent[0]["view"]["agentVersion"], "0.1.0");
    assert_eq!(sent[0]["seq"], 1);
}
