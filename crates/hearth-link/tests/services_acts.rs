//! Adopter et relâcher PAR LA LIAISON contre un VRAI agent (TLS 1.3, SQLite, vrai flux WebSocket, faux
//! moteur piloté par le test) : le défi, la preuve de la clé de ce poste et le mot de passe sont ceux de
//! l'acte réel, la liste des candidats est lue par la liaison, et le service apparaît puis disparaît dans le
//! flux `services`. Une unité dont le nom demande un encodage dans le chemin (`\x2d`) est adoptée par sa clé
//! en clair et relâchée par son chemin encodé (HRT-80, ADR-0043, BR-ADOPT-008, BR-ADOPT-013). Aucune attente
//! de durée : des attentes sur des faits observables.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use hearth_agent::domain::services::RunState;
use hearth_link::domain::event::Event;
use hearth_link::domain::secret::Secret;
use hearth_link::{Acted, LinkError};
use hearth_proto::api::services::{AdoptionResult, EngineState, ServicesState};
use support::agent::StaticSupervisor;
use support::agent::fake_engine::{FakeEngine, container};
use support::{Options, PASSWORD, WAIT, World};

const ESCAPED_UNIT: &str = "docker-my\\x2dapp.service";

fn services_of(event: &Event) -> Option<&ServicesState> {
    match event {
        Event::Services { state, .. } => Some(state),
        _ => None,
    }
}

fn names(state: &ServicesState) -> Vec<String> {
    state
        .services
        .iter()
        .map(|item| item.name.clone())
        .collect()
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

/// Un agent qui exige la confirmation, avec `web` (sans unité) et `app` (lancé par l'unité au nom
/// échappé), tous deux non adoptés.
fn options() -> Options {
    let engine = Arc::new(FakeEngine::new());
    for name in ["web", "app"] {
        engine.put_silently(container(name, RunState::Running));
    }
    let app_id = container("app", RunState::Running).id;
    Options {
        device_key: true,
        engine: Some(engine),
        supervisor: Some(StaticSupervisor::new(&[(&app_id, ESCAPED_UNIT)])),
        ..Options::default()
    }
}

/// Attend que la liaison lise les deux candidats (le registre de l'agent les a vus).
async fn candidates(world: &World) -> Vec<String> {
    for _ in 0..500 {
        let found = world.manager.services_external(&world.id).await.unwrap();
        if found.engine == EngineState::Ready && found.containers.len() == 2 {
            return found.containers.into_iter().map(|c| c.key).collect();
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("les deux candidats n'ont pas été lus");
}

#[tokio::test]
async fn an_adoption_and_a_release_sent_by_the_link_show_and_hide_the_service_in_its_stream() {
    let world = World::connected(options()).await;
    world.agent.grant_services("marie").await;
    let password = Secret::from(PASSWORD);

    // Rien n'est publié tant que rien n'est adopté.
    let first = wait_state(&world, 0, "l'état initial", |state| {
        state.engine.state == EngineState::Ready
    })
    .await;
    assert!(first.services.is_empty(), "{first:?}");

    // La liaison lit les candidats, dont la clé au nom échappé, en clair.
    let keys = candidates(&world).await;
    let escaped = format!("u_{ESCAPED_UNIT}");
    assert!(keys.contains(&escaped), "{keys:?}");
    let web = keys
        .iter()
        .find(|key| key.starts_with("c_"))
        .unwrap()
        .clone();

    // Adopter les deux : acte signé (défi neuf, preuve, mot de passe), puis ils apparaissent dans le flux.
    let mark = world.recorder.mark();
    let Acted::Done(reply) = world
        .manager
        .services_adopt(&world.id, &[web.clone(), escaped.clone()], &password)
        .await
        .unwrap()
    else {
        panic!("réponse attendue");
    };
    assert_eq!(reply.adopted, 2);
    assert!(
        reply
            .results
            .iter()
            .all(|item| item.result == AdoptionResult::Adopted),
        "{reply:?}"
    );
    let listed = wait_state(&world, mark, "les deux services adoptés", |state| {
        state.services.len() == 2
    })
    .await;
    assert_eq!(names(&listed), ["app", "web"]);

    // Relâcher celui dont la clé demande un encodage : la liaison encode le chemin et signe la clé en clair.
    let mark = world.recorder.mark();
    let Acted::Done(released) = world
        .manager
        .services_release(&world.id, &escaped, &password)
        .await
        .unwrap()
    else {
        panic!("réponse attendue");
    };
    assert!(released.released);
    let left = wait_state(&world, mark, "un service relâché", |state| {
        state.services.len() == 1
    })
    .await;
    assert_eq!(names(&left), ["web"]);

    // Le relâcher de nouveau ne fait rien, et le dit.
    let Acted::Done(again) = world
        .manager
        .services_release(&world.id, &escaped, &password)
        .await
        .unwrap()
    else {
        panic!("réponse attendue");
    };
    assert!(!again.released);

    // Relâché, il est de nouveau un candidat.
    let found = world.manager.services_external(&world.id).await.unwrap();
    assert!(found.containers.iter().any(|c| c.key == escaped));
}

#[tokio::test]
async fn the_refusals_of_the_agent_come_back_typed_and_nothing_is_adopted() {
    let world = World::connected(options()).await;
    let password = Secret::from(PASSWORD);
    let web = format!("c_{}", container("web", RunState::Running).id);

    // Sans le droit « gérer les services » : la lecture des candidats et l'acte sont refusés, le droit
    // manquant est nommé.
    assert!(matches!(
        world.manager.services_external(&world.id).await,
        Err(LinkError::MissingRight(_))
    ));
    assert!(matches!(
        world
            .manager
            .services_adopt(&world.id, std::slice::from_ref(&web), &password)
            .await,
        Err(LinkError::MissingRight(_))
    ));

    // Avec le droit mais un mot de passe faux : refus de la confirmation, rien n'est adopté.
    world.agent.grant_services("marie").await;
    let wrong = Secret::from("Wrong-Horse-9999");
    let refused = world
        .manager
        .services_adopt(&world.id, std::slice::from_ref(&web), &wrong)
        .await;
    assert!(
        matches!(refused, Err(LinkError::Rejected(Some(_)))),
        "{refused:?}"
    );

    // Une clé qui n'a pas la forme d'une clé de service ne part pas.
    assert_eq!(
        world
            .manager
            .services_release(&world.id, "../accounts", &password)
            .await,
        Err(LinkError::UnreadableAct)
    );
}
