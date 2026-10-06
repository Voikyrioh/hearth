//! Mise à jour de l'agent vue par la bibliothèque de liaison, contre un VRAI agent (TLS 1.3, SQLite,
//! WebSocket ; seuls le téléchargement et la machine sont simulés par le banc de l'agent) : lectures
//! typées de l'état et du dernier résultat, progression sur le flux (sujet `update`), et coupure
//! ATTENDUE pendant le redémarrage : « Reconnexion en cours », jamais « Hors ligne », aucun échec
//! compté, puis le résultat relu au retour du lien. HRT-17, BR-UPDATE-013, 014, 017.
//!
//! Aucune assertion de durée : chaque attente porte sur un fait observable.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::update::UpdateRecord;
use hearth_link::domain::event::Event;
use hearth_link::domain::state::{LinkState, Thresholds};
use hearth_link::ports::Transport as _;
use hearth_link::ports::transport::{ApiRequest, Method, Pin, Target};
use hearth_link::{ActionOutcome, ActionRequest, LinkConfig, LinkError};
use hearth_proto::api::sessions::LoginRequest;
use hearth_proto::api::update::{UpdateOutcome, UpdateProgress, UpdateStep};
use support::update_rig::{self, Rig, rig};
use support::{Options, PASSWORD, SCALE, WAIT, World, never};

const BINARY: &[u8] = b"nouvel agent";

fn options(rig: &Arc<Rig>, role: Role, config: LinkConfig) -> Options {
    let rig = rig.clone();
    Options {
        role,
        config,
        updating: Some(update_rig::factory(rig)),
        ..Options::default()
    }
}

async fn start_update(world: &World, rig: &Rig) -> ActionOutcome {
    world
        .manager
        .execute(
            &world.id,
            ActionRequest {
                method: Method::Post,
                path: "/agent/update".into(),
                body: Some(rig.request("0.2.0", BINARY)),
            },
        )
        .await
        .unwrap()
}

fn update_event(event: &Event, step: UpdateStep) -> bool {
    matches!(event, Event::AgentUpdate { progress, .. } if progress.step == step)
}

fn progress_of(event: Event) -> UpdateProgress {
    match event {
        Event::AgentUpdate { progress, .. } => (*progress).clone(),
        other => panic!("pas une progression : {other:?}"),
    }
}

#[tokio::test]
async fn the_state_and_the_last_result_are_read_with_typed_calls_by_any_account() {
    let rig = Arc::new(rig(true, false));
    for role in [Role::Admin, Role::ReadOnly] {
        let world = World::connected(options(&rig, role, support::fast_config())).await;
        let status = world.manager.agent_update_status(&world.id).await.unwrap();
        assert!(!status.managed);
        assert!(!status.in_progress);
        assert!(status.progress.is_none());
        assert!(status.last.is_none());
        assert!(!status.current.is_empty());
        assert_eq!(
            world.manager.agent_update_last(&world.id).await.unwrap(),
            None
        );
    }
}

#[tokio::test]
async fn a_managed_installation_says_so() {
    let rig = Arc::new(rig(false, false));
    let world = World::connected(options(&rig, Role::Admin, support::fast_config())).await;
    let status = world.manager.agent_update_status(&world.id).await.unwrap();
    assert!(
        status.managed,
        "installation gérée : pas de mise à jour à distance"
    );
}

#[tokio::test]
async fn nothing_is_read_while_the_link_is_down() {
    let rig = Arc::new(rig(true, false));
    let world = World::connected(options(&rig, Role::Admin, support::fast_config())).await;
    world.manager.logout(&world.id).await.unwrap();
    assert_eq!(
        world
            .manager
            .agent_update_status(&world.id)
            .await
            .unwrap_err(),
        LinkError::NotConnected
    );
    assert_eq!(
        world
            .manager
            .agent_update_last(&world.id)
            .await
            .unwrap_err(),
        LinkError::NotConnected
    );
}

#[tokio::test]
async fn the_steps_arrive_on_the_stream_in_order_with_the_download_percentage() {
    let rig = Arc::new(rig(true, true));
    let world = World::connected(options(&rig, Role::Admin, support::fast_config())).await;
    let mark = world.recorder.mark();
    let outcome = start_update(&world, &rig).await;
    assert!(
        matches!(&outcome, ActionOutcome::Completed { status: 202, .. }),
        "{outcome:?}"
    );
    // Téléchargement : la porte est fermée, la taille n'est pas encore connue (pas de pourcentage).
    let (_, first) = world
        .recorder
        .wait_for(mark, "le téléchargement", WAIT, |e| {
            update_event(e, UpdateStep::Download)
        })
        .await;
    let first = progress_of(first);
    assert_eq!(first.version, "0.2.0");
    assert!(first.percent.is_none_or(|percent| percent <= 100));
    // Lu aussi par la lecture typée, pendant qu'elle est en cours.
    let status = world.manager.agent_update_status(&world.id).await.unwrap();
    assert!(status.in_progress);
    assert_eq!(status.progress.unwrap().step, UpdateStep::Download);

    rig.release_gate();
    world
        .recorder
        .wait_for(mark, "le redémarrage", WAIT, |e| {
            update_event(e, UpdateStep::Restart)
        })
        .await;
    let seen = world.recorder.since(mark);
    let mut steps: Vec<UpdateStep> = seen
        .iter()
        .filter_map(|(_, event)| match event {
            Event::AgentUpdate { progress, .. } => Some(progress.step),
            _ => None,
        })
        .collect();
    steps.dedup();
    assert_eq!(
        steps,
        [
            UpdateStep::Download,
            UpdateStep::Verify,
            UpdateStep::Install,
            UpdateStep::Restart
        ]
    );
    for (_, event) in &seen {
        if let Event::AgentUpdate { progress, .. } = event {
            assert!(progress.percent.is_none_or(|percent| percent <= 100));
        }
    }
}

/// Seuils où une coupure ordinaire est « Hors ligne » AUSSITÔT (1 ms) : si la coupure attendue
/// n'était pas respectée, « Hors ligne » apparaîtrait à la première tentative échouée, sans qu'aucune
/// durée ne soit en jeu. La fenêtre d'attente est très large (rien ne l'atteint par hasard).
fn impatient_thresholds() -> LinkConfig {
    LinkConfig {
        thresholds: Thresholds {
            silence: never(),
            reconnecting_after: Duration::from_millis(1),
            offline_after: Duration::from_millis(1),
            restart_window: never(),
            ..Thresholds::scaled(SCALE)
        },
        heartbeat_period: Duration::from_secs(5),
        ..support::fast_config()
    }
}

/// Attend qu'une tentative de plus ait atteint le mandataire (un fait), avec une garde qui dit quoi.
async fn wait_attempt(world: &World, before: u64) {
    let started = std::time::Instant::now();
    while world.proxy.accepted() == before {
        assert!(
            started.elapsed() < WAIT,
            "délai dépassé en attendant : une tentative de connexion de plus"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

fn succeeded_record() -> UpdateRecord {
    UpdateRecord {
        version: Some("0.2.0".to_owned()),
        previous: "0.1.0".to_owned(),
        outcome: UpdateOutcome::Succeeded,
        reason: None,
        at: "2026-10-06T10:00:00Z".to_owned(),
        requested_by: Some("marie".to_owned()),
        client_name: None,
        client_addr: None,
        reported: false,
    }
}

#[tokio::test]
async fn a_restart_announced_by_the_agent_is_an_expected_cut_then_the_result_is_read_back() {
    let rig = Arc::new(rig(true, true));
    let mut world = World::connected(options(&rig, Role::Admin, impatient_thresholds())).await;
    let mark = world.recorder.mark();
    start_update(&world, &rig).await;
    rig.release_gate();
    world
        .recorder
        .wait_for(mark, "le redémarrage annoncé", WAIT, |e| {
            update_event(e, UpdateStep::Restart)
        })
        .await;

    // L'ancien agent s'arrête (l'échange de binaire est celui du superviseur, simulé ici par le
    // résultat qu'il écrit) : le lien tombe.
    let cut = world.recorder.mark();
    world.agent.stop().await;
    world
        .recorder
        .wait_state(cut, LinkState::Reconnecting, WAIT)
        .await;
    // Les tentatives échouent à répétition : sans coupure attendue, « Hors ligne » viendrait dès la
    // première (seuil de 1 ms) ; ici le lien reste « Reconnexion en cours », sans échec compté.
    // Chaque « Réessayer maintenant » est une tentative de plus, observée par le compteur de
    // connexions du mandataire (un fait, pas une durée).
    for _ in 0..3 {
        let before = world.proxy.accepted();
        world.manager.retry_now(&world.id).unwrap();
        wait_attempt(&world, before).await;
    }
    let info = world.state();
    assert_eq!(info.state, LinkState::Reconnecting);
    assert_eq!(
        info.failed_attempts, 0,
        "une coupure annoncée ne compte aucun échec"
    );
    assert_eq!(world.recorder.states_since(cut), [LinkState::Reconnecting]);

    // Le nouvel agent revient (résultat écrit par le superviseur, jamais annoncé).
    // Le superviseur simulé écrit SON résultat avant le démarrage du nouvel agent. Aucune attente : la
    // surveillance de l'ancien agent ne s'éveille jamais (`update_rig::factory`).
    rig.host.with(|state| state.last = Some(succeeded_record()));
    world.agent.restart().await;
    world.proxy.set_target(world.agent.addr);
    world.manager.retry_now(&world.id).unwrap();
    world
        .recorder
        .wait_state(cut, LinkState::Connected, WAIT)
        .await;
    assert_eq!(
        world.recorder.states_since(cut),
        [LinkState::Reconnecting, LinkState::Connected],
        "ni « Hors ligne » ni autre état pendant la coupure attendue"
    );
    // Le résultat se RELIT par les deux lectures typées (BR-UPDATE-017). Il n'est pas attendu sur le
    // flux : le nouvel agent l'annonce dès son démarrage, souvent AVANT que le client ne se soit
    // abonné de nouveau, et l'état courant d'un abonnement ne rejoue que ce qui est en cours.
    let last = world
        .manager
        .agent_update_last(&world.id)
        .await
        .unwrap()
        .expect("un dernier résultat");
    assert_eq!(last.outcome, UpdateOutcome::Succeeded, "{last:?}");
    assert_eq!(last.version, "0.2.0");
    let status = world.manager.agent_update_status(&world.id).await.unwrap();
    assert_eq!(status.last.unwrap().outcome, UpdateOutcome::Succeeded);
}

#[tokio::test]
async fn an_ordinary_cut_with_the_same_thresholds_is_offline_at_once() {
    // Le témoin du test précédent : sans annonce, les mêmes seuils donnent « Hors ligne ».
    let rig = Arc::new(rig(true, false));
    let world = World::connected(options(&rig, Role::Admin, impatient_thresholds())).await;
    let mark = world.recorder.mark();
    world.proxy.cut();
    world
        .recorder
        .wait_state(mark, LinkState::Offline, WAIT)
        .await;
}

#[tokio::test]
async fn a_read_only_account_sees_an_update_started_by_someone_else_and_every_connection_gets_the_current_step()
 {
    let rig = Arc::new(rig(true, true));
    let world = World::connected(options(&rig, Role::ReadOnly, support::fast_config())).await;
    // Un autre administrateur lance la mise à jour, par sa propre session.
    world.agent.create_account("paul", Role::Admin).await;
    let transport = support::transport();
    let target = Target {
        host: "127.0.0.1".into(),
        port: world.proxy.port(),
        pin: Pin::Pinned(world.fingerprint),
    };
    let login = transport
        .login(
            &target,
            &LoginRequest {
                username: "paul".into(),
                password: PASSWORD.into(),
            },
        )
        .await
        .unwrap();
    let mark = world.recorder.mark();
    let reply = transport
        .request(
            &target,
            &hearth_link::domain::secret::Secret::from(login.token.as_str()),
            &ApiRequest {
                method: Method::Post,
                path: "/agent/update".into(),
                body: Some(rig.request("0.2.0", BINARY)),
                idempotency_key: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(reply.status, 202);
    world
        .recorder
        .wait_for(
            mark,
            "le téléchargement vu par le compte Lecture seule",
            WAIT,
            |e| update_event(e, UpdateStep::Download),
        )
        .await;
    // Une nouvelle connexion (le lien tombe puis revient) reçoit d'abord l'état courant.
    let again = world.recorder.mark();
    world.proxy.cut();
    world.proxy.heal();
    world
        .recorder
        .wait_for(again, "l'état courant à la reconnexion", WAIT, |e| {
            update_event(e, UpdateStep::Download)
        })
        .await;
    rig.release_gate();
}

#[tokio::test]
async fn a_client_that_connects_during_the_restart_step_expects_the_cut_too() {
    let rig = Arc::new(rig(true, true));
    let mut world = World::connected(options(&rig, Role::Admin, impatient_thresholds())).await;
    let mark = world.recorder.mark();
    start_update(&world, &rig).await;
    rig.release_gate();
    world
        .recorder
        .wait_for(mark, "le redémarrage annoncé", WAIT, |e| {
            update_event(e, UpdateStep::Restart)
        })
        .await;
    // Le lien tombe puis revient PENDANT l'étape `restart` : le retour du lien lève l'attente, et
    // l'agent envoie l'état courant (`restart`) avant l'instantané : il la rouvre.
    let back = world.recorder.mark();
    world.proxy.cut();
    world.proxy.heal();
    world.manager.retry_now(&world.id).unwrap();
    world
        .recorder
        .wait_state(back, LinkState::Connected, WAIT)
        .await;
    let cut = world.recorder.mark();
    world.agent.stop().await;
    world
        .recorder
        .wait_state(cut, LinkState::Reconnecting, WAIT)
        .await;
    let before = world.proxy.accepted();
    world.manager.retry_now(&world.id).unwrap();
    wait_attempt(&world, before).await;
    let info = world.state();
    assert_eq!(info.state, LinkState::Reconnecting);
    assert_eq!(info.failed_attempts, 0);
    assert!(
        !world
            .recorder
            .states_since(cut)
            .contains(&LinkState::Offline),
        "jamais « Hors ligne » : la coupure annoncée est attendue"
    );
}
