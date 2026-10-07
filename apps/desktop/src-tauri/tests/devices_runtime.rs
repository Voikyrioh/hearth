//! HRT-23 : les postes de confiance de la coquille contre un VRAI agent (TLS 1.3, SQLite) : la clé de
//! ce PC est créée à la première connexion, la liste dit « ce poste », le retrait demande le mot de
//! passe ET la preuve de la clé, et ce que reçoit l'interface ne contient aucune clé. Le coffre est en
//! mémoire (jamais le vrai Gestionnaire d'identification). Aucune attente de durée : seuls des faits
//! observables (état du lien, par les événements de la bibliothèque).
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "../../../../crates/hearth-link/tests/support/agent.rs"]
mod agent;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent::{PASSWORD, TestAgent};
use hearth_agent::domain::accounts::Role;
use hearth_desktop_lib::devices::dto::{
    DeviceRemovalOutcome, DeviceRemovalRefusal, TrustedDeviceDto, TrustedDevicesDto,
};
use hearth_desktop_lib::devices::service;
use hearth_desktop_lib::link::{LinkRuntime, UiSink};
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
    _dir: tempfile::TempDir,
}

async fn client(port: u16) -> Client {
    let dir = tempfile::tempdir().unwrap();
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
    async fn devices(&self) -> Vec<TrustedDeviceDto> {
        match service::list(self.runtime.manager(), &self.id)
            .await
            .unwrap()
        {
            TrustedDevicesDto::Listed { devices, max } => {
                assert_eq!(max, 8);
                devices
            }
            TrustedDevicesDto::Unsupported => panic!("la liste existe sur cet agent"),
        }
    }

    async fn remove(&self, device: &str, password: &str) -> DeviceRemovalOutcome {
        service::remove(
            self.runtime.manager(),
            &self.id,
            device,
            &Secret::new(password),
        )
        .await
        .unwrap()
    }

    fn private_key(&self) -> String {
        let target = credential_target(&self.id, SecretKind::DeviceKey);
        let bytes = self.secrets.0.lock().unwrap().get(&target).cloned();
        String::from_utf8(bytes.expect("la clé est au coffre")).unwrap()
    }
}

#[tokio::test]
async fn the_key_is_created_at_the_first_login_and_the_list_says_this_pc_without_any_key() {
    let agent = TestAgent::install().await;
    agent.create_account("marie", Role::Admin).await;
    let this_pc = client(agent.addr.port()).await;
    let key = this_pc.private_key();
    assert!(!key.is_empty());
    let devices = this_pc.devices().await;
    assert_eq!(devices.len(), 1);
    assert!(devices[0].current);
    assert_eq!(devices[0].name, "poste-test/0.1");
    // Ce que l'interface reçoit : aucune clé, aucun défi, aucune signature, aucun jeton.
    let shown =
        serde_json::to_string(&vec![devices[0].clone()]).unwrap() + &format!("{:?}", devices[0]);
    assert!(!shown.contains(&key), "{shown}");
    for word in ["public", "challenge", "signature", "token", "key"] {
        assert!(!shown.to_lowercase().contains(word), "{word} : {shown}");
    }
}

#[tokio::test]
async fn removing_a_device_asks_for_the_password_and_proves_the_key_of_this_pc() {
    let agent = TestAgent::install().await;
    agent.create_account("marie", Role::Admin).await;
    let this_pc = client(agent.addr.port()).await;
    let other_pc = client(agent.addr.port()).await;
    let devices = this_pc.devices().await;
    assert_eq!(devices.len(), 2);
    let target = devices.iter().find(|d| !d.current).unwrap().id.clone();
    let current = devices.iter().find(|d| d.current).unwrap().id.clone();

    assert_eq!(
        this_pc.remove(&target, "Faux-Mot-De-Passe-1").await,
        DeviceRemovalOutcome::Refused {
            refusal: DeviceRemovalRefusal::WrongPassword
        }
    );
    assert_eq!(
        this_pc.remove(&current, PASSWORD).await,
        DeviceRemovalOutcome::Refused {
            refusal: DeviceRemovalRefusal::CurrentDevice
        }
    );
    assert_eq!(this_pc.devices().await.len(), 2, "rien n'a été retiré");
    assert_eq!(
        this_pc.remove(&target, PASSWORD).await,
        DeviceRemovalOutcome::Done
    );
    let after = this_pc.devices().await;
    assert_eq!(after.len(), 1);
    assert!(after[0].current);
    // Un poste déjà retiré : refus typé, la liste se relit.
    assert_eq!(
        this_pc.remove(&target, PASSWORD).await,
        DeviceRemovalOutcome::Refused {
            refusal: DeviceRemovalRefusal::NotFound
        }
    );
    // Un identifiant qui n'est pas un identifiant de poste ne part pas.
    let bad = service::remove(
        this_pc.runtime.manager(),
        &this_pc.id,
        "../accounts",
        &Secret::new(PASSWORD),
    )
    .await;
    assert!(bad.is_err());
    let _ = other_pc;
}

#[tokio::test]
async fn a_pc_without_a_key_cannot_remove_anything() {
    let agent = TestAgent::install().await;
    agent.create_account("marie", Role::Admin).await;
    let this_pc = client(agent.addr.port()).await;
    // Le coffre perd la clé (poste « pas encore enregistré » du point de vue de ce PC).
    this_pc
        .secrets
        .0
        .lock()
        .unwrap()
        .remove(&credential_target(&this_pc.id, SecretKind::DeviceKey));
    let outcome = this_pc.remove("01J9ZY0G3Q8M2K6W4T7V5N1B9D", PASSWORD).await;
    assert_eq!(
        outcome,
        DeviceRemovalOutcome::Refused {
            refusal: DeviceRemovalRefusal::NoDeviceKey
        }
    );
}
