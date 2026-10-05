//! Résilience du lien contre un vrai agent, à travers un mandataire TCP à pannes : couper net,
//! geler, retarder, fermer proprement, refuser les connexions.
//!
//! Les durées du produit sont divisées par 6 (`support::SCALE`) : 3 s deviennent 0,5 s, 30 s
//! deviennent 5 s, les délais de reconnexion suivent. Chaque test porte le nom de la ligne du
//! tableau des transitions de la spec (section 6) ou du cas limite qu'il couvre.

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
use support::{Options, PASSWORD, WAIT, World, scaled};
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

/// Attend que la prochaine tentative automatique soit lointaine (au moins `at_least`), pour
/// prouver qu'un déclencheur ne l'attend pas.
async fn wait_for_a_distant_retry(world: &World, at_least: Duration) {
    let deadline = Instant::now() + WAIT;
    loop {
        let info = world.state();
        if let Some(next) = info.next_retry_at {
            let now = hearth_link::ports::Clock::wall(&*world.clock);
            if next.since(now) >= at_least {
                return;
            }
        }
        assert!(
            Instant::now() < deadline,
            "aucune tentative lointaine : {info:?}"
        );
        tokio::time::sleep(ms(10)).await;
    }
}

// ── Coupures ────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_one_second_cut_is_invisible() {
    // Seuils à l'échelle 1/3 (silence et « Reconnexion » à 1 s) pour une coupure de 1/6 de
    // seconde : de la marge pour une machine chargée, sans changer ce qui est prouvé.
    let world = World::connected(Options {
        config: hearth_link::LinkConfig {
            thresholds: hearth_link::domain::state::Thresholds::scaled(3),
            ..support::fast_config()
        },
        ..Options::default()
    })
    .await;
    let mark = world.recorder.mark();
    world.proxy.cut();
    tokio::time::sleep(scaled(Duration::from_secs(1))).await;
    world.proxy.heal();
    let resumed = world.recorder.mark();
    world.recorder.wait_metrics(resumed, WAIT).await;
    assert_eq!(
        world.recorder.states_since(mark),
        [],
        "une coupure de 1 s ne change rien à l'écran"
    );
    assert_eq!(world.state().state, LinkState::Connected);
}

#[tokio::test]
async fn a_ten_second_cut_shows_reconnecting_then_connected() {
    let world = World::connected(Options::default()).await;
    let mark = world.recorder.mark();
    world.proxy.cut();
    tokio::time::sleep(scaled(Duration::from_secs(10))).await;
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
    let world = World::connected(Options::default()).await;
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
    tokio::time::sleep(scaled(Duration::from_secs(40))).await;
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
async fn the_thresholds_are_respected_to_the_scale() {
    let world = World::connected(Options::default()).await;
    let mark = world.recorder.mark();
    let freeze = Instant::now();
    world.proxy.freeze();
    let reconnecting = world
        .recorder
        .wait_state(mark, LinkState::Reconnecting, WAIT)
        .await;
    let offline = world
        .recorder
        .wait_state(mark, LinkState::Offline, WAIT)
        .await;
    let reconnecting = reconnecting - freeze;
    let offline = offline - freeze;
    // 0,5 s et 5 s, comptés depuis le dernier message reçu (au plus quelques dizaines de ms
    // avant le gel), avec la marge d'ordonnancement.
    assert!(
        reconnecting >= ms(380) && reconnecting <= ms(1_200),
        "{reconnecting:?}"
    );
    assert!(offline >= ms(4_700) && offline <= ms(8_000), "{offline:?}");
    world.proxy.heal();
    world
        .recorder
        .wait_state(mark, LinkState::Connected, WAIT)
        .await;
}

#[tokio::test]
async fn a_prolonged_freeze_without_closing_is_detected_by_the_heartbeat() {
    let world = World::connected(Options::default()).await;
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
async fn a_slow_link_below_the_silence_threshold_stays_connected() {
    let world = World::connected(Options::default()).await;
    let mark = world.recorder.mark();
    world.proxy.delay(ms(120));
    tokio::time::sleep(ms(2_000)).await;
    assert_eq!(world.recorder.states_since(mark), []);
    assert_eq!(world.state().state, LinkState::Connected);
}

#[tokio::test]
async fn a_clean_close_reconnects_by_itself() {
    let world = World::connected(Options::default()).await;
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
    let refused = world.proxy.accepted();
    tokio::time::sleep(ms(800)).await;
    assert!(
        world.proxy.accepted() > refused,
        "les tentatives continuent"
    );
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
    let restarted = Instant::now();
    // L'agent est de retour : les mesures reprennent sur une nouvelle connexion.
    let back = world.recorder.mark();
    world.recorder.wait_metrics(back, WAIT).await;
    // « Reconnexion réussie en moins de 10 s » (échelle : 1,7 s).
    assert!(
        restarted.elapsed() < scaled(Duration::from_secs(10)),
        "{:?}",
        restarted.elapsed()
    );
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
        ..Options::default()
    })
    .await;
    let mark = world.recorder.mark();
    world.proxy.cut();
    tokio::time::sleep(scaled(Duration::from_secs(10))).await;
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
    wait_for_a_distant_retry(&world, ms(2_500)).await;
    world.proxy.heal();
    let clicked = world.recorder.mark();
    let click = Instant::now();
    world.manager.retry_now(&world.id).unwrap();
    let back = world
        .recorder
        .wait_state(clicked, LinkState::Connected, WAIT)
        .await;
    assert!(back - click < ms(2_000), "{:?}", back - click);
    let states = world.recorder.states_since(clicked);
    assert_eq!(states.first(), Some(&LinkState::Reconnecting), "{states:?}");
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
    wait_for_a_distant_retry(&world, ms(2_500)).await;
    world.proxy.heal();
    let changed = world.recorder.mark();
    let at = Instant::now();
    // Câble débranché, Wi-Fi : la liste des adresses locales change.
    world.net.set(&["10.8.0.2"]);
    let back = world
        .recorder
        .wait_state(changed, LinkState::Connected, WAIT)
        .await;
    assert!(back - at < ms(2_000), "{:?}", back - at);
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
    wait_for_a_distant_retry(&world, ms(2_500)).await;
    world.proxy.heal();
    let woke = world.recorder.mark();
    let at = Instant::now();
    // L'horloge murale saute de dix minutes sans que la monotone bouge : veille puis réveil.
    world.clock.jump(Duration::from_secs(600));
    let back = world
        .recorder
        .wait_state(woke, LinkState::Connected, WAIT)
        .await;
    assert!(back - at < ms(2_000), "{:?}", back - at);
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
        ..Options::default()
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
    let world = World::connected(Options::default()).await;
    world
        .agent
        .hasher
        .delay_ms
        .store(900, std::sync::atomic::Ordering::SeqCst);
    let manager = world.manager.clone();
    let id = world.id.clone();
    let sent = tokio::spawn(async move { manager.execute(&id, change_password()).await });
    tokio::time::sleep(ms(200)).await;
    let mark = world.recorder.mark();
    world.proxy.cut();
    let answered = Instant::now();
    let outcome = tokio::time::timeout(Duration::from_secs(3), sent)
        .await
        .expect("l'appel ne reste pas suspendu")
        .unwrap()
        .unwrap();
    assert!(
        answered.elapsed() < ms(2_000),
        "la réponse « inconnu » est immédiate"
    );
    let ActionOutcome::ResultUnknown { id: operation } = outcome else {
        panic!("résultat inconnu attendu, reçu {outcome:?}");
    };
    // L'agent finit l'action pendant que le lien est coupé, puis le lien revient.
    tokio::time::sleep(ms(1_300)).await;
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
    let world = World::connected(Options::default()).await;
    // Trou noir : la requête part dans le vide, l'agent ne la reçoit jamais.
    world.proxy.freeze();
    let outcome = world
        .manager
        .execute(&world.id, change_password())
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
    let world = World::connected(Options::default()).await;
    world
        .agent
        .hasher
        .delay_ms
        .store(2_500, std::sync::atomic::Ordering::SeqCst);
    let manager = world.manager.clone();
    let id = world.id.clone();
    let sent = tokio::spawn(async move { manager.execute(&id, change_password()).await });
    tokio::time::sleep(ms(200)).await;
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
    let world = World::connected(Options::default()).await;
    let mark = world.recorder.mark();
    world.proxy.cut();
    world
        .recorder
        .wait_state(mark, LinkState::Reconnecting, WAIT)
        .await;
    let result = world.manager.execute(&world.id, change_password()).await;
    assert_eq!(result.unwrap_err(), LinkError::NotConnected);
}

#[tokio::test]
async fn a_completed_action_returns_the_agent_answer_even_when_it_is_a_refusal() {
    let world = World::connected(Options::default()).await;
    let wrong = ActionRequest {
        method: Method::Put,
        path: "/me/password".into(),
        body: Some(json!({ "current": "Not-The-Password-1", "password": "New-Password-12" })),
    };
    match world.manager.execute(&world.id, wrong).await.unwrap() {
        ActionOutcome::Completed { status, body, .. } => {
            assert_eq!(status, 422);
            assert_eq!(body["error"]["code"], "WRONG_PASSWORD");
        }
        other => panic!("réponse attendue, reçu {other:?}"),
    }
    match world
        .manager
        .execute(&world.id, change_password())
        .await
        .unwrap()
    {
        ActionOutcome::Completed { status, .. } => assert_eq!(status, 200),
        other => panic!("réponse attendue, reçu {other:?}"),
    }
}
