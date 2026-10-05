//! Épinglage de l'empreinte, première prise de contact, session, carnet : l'API du
//! `LinkManager` contre un vrai agent (sans mandataire à pannes).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use hearth_agent::domain::accounts::Role;
use hearth_link::adapters::MemoryVault;
use hearth_link::domain::compat::Compatibility;
use hearth_link::domain::secret::Secret;
use hearth_link::domain::state::LinkState;
use hearth_link::ports::vault::{SecretKind, Vault as _};
use hearth_link::{LinkError, NewServer};
use hearth_proto::fingerprint::Fingerprint;
use support::{
    JumpClock, Options, PASSWORD, Recorder, ScriptedNet, TestAgent, WAIT, World, fast_config,
    start_manager,
};
use tokio::io::AsyncWriteExt as _;
use tokio::net::TcpListener;

async fn bare_manager() -> (
    hearth_link::LinkManager,
    tempfile::TempDir,
    Arc<MemoryVault>,
) {
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(MemoryVault::new());
    let manager = start_manager(
        dir.path(),
        vault.clone(),
        Arc::new(ScriptedNet::new()),
        Arc::new(JumpClock::new()),
        fast_config(),
    )
    .await;
    (manager, dir, vault)
}

fn server(port: u16, fingerprint: Fingerprint) -> NewServer {
    NewServer {
        name: "Forge".into(),
        color: "#7aa2f7".into(),
        host: "127.0.0.1".into(),
        port,
        fingerprint,
        mac_addresses: vec![],
    }
}

#[tokio::test]
async fn the_probe_reads_the_identity_and_the_fingerprint_without_authenticating() {
    let agent = TestAgent::install().await;
    let (manager, _dir, _vault) = bare_manager().await;
    let probe = manager.probe("127.0.0.1", agent.addr.port()).await.unwrap();
    assert_eq!(probe.hello.product, "hearth");
    assert_eq!(probe.compatibility, Compatibility::Compatible);
    assert!(!probe.hello.machine_name.is_empty());
    // Même agent, même empreinte à chaque contact.
    let again = manager.probe("127.0.0.1", agent.addr.port()).await.unwrap();
    assert_eq!(again.fingerprint, probe.fingerprint);
    // Un autre agent a une autre empreinte.
    let other = TestAgent::install().await;
    let probe_other = manager.probe("127.0.0.1", other.addr.port()).await.unwrap();
    assert_ne!(probe_other.fingerprint, probe.fingerprint);
    assert_eq!(agent.sessions_open("marie").await, 0);
}

#[tokio::test]
async fn the_probe_of_something_that_is_not_an_agent_fails_cleanly() {
    let (manager, _dir, _vault) = bare_manager().await;
    // Rien n'écoute.
    let closed = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = closed.local_addr().unwrap().port();
    drop(closed);
    assert!(matches!(
        manager.probe("127.0.0.1", port).await,
        Err(LinkError::Unreachable(_) | LinkError::Timeout)
    ));
    // Un serveur qui répond n'importe quoi, puis un qui ne répond jamais.
    let garbage = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let garbage_port = garbage.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = garbage.accept().await else {
                return;
            };
            let _ = socket
                .write_all(b"HTTP/1.1 200 OK\r\n\r\nceci n'est pas du TLS")
                .await;
        }
    });
    assert!(manager.probe("127.0.0.1", garbage_port).await.is_err());
    let silent = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let silent_port = silent.local_addr().unwrap().port();
    tokio::spawn(async move {
        let mut held = Vec::new();
        loop {
            let Ok((socket, _)) = silent.accept().await else {
                return;
            };
            held.push(socket);
        }
    });
    let started = std::time::Instant::now();
    assert!(manager.probe("127.0.0.1", silent_port).await.is_err());
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "la sonde ne bloque pas"
    );
}

#[tokio::test]
async fn nothing_is_sent_to_a_server_whose_fingerprint_does_not_match() {
    let agent = TestAgent::install().await;
    agent.create_account("marie", Role::Admin).await;
    let (manager, _dir, vault) = bare_manager().await;
    let id = manager
        .add_server(server(agent.addr.port(), Fingerprint::from_bytes([9; 32])))
        .await
        .unwrap();
    let result = manager
        .login(&id, "marie", Secret::from(PASSWORD), true)
        .await;
    assert_eq!(result.unwrap_err(), LinkError::FingerprintChanged);
    // Aucun identifiant n'est parti : le serveur n'a ouvert aucune session, le coffre est vide.
    assert_eq!(agent.sessions_open("marie").await, 0);
    assert!(vault.get(&id, SecretKind::Token).unwrap().is_none());
    assert!(vault.get(&id, SecretKind::Password).unwrap().is_none());
}

#[tokio::test]
async fn a_wrong_password_gets_the_generic_refusal_and_stores_nothing() {
    let agent = TestAgent::install().await;
    agent.create_account("marie", Role::Admin).await;
    let (manager, _dir, vault) = bare_manager().await;
    let probe = manager.probe("127.0.0.1", agent.addr.port()).await.unwrap();
    let id = manager
        .add_server(server(agent.addr.port(), probe.fingerprint))
        .await
        .unwrap();
    for user in ["marie", "personne"] {
        let result = manager
            .login(&id, user, Secret::from("Wrong-Password-1"), true)
            .await;
        assert_eq!(result.unwrap_err(), LinkError::InvalidCredentials, "{user}");
    }
    assert!(vault.get(&id, SecretKind::Token).unwrap().is_none());
    assert_eq!(manager.state(&id).unwrap().state, LinkState::SessionExpired);
    assert_eq!(
        manager
            .login(&id, "", Secret::from(PASSWORD), false)
            .await
            .unwrap_err(),
        LinkError::InvalidInput("identifiant ou mot de passe vide")
    );
}

#[tokio::test]
async fn servers_are_validated_listed_and_removed_with_their_secrets() {
    let agent = TestAgent::install().await;
    agent.create_account("marie", Role::Admin).await;
    let (manager, dir, vault) = bare_manager().await;
    let probe = manager.probe("127.0.0.1", agent.addr.port()).await.unwrap();
    let fingerprint = probe.fingerprint;
    for (host, port) in [("", 7341), ("a b", 7341), ("a/b", 7341), ("host", 0)] {
        let mut bad = server(port, fingerprint);
        bad.host = host.into();
        assert!(matches!(
            manager.add_server(bad).await,
            Err(LinkError::InvalidInput(_))
        ));
    }
    let mut unnamed = server(agent.addr.port(), fingerprint);
    unnamed.name = "  ".into();
    assert!(matches!(
        manager.add_server(unnamed).await,
        Err(LinkError::InvalidInput(_))
    ));
    let id = manager
        .add_server(server(agent.addr.port(), fingerprint))
        .await
        .unwrap();
    assert_eq!(
        manager
            .add_server(server(agent.addr.port(), fingerprint))
            .await
            .unwrap_err(),
        LinkError::AlreadyExists
    );
    assert_eq!(manager.servers().len(), 1);
    assert_eq!(manager.states().len(), 1);

    manager
        .login(&id, "marie", Secret::from(PASSWORD), true)
        .await
        .unwrap();
    assert!(vault.get(&id, SecretKind::Token).unwrap().is_some());
    assert!(vault.get(&id, SecretKind::Password).unwrap().is_some());
    assert!(dir.path().join("servers.json").exists());

    manager.remove_server(&id).await.unwrap();
    assert!(manager.servers().is_empty());
    assert_eq!(manager.state(&id).unwrap_err(), LinkError::UnknownServer);
    assert_eq!(
        manager.remove_server(&id).await.unwrap_err(),
        LinkError::UnknownServer
    );
    assert!(vault.get(&id, SecretKind::Token).unwrap().is_none());
    assert!(vault.get(&id, SecretKind::Password).unwrap().is_none());
    let book = std::fs::read_to_string(dir.path().join("servers.json")).unwrap();
    assert!(
        !book.contains(id.as_str()),
        "le carnet ne garde plus le serveur"
    );
}

#[tokio::test]
async fn logout_closes_the_session_and_forgets_the_secrets() {
    let world = World::connected(Options {
        remember: true,
        ..Options::default()
    })
    .await;
    let mark = world.recorder.mark();
    world.manager.logout(&world.id).await.unwrap();
    world
        .recorder
        .wait_state(mark, LinkState::SessionExpired, WAIT)
        .await;
    assert!(
        world
            .vault
            .get(&world.id, SecretKind::Token)
            .unwrap()
            .is_none()
    );
    assert!(
        world
            .vault
            .get(&world.id, SecretKind::Password)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        world.agent.sessions_open("marie").await,
        0,
        "la session est fermée côté agent"
    );
    assert!(!world.manager.servers()[0].remember);
}

#[tokio::test]
async fn the_application_resumes_a_saved_session_after_a_restart() {
    let world = World::connected(Options::default()).await;
    // Laisse le temps de sauvegarder la dernière vue, puis ferme l'application.
    tokio::time::sleep(Duration::from_millis(500)).await;
    world.manager.shutdown().await;
    drop(world.manager);

    let manager = start_manager(
        world.dir.path(),
        world.vault.clone(),
        world.net.clone(),
        world.clock.clone(),
        fast_config(),
    )
    .await;
    let recorder = Recorder::spawn(manager.subscribe());
    assert_eq!(manager.servers().len(), 1, "le carnet est rechargé");
    // La dernière vue est disponible tout de suite, avant toute connexion.
    let last = manager.last_known(&world.id).await.unwrap().unwrap();
    assert!(last.machine.is_some());
    assert!(!last.history.is_empty());
    // Une session mémorisée : on se reconnecte sans rien demander.
    let mark = recorder.mark();
    recorder.wait_state(mark, LinkState::Connected, WAIT).await;
    recorder.wait_metrics(mark, WAIT).await;
}

#[tokio::test]
async fn each_server_has_its_own_independent_link() {
    let world = World::connected(Options::default()).await;
    // Un second serveur, sur un autre agent derrière son propre mandataire.
    let second_agent = TestAgent::install().await;
    second_agent.create_account("paul", Role::ReadOnly).await;
    let second_proxy = support::FaultProxy::start(second_agent.addr).await;
    let probe = world
        .manager
        .probe("127.0.0.1", second_proxy.port())
        .await
        .unwrap();
    let second = world
        .manager
        .add_server(server(second_proxy.port(), probe.fingerprint))
        .await
        .unwrap();
    let mark = world.recorder.mark();
    world
        .manager
        .login(&second, "paul", Secret::from(PASSWORD), false)
        .await
        .unwrap();
    world
        .recorder
        .wait_for(mark, "second serveur connecté", WAIT, |event| {
            matches!(event, hearth_link::domain::event::Event::State { server, info }
                if server == &second && info.state == LinkState::Connected)
        })
        .await;

    // Le premier tombe : le second ne bouge pas (BR-RESIL-020).
    let mark = world.recorder.mark();
    world.proxy.cut();
    world
        .recorder
        .wait_state(mark, LinkState::Offline, WAIT)
        .await;
    assert_eq!(
        world.manager.state(&world.id).unwrap().state,
        LinkState::Offline
    );
    assert_eq!(
        world.manager.state(&second).unwrap().state,
        LinkState::Connected
    );
    let second_changes = world
        .recorder
        .since(mark)
        .into_iter()
        .filter(|(_, event)| {
            matches!(event, hearth_link::domain::event::Event::State { server, .. } if server == &second)
        })
        .count();
    assert_eq!(second_changes, 0, "l'état du second serveur n'a pas changé");
    assert_eq!(world.manager.states().len(), 2);
}

#[tokio::test]
async fn an_unreadable_server_book_does_not_prevent_starting() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("servers.json"), b"\xff\xfe{ pas du json").unwrap();
    let manager = start_manager(
        dir.path(),
        Arc::new(MemoryVault::new()),
        Arc::new(ScriptedNet::new()),
        Arc::new(JumpClock::new()),
        fast_config(),
    )
    .await;
    assert!(manager.servers().is_empty());
}
