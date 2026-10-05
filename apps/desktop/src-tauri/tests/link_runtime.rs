//! Le pont réel contre un vrai agent (TLS 1.3, SQLite, WebSocket) : ajout d'un serveur avec
//! empreinte, connexion, mot de passe au coffre sous `Hearth/{id}`, empreinte changée refusée puis
//! acceptée, oubli des identifiants, suppression. Les événements sont ceux que l'interface reçoit.
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
use hearth_desktop_lib::link_dto::{LinkFailure, RoleDto};
use hearth_desktop_lib::vault::{CredentialBackend, CredentialVault};
use hearth_link::{LinkConfig, LinkManager};
use hearth_proto::fingerprint::Fingerprint;
use proxy::FaultProxy;
use serde_json::Value;
use tokio::net::TcpListener;

#[derive(Default)]
struct Memory(Mutex<HashMap<String, Vec<u8>>>);

/// Le même coffre en mémoire, visible du test pendant que la liaison s'en sert.
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

impl Memory {
    fn keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.0.lock().unwrap().keys().cloned().collect();
        keys.sort();
        keys
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

    async fn wait(&self, what: &str, mut condition: impl FnMut(&Recorder) -> bool) {
        let started = Instant::now();
        while !condition(self) {
            assert!(
                started.elapsed() < Duration::from_secs(15),
                "attendu : {what}"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    fn last_state(&self, server: &str) -> Option<Value> {
        self.of("link://state")
            .into_iter()
            .rfind(|state| state["serverId"] == server)
    }
}

struct Rig {
    _agent: TestAgent,
    /// Entre l'interface et l'agent : de quoi couper le lien et le rediriger vers une autre machine.
    proxy: FaultProxy,
    runtime: Arc<LinkRuntime>,
    sink: Arc<Recorder>,
    secrets: Arc<Memory>,
    _dir: tempfile::TempDir,
}

async fn rig() -> Rig {
    let agent = TestAgent::install().await;
    agent.create_account("marie", Role::Admin).await;
    let proxy = FaultProxy::start(agent.addr).await;
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(Memory::default());
    let vault = Arc::new(CredentialVault::new(Shared(secrets.clone())));
    let manager = LinkManager::open(dir.path(), vault, "poste-test/0.1", LinkConfig::default())
        .await
        .unwrap();
    let runtime = Arc::new(LinkRuntime::new(manager));
    let sink = Arc::new(Recorder::default());
    let stream = runtime.manager().subscribe();
    let (forwarded, to) = (runtime.clone(), sink.clone());
    tokio::spawn(async move { forwarded.forward(stream, &*to).await });
    Rig {
        _agent: agent,
        proxy,
        runtime,
        sink,
        secrets,
        _dir: dir,
    }
}

impl Rig {
    fn port(&self) -> u16 {
        self.proxy.port()
    }

    /// Sonde, confirme l'empreinte et enregistre le serveur : l'assistant, temps 1 et 2.
    async fn add(&self, name: &str) -> (String, String) {
        let probe = self
            .runtime
            .probe("127.0.0.1", Some(self.port()))
            .await
            .unwrap();
        let server = self
            .runtime
            .add_server(
                name.into(),
                2,
                "127.0.0.1".into(),
                Some(self.port()),
                &probe.fingerprint,
                probe.mac_addresses,
                &*self.sink,
            )
            .await
            .unwrap();
        (server.id, probe.fingerprint)
    }
}

#[tokio::test]
async fn the_wizard_flow_registers_connects_remembers_forgets_and_removes() {
    let rig = rig().await;
    // Temps 1 et 2 : l'empreinte s'affiche en 8 groupes de 4 ; rien n'est encore enregistré.
    let probe = rig
        .runtime
        .probe("127.0.0.1", Some(rig.port()))
        .await
        .unwrap();
    assert_eq!(probe.fingerprint.len(), 64);
    let groups: Vec<&str> = probe.display.split(' ').collect();
    assert_eq!(groups.len(), 8);
    assert!(groups.iter().all(|g| g.len() == 4));
    assert!(rig.runtime.servers().is_empty());
    assert!(rig.secrets.keys().is_empty());

    let (id, fingerprint) = rig.add("Forge").await;
    assert_eq!(fingerprint, probe.fingerprint);
    let listed = rig.runtime.servers();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].color, 2);
    assert!(!listed[0].remember);
    // Le carnet annonce l'ajout à l'interface.
    assert_eq!(rig.sink.of("link://servers").len(), 1);
    // Pas de doublon de nom, même avec une autre casse (BR-CONN-008).
    let again = rig
        .runtime
        .add_server(
            "FORGE".into(),
            1,
            "localhost".into(),
            Some(rig.port()),
            &probe.fingerprint,
            vec![],
            &*rig.sink,
        )
        .await;
    assert_eq!(again.unwrap_err(), LinkFailure::NameTaken);

    // Temps 3 : mot de passe faux (message générique), puis bon, mémorisé au coffre.
    let refused = rig
        .runtime
        .login(
            &id,
            "marie",
            "Mauvais-mot-de-passe-1".into(),
            true,
            &*rig.sink,
        )
        .await;
    assert_eq!(refused.unwrap_err(), LinkFailure::InvalidCredentials);
    assert!(
        rig.secrets.keys().is_empty(),
        "rien au coffre après un refus"
    );
    let logged = rig
        .runtime
        .login(&id, "marie", PASSWORD.into(), true, &*rig.sink)
        .await
        .unwrap();
    assert_eq!(logged.role, RoleDto::Admin);
    assert_eq!(
        rig.secrets.keys(),
        vec![format!("Hearth/{id}"), format!("Hearth/{id}/token")]
    );
    let password = rig.secrets.0.lock().unwrap()[&format!("Hearth/{id}")].clone();
    assert_eq!(password, PASSWORD.as_bytes());
    rig.sink
        .wait("état Connecté", |sink| {
            sink.last_state(&id)
                .is_some_and(|s| s["state"] == "connected")
        })
        .await;
    // Les numéros de séquence croissent strictement pour ce serveur.
    let seqs: Vec<u64> = rig
        .sink
        .of("link://state")
        .iter()
        .filter(|s| s["serverId"] == id.as_str())
        .map(|s| s["seq"].as_u64().unwrap())
        .collect();
    assert!(seqs.windows(2).all(|pair| pair[0] < pair[1]), "{seqs:?}");
    // Le carnet annonce l'identifiant, le rôle et « se souvenir ».
    let last = rig.sink.of("link://servers").pop().unwrap();
    assert_eq!(last["servers"][0]["username"], "marie");
    assert_eq!(last["servers"][0]["role"], "admin");
    assert_eq!(last["servers"][0]["remember"], true);
    // Rejouer l'état courant à un nouvel abonné redonne le même numéro (rien de nouveau).
    let replay = rig.runtime.states();
    assert_eq!(replay.len(), 1);
    assert_eq!(
        f64::from(replay[0].seq),
        rig.sink.last_state(&id).unwrap()["seq"].as_f64().unwrap()
    );

    // Oubli des identifiants : le mot de passe part, la session reste (BR-CONN-004).
    rig.runtime
        .forget_credentials(&id, &*rig.sink)
        .await
        .unwrap();
    assert_eq!(rig.secrets.keys(), vec![format!("Hearth/{id}/token")]);
    assert!(!rig.runtime.servers()[0].remember);

    // Suppression : plus aucun secret, plus de serveur, plus d'état (BR-CONN-010).
    rig.runtime.remove_server(&id, &*rig.sink).await.unwrap();
    assert!(rig.secrets.keys().is_empty());
    assert!(rig.runtime.servers().is_empty());
    assert!(rig.runtime.states().is_empty());
    let last = rig.sink.of("link://servers").pop().unwrap();
    assert_eq!(last["servers"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn a_changed_fingerprint_blocks_the_link_until_it_is_accepted() {
    let rig = rig().await;
    let (id, real) = rig.add("Forge").await;
    rig.runtime
        .login(&id, "marie", PASSWORD.into(), true, &*rig.sink)
        .await
        .unwrap();
    rig.sink
        .wait("état Connecté", |sink| {
            sink.last_state(&id)
                .is_some_and(|s| s["state"] == "connected")
        })
        .await;
    // Le serveur est réinstallé : même adresse, autre certificat.
    let reinstalled = TestAgent::install().await;
    reinstalled.create_account("marie", Role::Admin).await;
    rig.proxy.cut();
    rig.proxy.set_target(reinstalled.addr);
    rig.proxy.heal();
    rig.sink
        .wait("alerte d'empreinte", |sink| {
            !sink.of("link://fingerprint").is_empty()
        })
        .await;
    let alert = rig.sink.of("link://fingerprint").pop().unwrap();
    assert_eq!(alert["serverId"], id.as_str());
    let expected = Fingerprint::from_hex(&real).unwrap().short();
    assert_eq!(alert["expected"], expected.as_str());
    assert_ne!(alert["presented"], alert["expected"]);
    assert_ne!(alert["presentedHex"], real.as_str());
    assert_eq!(alert["presented"].as_str().unwrap().split(' ').count(), 8);
    rig.sink
        .wait("état bloqué", |sink| {
            sink.last_state(&id)
                .is_some_and(|s| s["blocked"] == "fingerprint_changed")
        })
        .await;
    // Aucun identifiant n'est parti vers la machine à l'identité douteuse (BR-CONN-003).
    assert_eq!(reinstalled.sessions_open("marie").await, 0);
    // L'utilisateur accepte la nouvelle empreinte : le lien repart.
    rig.runtime
        .accept_fingerprint(&id, alert["presentedHex"].as_str().unwrap())
        .await
        .unwrap();
    rig.sink
        .wait("état Connecté de nouveau", |sink| {
            sink.last_state(&id)
                .is_some_and(|s| s["state"] == "connected" && s["blocked"].is_null())
        })
        .await;
    assert_eq!(reinstalled.sessions_open("marie").await, 1);
}

#[tokio::test]
async fn probing_something_that_is_not_an_agent_fails_with_a_typed_error() {
    let rig = rig().await;
    let closed = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = closed.local_addr().unwrap().port();
    drop(closed);
    assert_eq!(
        rig.runtime
            .probe("127.0.0.1", Some(port))
            .await
            .unwrap_err(),
        LinkFailure::Unreachable
    );
    assert!(matches!(
        rig.runtime
            .probe("pas une adresse", None)
            .await
            .unwrap_err(),
        LinkFailure::InvalidInput { .. }
    ));
    assert_eq!(
        rig.runtime.retry_now("inconnu").unwrap_err(),
        LinkFailure::UnknownServer
    );
}

#[tokio::test]
async fn moving_a_server_asks_for_a_new_fingerprint_and_keeps_the_stored_login() {
    let rig = rig().await;
    let (id, _) = rig.add("Forge").await;
    rig.runtime
        .login(&id, "marie", PASSWORD.into(), true, &*rig.sink)
        .await
        .unwrap();
    let moved = rig
        .runtime
        .update_server(
            &id,
            "Forge".into(),
            2,
            "127.0.0.1".into(),
            Some(1),
            None,
            &*rig.sink,
        )
        .await;
    assert_eq!(moved.unwrap_err(), LinkFailure::VerificationRequired);
    // Renommer et recolorer ne demande rien.
    let renamed = rig
        .runtime
        .update_server(
            &id,
            "Cave".into(),
            5,
            "127.0.0.1".into(),
            Some(rig.port()),
            None,
            &*rig.sink,
        )
        .await
        .unwrap();
    assert_eq!((renamed.name.as_str(), renamed.color), ("Cave", 5));
    assert!(renamed.remember);
    assert!(rig.secrets.keys().contains(&format!("Hearth/{id}")));
}
