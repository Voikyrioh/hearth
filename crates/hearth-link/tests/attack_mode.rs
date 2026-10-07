//! L'état de sécurité et le mode attaque contre un VRAI agent (TLS 1.3, SQLite) : événement
//! `security` du flux, lecture de `GET /security`, activation et désactivation par le mot de passe ET
//! la preuve de la clé de ce poste (usage `0x03`), rien d'envoyé sans clé ni hors « Connecté »,
//! rôle Lecture seule. HRT-26 (ADR-0025, BR-TRUST-010, 018, 028). Aucune attente de durée.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use hearth_agent::domain::accounts::Role;
use hearth_link::domain::event::Event;
use hearth_link::domain::secret::Secret;
use hearth_link::domain::state::LinkState;
use hearth_link::{ActionOutcome, LinkError};
use hearth_proto::api::security::{AttackModeState, SessionDevice};
use hearth_proto::api::sessions::ChallengePurpose;
use support::{ChallengeMode, Options, PASSWORD, WAIT, World};

fn keyed() -> Options {
    Options {
        device_key: true,
        ..Options::default()
    }
}

fn status_and_code(outcome: &ActionOutcome) -> (u16, String) {
    let ActionOutcome::Completed { status, body, .. } = outcome else {
        panic!("réponse attendue, reçu {outcome:?}");
    };
    (
        *status,
        body["error"]["code"].as_str().unwrap_or("").to_owned(),
    )
}

#[tokio::test]
async fn the_stream_announces_the_security_state_right_after_the_authentication() {
    let world = World::connected(keyed()).await;
    let seen = world
        .recorder
        .since(0)
        .into_iter()
        .filter_map(|(_, event)| match event {
            Event::Security { server, view } => Some((server, view)),
            _ => None,
        })
        .next()
        .expect("l'état de sécurité est annoncé à la connexion");
    assert_eq!(seen.0, world.id);
    assert!(!seen.1.alert.own);
    assert_eq!(seen.1.attack_mode.state, AttackModeState::Off);
}

#[tokio::test]
async fn the_security_state_is_read_with_the_proof_status_of_the_session() {
    let world = World::connected(keyed()).await;
    let read = world.manager.security(&world.id).await.unwrap();
    assert!(!read.alert.own);
    assert_eq!(read.attack_mode.state, AttackModeState::Off);
    assert_eq!(read.device, SessionDevice::Proven);
    assert!(world.manager.has_device_key(&world.id));
}

#[tokio::test]
async fn an_administrator_with_the_key_turns_the_attack_mode_on_then_off() {
    let world = World::connected(keyed()).await;

    // Mot de passe faux : refusé par l'agent, le mode ne bouge pas.
    let wrong = world
        .manager
        .set_attack_mode(&world.id, true, &Secret::from("Faux-Mot-De-Passe-1"))
        .await
        .unwrap();
    assert_eq!(status_and_code(&wrong), (422, "WRONG_PASSWORD".to_owned()));
    assert_eq!(
        world
            .manager
            .security(&world.id)
            .await
            .unwrap()
            .attack_mode
            .state,
        AttackModeState::Off
    );

    let on = world
        .manager
        .set_attack_mode(&world.id, true, &Secret::from(PASSWORD))
        .await
        .unwrap();
    assert_eq!(status_and_code(&on).0, 200);
    assert_eq!(
        world
            .manager
            .security(&world.id)
            .await
            .unwrap()
            .attack_mode
            .state,
        AttackModeState::Active
    );

    // Une preuve par essai, jamais la même : faux mot de passe, activation, désactivation.
    let off = world
        .manager
        .set_attack_mode(&world.id, false, &Secret::from(PASSWORD))
        .await
        .unwrap();
    assert_eq!(status_and_code(&off).0, 200);
    assert_eq!(
        world
            .manager
            .security(&world.id)
            .await
            .unwrap()
            .attack_mode
            .state,
        AttackModeState::Off
    );
    assert_eq!(world.spy.calls_for(ChallengePurpose::AttackMode), 3);
}

#[tokio::test]
async fn without_a_key_nothing_is_sent_not_even_a_challenge() {
    let world = World::connected(Options {
        device_key: false,
        ..Options::default()
    })
    .await;
    assert!(!world.manager.has_device_key(&world.id));
    let error = world
        .manager
        .set_attack_mode(&world.id, true, &Secret::from(PASSWORD))
        .await
        .unwrap_err();
    assert_eq!(error, LinkError::NoDeviceKey);
    assert_eq!(world.spy.calls_for(ChallengePurpose::AttackMode), 0);
}

#[tokio::test]
async fn an_empty_password_is_refused_before_any_sending() {
    let world = World::connected(keyed()).await;
    let error = world
        .manager
        .set_attack_mode(&world.id, true, &Secret::from(""))
        .await
        .unwrap_err();
    assert!(matches!(error, LinkError::InvalidInput(_)));
    assert_eq!(world.spy.calls_for(ChallengePurpose::AttackMode), 0);
}

#[tokio::test]
async fn nothing_is_sent_while_the_link_is_down() {
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
        .set_attack_mode(&world.id, true, &Secret::from(PASSWORD))
        .await
        .unwrap_err();
    assert_eq!(error, LinkError::NotConnected);
    assert_eq!(world.spy.calls_for(ChallengePurpose::AttackMode), 0);
}

#[tokio::test]
async fn a_read_only_account_is_refused_by_the_agent_and_changes_nothing() {
    let world = World::connected(Options {
        device_key: true,
        role: Role::ReadOnly,
        ..Options::default()
    })
    .await;
    let outcome = world
        .manager
        .set_attack_mode(&world.id, true, &Secret::from(PASSWORD))
        .await
        .unwrap();
    assert_eq!(
        status_and_code(&outcome),
        (403, "FORBIDDEN_ROLE".to_owned())
    );
    // Il lit pourtant l'état : l'alerte se voit de tout rôle.
    assert_eq!(
        world
            .manager
            .security(&world.id)
            .await
            .unwrap()
            .attack_mode
            .state,
        AttackModeState::Off
    );
}

#[tokio::test]
async fn an_unavailable_challenge_is_said_so_and_nothing_else_is_sent() {
    for mode in [ChallengeMode::Unreachable, ChallengeMode::Garbage] {
        let world = World::connected(keyed()).await;
        world.spy.set(mode);
        let error = world
            .manager
            .set_attack_mode(&world.id, true, &Secret::from(PASSWORD))
            .await
            .unwrap_err();
        assert_eq!(error, LinkError::DeviceChallengeUnavailable, "{mode:?}");
        // Rien n'a changé chez l'agent.
        world.spy.set(ChallengeMode::Real);
        assert_eq!(
            world
                .manager
                .security(&world.id)
                .await
                .unwrap()
                .attack_mode
                .state,
            AttackModeState::Off
        );
    }
}
