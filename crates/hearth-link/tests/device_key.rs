//! La clé d'appareil du client contre un VRAI agent (TLS 1.3, SQLite) : création à la première
//! connexion par mot de passe, défi signé à la connexion et à chaque ouverture du flux, agent ancien,
//! coffre en panne, défi coupé ou refusé, retrait d'un poste (mot de passe ET preuve de clé), et
//! secret jamais visible. HRT-23 (ADR-0023, BR-TRUST-003, 005, 026). Aucune attente de durée :
//! seuls des faits observables (états, événements, comptes de requêtes) et l'horloge de l'agent.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::{Arc, Mutex};

use hearth_link::adapters::MemoryVault;
use hearth_link::domain::event::Event;
use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use hearth_link::domain::state::LinkState;
use hearth_link::ports::vault::{SecretKind, Vault, VaultError};
use hearth_link::{ActionOutcome, LinkError, LinkManager, NewServer, Ports};
use hearth_proto::api::sessions::ChallengePurpose;
use support::{
    ChallengeMode, JumpClock, Options, PASSWORD, Recorder, ScriptedNet, TestAgent, WAIT, World,
    fast_config,
};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event as TracingEvent, Metadata, Subscriber};

fn keyed() -> Options {
    Options {
        device_key: true,
        ..Options::default()
    }
}

fn key_of(vault: &MemoryVault, id: &ServerId) -> Option<String> {
    vault
        .get(id, SecretKind::DeviceKey)
        .unwrap()
        .map(|secret| secret.expose().to_owned())
}

/// Un deuxième PC du même compte : son propre carnet, son propre coffre, donc sa propre clé.
struct SecondPc {
    manager: LinkManager,
    id: ServerId,
    vault: Arc<MemoryVault>,
    recorder: Recorder,
    _dir: tempfile::TempDir,
}

async fn second_pc(world: &World) -> SecondPc {
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(MemoryVault::new());
    let manager = support::start_manager(
        dir.path(),
        vault.clone(),
        Arc::new(ScriptedNet::new()),
        Arc::new(JumpClock::new()),
        fast_config(),
    )
    .await;
    let recorder = Recorder::spawn(manager.subscribe());
    let probe = manager
        .probe("127.0.0.1", world.proxy.port())
        .await
        .unwrap();
    let id = manager
        .add_server(NewServer {
            name: "Forge".into(),
            color: "#7aa2f7".into(),
            host: "127.0.0.1".into(),
            port: world.proxy.port(),
            fingerprint: probe.fingerprint,
            mac_addresses: probe.hello.mac_addresses.clone(),
        })
        .await
        .unwrap();
    let mark = recorder.mark();
    manager
        .login(&id, "marie", Secret::from(PASSWORD), false)
        .await
        .unwrap();
    recorder.wait_state(mark, LinkState::Connected, WAIT).await;
    SecondPc {
        manager,
        id,
        vault,
        recorder,
        _dir: dir,
    }
}

/// Coupe le lien puis le rétablit, et attend « Connecté » de nouveau.
async fn reconnect(world: &World) {
    let mark = world.recorder.mark();
    world.proxy.cut();
    world
        .recorder
        .wait_for(
            mark,
            "une coupure vue",
            WAIT,
            |e| matches!(e, Event::State { info, .. } if info.state != LinkState::Connected),
        )
        .await;
    let mark = world.recorder.mark();
    world.proxy.heal();
    world
        .recorder
        .wait_state(mark, LinkState::Connected, WAIT)
        .await;
}

// ── Création, stockage, une clé par serveur ──────────────────────────────────────────────

#[tokio::test]
async fn the_first_password_login_creates_the_key_and_the_agent_enrolls_this_pc() {
    let world = World::connected(keyed()).await;
    assert!(key_of(&world.vault, &world.id).is_some(), "clé au coffre");
    let list = world.manager.devices_list(&world.id).await.unwrap();
    assert_eq!(list.max, 8);
    assert_eq!(list.devices.len(), 1);
    let device = &list.devices[0];
    assert!(device.current, "c'est ce poste");
    assert_eq!(device.name, "poste-test/0.1");
    assert_eq!(device.last_addr, "127.0.0.1");
    // Un défi à la connexion, puis un défi à l'ouverture du flux : le message signé est celui que
    // l'agent vérifie (sans quoi il n'aurait rien inscrit).
    let purposes = world.spy.purposes.lock().unwrap().clone();
    assert_eq!(
        purposes,
        [ChallengePurpose::Login, ChallengePurpose::Session]
    );
}

#[tokio::test]
async fn every_stream_authentication_proves_the_key_again() {
    let world = World::connected(keyed()).await;
    let before = world.manager.devices_list(&world.id).await.unwrap().devices[0]
        .last_proved_at
        .clone();
    world.agent.clock.advance(time::Duration::hours(1));
    reconnect(&world).await;
    let after = world.manager.devices_list(&world.id).await.unwrap().devices[0]
        .last_proved_at
        .clone();
    // L'agent n'a mis à jour la dernière preuve que parce que la signature de l'`auth` du flux,
    // liée au jeton, est valide.
    assert!(after > before, "{before} puis {after}");
    assert!(world.spy.calls_for(ChallengePurpose::Session) >= 2);
    // Toujours un seul poste : la même clé.
    assert_eq!(
        world
            .manager
            .devices_list(&world.id)
            .await
            .unwrap()
            .devices
            .len(),
        1
    );
}

/// Un coffre qui compte les clés d'appareil qu'on lui range (le test lit, le gestionnaire écrit).
struct CountingVault {
    inner: MemoryVault,
    /// Entrées « clé d'appareil » vivantes au coffre (rangée = +1, effacée = -1).
    key_puts: std::sync::atomic::AtomicUsize,
}

impl Vault for CountingVault {
    fn get(&self, server: &ServerId, kind: SecretKind) -> Result<Option<Secret>, VaultError> {
        self.inner.get(server, kind)
    }

    fn put(&self, server: &ServerId, kind: SecretKind, secret: &Secret) -> Result<(), VaultError> {
        let existed = kind == SecretKind::DeviceKey && self.inner.get(server, kind)?.is_some();
        if kind == SecretKind::DeviceKey && !existed {
            self.key_puts
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        self.inner.put(server, kind, secret)
    }

    fn delete(&self, server: &ServerId, kind: SecretKind) -> Result<(), VaultError> {
        if kind == SecretKind::DeviceKey && self.inner.get(server, kind)?.is_some() {
            self.key_puts
                .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        }
        self.inner.delete(server, kind)
    }
}

#[tokio::test]
async fn a_refused_connection_leaves_no_key_behind() {
    let agent = TestAgent::install().await;
    agent
        .create_account("marie", hearth_agent::domain::accounts::Role::Admin)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(CountingVault {
        inner: MemoryVault::new(),
        key_puts: std::sync::atomic::AtomicUsize::new(0),
    });
    let manager = support::start_manager_shared(
        dir.path(),
        vault.clone(),
        Arc::new(ScriptedNet::new()),
        Arc::new(JumpClock::new()),
        fast_config(),
        Arc::new(support::transport()),
    )
    .await;
    let probe = manager.probe("127.0.0.1", agent.addr.port()).await.unwrap();
    let new = || NewServer {
        name: "Forge".into(),
        color: "#7aa2f7".into(),
        host: "127.0.0.1".into(),
        port: agent.addr.port(),
        fingerprint: probe.fingerprint,
        mac_addresses: vec![],
    };
    // Première connexion refusée (mauvais mot de passe) : pas de serveur, pas de clé.
    let error = manager
        .add_and_login(
            new(),
            "marie",
            Secret::from("pas-le-bon-mot-de-passe"),
            false,
        )
        .await
        .unwrap_err();
    assert_eq!(error, LinkError::InvalidCredentials);
    assert!(manager.servers().is_empty());
    assert_eq!(vault.key_puts.load(std::sync::atomic::Ordering::SeqCst), 0);
    // Une connexion refusée sur un serveur du carnet : pas de clé non plus.
    let id = manager.add_server(new()).await.unwrap();
    let error = manager
        .login(&id, "marie", Secret::from("pas-le-bon-mot-de-passe"), false)
        .await
        .unwrap_err();
    assert_eq!(error, LinkError::InvalidCredentials);
    assert_eq!(vault.key_puts.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert_eq!(key_of(&vault.inner, &id), None);
    // Réussie : une seule clé.
    manager
        .login(&id, "marie", Secret::from(PASSWORD), false)
        .await
        .unwrap();
    assert_eq!(vault.key_puts.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn one_key_per_server_and_removing_a_server_removes_its_key() {
    let world = World::connected(keyed()).await;
    let other = TestAgent::install().await;
    other
        .create_account("marie", hearth_agent::domain::accounts::Role::Admin)
        .await;
    let probe = world
        .manager
        .probe("127.0.0.1", other.addr.port())
        .await
        .unwrap();
    let (second, _) = world
        .manager
        .add_and_login(
            NewServer {
                name: "Autre forge".into(),
                color: "#7aa2f7".into(),
                host: "127.0.0.1".into(),
                port: other.addr.port(),
                fingerprint: probe.fingerprint,
                mac_addresses: vec![],
            },
            "marie",
            Secret::from(PASSWORD),
            false,
        )
        .await
        .unwrap();
    let first_key = key_of(&world.vault, &world.id).unwrap();
    let second_key = key_of(&world.vault, &second).expect("la première connexion crée la clé");
    assert_ne!(first_key, second_key, "deux serveurs, deux clés");
    world.manager.remove_server(&second).await.unwrap();
    assert_eq!(
        key_of(&world.vault, &second),
        None,
        "la clé part avec le serveur"
    );
    assert_eq!(key_of(&world.vault, &world.id), Some(first_key));
}

#[tokio::test]
async fn restarting_the_client_keeps_the_key_and_the_enrolled_pc() {
    let world = World::connected(keyed()).await;
    let key = key_of(&world.vault, &world.id).unwrap();
    world.manager.shutdown().await;
    // Mise à jour ou réinstallation en gardant les données : mêmes carnet et coffre.
    let manager = support::start_manager(
        world.dir.path(),
        world.vault.clone(),
        Arc::new(ScriptedNet::new()),
        Arc::new(JumpClock::new()),
        fast_config(),
    )
    .await;
    let recorder = Recorder::spawn(manager.subscribe());
    recorder.wait_state(0, LinkState::Connected, WAIT).await;
    assert_eq!(key_of(&world.vault, &world.id), Some(key), "même clé");
    let list = manager.devices_list(&world.id).await.unwrap();
    assert_eq!(list.devices.len(), 1, "aucun nouveau poste");
    assert!(list.devices[0].current);
}

// ── Agent ancien, coffre en panne, défi coupé ou refusé ───────────────────────────────────

#[tokio::test]
async fn an_old_agent_is_used_as_before_without_error_and_without_a_loop() {
    let world = World::connected(Options {
        remember: true,
        ..Options::default()
    })
    .await;
    assert_eq!(
        key_of(&world.vault, &world.id),
        None,
        "pas de défi, pas de clé"
    );
    for _ in 0..3 {
        reconnect(&world).await;
    }
    // Un défi à la connexion, une sonde d'inscription : deux demandes, jamais une par reconnexion.
    assert_eq!(
        world.spy.calls(),
        2,
        "{:?}",
        world.spy.purposes.lock().unwrap()
    );
    assert_eq!(world.state().state, LinkState::Connected);
}

#[tokio::test]
async fn a_client_updated_with_a_remembered_password_enrolls_silently_once() {
    // Un client d'avant la clé : session ouverte, mot de passe mémorisé, aucune clé.
    let world = World::connected(Options {
        remember: true,
        ..Options::default()
    })
    .await;
    assert_eq!(key_of(&world.vault, &world.id), None);
    let sessions = world.agent.sessions_open("marie").await;
    // Le client est mis à jour (nouvelle exécution, mêmes données) : l'agent connaît le défi.
    world.manager.shutdown().await;
    let manager = support::start_manager(
        world.dir.path(),
        world.vault.clone(),
        Arc::new(ScriptedNet::new()),
        Arc::new(JumpClock::new()),
        fast_config(),
    )
    .await;
    let recorder = Recorder::spawn(manager.subscribe());
    recorder.wait_state(0, LinkState::Connected, WAIT).await;
    assert!(
        key_of(&world.vault, &world.id).is_some(),
        "la clé est créée"
    );
    let list = manager.devices_list(&world.id).await.unwrap();
    assert_eq!(list.devices.len(), 1);
    assert!(list.devices[0].current, "ce poste est inscrit");
    // L'ancienne session est fermée chez l'agent : une seule session ouverte, pas deux.
    assert_eq!(world.agent.sessions_open("marie").await, sessions);
    // Pas une deuxième fois, même après une coupure.
    let mark = recorder.mark();
    world.proxy.cut();
    recorder
        .wait_for(
            mark,
            "coupure",
            WAIT,
            |e| matches!(e, Event::State { info, .. } if info.state != LinkState::Connected),
        )
        .await;
    let mark = recorder.mark();
    world.proxy.heal();
    recorder.wait_state(mark, LinkState::Connected, WAIT).await;
    assert_eq!(world.agent.sessions_open("marie").await, sessions);
    assert_eq!(
        manager.devices_list(&world.id).await.unwrap().devices.len(),
        1
    );
}

#[tokio::test]
async fn without_a_remembered_password_an_updated_client_does_not_reconnect_by_itself() {
    let world = World::connected(Options::default()).await;
    let sessions = world.agent.sessions_open("marie").await;
    world.spy.set(ChallengeMode::Real);
    reconnect(&world).await;
    assert_eq!(key_of(&world.vault, &world.id), None);
    assert_eq!(
        world.agent.sessions_open("marie").await,
        sessions,
        "aucune nouvelle session"
    );
    let list = world.manager.devices_list(&world.id).await.unwrap();
    assert!(
        list.devices.iter().all(|d| !d.current),
        "ce poste n'est pas encore enregistré"
    );
}

/// Un coffre qui refuse la clé d'appareil (lecture et écriture) mais garde le reste.
struct KeyBlindVault(MemoryVault);

impl Vault for KeyBlindVault {
    fn get(&self, server: &ServerId, kind: SecretKind) -> Result<Option<Secret>, VaultError> {
        if kind == SecretKind::DeviceKey {
            return Err(VaultError("coffre en panne".into()));
        }
        self.0.get(server, kind)
    }

    fn put(&self, server: &ServerId, kind: SecretKind, secret: &Secret) -> Result<(), VaultError> {
        if kind == SecretKind::DeviceKey {
            return Err(VaultError("coffre en panne".into()));
        }
        self.0.put(server, kind, secret)
    }

    fn delete(&self, server: &ServerId, kind: SecretKind) -> Result<(), VaultError> {
        self.0.delete(server, kind)
    }
}

#[tokio::test]
async fn a_vault_that_fails_for_the_key_never_stops_the_connection() {
    let agent = TestAgent::install().await;
    agent
        .create_account("marie", hearth_agent::domain::accounts::Role::Admin)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let manager = LinkManager::start(
        Ports {
            transport: Arc::new(support::transport()),
            vault: Arc::new(KeyBlindVault(MemoryVault::new())),
            servers: Arc::new(hearth_link::adapters::FileServerStore::new(
                dir.path().join("servers.json"),
            )),
            snapshots: Arc::new(hearth_link::adapters::FileSnapshotStore::new(
                dir.path().join("snapshots"),
            )),
            operations: Arc::new(hearth_link::adapters::FileOperationStore::new(
                dir.path().join("operations"),
            )),
            clock: Arc::new(JumpClock::new()),
            rng: Arc::new(hearth_link::adapters::OsRng::default()),
            net: Arc::new(ScriptedNet::new()),
            extra_sink: None,
        },
        fast_config(),
    )
    .await
    .unwrap();
    let recorder = Recorder::spawn(manager.subscribe());
    let probe = manager.probe("127.0.0.1", agent.addr.port()).await.unwrap();
    let mark = recorder.mark();
    manager
        .add_and_login(
            NewServer {
                name: "Forge".into(),
                color: "#7aa2f7".into(),
                host: "127.0.0.1".into(),
                port: agent.addr.port(),
                fingerprint: probe.fingerprint,
                mac_addresses: vec![],
            },
            "marie",
            Secret::from(PASSWORD),
            false,
        )
        .await
        .unwrap();
    recorder.wait_state(mark, LinkState::Connected, WAIT).await;
}

#[tokio::test]
async fn a_challenge_cut_at_login_connects_without_a_key() {
    let world = World::connected(keyed()).await;
    let other = second_pc_without_login(&world).await;
    world.spy.set(ChallengeMode::Unreachable);
    other
        .manager
        .login(&other.id, "marie", Secret::from(PASSWORD), false)
        .await
        .unwrap();
    assert_eq!(key_of(&other.vault, &other.id), None, "rien n'est inventé");
}

#[tokio::test]
async fn a_challenge_the_agent_never_issued_connects_and_keeps_no_key() {
    let world = World::connected(keyed()).await;
    let other = second_pc_without_login(&world).await;
    world.spy.set(ChallengeMode::Bogus);
    other
        .manager
        .login(&other.id, "marie", Secret::from(PASSWORD), false)
        .await
        .unwrap();
    // L'agent a ignoré la preuve : la clé créée pour elle n'est pas gardée.
    assert_eq!(key_of(&other.vault, &other.id), None);
    world.spy.set(ChallengeMode::Real);
    other
        .manager
        .login(&other.id, "marie", Secret::from(PASSWORD), false)
        .await
        .unwrap();
    assert!(
        key_of(&other.vault, &other.id).is_some(),
        "réparé à la connexion suivante"
    );
}

#[tokio::test]
async fn bad_challenges_at_each_reconnection_keep_the_link_connected_and_announce_nothing_wrong() {
    for mode in [
        ChallengeMode::Bogus,
        ChallengeMode::Replay,
        ChallengeMode::OldAgent,
    ] {
        let world = World::connected(keyed()).await;
        world.spy.set(mode);
        let mark = world.recorder.mark();
        reconnect(&world).await;
        let states = world.recorder.states_since(mark);
        assert!(
            states.iter().all(|s| matches!(
                s,
                LinkState::Reconnecting | LinkState::Connected | LinkState::Offline
            )),
            "{mode:?} : {states:?}"
        );
        assert_eq!(world.state().state, LinkState::Connected, "{mode:?}");
        // La clé n'est pas perdue : un défi refusé n'efface rien.
        assert!(key_of(&world.vault, &world.id).is_some(), "{mode:?}");
    }
}

struct Unlogged {
    manager: LinkManager,
    id: ServerId,
    vault: Arc<MemoryVault>,
    /// Vrai : la requête de connexion ne répond jamais (le délai de l'appel la coupe).
    hang: Arc<std::sync::atomic::AtomicBool>,
    _dir: tempfile::TempDir,
}

/// Un deuxième PC dont le serveur est au carnet mais où personne ne s'est encore connecté.
async fn second_pc_without_login(world: &World) -> Unlogged {
    second_pc_without_login_with(world, fast_config()).await
}

async fn second_pc_without_login_with(world: &World, config: hearth_link::LinkConfig) -> Unlogged {
    let hang = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(MemoryVault::new());
    let spy = Arc::new(support::Spy::new(support::transport(), ChallengeMode::Real));
    // Le défi de ce PC suit le mode du test (le `Spy` du monde est partagé par `state`).
    let manager = support::start_manager_shared(
        dir.path(),
        vault.clone(),
        Arc::new(ScriptedNet::new()),
        Arc::new(JumpClock::new()),
        config,
        Arc::new(WorldSpy {
            inner: support::SharedSpy(spy),
            mode: world.spy.clone(),
            hang: hang.clone(),
        }),
    )
    .await;
    let probe = manager
        .probe("127.0.0.1", world.proxy.port())
        .await
        .unwrap();
    let id = manager
        .add_server(NewServer {
            name: "Forge".into(),
            color: "#7aa2f7".into(),
            host: "127.0.0.1".into(),
            port: world.proxy.port(),
            fingerprint: probe.fingerprint,
            mac_addresses: probe.hello.mac_addresses.clone(),
        })
        .await
        .unwrap();
    Unlogged {
        manager,
        id,
        vault,
        hang,
        _dir: dir,
    }
}

/// Transport d'un deuxième PC dont le défi suit le mode du monde.
struct WorldSpy {
    inner: support::SharedSpy,
    mode: Arc<support::SpyState>,
    hang: Arc<std::sync::atomic::AtomicBool>,
}

#[async_trait::async_trait]
impl hearth_link::ports::Transport for WorldSpy {
    async fn hello(
        &self,
        target: &hearth_link::ports::transport::Target,
    ) -> Result<hearth_link::ports::transport::Probed, hearth_link::ports::transport::TransportError>
    {
        self.inner.hello(target).await
    }

    async fn login(
        &self,
        target: &hearth_link::ports::transport::Target,
        request: &hearth_proto::api::sessions::LoginRequest,
    ) -> Result<
        hearth_proto::api::sessions::LoginResponse,
        hearth_link::ports::transport::TransportError,
    > {
        self.inner.login(target, request).await
    }

    async fn challenge(
        &self,
        target: &hearth_link::ports::transport::Target,
        request: &hearth_proto::api::sessions::ChallengeRequest,
    ) -> Result<
        hearth_proto::api::sessions::ChallengeResponse,
        hearth_link::ports::transport::TransportError,
    > {
        let mode = *self.mode.mode.lock().unwrap();
        self.inner.0.state.set(mode);
        self.inner.challenge(target, request).await
    }

    async fn login_with_device(
        &self,
        target: &hearth_link::ports::transport::Target,
        request: &hearth_proto::api::sessions::DeviceLoginRequest,
    ) -> Result<
        hearth_proto::api::sessions::DeviceLoginResponse,
        hearth_link::ports::transport::TransportError,
    > {
        if self.hang.load(std::sync::atomic::Ordering::SeqCst) {
            std::future::pending::<()>().await;
        }
        self.inner.login_with_device(target, request).await
    }

    async fn logout(
        &self,
        target: &hearth_link::ports::transport::Target,
        token: &Secret,
    ) -> Result<(), hearth_link::ports::transport::TransportError> {
        self.inner.logout(target, token).await
    }

    async fn request(
        &self,
        target: &hearth_link::ports::transport::Target,
        token: &Secret,
        request: &hearth_link::ports::transport::ApiRequest,
    ) -> Result<
        hearth_link::ports::transport::ApiResponse,
        hearth_link::ports::transport::TransportError,
    > {
        self.inner.request(target, token, request).await
    }

    async fn operation(
        &self,
        target: &hearth_link::ports::transport::Target,
        token: &Secret,
        id: &hearth_link::domain::pending_ops::OperationId,
    ) -> Result<
        hearth_proto::api::operations::OperationResponse,
        hearth_link::ports::transport::TransportError,
    > {
        self.inner.operation(target, token, id).await
    }

    async fn open_stream(
        &self,
        target: &hearth_link::ports::transport::Target,
    ) -> Result<
        Box<dyn hearth_link::ports::StreamConn>,
        hearth_link::ports::transport::TransportError,
    > {
        self.inner.open_stream(target).await
    }
}

// ── Retrait d'un poste : mot de passe ET preuve de clé ─────────────────────────────────────

fn status_and_code(outcome: &ActionOutcome) -> (u16, String) {
    let ActionOutcome::Completed { status, body, .. } = outcome else {
        panic!("réponse attendue, reçu {outcome:?}");
    };
    let code = body["error"]["code"].as_str().unwrap_or("").to_owned();
    (*status, code)
}

#[tokio::test]
async fn removing_a_trusted_device_needs_the_password_and_the_proof_of_this_pcs_key() {
    let world = World::connected(keyed()).await;
    let other = second_pc(&world).await;
    let list = world.manager.devices_list(&world.id).await.unwrap();
    assert_eq!(list.devices.len(), 2);
    let target = list.devices.iter().find(|d| !d.current).unwrap().id.clone();
    let current = list.devices.iter().find(|d| d.current).unwrap().id.clone();

    // Mot de passe faux : refusé par l'agent, rien n'est retiré.
    let wrong = world
        .manager
        .remove_trusted_device(&world.id, &target, &Secret::from("Faux-Mot-De-Passe-1"))
        .await
        .unwrap();
    assert_eq!(status_and_code(&wrong), (422, "WRONG_PASSWORD".to_owned()));
    // Le poste courant ne se retire pas depuis lui-même.
    let itself = world
        .manager
        .remove_trusted_device(&world.id, &current, &Secret::from(PASSWORD))
        .await
        .unwrap();
    assert_eq!(status_and_code(&itself).0, 422);
    assert_eq!(
        world
            .manager
            .devices_list(&world.id)
            .await
            .unwrap()
            .devices
            .len(),
        2
    );
    // Mot de passe vide ou identifiant qui ressemble à un chemin : refusé avant tout envoi.
    let before = world.spy.calls_for(ChallengePurpose::DeviceRemoval);
    assert!(matches!(
        world
            .manager
            .remove_trusted_device(&world.id, &target, &Secret::from(""))
            .await,
        Err(LinkError::InvalidInput(_))
    ));
    assert!(matches!(
        world
            .manager
            .remove_trusted_device(&world.id, "../accounts", &Secret::from(PASSWORD))
            .await,
        Err(LinkError::InvalidInput(_))
    ));
    assert_eq!(world.spy.calls_for(ChallengePurpose::DeviceRemoval), before);

    // Bon mot de passe et preuve : retiré, et ses sessions sont fermées.
    let mark = other.recorder.mark();
    let done = world
        .manager
        .remove_trusted_device(&world.id, &target, &Secret::from(PASSWORD))
        .await
        .unwrap();
    assert_eq!(status_and_code(&done).0, 204);
    let list = world.manager.devices_list(&world.id).await.unwrap();
    assert_eq!(list.devices.len(), 1);
    assert!(list.devices[0].current);
    other
        .recorder
        .wait_state(mark, LinkState::AccessRevoked, WAIT)
        .await;
    // Une preuve par essai (faux mot de passe, poste courant, succès) : jamais deux fois la même.
    assert_eq!(world.spy.calls_for(ChallengePurpose::DeviceRemoval), 3);
    assert_eq!(other.manager.servers().len(), 1);
    let _ = (&other.vault, &other.id);
}

#[tokio::test]
async fn removing_a_device_without_a_key_sends_nothing() {
    let world = World::connected(Options {
        device_key: false,
        ..Options::default()
    })
    .await;
    let error = world
        .manager
        .remove_trusted_device(
            &world.id,
            "01J9ZY0G3Q8M2K6W4T7V5N1B9D",
            &Secret::from(PASSWORD),
        )
        .await
        .unwrap_err();
    assert_eq!(error, LinkError::NoDeviceKey);
    assert_eq!(world.spy.calls_for(ChallengePurpose::DeviceRemoval), 0);
}

#[tokio::test]
async fn removing_a_device_is_refused_without_sending_anything_when_the_link_is_down() {
    let world = World::connected(keyed()).await;
    let mark = world.recorder.mark();
    world.proxy.cut();
    world
        .recorder
        .wait_for(
            mark,
            "coupure",
            WAIT,
            |e| matches!(e, Event::State { info, .. } if info.state != LinkState::Connected),
        )
        .await;
    let error = world
        .manager
        .remove_trusted_device(
            &world.id,
            "01J9ZY0G3Q8M2K6W4T7V5N1B9D",
            &Secret::from(PASSWORD),
        )
        .await
        .unwrap_err();
    assert_eq!(error, LinkError::NotConnected);
    assert_eq!(world.spy.calls_for(ChallengePurpose::DeviceRemoval), 0);
}

#[tokio::test]
async fn a_link_cut_during_a_removal_is_unknown_and_never_replayed() {
    let world = World::connected(keyed()).await;
    let other = second_pc(&world).await;
    let target = world
        .manager
        .devices_list(&world.id)
        .await
        .unwrap()
        .devices
        .into_iter()
        .find(|d| !d.current)
        .unwrap()
        .id;
    // L'agent retient la vérification du mot de passe : le retrait est « en cours ».
    world.agent.hold_actions();
    let started = world.agent.verifications_started();
    let manager = world.manager.clone();
    let id = world.id.clone();
    let device = target.clone();
    let mut sent = tokio::spawn(async move {
        manager
            .remove_trusted_device(&id, &device, &Secret::from(PASSWORD))
            .await
    });
    support::wait_started_or_returned(&world.agent, started, &mut sent).await;
    let mark = world.recorder.mark();
    world.proxy.cut();
    let outcome = tokio::time::timeout(WAIT, sent)
        .await
        .expect("l'appel ne reste pas suspendu")
        .unwrap()
        .unwrap();
    let ActionOutcome::ResultUnknown { id: operation } = outcome else {
        panic!("résultat inconnu attendu, reçu {outcome:?}");
    };
    world.agent.release_actions();
    world
        .agent
        .wait_operation_settled("marie", operation.as_str())
        .await;
    world.proxy.heal();
    world
        .recorder
        .wait_for(mark, "issue de l'opération", WAIT, |e| {
            matches!(e, Event::Operation { .. })
        })
        .await;
    // Une seule preuve, une seule exécution : jamais rejoué.
    assert_eq!(world.spy.calls_for(ChallengePurpose::DeviceRemoval), 1);
    let list = world.manager.devices_list(&world.id).await.unwrap();
    assert_eq!(list.devices.len(), 1, "retiré une seule fois par l'agent");
    let _ = other;
}

// ── La clé privée n'apparaît nulle part ─────────────────────────────────────────────────

/// Capture tout ce que `tracing` reçoit (message et champs), pour y chercher un secret.
#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<String>>>);

struct Line(String);

impl Visit for Line {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.push_str(&format!("{}={value:?} ", field.name()));
    }
}

impl Subscriber for Capture {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }

    fn record(&self, _: &Id, _: &Record<'_>) {}

    fn record_follows_from(&self, _: &Id, _: &Id) {}

    fn event(&self, event: &TracingEvent<'_>) {
        let mut line = Line(format!("{} ", event.metadata().target()));
        event.record(&mut line);
        self.0.lock().unwrap().push(line.0);
    }

    fn enter(&self, _: &Id) {}

    fn exit(&self, _: &Id) {}
}

#[tokio::test]
async fn the_private_key_appears_in_no_event_no_answer_no_error_no_log_and_no_debug() {
    let capture = Capture::default();
    let _guard = tracing::subscriber::set_default(capture.clone());
    let world = World::connected(Options {
        device_key: true,
        remember: true,
        ..Options::default()
    })
    .await;
    let other = second_pc(&world).await;
    let secret = key_of(&world.vault, &world.id).unwrap();
    let mut seen = String::new();
    let list = world.manager.devices_list(&world.id).await.unwrap();
    seen.push_str(&format!("{list:?}"));
    let target = list.devices.iter().find(|d| !d.current).unwrap().id.clone();
    for password in ["Faux-Mot-De-Passe-1", PASSWORD] {
        let result = world
            .manager
            .remove_trusted_device(&world.id, &target, &Secret::from(password))
            .await;
        seen.push_str(&format!("{result:?}"));
    }
    seen.push_str(&format!(
        "{:?}",
        world
            .manager
            .remove_trusted_device(&world.id, "bad/id", &Secret::from(PASSWORD))
            .await
    ));
    reconnect(&world).await;
    for (_, event) in world.recorder.since(0) {
        seen.push_str(&format!("{event:?}"));
    }
    for (_, event) in other.recorder.since(0) {
        seen.push_str(&format!("{event:?}"));
    }
    seen.push_str(&format!("{:?}", world.manager.servers()));
    seen.push_str(&format!("{:?}", world.manager.states()));
    seen.push_str(&capture.0.lock().unwrap().join("\n"));
    // Le coffre (le test peut le lire) : c'est lui seul qui garde la clé.
    assert!(!secret.is_empty());
    // La clé sous TOUTES ses formes : le document PKCS#8 (base64, hexadécimal), sa graine de 32 octets
    // (base64 seule, base64 URL, hexadécimal, liste d'octets avec ou sans espaces), le mot de passe.
    use base64::Engine as _;
    let der = base64::engine::general_purpose::STANDARD
        .decode(&secret)
        .unwrap();
    assert!(
        der.len() > 48,
        "PKCS#8 de ring : 16 octets d'en-tête puis la graine de 32 octets"
    );
    let seed = &der[16..48];
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let list = format!("{seed:?}");
    let forms = [
        secret.clone(),
        secret[..secret.len().min(40)].to_owned(),
        base64::engine::general_purpose::STANDARD.encode(seed),
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(seed),
        hex(seed),
        hex(seed).to_uppercase(),
        hex(&der),
        list.clone(),
        list.replace(", ", ","),
        PASSWORD.to_owned(),
    ];
    for needle in &forms {
        assert!(needle.len() >= 12);
        assert!(
            !seen.contains(needle.as_str()),
            "un secret est visible : {}",
            &needle[..8]
        );
    }
    let _ = Secret::from("x");
}

#[tokio::test]
async fn with_a_key_an_unavailable_challenge_is_a_transient_failure_never_a_proofless_connection() {
    for mode in [ChallengeMode::Unreachable, ChallengeMode::Garbage] {
        let world = World::connected(keyed()).await;
        // À la connexion par mot de passe : rien n'est envoyé (pas de mot de passe, pas de preuve).
        world.spy.set(mode);
        let verifications = world.agent.verifications_started();
        let error = world
            .manager
            .login(&world.id, "marie", Secret::from(PASSWORD), false)
            .await
            .unwrap_err();
        assert!(
            matches!(error, LinkError::DeviceChallengeUnavailable),
            "{mode:?} : {error:?}"
        );
        assert_eq!(
            world.agent.verifications_started(),
            verifications,
            "{mode:?}"
        );
        // À l'ouverture du flux : le lien ne se rétablit pas sans preuve, il réessaie.
        let mark = world.recorder.mark();
        world.proxy.cut();
        world
            .recorder
            .wait_for(
                mark,
                "une coupure vue",
                WAIT,
                |e| matches!(e, Event::State { info, .. } if info.state != LinkState::Connected),
            )
            .await;
        world.proxy.heal();
        let before = world.spy.calls_for(ChallengePurpose::Session);
        wait_session_challenges(&world, before + 3).await;
        assert!(
            world.spy.calls_for(ChallengePurpose::Session) > before,
            "{mode:?}"
        );
        assert_ne!(world.state().state, LinkState::Connected, "{mode:?}");
        // Le défi revient : le lien se rétablit, rien n'a été annoncé à tort.
        world.spy.set(ChallengeMode::Real);
        let mark = world.recorder.mark();
        world
            .recorder
            .wait_state(mark, LinkState::Connected, WAIT)
            .await;
        let states = world.recorder.states_since(mark);
        assert!(
            states.iter().all(|s| matches!(
                s,
                LinkState::Reconnecting | LinkState::Connected | LinkState::Offline
            )),
            "{mode:?} : {states:?}"
        );
        assert!(key_of(&world.vault, &world.id).is_some(), "{mode:?}");
    }
}

/// Un coffre qui dit « absent » pour la clé d'appareil et refuse de l'écrire (stratégie Windows qui
/// interdit l'enregistrement d'identifiants), le reste fonctionne.
struct BlindWriteVault(MemoryVault);

impl Vault for BlindWriteVault {
    fn get(&self, server: &ServerId, kind: SecretKind) -> Result<Option<Secret>, VaultError> {
        if kind == SecretKind::DeviceKey {
            return Ok(None);
        }
        self.0.get(server, kind)
    }

    fn put(&self, server: &ServerId, kind: SecretKind, secret: &Secret) -> Result<(), VaultError> {
        if kind == SecretKind::DeviceKey {
            return Err(VaultError("écriture interdite".into()));
        }
        self.0.put(server, kind, secret)
    }

    fn delete(&self, server: &ServerId, kind: SecretKind) -> Result<(), VaultError> {
        self.0.delete(server, kind)
    }
}

#[tokio::test]
async fn a_vault_that_reads_absent_and_refuses_to_write_never_fills_the_eight_places() {
    let world = World::connected(keyed()).await;
    assert_eq!(
        world
            .manager
            .devices_list(&world.id)
            .await
            .unwrap()
            .devices
            .len(),
        1
    );
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(BlindWriteVault(MemoryVault::new()));
    let start = |vault: Arc<BlindWriteVault>| {
        let path = dir.path().to_owned();
        async move {
            support::start_manager_shared(
                &path,
                vault,
                Arc::new(ScriptedNet::new()),
                Arc::new(JumpClock::new()),
                fast_config(),
                Arc::new(support::transport()),
            )
            .await
        }
    };
    let manager = start(vault.clone()).await;
    let probe = manager
        .probe("127.0.0.1", world.proxy.port())
        .await
        .unwrap();
    let id = manager
        .add_server(NewServer {
            name: "Forge".into(),
            color: "#7aa2f7".into(),
            host: "127.0.0.1".into(),
            port: world.proxy.port(),
            fingerprint: probe.fingerprint,
            mac_addresses: vec![],
        })
        .await
        .unwrap();
    // Neuf connexions par mot de passe : aucune clé n'est présentée, aucun poste n'est inscrit.
    for attempt in 1..=9 {
        manager
            .login(&id, "marie", Secret::from(PASSWORD), true)
            .await
            .unwrap();
        let count = world
            .manager
            .devices_list(&world.id)
            .await
            .unwrap()
            .devices
            .len();
        assert_eq!(count, 1, "connexion {attempt}");
    }
    // Inscription silencieuse au lancement (mot de passe mémorisé) : même chose.
    manager.shutdown().await;
    let manager = start(vault.clone()).await;
    let recorder = Recorder::spawn(manager.subscribe());
    recorder.wait_state(0, LinkState::Connected, WAIT).await;
    assert_eq!(
        world
            .manager
            .devices_list(&world.id)
            .await
            .unwrap()
            .devices
            .len(),
        1
    );
    // Première connexion d'un serveur : l'entrée témoin refusée, la connexion réussit, rien d'inscrit.
    let other = TestAgent::install().await;
    other
        .create_account("marie", hearth_agent::domain::accounts::Role::Admin)
        .await;
    let probe = manager.probe("127.0.0.1", other.addr.port()).await.unwrap();
    let (first, _) = manager
        .add_and_login(
            NewServer {
                name: "Autre".into(),
                color: "#7aa2f7".into(),
                host: "127.0.0.1".into(),
                port: other.addr.port(),
                fingerprint: probe.fingerprint,
                mac_addresses: vec![],
            },
            "marie",
            Secret::from(PASSWORD),
            false,
        )
        .await
        .unwrap();
    // Le flux de ce serveur ouvert sans clé : aucun poste « courant » chez cet agent.
    let mut events = manager.subscribe();
    while manager.state(&first).unwrap().state != LinkState::Connected {
        events.recv().await.unwrap();
    }
    let list = manager.devices_list(&first).await.unwrap();
    assert!(list.devices.is_empty(), "{:?}", list.devices);
}

/// Attend (sur un fait observable : le nombre de défis demandés) que le lien ait réessayé.
async fn wait_session_challenges(world: &World, at_least: usize) {
    let deadline = std::time::Instant::now() + WAIT;
    while world.spy.calls_for(ChallengePurpose::Session) < at_least {
        assert!(
            std::time::Instant::now() < deadline,
            "le lien ne réessaie pas"
        );
        tokio::task::yield_now().await;
    }
}

// ── Entrée témoin du coffre ────────────────────────────────────────────────────────────────

/// Un coffre qui retient tout ce qui est écrit sous un identifiant provisoire (`attente-…`).
struct WitnessVault {
    inner: MemoryVault,
    witnessed: Mutex<Vec<String>>,
}

impl Vault for WitnessVault {
    fn get(&self, server: &ServerId, kind: SecretKind) -> Result<Option<Secret>, VaultError> {
        self.inner.get(server, kind)
    }

    fn put(&self, server: &ServerId, kind: SecretKind, secret: &Secret) -> Result<(), VaultError> {
        if server.as_str().starts_with("attente-") {
            self.witnessed
                .lock()
                .unwrap()
                .push(secret.expose().to_owned());
        }
        self.inner.put(server, kind, secret)
    }

    fn delete(&self, server: &ServerId, kind: SecretKind) -> Result<(), VaultError> {
        self.inner.delete(server, kind)
    }
}

#[tokio::test]
async fn the_witness_entry_of_a_first_connection_never_holds_the_real_private_key() {
    let agent = TestAgent::install().await;
    agent
        .create_account("marie", hearth_agent::domain::accounts::Role::Admin)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(WitnessVault {
        inner: MemoryVault::new(),
        witnessed: Mutex::new(Vec::new()),
    });
    let manager = support::start_manager_shared(
        dir.path(),
        vault.clone(),
        Arc::new(ScriptedNet::new()),
        Arc::new(JumpClock::new()),
        fast_config(),
        Arc::new(support::SharedSpy(Arc::new(support::Spy::new(
            support::transport(),
            ChallengeMode::Real,
        )))),
    )
    .await;
    let probe = manager.probe("127.0.0.1", agent.addr.port()).await.unwrap();
    let (id, _) = manager
        .add_and_login(
            NewServer {
                name: "Forge".into(),
                color: "#7aa2f7".into(),
                host: "127.0.0.1".into(),
                port: agent.addr.port(),
                fingerprint: probe.fingerprint,
                mac_addresses: vec![],
            },
            "marie",
            Secret::from(PASSWORD),
            false,
        )
        .await
        .unwrap();
    let real = vault
        .get(&id, SecretKind::DeviceKey)
        .unwrap()
        .expect("la clé est rangée sous le vrai identifiant")
        .expose()
        .to_owned();
    let witnessed = vault.witnessed.lock().unwrap().clone();
    assert_eq!(witnessed.len(), 1, "une seule entrée témoin");
    assert_ne!(witnessed[0], real, "la valeur du témoin n'est pas la clé");
    assert_eq!(witnessed[0].len(), real.len(), "même longueur");
    manager.shutdown().await;
}

#[tokio::test]
async fn a_call_abandoned_by_its_deadline_keeps_the_key_it_may_have_enrolled() {
    let world = World::connected(keyed()).await;
    let config = hearth_link::LinkConfig {
        request_timeout: std::time::Duration::from_millis(300),
        ..fast_config()
    };
    let pc = second_pc_without_login_with(&world, config).await;
    pc.hang.store(true, std::sync::atomic::Ordering::SeqCst);
    let error = pc
        .manager
        .login(&pc.id, "marie", Secret::from(PASSWORD), false)
        .await
        .unwrap_err();
    assert_eq!(error, LinkError::Timeout);
    // Le délai ne dit pas que l'agent n'a rien fait : la clé reste (inscrite ou reconnue à la
    // connexion suivante) ; l'effacer perdrait une place sur les huit si le poste a été inscrit.
    assert!(
        key_of(&pc.vault, &pc.id).is_some(),
        "la clé rangée pour cet appel reste au coffre"
    );
    // Ici l'agent n'a rien reçu : un seul poste, celui du monde.
    assert_eq!(
        world
            .manager
            .devices_list(&world.id)
            .await
            .unwrap()
            .devices
            .len(),
        1
    );
}
