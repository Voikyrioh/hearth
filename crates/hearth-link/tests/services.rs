//! Le moteur de conteneurs et les services contre un VRAI agent (TLS 1.3, SQLite, vrai flux WebSocket)
//! dont le moteur est un faux piloté par le test (HRT-62, ADR-0038) : le sujet `services` est reçu par la
//! liaison, fusionné (instantané puis lots), annoncé en état complet, rangé dans la dernière vue connue ;
//! un agent sans la capacité ne reçoit jamais le sujet et `metrics` continue. Aucune attente de durée : des
//! attentes sur des faits observables.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use hearth_agent::domain::services::RunState;
use hearth_link::domain::event::Event;
use hearth_link::domain::state::LinkState;
use hearth_proto::api::services::{EngineState, ServiceState, ServicesState};
use hearth_proto::stream::Topic;
use support::agent::fake_engine::FakeEngine;
use support::agent::fake_engine::container;
use support::{Options, WAIT, World};

fn engined(names: &[&str]) -> (Arc<FakeEngine>, Options) {
    let engine = Arc::new(FakeEngine::new());
    for name in names {
        engine.put_silently(container(name, RunState::Running));
    }
    (
        engine.clone(),
        Options {
            engine: Some(engine),
            ..Options::default()
        },
    )
}

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

/// Attend un état des services qui vérifie `predicate` et le rend.
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

#[tokio::test]
async fn a_real_agent_sends_the_engine_and_the_services_and_the_link_keeps_them_merged_and_sorted()
{
    let (engine, options) = engined(&["zeta", "Alpha", "mid"]);
    let world = World::connected(options).await;

    // L'instantané : le moteur prêt et les trois services, triés sans tenir compte de la casse.
    let first = wait_state(&world, 0, "l'instantané des services", |state| {
        state.engine.state == EngineState::Ready && state.services.len() == 3
    })
    .await;
    assert_eq!(names(&first), ["Alpha", "mid", "zeta"]);
    assert_eq!(first.engine.host.os_id, "debian");
    assert!(first.engine.name.is_some());
    assert!(
        first
            .services
            .iter()
            .all(|s| s.state == ServiceState::Running)
    );

    // Un lot : un service créé en ligne de commande apparaît à sa place, l'état reste COMPLET.
    let mark = world.recorder.mark();
    engine.put(container("beta", RunState::Running), "create");
    let second = wait_state(&world, mark, "un service créé", |state| {
        state.services.len() == 4
    })
    .await;
    assert_eq!(names(&second), ["Alpha", "beta", "mid", "zeta"]);

    // Un service disparu du serveur disparaît de la liste.
    let mark = world.recorder.mark();
    // L'identifiant du faux conteneur ne dépend que de son nom.
    engine.remove(&container("mid", RunState::Running).id);
    let third = wait_state(&world, mark, "un service supprimé", |state| {
        state.services.len() == 3
    })
    .await;
    assert_eq!(names(&third), ["Alpha", "beta", "zeta"]);

    // Les mesures n'ont pas bougé : le sujet des services s'ajoute, il ne remplace rien.
    let mark = world.recorder.mark();
    world.recorder.wait_metrics(mark, WAIT).await;
}

#[tokio::test]
async fn the_last_services_are_the_last_known_view_without_variable_names_nor_command() {
    let (_engine, options) = engined(&["web", "db"]);
    let world = World::connected(options).await;
    wait_state(&world, 0, "les services", |state| state.services.len() == 2).await;

    // La liaison les range dans la dernière vue connue (en mémoire, écrite sur le disque à la période).
    let known = world
        .manager
        .services_state(&world.id)
        .await
        .unwrap()
        .expect("la dernière liste est connue");
    assert_eq!(names(&known), ["db", "web"]);

    // Ce qui part au fichier `snapshots/{id}.json` : la vue entière, sérialisée comme elle l'est sur le
    // disque. Aucun nom de variable, aucune valeur, aucune commande (ils n'existent pas dans le protocole
    // de cette tranche).
    let view = world.manager.last_known(&world.id).await.unwrap().unwrap();
    let json = serde_json::to_string(&view).unwrap();
    assert!(json.contains("\"services\""));
    for forbidden in [
        "\"env\"",
        "\"env_names\"",
        "\"command\"",
        "\"cmd\"",
        "\"entrypoint\"",
    ] {
        assert!(!json.contains(forbidden), "{forbidden} dans {json}");
    }
}

#[tokio::test]
async fn an_agent_without_the_capability_never_gets_the_services_topic_and_metrics_keep_flowing() {
    let world = World::connected(Options {
        hide_capabilities: true,
        ..Options::default()
    })
    .await;
    // Le lien est connecté ET les mesures arrivent (World::connected les a attendues) : un sujet inconnu
    // aurait fait refuser tout le `subscribe`.
    assert_eq!(world.state().state, LinkState::Connected);

    let subscribed = world.spy.subscribed.lock().unwrap().clone();
    assert!(!subscribed.is_empty(), "le client s'est abonné");
    for topics in &subscribed {
        assert!(!topics.contains(&Topic::Services), "{topics:?}");
        assert!(topics.contains(&Topic::Metrics), "{topics:?}");
    }

    // L'interface en est informée, avec la version de l'agent.
    let (_, event) = world
        .recorder
        .wait_for(0, "l'agent ne sert pas les services", WAIT, |event| {
            matches!(event, Event::ServicesUnsupported { .. })
        })
        .await;
    let Event::ServicesUnsupported { agent_version, .. } = event else {
        unreachable!();
    };
    assert!(!agent_version.is_empty());
    assert!(
        world
            .recorder
            .since(0)
            .iter()
            .all(|(_, event)| services_of(event).is_none()),
        "aucun état de services pour un agent qui ne les sert pas"
    );
    // La dernière vue ne garde aucune liste.
    assert!(
        world
            .manager
            .services_state(&world.id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn an_agent_with_the_capability_is_asked_for_the_services_topic() {
    let (_engine, options) = engined(&["web"]);
    let world = World::connected(options).await;
    let subscribed = world.spy.subscribed.lock().unwrap().clone();
    assert!(
        subscribed
            .iter()
            .any(|topics| topics.contains(&Topic::Services)),
        "{subscribed:?}"
    );
}

#[tokio::test]
async fn check_again_asks_the_agent_to_probe_the_engine_and_writes_nothing() {
    let (engine, options) = engined(&["web"]);
    let world = World::connected(options).await;
    wait_state(&world, 0, "les services", |state| state.services.len() == 1).await;
    let before = engine.probe_count();
    let view = world.manager.engine_recheck(&world.id).await.unwrap();
    assert_eq!(view.state, EngineState::Ready);
    assert!(engine.probe_count() > before, "la détection a été relancée");
    // Rien ne modifie le serveur : aucune écriture partie vers l'agent hors lecture.
    // (`POST /engine/recheck` est une lecture : pas de clé d'opération, pas d'acte.)
    let writes = world.spy.writes.lock().unwrap().clone();
    assert_eq!(writes.len(), 1, "{writes:?}");
    assert!(writes[0].1.ends_with("/engine/recheck"), "{writes:?}");
}

/// Une mise à jour de l'agent change ses capacités : elles sont relues à CHAQUE connexion, pas seulement
/// au premier contact (sinon un agent mis à jour resterait « trop ancien » jusqu'à un nouvel ajout).
#[tokio::test]
async fn the_capabilities_are_read_again_at_every_connection() {
    let engine = Arc::new(FakeEngine::new());
    engine.put_silently(container("web", RunState::Running));
    let world = World::connected(Options {
        engine: Some(engine),
        hide_capabilities: true,
        ..Options::default()
    })
    .await;
    world
        .recorder
        .wait_for(0, "agent trop ancien", WAIT, |event| {
            matches!(event, Event::ServicesUnsupported { .. })
        })
        .await;

    // L'agent est « mis à jour » : son /hello annonce maintenant la capacité ; la connexion tombe puis revient.
    world
        .spy
        .hide_capabilities
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let mark = world.recorder.mark();
    world.proxy.close();
    let state = wait_state(&world, mark, "les services après la reconnexion", |state| {
        state.services.len() == 1
    })
    .await;
    assert_eq!(names(&state), ["web"]);
}
