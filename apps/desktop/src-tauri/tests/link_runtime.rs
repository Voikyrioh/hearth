//! Le pont réel contre un vrai agent (TLS 1.3, SQLite, WebSocket) : ajout d'un serveur à sa première
//! connexion réussie, mot de passe au coffre sous `Hearth/{id}`, empreinte changée refusée puis
//! acceptée, oubli des identifiants, suppression. Les événements sont ceux que l'interface reçoit ;
//! l'état qui doit survivre à une interface qui arrive après coup (alerte d'empreinte, suivis perdus)
//! est relu sans aucun abonné.
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
use hearth_desktop_lib::link_dto::{LinkFailure, NoticeKind, RoleDto};
use hearth_desktop_lib::vault::{CredentialBackend, CredentialVault};
use hearth_link::LinkConfig;
use hearth_proto::fingerprint::Fingerprint;
use proxy::FaultProxy;
use serde_json::Value;
use tokio::net::TcpListener;

#[derive(Default)]
struct Memory(Mutex<HashMap<String, Vec<u8>>>);

/// Le même coffre en mémoire, visible du test pendant que la liaison s'en sert.
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

    fn last_state(&self, server: &str) -> Option<Value> {
        self.of("link://state")
            .into_iter()
            .rfind(|state| state["serverId"] == server)
    }
}

/// Attend qu'une condition devienne vraie (au plus 15 s).
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

struct Rig {
    _agent: TestAgent,
    /// Entre l'interface et l'agent : de quoi couper le lien et le rediriger vers une autre machine.
    proxy: FaultProxy,
    runtime: Arc<LinkRuntime>,
    sink: Arc<Recorder>,
    secrets: Arc<Memory>,
    dir: tempfile::TempDir,
}

fn vault(secrets: &Arc<Memory>) -> Arc<CredentialVault<Shared>> {
    Arc::new(CredentialVault::new(Shared(secrets.clone())))
}

async fn open(dir: &std::path::Path, secrets: &Arc<Memory>) -> (Arc<LinkRuntime>, Arc<Recorder>) {
    let runtime = Arc::new(
        LinkRuntime::open_with(dir, vault(secrets), "poste-test/0.1", LinkConfig::default())
            .await
            .unwrap(),
    );
    let sink = Arc::new(Recorder::default());
    let stream = runtime.manager().subscribe();
    let (forwarded, to) = (runtime.clone(), sink.clone());
    tokio::spawn(async move { forwarded.forward(stream, &*to).await });
    (runtime, sink)
}

async fn rig() -> Rig {
    let agent = TestAgent::install().await;
    agent.create_account("marie", Role::Admin).await;
    let proxy = FaultProxy::start(agent.addr).await;
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(Memory::default());
    let (runtime, sink) = open(dir.path(), &secrets).await;
    Rig {
        _agent: agent,
        proxy,
        runtime,
        sink,
        secrets,
        dir,
    }
}

impl Rig {
    fn port(&self) -> u16 {
        self.proxy.port()
    }

    /// L'assistant en entier : sonde, empreinte confirmée, connexion ; le serveur n'existe qu'ensuite.
    async fn add(&self, name: &str) -> (String, String) {
        let probe = self
            .runtime
            .probe("127.0.0.1", Some(self.port()))
            .await
            .unwrap();
        let server = self
            .runtime
            .add_and_login(
                name.into(),
                2,
                "127.0.0.1".into(),
                Some(self.port()),
                &probe.fingerprint,
                probe.mac_addresses,
                "marie",
                PASSWORD.into(),
                true,
                &*self.sink,
            )
            .await
            .unwrap();
        (server.id, probe.fingerprint)
    }
}

#[tokio::test]
async fn the_wizard_registers_only_on_a_successful_login_then_remembers_forgets_and_removes() {
    let rig = rig().await;
    // Sonde : l'empreinte s'affiche en 8 groupes de 4 ; rien n'est enregistré.
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

    // Une empreinte que cette application n'a pas lue à cette adresse n'est pas confirmable.
    let invented = rig
        .runtime
        .add_and_login(
            "Forge".into(),
            2,
            "127.0.0.1".into(),
            Some(rig.port()),
            &"ab".repeat(32),
            vec![],
            "marie",
            PASSWORD.into(),
            true,
            &*rig.sink,
        )
        .await;
    assert_eq!(invented.unwrap_err(), LinkFailure::VerificationRequired);

    // Mot de passe faux (message générique) : le serveur n'existe pas, rien au coffre, rien au carnet.
    let refused = rig
        .runtime
        .add_and_login(
            "Forge".into(),
            2,
            "127.0.0.1".into(),
            Some(rig.port()),
            &probe.fingerprint,
            vec![],
            "marie",
            "Mauvais-mot-de-passe-1".into(),
            true,
            &*rig.sink,
        )
        .await;
    assert_eq!(refused.unwrap_err(), LinkFailure::InvalidCredentials);
    assert!(rig.runtime.servers().is_empty());
    assert!(rig.secrets.keys().is_empty());
    assert!(!rig.dir.path().join("servers.json").exists());
    assert!(rig.sink.of("link://servers").is_empty());

    // Le bon mot de passe : serveur, empreinte et secrets arrivent ensemble.
    let (id, fingerprint) = rig.add("Forge").await;
    assert_eq!(fingerprint, probe.fingerprint);
    let listed = rig.runtime.servers();
    assert_eq!(listed.len(), 1);
    assert_eq!((listed[0].color, listed[0].role), (2, RoleDto::Admin));
    assert_eq!(
        rig.secrets.keys(),
        vec![format!("Hearth/{id}"), format!("Hearth/{id}/token")]
    );
    let password = rig.secrets.0.lock().unwrap()[&format!("Hearth/{id}")].clone();
    assert_eq!(password, PASSWORD.as_bytes());
    let book = std::fs::read_to_string(rig.dir.path().join("servers.json")).unwrap();
    assert!(book.contains(&probe.fingerprint), "empreinte épinglée");
    eventually("état Connecté", || {
        rig.sink
            .last_state(&id)
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
    // La même adresse ne s'ajoute pas deux fois.
    let again = rig
        .runtime
        .add_and_login(
            "Autre".into(),
            1,
            "127.0.0.1".into(),
            Some(rig.port()),
            &probe.fingerprint,
            vec![],
            "marie",
            PASSWORD.into(),
            true,
            &*rig.sink,
        )
        .await;
    assert_eq!(again.unwrap_err(), LinkFailure::AlreadyExists);

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
async fn a_changed_fingerprint_is_a_state_that_a_late_listener_still_finds_and_only_that_one_is_accepted()
 {
    let rig = rig().await;
    let (id, real) = rig.add("Forge").await;
    eventually("état Connecté", || {
        rig.sink
            .last_state(&id)
            .is_some_and(|s| s["state"] == "connected")
    })
    .await;
    // Rien à décider tant que l'identité n'a pas changé : accepter n'est pas possible.
    assert_eq!(
        rig.runtime
            .accept_fingerprint(&id, &"cd".repeat(32))
            .await
            .unwrap_err(),
        LinkFailure::VerificationRequired
    );
    // Le serveur est réinstallé : même adresse, autre certificat.
    let reinstalled = TestAgent::install().await;
    reinstalled.create_account("marie", Role::Admin).await;
    rig.proxy.cut();
    rig.proxy.set_target(reinstalled.addr);
    rig.proxy.heal();
    // L'alerte est un ÉTAT : relue sans écouter aucun événement.
    eventually("alerte d'empreinte en attente", || {
        !rig.runtime.fingerprint_alerts().is_empty()
    })
    .await;
    let alert = rig.runtime.fingerprint_alerts().pop().unwrap();
    assert_eq!(alert.server_id, id);
    let expected = Fingerprint::from_hex(&real).unwrap().short();
    assert_eq!(alert.expected, expected);
    assert_ne!(alert.presented, alert.expected);
    assert_ne!(alert.presented_hex, real);
    assert_eq!(alert.presented.split(' ').count(), 8);
    // L'événement n'est qu'un signal : il est aussi parti, avec les mêmes empreintes.
    eventually("signal link://fingerprint", || {
        !rig.sink.of("link://fingerprint").is_empty()
    })
    .await;
    assert_eq!(
        rig.sink.of("link://fingerprint").pop().unwrap()["presentedHex"],
        alert.presented_hex.as_str()
    );
    eventually("état bloqué", || {
        rig.sink
            .last_state(&id)
            .is_some_and(|s| s["blocked"] == "fingerprint_changed")
    })
    .await;
    // Aucun identifiant n'est parti vers la machine à l'identité douteuse (BR-CONN-003).
    assert_eq!(reinstalled.sessions_open("marie").await, 0);
    // L'utilisateur accepte : seule l'empreinte présentée est épinglée, le lien repart, l'alerte part.
    // Une autre empreinte que celle en attente (un signal a sauté, l'interface en montre une autre).
    assert_eq!(
        rig.runtime
            .accept_fingerprint(&id, &"cd".repeat(32))
            .await
            .unwrap_err(),
        LinkFailure::VerificationRequired
    );
    rig.runtime
        .accept_fingerprint(&id, &alert.presented_hex)
        .await
        .unwrap();
    eventually("état Connecté de nouveau", || {
        rig.sink
            .last_state(&id)
            .is_some_and(|s| s["state"] == "connected" && s["blocked"].is_null())
    })
    .await;
    assert!(rig.runtime.fingerprint_alerts().is_empty());
    assert_eq!(reinstalled.sessions_open("marie").await, 1);
}

#[tokio::test]
async fn a_server_reinstalled_while_the_pc_was_off_shows_its_alert_to_an_interface_that_arrives_later()
 {
    let rig = rig().await;
    let (id, _) = rig.add("Forge").await;
    eventually("état Connecté", || {
        rig.sink
            .last_state(&id)
            .is_some_and(|s| s["state"] == "connected")
    })
    .await;
    // Le PC s'éteint (l'application s'arrête), le serveur est réinstallé, le PC se rallume.
    rig.runtime.manager().shutdown().await;
    let reinstalled = TestAgent::install().await;
    reinstalled.create_account("marie", Role::Admin).await;
    rig.proxy.set_target(reinstalled.addr);
    let (runtime, _) = open(rig.dir.path(), &rig.secrets).await;
    // La première tentative constate le changement avant qu'aucune fenêtre n'écoute : l'alerte est
    // pourtant là, en attente de décision.
    eventually("alerte relue au lancement", || {
        !runtime.fingerprint_alerts().is_empty()
    })
    .await;
    assert_eq!(runtime.fingerprint_alerts()[0].server_id, id);
    let states = runtime.states();
    assert_eq!(
        states[0].blocked.map(|b| format!("{b:?}")),
        Some("FingerprintChanged".into())
    );
    // Accepter débloque le serveur, même sans que l'événement ait jamais été reçu.
    let shown = runtime.fingerprint_alerts()[0].presented_hex.clone();
    runtime.accept_fingerprint(&id, &shown).await.unwrap();
    eventually("serveur reconnecté", || {
        runtime
            .states()
            .iter()
            .any(|s| format!("{:?}", s.state) == "Connected")
    })
    .await;
}

#[tokio::test]
async fn notices_are_kept_until_acknowledged_and_reading_destroys_nothing() {
    let rig = rig().await;
    let (id, _) = rig.add("Forge").await;
    rig.runtime.manager().shutdown().await;
    let operations = rig.dir.path().join("operations");
    std::fs::create_dir_all(&operations).unwrap();
    std::fs::write(operations.join(format!("{id}.json")), b"[{").unwrap();
    let (runtime, _) = open(rig.dir.path(), &rig.secrets).await;
    // Attend que la tâche ait lu le fichier (elle le met de côté), puis lit.
    eventually("fichier mis de côté", || {
        operations.join(format!("{id}.json.corrupt")).exists()
    })
    .await;
    let notices = runtime.notices();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].kind, NoticeKind::OperationsLost);
    assert_eq!(notices[0].server_id.as_deref(), Some(id.as_str()));
    // Lire ne détruit rien : un abonnement qui échoue puis recommence retrouve l'avis.
    assert_eq!(runtime.notices(), notices);
    // Le signal en direct porte le même numéro : l'interface écarte le doublon.
    let sink = Recorder::default();
    runtime.relay(
        hearth_link::domain::event::Event::OperationsLost {
            server: hearth_link::domain::server::ServerId::parse(&id).unwrap(),
        },
        &sink,
    );
    assert_eq!(sink.of("link://notice")[0]["id"], notices[0].id);
    // Seul l'acquittement l'efface.
    runtime.ack_notices(&[notices[0].id]);
    assert!(runtime.notices().is_empty());
}

#[tokio::test]
async fn lagging_listeners_get_the_whole_state_again_alerts_included() {
    let rig = rig().await;
    let (id, _) = rig.add("Forge").await;
    eventually("état Connecté", || {
        rig.sink
            .last_state(&id)
            .is_some_and(|s| s["state"] == "connected")
    })
    .await;
    let reinstalled = TestAgent::install().await;
    reinstalled.create_account("marie", Role::Admin).await;
    rig.proxy.cut();
    rig.proxy.set_target(reinstalled.addr);
    rig.proxy.heal();
    eventually("alerte en attente", || {
        !rig.runtime.fingerprint_alerts().is_empty()
    })
    .await;
    // Le signal link://fingerprint a sauté : à `Lagged`, l'alerte est réannoncée avec l'état.
    let late = Recorder::default();
    rig.runtime.relay(
        hearth_link::domain::event::Event::Lagged { skipped: 3 },
        &late,
    );
    assert_eq!(late.of("link://fingerprint").len(), 1);
    assert_eq!(late.of("link://fingerprint")[0]["serverId"], id.as_str());
    assert!(!late.of("link://state").is_empty());
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
async fn moving_a_server_asks_for_a_new_fingerprint_that_this_app_has_read_and_keeps_the_login() {
    let rig = rig().await;
    let (id, _) = rig.add("Forge").await;
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
    // Une empreinte jamais lue à la nouvelle adresse est refusée, même bien formée.
    let invented = rig
        .runtime
        .update_server(
            &id,
            "Forge".into(),
            2,
            "127.0.0.1".into(),
            Some(1),
            Some("ab".repeat(32)),
            &*rig.sink,
        )
        .await;
    assert_eq!(invented.unwrap_err(), LinkFailure::VerificationRequired);
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
