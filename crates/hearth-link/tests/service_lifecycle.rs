//! Démarrer, arrêter, redémarrer PAR LA LIAISON contre un VRAI agent (TLS 1.3, SQLite, vrai flux WebSocket, faux
//! moteur) : le défi, la preuve de la clé de ce poste et le mot de passe sont ceux de l'acte réel. L'acte que la
//! liaison reconstruit depuis la requête est celui que l'agent reconstruit : la signature passe (HRT-65,
//! BR-LIFE-016). Aucune attente de durée : des attentes sur des faits observables.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use hearth_agent::domain::services::RunState;
use hearth_link::domain::event::Event;
use hearth_link::domain::secret::Secret;
use hearth_link::domain::service_lifecycle::{Gesture, Refusal, refusal_of};
use hearth_link::{ActionOutcome, LinkError};
use hearth_proto::api::services::{ActionAccepted, ServiceState, ServicesState};
use support::agent::fake_engine::{FakeEngine, container};
use support::{Options, PASSWORD, WAIT, World};

fn services_of(event: &Event) -> Option<&ServicesState> {
    match event {
        Event::Services { state, .. } => Some(state),
        _ => None,
    }
}

async fn wait_state(
    world: &World,
    mark: usize,
    what: &str,
    predicate: impl Fn(&ServicesState) -> bool,
) -> ServicesState {
    let (_, event) = world
        .recorder
        .wait_for(mark, what, WAIT, |event| {
            services_of(event).is_some_and(&predicate)
        })
        .await;
    services_of(&event).unwrap().clone()
}

/// `web` (adopté par le test) et `other` (jamais adopté : non pilotable).
fn options() -> Options {
    let engine = Arc::new(FakeEngine::new());
    for name in ["web", "other"] {
        engine.put_silently(container(name, RunState::Running));
    }
    Options {
        device_key: true,
        engine: Some(engine),
        ..Options::default()
    }
}

fn web_key() -> String {
    format!("c_{}", container("web", RunState::Running).id)
}

fn other_key() -> String {
    format!("c_{}", container("other", RunState::Running).id)
}

async fn adopt_web(world: &World) {
    world.agent.grant_services("marie").await;
    let mark = world.recorder.mark();
    world
        .manager
        .services_adopt(&world.id, &[web_key()], &Secret::from(PASSWORD))
        .await
        .unwrap();
    wait_state(world, mark, "web adopté", |state| {
        state.services.len() == 1
    })
    .await;
}

fn accepted(outcome: ActionOutcome) -> ActionAccepted {
    let ActionOutcome::Completed { status, body, .. } = outcome else {
        panic!("réponse attendue, reçu {outcome:?}");
    };
    assert_eq!(status, 202, "{body}");
    serde_json::from_value(body).unwrap()
}

fn refused(outcome: ActionOutcome) -> (u16, serde_json::Value) {
    let ActionOutcome::Completed { status, body, .. } = outcome else {
        panic!("réponse attendue, reçu {outcome:?}");
    };
    assert!(status >= 400, "{status} {body}");
    (status, body)
}

#[tokio::test]
async fn the_three_gestures_sent_by_the_link_are_signed_like_the_agent_reads_them() {
    let world = World::connected(options()).await;
    adopt_web(&world).await;
    let password = Secret::from(PASSWORD);
    let key = web_key();

    // Arrêter : l'acte reconstruit par la liaison est celui de l'agent (preuve de clé, mot de passe).
    let mark = world.recorder.mark();
    let reply = accepted(
        world
            .manager
            .service_gesture(&world.id, &key, Gesture::Stop, Some(&password))
            .await
            .unwrap(),
    );
    assert!(!reply.action_id.is_empty());
    assert!(reply.grace_s > 0);
    wait_state(&world, mark, "web arrêté", |state| {
        state
            .services
            .iter()
            .any(|item| item.state == ServiceState::Stopped)
    })
    .await;

    // Démarrer, puis redémarrer.
    let mark = world.recorder.mark();
    accepted(
        world
            .manager
            .service_gesture(&world.id, &key, Gesture::Start, Some(&password))
            .await
            .unwrap(),
    );
    wait_state(&world, mark, "web en marche", |state| {
        state
            .services
            .iter()
            .any(|item| item.state == ServiceState::Running)
    })
    .await;
    accepted(
        world
            .manager
            .service_gesture(&world.id, &key, Gesture::Restart, Some(&password))
            .await
            .unwrap(),
    );
}

#[tokio::test]
async fn the_refusals_of_the_agent_come_back_typed_and_nothing_is_ordered() {
    let world = World::connected(options()).await;
    let password = Secret::from(PASSWORD);

    // Sans le droit « gérer les services » : le droit manquant est nommé.
    assert!(matches!(
        world
            .manager
            .service_gesture(&world.id, &web_key(), Gesture::Stop, Some(&password))
            .await,
        Err(LinkError::MissingRight(_)) | Ok(ActionOutcome::Completed { status: 403, .. })
    ));
    adopt_web(&world).await;

    // Un conteneur non adopté n'est pas pilotable : même réponse qu'une clé inconnue (BR-LIFE-015, 024).
    let (status, body) = refused(
        world
            .manager
            .service_gesture(&world.id, &other_key(), Gesture::Stop, Some(&password))
            .await
            .unwrap(),
    );
    assert_eq!(refusal_of(status, &body), Refusal::ServiceNotFound);

    // Démarrer un service qui tourne : déjà dans l'état demandé (BR-LIFE-014), l'état est dit.
    let (status, body) = refused(
        world
            .manager
            .service_gesture(&world.id, &web_key(), Gesture::Start, Some(&password))
            .await
            .unwrap(),
    );
    assert_eq!(
        refusal_of(status, &body),
        Refusal::AlreadyInState {
            state: Some(ServiceState::Running)
        }
    );

    // Une clé qui n'a pas la forme d'une clé de service ne part pas.
    assert_eq!(
        world
            .manager
            .service_gesture(&world.id, "../accounts", Gesture::Stop, Some(&password))
            .await
            .unwrap_err(),
        LinkError::UnreadableAct
    );
}
