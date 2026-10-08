//! Résilience du lien contre un vrai agent, à travers un mandataire TCP à pannes : couper net,
//! geler, retarder, fermer proprement, refuser les connexions.
//!
//! Les durées du produit sont divisées par 6 (`support::SCALE`) : 3 s deviennent 0,5 s, 30 s
//! deviennent 5 s, les délais de reconnexion suivent. Chaque test porte le nom de la ligne du
//! tableau des transitions de la spec (section 6) ou du cas limite qu'il couvre.
//!
//! Déterminisme (HRT-12) : aucune assertion ne dépend de la vitesse de la machine. Un scénario
//! attend un fait observable (état, événement, connexion reçue, action arrivée chez l'agent,
//! opération terminée) ; l'agent retient les actions tant que le test ne les relâche pas
//! (`hold_actions`) au lieu d'un délai fixe ; les seuils du lien qui n'ont pas à jouer dans un
//! scénario sont relevés à « jamais » (`support::thresholds`) ; `WAIT` est un délai de garde.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::time::{Duration, Instant};

use hearth_agent::domain::accounts::Role;
use hearth_link::domain::event::{Event, SessionEnd};
use hearth_link::domain::pending_ops::Outcome;
use hearth_link::domain::secret::Secret;
use hearth_link::domain::state::{Blocked, LinkState};
use hearth_link::ports::transport::Method;
use hearth_link::ports::vault::{SecretKind, Vault as _};
use hearth_link::{ActionOutcome, ActionRequest, LinkError};
use serde_json::json;
use support::{
    Options, PASSWORD, WAIT, World, never, thresholds, wait_attempts, wait_metrics_times,
};
use time::Duration as TimeDuration;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn change_password() -> ActionRequest {
    ActionRequest {
        method: Method::Put,
        path: "/me/password".into(),
        body: Some(json!({ "current": PASSWORD, "password": "New-Password-12" })),
    }
}

// ── Coupures ────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_cut_healed_before_any_threshold_shows_nothing_and_the_stream_resumes() {
    // Le seuil de « Reconnexion » est hors d'atteinte : la coupure dure ce qu'elle dure (le test
    // attend qu'une tentative de reconnexion ait vu le lien coupé), elle est toujours plus courte.
    let world = World::connected(Options::with_thresholds(thresholds(
        Some(never()),
        false,
        false,
    )))
    .await;
    let mark = world.recorder.mark();
    world.proxy.cut();
    wait_attempts(&world.proxy, 2).await;
    world.proxy.heal();
    let resumed = world.recorder.mark();
    world.recorder.wait_metrics(resumed, WAIT).await;
    assert_eq!(
        world.recorder.states_since(mark),
        [],
        "une coupure plus courte que le seuil ne change rien à l'écran"
    );
    assert_eq!(world.state().state, LinkState::Connected);
}

#[tokio::test]
async fn a_ten_second_cut_shows_reconnecting_then_connected() {
    // « Hors ligne » est hors d'atteinte : seule la coupure provoque « Reconnexion ».
    let world = World::connected(Options::with_thresholds(thresholds(
        Some(never()),
        true,
        false,
    )))
    .await;
    let mark = world.recorder.mark();
    world.proxy.cut();
    world
        .recorder
        .wait_state(mark, LinkState::Reconnecting, WAIT)
        .await;
    world.proxy.heal();
    world
        .recorder
        .wait_state(mark, LinkState::Connected, WAIT)
        .await;
    assert_eq!(
        world.recorder.states_since(mark),
        [LinkState::Reconnecting, LinkState::Connected]
    );
}

#[tokio::test]
async fn a_long_cut_goes_offline_then_comes_back() {
    let world = World::connected(Options::with_thresholds(thresholds(
        Some(never()),
        true,
        true,
    )))
    .await;
    let mark = world.recorder.mark();
    world.proxy.cut();
    world
        .recorder
        .wait_state(mark, LinkState::Offline, WAIT)
        .await;
    let info = world.state();
    assert_eq!(info.state, LinkState::Offline);
    assert!(
        info.last_contact_at.is_some(),
        "le bandeau a une heure de dernier contact"
    );
    // Hors ligne, les dernières données restent disponibles, datées (BR-RESIL-007).
    let last = world.manager.last_known(&world.id).await.unwrap().unwrap();
    assert!(!last.history.is_empty());
    assert!(last.machine.is_some());
    // Les tentatives continuent sans fin tant que le lien est coupé.
    wait_attempts(&world.proxy, 2).await;
    world.proxy.heal();
    world
        .recorder
        .wait_state(mark, LinkState::Connected, WAIT)
        .await;
    assert_eq!(
        world.recorder.states_since(mark),
        [
            LinkState::Reconnecting,
            LinkState::Offline,
            LinkState::Connected
        ]
    );
}

#[tokio::test]
async fn a_freeze_goes_through_reconnecting_then_offline_in_that_order() {
    let world = World::connected(Options::silent_link()).await;
    let mark = world.recorder.mark();
    world.proxy.freeze();
    world
        .recorder
        .wait_state(mark, LinkState::Offline, WAIT)
        .await;
    // L'ordre, pas les durées : les seuils exacts sont prouvés par les tests du domaine.
    assert_eq!(
        world.recorder.states_since(mark),
        [LinkState::Reconnecting, LinkState::Offline]
    );
    world.proxy.heal();
    world
        .recorder
        .wait_state(mark, LinkState::Connected, WAIT)
        .await;
}

#[tokio::test]
async fn a_prolonged_freeze_without_closing_is_detected_by_the_heartbeat() {
    let world = World::connected(Options::silent_link()).await;
    let mark = world.recorder.mark();
    world.proxy.freeze();
    // Ni fermeture ni erreur : seul le silence révèle la coupure.
    world
        .recorder
        .wait_state(mark, LinkState::Reconnecting, WAIT)
        .await;
    world
        .recorder
        .wait_state(mark, LinkState::Offline, WAIT)
        .await;
    tokio::time::sleep(ms(500)).await;
    assert_eq!(world.state().state, LinkState::Offline);
    world.proxy.heal();
    world
        .recorder
        .wait_state(mark, LinkState::Connected, WAIT)
        .await;
    let resumed = world.recorder.mark();
    world.recorder.wait_metrics(resumed, WAIT).await;
}

#[tokio::test]
async fn a_delayed_agent_keeps_its_stream_open_and_nothing_is_shown() {
    // Chaque morceau arrive en retard mais arrive : le flux vit (plusieurs salves de mesures
    // reçues), n'est jamais rouvert et aucun état ne change. Ce que prouve ce test : un retard ne
    // fait pas rouvrir le flux. Le seuil exact du silence (2 999 ms tenu, 3 000 ms coupé) est prouvé
    // par `domain::state::tests::silence_of_exactly_3s_cuts_the_link_and_it_is_dated_at_the_last_message`.
    let world = World::connected(Options::with_thresholds(thresholds(
        Some(never()),
        false,
        false,
    )))
    .await;
    let mark = world.recorder.mark();
    let connections = world.proxy.accepted();
    world.proxy.delay(ms(250));
    wait_metrics_times(&world.recorder, 3).await;
    assert_eq!(world.proxy.accepted(), connections, "flux jamais rouvert");
    assert_eq!(world.recorder.states_since(mark), []);
    assert_eq!(world.state().state, LinkState::Connected);
}

#[tokio::test]
async fn a_clean_close_reconnects_by_itself() {
    let world = World::connected(Options::with_thresholds(thresholds(
        Some(never()),
        false,
        false,
    )))
    .await;
    let mark = world.recorder.mark();
    let before = world.proxy.accepted();
    world.proxy.close();
    let resumed = world.recorder.mark();
    world.recorder.wait_metrics(resumed, WAIT).await;
    assert!(
        world.proxy.accepted() > before,
        "une nouvelle connexion a été ouverte"
    );
    assert_eq!(world.recorder.states_since(mark), [], "invisible");
}

#[tokio::test]
async fn refused_connections_are_retried_until_one_goes_through() {
    let world = World::connected(Options::default()).await;
    let mark = world.recorder.mark();
    world.proxy.refuse();
    world.proxy.close();
    world
        .recorder
        .wait_state(mark, LinkState::Reconnecting, WAIT)
        .await;
    // Les tentatives continuent : de nouvelles connexions arrivent (attente d'un fait).
    wait_attempts(&world.proxy, 2).await;
    world.proxy.heal();
    world
        .recorder
        .wait_state(mark, LinkState::Connected, WAIT)
        .await;
}

#[tokio::test]
async fn an_agent_restart_is_a_short_reconnecting() {
    let mut world = World::connected(Options::default()).await;
    let mark = world.recorder.mark();
    world.agent.restart().await;
    world.proxy.set_target(world.agent.addr);
    // L'agent est de retour : les mesures reprennent sur une nouvelle connexion.
    let back = world.recorder.mark();
    world.recorder.wait_metrics(back, WAIT).await;
    let states = world.recorder.states_since(mark);
    assert!(
        states.is_empty() || states == [LinkState::Reconnecting, LinkState::Connected],
        "{states:?}"
    );
}

#[tokio::test]
async fn a_read_only_account_has_the_same_link_behaviour() {
    let world = World::connected(Options {
        role: Role::ReadOnly,
        ..Options::with_thresholds(thresholds(Some(never()), true, false))
    })
    .await;
    let mark = world.recorder.mark();
    world.proxy.cut();
    world
        .recorder
        .wait_state(mark, LinkState::Reconnecting, WAIT)
        .await;
    world.proxy.heal();
    world
        .recorder
        .wait_state(mark, LinkState::Connected, WAIT)
        .await;
    assert_eq!(
        world.recorder.states_since(mark),
        [LinkState::Reconnecting, LinkState::Connected]
    );
}

// ── Déclencheurs ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn retry_now_forces_an_attempt_without_waiting() {
    let world = World::connected(Options::default()).await;
    let mark = world.recorder.mark();
    world.proxy.cut();
    world
        .recorder
        .wait_state(mark, LinkState::Offline, WAIT)
        .await;
    // Le lien est toujours coupé : seul le clic peut faire afficher « Reconnexion » (les tentatives
    // planifiées, elles, laissent « Hors ligne »). Fait observé, aucune durée.
    let clicked = world.recorder.mark();
    world.manager.retry_now(&world.id).unwrap();
    world
        .recorder
        .wait_state(clicked, LinkState::Reconnecting, WAIT)
        .await;
    world.proxy.heal();
    world
        .recorder
        .wait_state(clicked, LinkState::Connected, WAIT)
        .await;
}

#[tokio::test]
async fn a_network_change_reconnects_immediately() {
    let world = World::connected(Options::default()).await;
    let mark = world.recorder.mark();
    world.proxy.cut();
    world
        .recorder
        .wait_state(mark, LinkState::Offline, WAIT)
        .await;
    // Câble débranché, Wi-Fi : la liste des adresses locales change. Le lien est toujours coupé :
    // seul le déclencheur fait afficher « Reconnexion » (fait observé, aucune durée).
    let changed = world.recorder.mark();
    world.net.set(&["10.8.0.2"]);
    world
        .recorder
        .wait_state(changed, LinkState::Reconnecting, WAIT)
        .await;
    world.proxy.heal();
    world
        .recorder
        .wait_state(changed, LinkState::Connected, WAIT)
        .await;
}

#[tokio::test]
async fn a_wake_up_reconnects_immediately() {
    let world = World::connected(Options::default()).await;
    let mark = world.recorder.mark();
    world.proxy.cut();
    world
        .recorder
        .wait_state(mark, LinkState::Offline, WAIT)
        .await;
    // L'horloge murale saute de dix minutes sans que la monotone bouge : veille puis réveil. Le lien
    // est toujours coupé : seul le réveil fait afficher « Reconnexion » (fait observé).
    let woke = world.recorder.mark();
    world.clock.jump(Duration::from_secs(600));
    world
        .recorder
        .wait_state(woke, LinkState::Reconnecting, WAIT)
        .await;
    world.proxy.heal();
    world
        .recorder
        .wait_state(woke, LinkState::Connected, WAIT)
        .await;
}

// ── Empreinte ───────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_reinstalled_agent_is_refused_by_the_pinned_fingerprint() {
    let mut world = World::connected(Options {
        remember: true,
        ..Options::default()
    })
    .await;
    let reinstalled = support::TestAgent::install().await;
    reinstalled.create_account("marie", Role::Admin).await;
    let mark = world.recorder.mark();
    world.proxy.cut();
    world.agent.stop().await;
    world.proxy.set_target(reinstalled.addr);
    world.proxy.heal();

    let (_, event) = world
        .recorder
        .wait_for(mark, "empreinte changée", WAIT, |e| {
            matches!(e, Event::FingerprintChanged { .. })
        })
        .await;
    let Event::FingerprintChanged {
        expected,
        presented,
        ..
    } = event
    else {
        unreachable!()
    };
    assert_eq!(expected, world.fingerprint);
    assert_ne!(presented, world.fingerprint);

    world
        .recorder
        .wait_state(mark, LinkState::Offline, WAIT)
        .await;
    let info = world.state();
    assert_eq!(info.blocked, Some(Blocked::FingerprintChanged));
    assert_eq!(info.next_retry_at, None, "aucune tentative n'est planifiée");

    // Le blocage tient : ni mot de passe ni jeton ne sont partis, et plus aucune tentative.
    let attempts = world.proxy.accepted();
    tokio::time::sleep(ms(1_200)).await;
    assert_eq!(
        world.proxy.accepted(),
        attempts,
        "aucune tentative pendant le blocage"
    );
    assert_eq!(
        reinstalled.sessions_open("marie").await,
        0,
        "rien n'a été envoyé"
    );
}

#[tokio::test]
async fn accepting_the_new_fingerprint_unblocks_the_link() {
    let mut world = World::connected(Options {
        remember: true,
        ..Options::default()
    })
    .await;
    let reinstalled = support::TestAgent::install().await;
    reinstalled.create_account("marie", Role::Admin).await;
    let mark = world.recorder.mark();
    world.proxy.cut();
    world.agent.stop().await;
    world.proxy.set_target(reinstalled.addr);
    world.proxy.heal();
    let (_, event) = world
        .recorder
        .wait_for(mark, "empreinte changée", WAIT, |e| {
            matches!(e, Event::FingerprintChanged { .. })
        })
        .await;
    let Event::FingerprintChanged { presented, .. } = event else {
        unreachable!()
    };

    world
        .manager
        .accept_fingerprint(&world.id, presented)
        .await
        .unwrap();
    world
        .recorder
        .wait_state(mark, LinkState::Connected, WAIT)
        .await;
    // Le jeton de l'ancien agent n'existe pas ici : le mot de passe mémorisé rouvre la session.
    assert_eq!(reinstalled.sessions_open("marie").await, 1);
    assert_eq!(world.manager.servers()[0].fingerprint, presented);
}

// ── Sessions ────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_session_revoked_during_the_stream_shows_access_revoked_and_stops() {
    let world = World::connected(Options {
        remember: true,
        ..Options::default()
    })
    .await;
    let mark = world.recorder.mark();
    world.agent.revoke_sessions("marie").await;
    world
        .recorder
        .wait_state(mark, LinkState::AccessRevoked, WAIT)
        .await;
    world
        .recorder
        .wait_for(mark, "fin de session", WAIT, |e| {
            matches!(
                e,
                Event::SessionEnded {
                    kind: SessionEnd::Revoked,
                    ..
                }
            )
        })
        .await;
    // Pas de reconnexion avec les anciens identifiants : plus de jeton, plus de mot de passe.
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
    let attempts = world.proxy.accepted();
    tokio::time::sleep(ms(1_200)).await;
    assert_eq!(world.proxy.accepted(), attempts);
    assert_eq!(world.state().state, LinkState::AccessRevoked);
    assert_eq!(world.agent.sessions_open("marie").await, 0);
    assert!(!world.manager.servers()[0].remember);
}

#[tokio::test]
async fn an_expired_session_with_a_saved_password_reconnects_silently() {
    let world = World::connected(Options {
        remember: true,
        ..Options::with_thresholds(thresholds(Some(never()), false, false))
    })
    .await;
    let old_token = world
        .vault
        .get(&world.id, SecretKind::Token)
        .unwrap()
        .unwrap();
    let mark = world.recorder.mark();
    world.agent.clock.advance(TimeDuration::days(31));
    // L'agent dit « expirée » sur le flux ; la bibliothèque rouvre une session sans rien afficher.
    let deadline = Instant::now() + WAIT;
    loop {
        let token = world
            .vault
            .get(&world.id, SecretKind::Token)
            .unwrap()
            .unwrap();
        if token != old_token {
            break;
        }
        assert!(Instant::now() < deadline, "le jeton n'a pas été renouvelé");
        tokio::time::sleep(ms(20)).await;
    }
    let resumed = world.recorder.mark();
    world.recorder.wait_metrics(resumed, WAIT).await;
    assert_eq!(world.recorder.states_since(mark), []);
    assert_eq!(world.state().state, LinkState::Connected);
    assert_eq!(world.agent.sessions_open("marie").await, 1);
}

#[tokio::test]
async fn an_expired_session_without_a_saved_password_asks_for_it_then_login_recovers() {
    let world = World::connected(Options::default()).await;
    let mark = world.recorder.mark();
    world.agent.clock.advance(TimeDuration::days(31));
    world
        .recorder
        .wait_state(mark, LinkState::SessionExpired, WAIT)
        .await;
    world
        .recorder
        .wait_for(mark, "fin de session", WAIT, |e| {
            matches!(
                e,
                Event::SessionEnded {
                    kind: SessionEnd::Expired,
                    ..
                }
            )
        })
        .await;
    // Pas de boucle de reconnexion.
    let attempts = world.proxy.accepted();
    tokio::time::sleep(ms(1_000)).await;
    assert_eq!(world.proxy.accepted(), attempts);

    // Mot de passe refusé : le panneau reste ouvert.
    let refused = world
        .manager
        .login(&world.id, "marie", Secret::from("Wrong-Password-1"), false)
        .await;
    assert_eq!(refused.unwrap_err(), LinkError::InvalidCredentials);
    assert_eq!(world.state().state, LinkState::SessionExpired);

    // Mot de passe accepté : « Connecté ».
    let again = world.recorder.mark();
    world
        .manager
        .login(&world.id, "marie", Secret::from(PASSWORD), false)
        .await
        .unwrap();
    world
        .recorder
        .wait_state(again, LinkState::Connected, WAIT)
        .await;
}

// ── Actions coupées avant la réponse ────────────────────────────────────────────────────────

#[tokio::test]
async fn an_action_cut_before_the_answer_is_unknown_and_never_replayed() {
    // Issue 1 : l'agent a exécuté pendant la coupure.
    let world = World::connected(Options::default().accepting_bare_acts()).await;
    // L'agent retient l'action : « en cours » tant que le test ne la relâche pas.
    world.agent.hold_actions();
    let started = world.agent.verifications_started();
    let manager = world.manager.clone();
    let id = world.id.clone();
    let mut sent = tokio::spawn(async move { manager.execute_raw(&id, change_password()).await });
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
    // L'agent finit l'action pendant que le lien est coupé, puis le lien revient.
    world.agent.release_actions();
    world
        .agent
        .wait_operation_settled("marie", operation.as_str())
        .await;
    world.proxy.heal();
    let (_, event) = world
        .recorder
        .wait_for(mark, "issue de l'opération", WAIT, |e| {
            matches!(e, Event::Operation { .. })
        })
        .await;
    let Event::Operation {
        id: got, outcome, ..
    } = event
    else {
        unreachable!()
    };
    assert_eq!(got, operation);
    assert!(
        matches!(outcome, Outcome::DoneDuringOutage { .. }),
        "« fait pendant la coupure », reçu {outcome:?}"
    );
}

#[tokio::test]
async fn an_action_that_never_reached_the_agent_is_announced_as_not_executed() {
    let world = World::connected(Options::silent_link().accepting_bare_acts()).await;
    // Trou noir : la requête part dans le vide, l'agent ne la reçoit jamais.
    world.proxy.freeze();
    let outcome = world
        .manager
        .execute_raw(&world.id, change_password())
        .await
        .unwrap();
    let ActionOutcome::ResultUnknown { id: operation } = outcome else {
        panic!("résultat inconnu attendu, reçu {outcome:?}");
    };
    let mark = world.recorder.mark();
    world.proxy.heal();
    let (_, event) = world
        .recorder
        .wait_for(mark, "issue de l'opération", WAIT, |e| {
            matches!(e, Event::Operation { .. })
        })
        .await;
    let Event::Operation {
        id: got, outcome, ..
    } = event
    else {
        unreachable!()
    };
    assert_eq!(got, operation);
    assert_eq!(
        outcome,
        Outcome::NotExecuted,
        "« non exécuté, tu peux relancer »"
    );
}

#[tokio::test]
async fn an_action_interrupted_by_the_agent_stopping_stays_unknown() {
    let world = World::connected(Options::default().accepting_bare_acts()).await;
    // Retenue pour de bon : l'agent « s'arrête » en pleine exécution, l'action ne finit jamais.
    world.agent.hold_actions();
    let started = world.agent.verifications_started();
    let manager = world.manager.clone();
    let id = world.id.clone();
    let mut sent = tokio::spawn(async move { manager.execute_raw(&id, change_password()).await });
    support::wait_started_or_returned(&world.agent, started, &mut sent).await;
    let mark = world.recorder.mark();
    world.proxy.cut();
    let ActionOutcome::ResultUnknown { id: operation } = sent.await.unwrap().unwrap() else {
        panic!("résultat inconnu attendu");
    };
    // L'agent s'arrête en pleine exécution : au démarrage suivant, l'opération est « interrompue ».
    assert_eq!(world.agent.interrupt_running().await, 1);
    world.proxy.heal();
    let (_, event) = world
        .recorder
        .wait_for(mark, "issue de l'opération", WAIT, |e| {
            matches!(e, Event::Operation { .. })
        })
        .await;
    let Event::Operation {
        id: got, outcome, ..
    } = event
    else {
        unreachable!()
    };
    assert_eq!(got, operation);
    assert_eq!(
        outcome,
        Outcome::StillUnknown,
        "« résultat inconnu, vérifie l'état »"
    );
}

#[tokio::test]
async fn an_action_is_refused_without_sending_anything_when_the_link_is_not_connected() {
    let world = World::connected(Options::default().accepting_bare_acts()).await;
    let mark = world.recorder.mark();
    world.proxy.cut();
    world
        .recorder
        .wait_state(mark, LinkState::Reconnecting, WAIT)
        .await;
    let result = world
        .manager
        .execute_raw(&world.id, change_password())
        .await;
    assert_eq!(result.unwrap_err(), LinkError::NotConnected);
}

#[tokio::test]
async fn a_completed_action_returns_the_agent_answer_even_when_it_is_a_refusal() {
    let world = World::connected(Options::default().accepting_bare_acts()).await;
    let wrong = ActionRequest {
        method: Method::Put,
        path: "/me/password".into(),
        body: Some(json!({ "current": "Not-The-Password-1", "password": "New-Password-12" })),
    };
    match world.manager.execute_raw(&world.id, wrong).await.unwrap() {
        ActionOutcome::Completed { status, body, .. } => {
            assert_eq!(status, 422);
            assert_eq!(body["error"]["code"], "WRONG_PASSWORD");
        }
        other => panic!("réponse attendue, reçu {other:?}"),
    }
    match world
        .manager
        .execute_raw(&world.id, change_password())
        .await
        .unwrap()
    {
        ActionOutcome::Completed { status, .. } => assert_eq!(status, 200),
        other => panic!("réponse attendue, reçu {other:?}"),
    }
}

#[tokio::test]
async fn a_network_change_does_not_cut_a_healthy_stream() {
    let world = World::connected(Options::with_thresholds(thresholds(
        Some(never()),
        false,
        false,
    )))
    .await;
    let mark = world.recorder.mark();
    let connections = world.proxy.accepted();
    // Docker, WSL, Tailscale : la liste d'adresses change, le réseau utile non.
    world.net.set(&["172.17.0.1", "192.168.1.20"]);
    world.net.wait_seen(WAIT).await;
    wait_metrics_times(&world.recorder, 2).await;
    world.net.set(&["192.168.1.20"]);
    world.net.wait_seen(WAIT).await;
    wait_metrics_times(&world.recorder, 2).await;
    assert_eq!(world.recorder.states_since(mark), []);
    assert_eq!(
        world.proxy.accepted(),
        connections,
        "le flux sain n'a pas été rouvert"
    );
    let resumed = world.recorder.mark();
    world.recorder.wait_metrics(resumed, WAIT).await;
}

#[tokio::test]
async fn an_action_in_flight_is_not_made_unknown_by_a_network_change() {
    let world = World::connected(Options::default().accepting_bare_acts()).await;
    world.agent.hold_actions();
    let started = world.agent.verifications_started();
    let manager = world.manager.clone();
    let id = world.id.clone();
    let mut sent = tokio::spawn(async move { manager.execute_raw(&id, change_password()).await });
    support::wait_started_or_returned(&world.agent, started, &mut sent).await;
    world.net.set(&["10.8.0.2"]);
    // Le veilleur a LU la nouvelle liste (fait), et le flux a continué après (ordre des commandes de
    // la tâche) pendant que l'action est retenue côté agent ; puis l'agent la relâche.
    world.net.wait_seen(WAIT).await;
    wait_metrics_times(&world.recorder, 2).await;
    world.agent.release_actions();
    let outcome = tokio::time::timeout(WAIT, sent)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        matches!(outcome, ActionOutcome::Completed { status: 200, .. }),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn an_abandoned_action_stays_tracked_and_its_outcome_is_announced() {
    let world = World::connected(Options::default().accepting_bare_acts()).await;
    world.agent.hold_actions();
    let started = world.agent.verifications_started();
    let mark = world.recorder.mark();
    let manager = world.manager.clone();
    let id = world.id.clone();
    let mut sent = tokio::spawn(async move { manager.execute_raw(&id, change_password()).await });
    support::wait_started_or_returned(&world.agent, started, &mut sent).await;
    // L'appelant n'attend plus (fenêtre fermée, délai) : la requête est partie, elle reste suivie.
    sent.abort();
    world.agent.release_actions();
    let (_, event) = world
        .recorder
        .wait_for(mark, "issue de l'opération", WAIT, |e| {
            matches!(e, Event::Operation { .. })
        })
        .await;
    let Event::Operation { outcome, .. } = event else {
        unreachable!()
    };
    assert!(
        matches!(outcome, Outcome::DoneDuringOutage { .. }),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn an_unknown_operation_survives_a_restart_of_the_application() {
    let world = World::connected(Options::default().accepting_bare_acts()).await;
    world.agent.hold_actions();
    let started = world.agent.verifications_started();
    let manager = world.manager.clone();
    let id = world.id.clone();
    let mut sent = tokio::spawn(async move { manager.execute_raw(&id, change_password()).await });
    support::wait_started_or_returned(&world.agent, started, &mut sent).await;
    world.proxy.cut();
    let ActionOutcome::ResultUnknown { id: operation } = sent.await.unwrap().unwrap() else {
        panic!("résultat inconnu attendu");
    };
    // L'application se ferme pendant la coupure ; l'agent finit l'action.
    world.manager.shutdown().await;
    world.agent.release_actions();
    world
        .agent
        .wait_operation_settled("marie", operation.as_str())
        .await;
    world.proxy.heal();
    let manager = support::start_manager(
        world.dir.path(),
        world.vault.clone(),
        world.net.clone(),
        world.clock.clone(),
        support::fast_config(),
    )
    .await;
    let recorder = support::Recorder::spawn(manager.subscribe());
    let (_, event) = recorder
        .wait_for(0, "issue de l'opération", WAIT, |e| {
            matches!(e, Event::Operation { .. })
        })
        .await;
    let Event::Operation {
        id: got, outcome, ..
    } = event
    else {
        unreachable!()
    };
    assert_eq!(got, operation);
    assert!(
        matches!(outcome, Outcome::DoneDuringOutage { .. }),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn a_stored_password_that_is_refused_asks_for_the_login_form_not_access_revoked() {
    let world = World::connected(Options {
        remember: true,
        ..Options::default()
    })
    .await;
    // Le mot de passe a changé côté serveur : celui du coffre n'est plus le bon.
    world
        .vault
        .put(
            &world.id,
            SecretKind::Password,
            &Secret::from("Changed-Elsewhere-1"),
        )
        .unwrap();
    let mark = world.recorder.mark();
    world.agent.clock.advance(TimeDuration::days(31));
    world
        .recorder
        .wait_state(mark, LinkState::SessionExpired, WAIT)
        .await;
    let info = world.state();
    assert_eq!(
        info.reason,
        Some(hearth_link::domain::state::Reason::StoredPasswordRefused)
    );
    assert!(
        !world
            .recorder
            .states_since(mark)
            .contains(&LinkState::AccessRevoked)
    );
    assert_eq!(
        world.manager.servers()[0].username,
        "marie",
        "identifiant conservé pour préremplir"
    );
    let attempts = world.proxy.accepted();
    tokio::time::sleep(ms(1_000)).await;
    assert_eq!(
        world.proxy.accepted(),
        attempts,
        "aucune nouvelle tentative"
    );
}

#[tokio::test]
async fn a_stall_of_the_whole_machine_cannot_cut_the_link_of_a_scenario() {
    // Cause établie du rouge de la revue (round 2) : avec le silence à l'échelle (0,5 s), un arrêt de
    // la machine plus long le fait atteindre (mesuré : `execute` rend « résultat inconnu » sans que
    // l'action parte, état « Reconnexion »). La configuration par défaut des scénarios met donc tous
    // les délais hors d'atteinte : un arrêt de 1,2 s ne change rien.
    let world = World::connected(Options::default().accepting_bare_acts()).await;
    std::thread::sleep(ms(1_200));
    world.agent.hold_actions();
    let started = world.agent.verifications_started();
    let manager = world.manager.clone();
    let id = world.id.clone();
    let mut sent = tokio::spawn(async move { manager.execute_raw(&id, change_password()).await });
    support::wait_started_or_returned(&world.agent, started, &mut sent).await;
    assert_eq!(world.state().state, LinkState::Connected);
    world.agent.release_actions();
    let outcome = tokio::time::timeout(WAIT, sent)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        matches!(outcome, ActionOutcome::Completed { status: 200, .. }),
        "{outcome:?}"
    );
}
