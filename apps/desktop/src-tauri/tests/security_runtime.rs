//! HRT-26 : la sécurité d'un serveur dans la coquille contre un VRAI agent (TLS 1.3, SQLite) : lecture
//! de l'état, activation et désactivation du mode attaque (mot de passe ET preuve de la clé de ce PC),
//! rôle Lecture seule, PC sans clé, relais de `link://security` et rejeu à l'abonnement. Le coffre est
//! en mémoire (jamais le vrai Gestionnaire d'identification). Aucune attente de durée.
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "../../../../crates/hearth-link/tests/support/agent.rs"]
mod agent;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent::{PASSWORD, TestAgent};
use hearth_agent::domain::accounts::Role;
use hearth_desktop_lib::link::{LinkRuntime, UiSink};
use hearth_desktop_lib::link_dto::LinkFailure;
use hearth_desktop_lib::security::dto::{
    AttackModeOutcome, AttackModeRefusal, AttackModeStateDto, SecurityDeviceDto, SecurityRead,
};
use hearth_desktop_lib::security::service;
use hearth_desktop_lib::vault::{CredentialBackend, CredentialVault, credential_target};
use hearth_link::LinkConfig;
use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use hearth_link::domain::state::{LinkState, Thresholds};
use hearth_link::ports::vault::SecretKind;

const GUARD: Duration = Duration::from_secs(60);

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

struct Client {
    runtime: Arc<LinkRuntime>,
    id: ServerId,
    secrets: Arc<Memory>,
    _dir: agent::tmp::TestDir,
}

async fn client(port: u16) -> Client {
    let dir = agent::tmp::tempdir().unwrap();
    let secrets = Arc::new(Memory::default());
    let vault = Arc::new(CredentialVault::new(Shared(secrets.clone())));
    let runtime = Arc::new(
        LinkRuntime::open_with(dir.path(), vault, "poste-test/0.1", config())
            .await
            .unwrap(),
    );
    let mut events = runtime.manager().subscribe();
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
            false,
            &Nowhere,
        )
        .await
        .unwrap();
    let id = ServerId::parse(&server.id).unwrap();
    // Attend « Connecté » par les événements de la bibliothèque, sans horloge.
    tokio::time::timeout(GUARD, async {
        while runtime.manager().state(&id).unwrap().state != LinkState::Connected {
            events.recv().await.expect("le flux d'événements vit");
        }
    })
    .await
    .expect("connecté");
    Client {
        runtime,
        id,
        secrets,
        _dir: dir,
    }
}

impl Client {
    async fn read(&self) -> SecurityRead {
        service::read(&self.runtime, &self.id).await.unwrap()
    }

    async fn set(&self, active: bool, password: &str) -> Result<AttackModeOutcome, LinkFailure> {
        service::set(
            self.runtime.manager(),
            &self.id,
            active,
            &Secret::new(password),
        )
        .await
    }

    fn private_key(&self) -> String {
        let target = credential_target(&self.id, SecretKind::DeviceKey);
        let bytes = self.secrets.0.lock().unwrap().get(&target).cloned();
        String::from_utf8(bytes.expect("la clé est au coffre")).unwrap()
    }
}

fn known(read: SecurityRead) -> hearth_desktop_lib::security::dto::SecurityEvent {
    match read {
        SecurityRead::Known { snapshot } => snapshot,
        SecurityRead::Unsupported => panic!("la sécurité existe sur cet agent"),
    }
}

#[tokio::test]
async fn the_read_says_off_proven_and_a_key_at_hand_without_leaking_any_key() {
    let agent = TestAgent::install().await;
    agent.create_account("marie", Role::Admin).await;
    let pc = client(agent.addr.port()).await;
    let state = known(pc.read().await);
    assert_eq!(state.attack_mode.state, AttackModeStateDto::Off);
    assert!(!state.alert.own);
    assert_eq!(state.device, SecurityDeviceDto::Proven);
    assert!(state.key_at_hand);
    // Ce que reçoit l'interface : aucune clé, aucun défi, aucune signature, aucun jeton.
    let shown = serde_json::to_string(&state).unwrap() + &format!("{state:?}");
    assert!(!shown.contains(&pc.private_key()), "{shown}");
    for word in ["public", "challenge", "signature", "token", "secret"] {
        assert!(!shown.to_lowercase().contains(word), "{word} : {shown}");
    }
}

#[tokio::test]
async fn an_administrator_with_the_key_activates_then_deactivates_with_the_password() {
    let agent = TestAgent::install().await;
    agent.create_account("marie", Role::Admin).await;
    let pc = client(agent.addr.port()).await;

    assert_eq!(
        pc.set(true, "Faux-Mot-De-Passe-1").await.unwrap(),
        AttackModeOutcome::Refused {
            refusal: AttackModeRefusal::WrongPassword
        }
    );
    assert_eq!(
        known(pc.read().await).attack_mode.state,
        AttackModeStateDto::Off,
        "un mot de passe faux ne change rien"
    );
    let AttackModeOutcome::Done { attack_mode } = pc.set(true, PASSWORD).await.unwrap() else {
        panic!("activation attendue");
    };
    assert_eq!(attack_mode.state, AttackModeStateDto::Active);
    let state = known(pc.read().await);
    assert_eq!(state.attack_mode.state, AttackModeStateDto::Active);
    assert!(state.attack_mode.since.is_some());
    let AttackModeOutcome::Done { attack_mode } = pc.set(false, PASSWORD).await.unwrap() else {
        panic!("désactivation attendue");
    };
    assert_eq!(attack_mode.state, AttackModeStateDto::Off);
}

#[tokio::test]
async fn a_pc_without_a_key_is_not_recognized_and_nothing_is_sent() {
    let agent = TestAgent::install().await;
    agent.create_account("marie", Role::Admin).await;
    let pc = client(agent.addr.port()).await;
    pc.secrets
        .0
        .lock()
        .unwrap()
        .remove(&credential_target(&pc.id, SecretKind::DeviceKey));
    assert!(!known(pc.read().await).key_at_hand);
    assert_eq!(
        pc.set(true, PASSWORD).await,
        Err(LinkFailure::NotRecognized)
    );
    assert_eq!(
        pc.set(false, PASSWORD).await,
        Err(LinkFailure::NotRecognized)
    );
    // Rien n'a bougé chez l'agent.
    assert_eq!(
        known(pc.read().await).attack_mode.state,
        AttackModeStateDto::Off
    );
}

#[tokio::test]
async fn a_read_only_account_sees_the_state_but_is_forbidden_to_change_it() {
    let agent = TestAgent::install().await;
    agent.create_account("marie", Role::ReadOnly).await;
    let pc = client(agent.addr.port()).await;
    let state = known(pc.read().await);
    assert_eq!(state.attack_mode.state, AttackModeStateDto::Off);
    assert_eq!(
        state.alert.others, None,
        "jamais le nombre des autres comptes"
    );
    assert_eq!(pc.set(true, PASSWORD).await, Err(LinkFailure::Forbidden));
    assert_eq!(pc.set(false, PASSWORD).await, Err(LinkFailure::Forbidden));
}

#[tokio::test]
async fn the_relay_keeps_the_last_state_and_replays_it_to_a_late_window() {
    use hearth_link::domain::event::Event;
    use hearth_proto::api::security::{AlertInfo, AttackModeInfo, AttackModeState, SecurityView};
    let agent = TestAgent::install().await;
    agent.create_account("marie", Role::Admin).await;
    let pc = client(agent.addr.port()).await;
    let sink = Recorder::default();
    assert!(
        pc.runtime.security_states().is_empty(),
        "rien avant un état"
    );
    let view = |own: bool, state: AttackModeState| {
        Arc::new(SecurityView {
            alert: AlertInfo {
                own,
                since: own.then(|| "2026-10-07T01:00:00Z".to_owned()),
                others: None,
            },
            attack_mode: AttackModeInfo {
                state,
                since: None,
                resumes_in_s: None,
                last_end: None,
            },
        })
    };
    pc.runtime.relay(
        Event::Security {
            server: pc.id.clone(),
            view: view(true, AttackModeState::Off),
        },
        &sink,
    );
    pc.runtime.relay(
        Event::Security {
            server: pc.id.clone(),
            view: view(true, AttackModeState::Active),
        },
        &sink,
    );
    let sent = sink.0.lock().unwrap().clone();
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0].0, "link://security");
    assert_eq!(sent[0].1["serverId"], pc.id.as_str());
    assert_eq!(sent[0].1["seq"], 1);
    assert_eq!(sent[1].1["seq"], 2, "croît strictement");
    assert_eq!(sent[1].1["attackMode"]["state"], "active");
    assert_eq!(sent[1].1["alert"]["own"], true);
    // Rien de la session de ce poste n'est dans le message du flux : l'interface relit.
    assert_eq!(sent[1].1["device"], "unknown");
    assert_eq!(sent[1].1["keyAtHand"], true);
    // Une fenêtre qui arrive après coup retrouve le dernier état.
    let replay = pc.runtime.security_states();
    assert_eq!(replay.len(), 1);
    assert_eq!(replay[0].seq, 2);
    assert_eq!(replay[0].attack_mode.state, AttackModeStateDto::Active);
    // Une lecture complète garde la même suite de numéros et apporte le poste.
    let read = known(pc.read().await);
    assert_eq!(read.seq, 3);
    assert_eq!(read.device, SecurityDeviceDto::Proven);
}
