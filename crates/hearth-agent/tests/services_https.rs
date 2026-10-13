//! Le moteur de conteneurs et la liste des services de bout en bout (HRT-61) : vrai agent en HTTPS,
//! vrai WebSocket, faux moteur (ou `bollard` sur un faux démon), vraie base. Lecture seule : aucune
//! de ces routes n'écrit dans le moteur, aucune dans la base.
//!
//! Règles : BR-ENGINE-012, 013, 016, 019, 020 ; BR-SVC-001, 012, 015, 019, 020, 024.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use hearth_agent::app::EngineParts;
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::services::{Hidden, ProbeOutcome, RunState, UnitActivity, UnitStatus};
use hearth_agent::infrastructure::engine::{BollardEngine, InstallMarkers, Target};
use hearth_agent::infrastructure::logging::{self, LogFormat};
use hearth_proto::api::services::{
    Concealed, EngineState, RestartMode, ServiceDriver, ServiceItem, ServiceState, ServicesState,
};
use hearth_proto::stream::{ServicesMessage, Topic};
use serde_json::json;
use support::engine::{fast_timings, parts, parts_supervised, parts_with};
use support::fake_daemon::{DaemonContainer, FakeDaemon};
use support::fake_engine::{FakeEngine, container, ready_probe};
use support::fake_supervisor::{FakeSupervisor, running_unit, unit_named};
use support::https::{self, Agent};
use support::probe::{fast_stream, metering_with};
use support::ws::{self, WsClient};
use support::{Env, PASSWORD, env};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::writer::BoxMakeWriter;

async fn token(env: &Env, agent: &Agent, username: &str, role: Role) -> String {
    env.create(username, role).await;
    let reply = agent
        .request("POST", "/sessions")
        .json(&json!({ "username": username, "password": PASSWORD }))
        .send()
        .await;
    assert_eq!(reply.status, 201, "{:?}", reply.body);
    reply.body["token"].as_str().unwrap().to_owned()
}

async fn services_of(agent: &Agent, token: &str) -> (ServicesState, String) {
    let reply = agent.request("GET", "/services").token(token).send().await;
    assert_eq!(reply.status, 200, "{:?}", reply.body);
    (
        serde_json::from_value(reply.body.clone()).expect("ServicesState"),
        reply.text,
    )
}

/// Attend qu'une condition soit vraie (le registre suit le moteur en tâche de fond).
async fn eventually<F, Fut>(what: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let started = Instant::now();
    while !check().await {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "délai dépassé : {what}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn subscribed(agent: &Agent, token: &str) -> (WsClient, ServicesState) {
    let mut client = ws::open(agent).await;
    client.auth(token).await;
    client.subscribe(&[Topic::Services]).await;
    let ServicesMessage::ServicesSnapshot(snapshot) = client.next_services().await else {
        panic!("le services_snapshot vient en premier");
    };
    (client, snapshot)
}

fn names(state: &ServicesState) -> Vec<&str> {
    state
        .services
        .iter()
        .map(|service| service.name.as_str())
        .collect()
}

async fn ready_agent(env: &Env, engine: &Arc<FakeEngine>) -> Agent {
    let agent = https::start_engined(env, parts(engine.clone())).await;
    let registry = agent.services();
    eventually("le moteur est détecté prêt", || {
        let registry = registry.clone();
        async move {
            registry.engine_status().state == hearth_agent::domain::services::EngineState::Ready
        }
    })
    .await;
    agent
}

#[tokio::test]
async fn the_engine_task_stops_with_the_agent() {
    // Après l'arrêt de l'agent, plus rien ne sonde le moteur ni ne relit la liste (une tâche
    // abandonnée sans être arrêtée tournerait tant que le runtime vit).
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    engine.put_silently(container("web", RunState::Running));
    let agent = ready_agent(&env, &engine).await;
    let before = engine.probe_count();
    eventually("la sonde tourne", || {
        let engine = engine.clone();
        async move { engine.probe_count() > before + 2 }
    })
    .await;
    agent.shutdown().await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    let (probes, lists) = (engine.probe_count(), engine.list_count());
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(
        engine.probe_count(),
        probes,
        "plus aucune sonde après l'arrêt"
    );
    assert_eq!(
        engine.list_count(),
        lists,
        "plus aucune relecture après l'arrêt"
    );
}

#[tokio::test]
async fn hello_announces_the_services_capability() {
    let env = env().await;
    let agent = https::start_engined(&env, parts(Arc::new(FakeEngine::new()))).await;
    let reply = agent.request("GET", "/hello").send().await;
    assert_eq!(reply.status, 200);
    assert_eq!(reply.body["capabilities"], json!(["services"]));
    // L'interface ne monte pas : un client ancien parle encore à cet agent (ADR-0033, ADR-0038).
    assert_eq!(reply.body["api"], json!({ "min": 1, "max": 1 }));
    agent.shutdown().await;
}

#[tokio::test]
async fn both_roles_get_the_same_engine_and_services_sorted_by_name_stopped_ones_included() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    for (name, state) in [
        ("web", RunState::Running),
        ("Api", RunState::Exited),
        ("db", RunState::Created),
    ] {
        engine.put_silently(container(name, state));
    }
    let agent = ready_agent(&env, &engine).await;
    let registry = agent.services();
    eventually("la liste est lue", || {
        let registry = registry.clone();
        async move { registry.snapshot().services.len() == 3 }
    })
    .await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let readonly = token(&env, &agent, "lucas", Role::ReadOnly).await;

    let (as_admin, admin_text) = services_of(&agent, &admin).await;
    let (as_readonly, readonly_text) = services_of(&agent, &readonly).await;
    assert_eq!(as_admin.engine.state, EngineState::Ready);
    assert_eq!(names(&as_admin), vec!["Api", "db", "web"]);
    // Les deux rôles reçoivent la même chose, octet pour octet à la date de contrôle près.
    assert_eq!(as_admin.services, as_readonly.services);
    assert_eq!(as_admin.engine.state, as_readonly.engine.state);
    assert_eq!(as_admin.engine.host, as_readonly.engine.host);
    assert_eq!(as_admin.rev, as_readonly.rev);
    assert_eq!(admin_text.len(), readonly_text.len());
    // Le service jamais démarré, le service sorti et celui qui tourne.
    let by_name: BTreeMap<_, _> = as_admin
        .services
        .iter()
        .map(|service| (service.name.as_str(), service))
        .collect();
    assert_eq!(by_name["db"].state, ServiceState::Stopped);
    assert!(by_name["db"].never_started);
    assert_eq!(by_name["Api"].state, ServiceState::Stopped);
    assert_eq!(by_name["web"].state, ServiceState::Running);
    // L'état du moteur dit sur quel compte tourne l'agent (BR-ENGINE-008).
    assert_eq!(as_admin.engine.agent_account, "root");
    agent.shutdown().await;
}

#[tokio::test]
async fn reading_the_engine_and_the_services_writes_nothing_to_the_journal() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let agent = ready_agent(&env, &engine).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let readonly = token(&env, &agent, "lucas", Role::ReadOnly).await;
    let entries = || async {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM audit_events")
            .fetch_one(env.db.pool())
            .await
            .unwrap()
    };
    let before = entries().await;
    for token in [&admin, &readonly] {
        let (state, _) = services_of(&agent, token).await;
        assert_eq!(state.engine.state, EngineState::Ready);
        let recheck = agent
            .request("POST", "/engine/recheck")
            .token(token)
            .send()
            .await;
        assert_eq!(recheck.status, 200, "{:?}", recheck.body);
        assert_eq!(recheck.body["state"], "ready");
    }
    // Un abonnement au sujet `services` n'écrit rien non plus (BR-ENGINE-020, BR-SVC-024).
    let (_client, _snapshot) = subscribed(&agent, &admin).await;
    assert_eq!(entries().await, before);
    agent.shutdown().await;
}

#[tokio::test]
async fn an_unauthenticated_caller_is_refused_and_the_routes_check_the_interface_version() {
    let env = env().await;
    let agent = https::start_engined(&env, parts(Arc::new(FakeEngine::new()))).await;
    let reply = agent.request("GET", "/services").send().await;
    assert_eq!((reply.status, reply.code()), (401, "UNAUTHENTICATED"));
    let reply = agent.request("POST", "/engine/recheck").send().await;
    assert_eq!((reply.status, reply.code()), (401, "UNAUTHENTICATED"));
    let readonly = token(&env, &agent, "lucas", Role::ReadOnly).await;
    let reply = agent
        .request("GET", "/services")
        .header("x-hearth-api", "2")
        .token(&readonly)
        .send()
        .await;
    assert_eq!(reply.status, 426);
    agent.shutdown().await;
}

#[tokio::test]
async fn subscribing_sends_a_snapshot_with_the_engine_and_every_service_then_deltas_in_time() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    engine.put_silently(container("a-stopped", RunState::Exited));
    engine.put_silently(container("b-running", RunState::Running));
    let agent = ready_agent(&env, &engine).await;
    let registry = agent.services();
    eventually("la liste est lue", || {
        let registry = registry.clone();
        async move { registry.snapshot().services.len() == 2 }
    })
    .await;
    let readonly = token(&env, &agent, "lucas", Role::ReadOnly).await;
    let (mut client, snapshot) = subscribed(&agent, &readonly).await;
    assert_eq!(snapshot.engine.state, EngineState::Ready);
    assert_eq!(names(&snapshot), vec!["a-stopped", "b-running"]);

    // Un nouveau service : le delta arrive moins de 500 ms après l'événement (BR-SVC-015).
    let fresh = container("c-fresh", RunState::Running);
    let started = Instant::now();
    engine.put(fresh.clone(), "start");
    let ServicesMessage::ServicesDelta(delta) = client.next_services().await else {
        panic!("un services_delta était attendu");
    };
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "{:?}",
        started.elapsed()
    );
    assert!(delta.rev > snapshot.rev);
    assert_eq!(delta.upserts.len(), 1);
    assert_eq!(delta.upserts[0].name, "c-fresh");
    assert!(delta.removed.is_empty() && delta.engine.is_none());

    // Il disparaît : `removed`.
    engine.remove(&fresh.id);
    let ServicesMessage::ServicesDelta(delta) = client.next_services().await else {
        panic!("un services_delta était attendu");
    };
    assert_eq!(delta.removed, vec![format!("c_{}", fresh.id)]);
    assert!(delta.upserts.is_empty());
    agent.shutdown().await;
}

#[tokio::test]
async fn two_hundred_services_are_in_the_first_snapshot_in_under_a_second() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    for index in 0..200 {
        engine.put_silently(container(&format!("svc-{index:03}"), RunState::Running));
    }
    let agent = ready_agent(&env, &engine).await;
    let registry = agent.services();
    eventually("200 services lus", || {
        let registry = registry.clone();
        async move { registry.snapshot().services.len() == 200 }
    })
    .await;
    let readonly = token(&env, &agent, "lucas", Role::ReadOnly).await;
    let started = Instant::now();
    let (_client, snapshot) = subscribed(&agent, &readonly).await;
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(snapshot.services.len(), 200);
    assert_eq!(snapshot.services[0].name, "svc-000");
    assert_eq!(snapshot.services[199].name, "svc-199");
    agent.shutdown().await;
}

#[tokio::test]
async fn the_engine_state_changes_in_time_and_never_flickers() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let agent = ready_agent(&env, &engine).await;
    let readonly = token(&env, &agent, "lucas", Role::ReadOnly).await;
    let (mut client, snapshot) = subscribed(&agent, &readonly).await;
    assert_eq!(snapshot.engine.state, EngineState::Ready);

    // Le moteur oscille plus vite que la fenêtre (250 ms dans ces tests) : au plus un changement par
    // fenêtre, jamais un aller-retour plus court qu'elle.
    let flapping = tokio::spawn({
        let engine = engine.clone();
        async move {
            for round in 0..20 {
                if round % 2 == 0 {
                    engine.set_outcome(ProbeOutcome::Refused, true);
                } else {
                    engine.set_probe(ready_probe());
                }
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
            engine.set_probe(ready_probe());
        }
    });
    // BR-ENGINE-013 vise les changements d'ÉTAT : un message qui porte le moteur sans que l'état
    // change (la marque de fraîcheur de la liste, par exemple) n'est pas un changement.
    let mut shown = snapshot.engine.state;
    let mut changes = Vec::new();
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(1_200) {
        if let Some(ServicesMessage::ServicesDelta(delta)) =
            client.try_next_services(Duration::from_millis(100)).await
            && let Some(engine) = delta.engine
            && engine.state != shown
        {
            shown = engine.state;
            changes.push((started.elapsed(), engine.state));
        }
    }
    flapping.await.unwrap();
    // 20 bascules en 600 ms : l'interface en a vu très peu, et jamais deux dans la même fenêtre.
    assert!(changes.len() <= 4, "{changes:?}");
    for pair in changes.windows(2) {
        assert!(
            pair[1].0 - pair[0].0 >= Duration::from_millis(200),
            "deux changements à moins d'une fenêtre : {changes:?}"
        );
    }
    // Il finit prêt.
    eventually("l'état final est « prêt »", || {
        let registry = agent.services();
        async move {
            registry.engine_status().state == hearth_agent::domain::services::EngineState::Ready
        }
    })
    .await;
    agent.shutdown().await;
}

#[tokio::test]
async fn a_missing_engine_is_absent_quietly_and_a_returning_engine_is_picked_up_alone() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    engine.set_outcome(ProbeOutcome::SocketMissing, false);
    let agent = https::start_engined(&env, parts(engine.clone())).await;
    let readonly = token(&env, &agent, "lucas", Role::ReadOnly).await;
    let registry = agent.services();
    eventually("l'état « absent »", || {
        let registry = registry.clone();
        async move {
            registry.engine_status().state == hearth_agent::domain::services::EngineState::Absent
        }
    })
    .await;
    let (state, _) = services_of(&agent, &readonly).await;
    assert_eq!(state.engine.state, EngineState::Absent);
    assert!(state.services.is_empty());
    // Aucune relance serrée : en 400 ms (période de 40 ms dans ces tests) la sonde reste raisonnable.
    let probes = engine.probe_count();
    tokio::time::sleep(Duration::from_millis(400)).await;
    let made = engine.probe_count() - probes;
    assert!((1..=15).contains(&made), "{made} sondes en 400 ms");
    assert_eq!(engine.list_count(), 0, "pas de relecture sans moteur");

    // Le moteur est installé après coup : l'état repasse à prêt tout seul, services lus.
    engine.put_silently(container("late", RunState::Running));
    engine.set_ready();
    eventually("le moteur est détecté sans rechargement", || {
        let agent = &agent;
        let token = readonly.clone();
        async move {
            let (state, _) = services_of(agent, &token).await;
            state.engine.state == EngineState::Ready && state.services.len() == 1
        }
    })
    .await;
    agent.shutdown().await;
}

#[tokio::test]
async fn a_broken_event_stream_triggers_a_probe_and_a_full_reread() {
    // Cadences LONGUES (3 s) : ni la sonde périodique ni aucune relecture de calage ne peut tomber
    // dans la fenêtre du test. Seule la réaction à la rupture du flux peut expliquer une sonde et une
    // relecture complète en moins d'une seconde.
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    engine.put_silently(container("one", RunState::Running));
    let mut timings = fast_timings();
    timings.ready_period = Duration::from_secs(3);
    timings.other_period = Duration::from_secs(3);
    let agent = https::start_engined(&env, parts_with(engine.clone(), timings)).await;
    let registry = agent.services();
    // Démarrage fini : sonde, relecture, relecture de calage (4 x 20 ms), flux ouvert.
    eventually("la relecture de calage est passée", || {
        let engine = engine.clone();
        async move { engine.list_count() >= 2 && engine.event_subscribers() > 0 }
    })
    .await;
    let probes = engine.probe_count();
    let lists = engine.list_count();
    // Pendant la rupture, le monde change sans événement.
    engine.put_silently(container("two", RunState::Running));
    let broken_at = Instant::now();
    engine.break_events();
    eventually(
        "le deuxième service est vu par la relecture complète",
        || {
            let registry = registry.clone();
            async move { registry.snapshot().services.len() == 2 }
        },
    )
    .await;
    assert!(
        broken_at.elapsed() < Duration::from_millis(1500),
        "la réaction doit précéder la période de 3 s : {:?}",
        broken_at.elapsed()
    );
    assert!(engine.probe_count() > probes, "sonde immédiate");
    assert!(engine.list_count() > lists, "relecture complète");
    agent.shutdown().await;
}

#[tokio::test]
async fn a_disabled_engine_is_told_as_disabled_and_the_socket_is_never_opened() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let mut config_parts = parts(engine.clone());
    config_parts.timings = fast_timings();
    // Le réglage `engine.enabled = false` : `start_engined` lit la configuration par défaut (permis),
    // on passe donc par un agent démarré avec la configuration « désactivé ».
    let agent = https::start_engine_disabled(&env, config_parts).await;
    let readonly = token(&env, &agent, "lucas", Role::ReadOnly).await;
    let (state, _) = services_of(&agent, &readonly).await;
    assert_eq!(state.engine.state, EngineState::Disabled);
    assert!(state.services.is_empty());
    let recheck = agent
        .request("POST", "/engine/recheck")
        .token(&readonly)
        .send()
        .await;
    assert_eq!(recheck.body["state"], "disabled");
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(engine.probe_count(), 0, "le socket n'est jamais ouvert");
    assert_eq!(engine.list_count(), 0);
    agent.shutdown().await;
}

// Exécuteur à un seul fil : la rafale ci-dessous ne rend jamais la main, la tâche de la connexion ne
// lit rien pendant ce temps et son récepteur prend 400 lots de retard sur une file de 64.
#[tokio::test(flavor = "current_thread")]
async fn a_slow_subscriber_is_resynchronised_with_a_new_snapshot() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    // 30 services au nom très long : chaque lot pèse des dizaines de kilo-octets.
    let long = "x".repeat(240);
    let ids: Vec<String> = (0..30)
        .map(|index| {
            let observation = container(&format!("{long}-{index:02}"), RunState::Running);
            let id = observation.id.clone();
            engine.put_silently(observation);
            id
        })
        .collect();
    // Aucune limite d'envoi courte : le client bloqué reste connecté pendant tout le test.
    let mut stream = fast_stream();
    stream.send_timeout = Duration::from_secs(60);
    stream.idle_timeout = Duration::from_secs(120);
    let agent = {
        // Le registre est démarré par `ready_agent`, mais avec ces délais de flux.
        let metering = metering_with(stream);
        let agent = https::start_engined_metered(&env, parts(engine.clone()), metering).await;
        let registry = agent.services();
        eventually("30 services lus", || {
            let registry = registry.clone();
            async move { registry.snapshot().services.len() == 30 }
        })
        .await;
        agent
    };
    let readonly = token(&env, &agent, "lucas", Role::ReadOnly).await;

    // Un client abonné, dont l'instantané de départ est lu ; puis il ne lit plus rien pendant la rafale.
    let (mut stuck, first) = subscribed(&agent, &readonly).await;
    assert_eq!(first.services.len(), 30);

    // La rafale : des centaines de lots énormes, sans attendre les fenêtres des événements.
    let registry = agent.services();
    for round in 0..400 {
        let state = if round % 2 == 0 {
            RunState::Exited
        } else {
            RunState::Running
        };
        for id in &ids {
            engine.update(id, |observation| observation.run_state = state);
        }
        registry.resync().await.expect("relecture");
    }
    let last = registry.snapshot();
    assert!(
        last.rev > first.rev + 64,
        "la rafale doit dépasser la file de diffusion"
    );

    // Le client se remet à lire : il reçoit un nouvel instantané, puis des lots, et finit exactement
    // dans l'état du registre.
    let mut services: BTreeMap<String, ServiceItem> = BTreeMap::new();
    let mut snapshots = 0;
    let mut rev = first.rev;
    while rev < last.rev {
        match stuck.next_services().await {
            ServicesMessage::ServicesSnapshot(snapshot) => {
                snapshots += 1;
                rev = snapshot.rev;
                services = snapshot
                    .services
                    .into_iter()
                    .map(|service| (service.key.clone(), service))
                    .collect();
            }
            ServicesMessage::ServicesDelta(delta) => {
                rev = delta.rev;
                for key in &delta.removed {
                    services.remove(key);
                }
                for service in delta.upserts {
                    services.insert(service.key.clone(), service);
                }
            }
            ServicesMessage::ServiceAction(_) => {
                panic!("aucune action dans une tranche en lecture seule")
            }
        }
    }
    assert!(
        snapshots >= 1,
        "le client lent doit recevoir un nouvel instantané ({snapshots})"
    );
    let expected: BTreeMap<String, ServiceState> = last
        .services
        .iter()
        .map(|view| {
            (
                view.key.as_str().to_owned(),
                hearth_agent::entrypoint::services_wire::service_item(view).state,
            )
        })
        .collect();
    let got: BTreeMap<String, ServiceState> = services
        .iter()
        .map(|(key, service)| (key.clone(), service.state))
        .collect();
    assert_eq!(got, expected);
    agent.shutdown().await;
}

#[tokio::test]
async fn rev_grows_with_every_change_and_starts_again_with_the_agent() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let agent = ready_agent(&env, &engine).await;
    let registry = agent.services();
    let first = registry.snapshot().rev;
    assert!(first >= 1, "l'état « prêt » est déjà un changement");
    engine.put(container("one", RunState::Running), "start");
    eventually("rev a grandi", || {
        let registry = registry.clone();
        async move { registry.snapshot().rev > first }
    })
    .await;
    agent.shutdown().await;

    // Un autre agent, autre registre : `rev` repart de zéro.
    let env2 = self::env().await;
    let fresh = https::start_engined(
        &env2,
        parts_with(Arc::new(FakeEngine::new()), fast_timings()),
    )
    .await;
    let at_start = fresh.services().snapshot().rev;
    assert!(at_start <= 1, "rev = {at_start}");
    fresh.shutdown().await;
}

#[tokio::test]
async fn a_client_that_does_not_subscribe_to_services_never_receives_them() {
    // Un client ancien ne connaît pas le sujet : il s'abonne comme avant et ne voit rien de nouveau.
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let agent = ready_agent(&env, &engine).await;
    let readonly = token(&env, &agent, "lucas", Role::ReadOnly).await;
    let mut old_client = ws::open(&agent).await;
    old_client.auth(&readonly).await;
    old_client
        .subscribe(&[Topic::Metrics, Topic::Session, Topic::Update])
        .await;
    engine.put(container("late", RunState::Running), "start");
    assert!(
        old_client
            .try_next_services(Duration::from_millis(400))
            .await
            .is_none(),
        "aucun message services sans abonnement au sujet"
    );
    agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Aucune valeur de variable ne voyage : ni dans les messages, ni dans les traces
// ---------------------------------------------------------------------------------------------

/// Les traces de tout le binaire de test, capturées en mémoire. Elles passent par le MÊME chemin
/// d'initialisation que la production (`logging::try_init_with` : filtre de sécurité et pont `log`
/// vers `tracing` compris), au niveau le plus bavard, avec une directive qui vise `bollard` lui-même.
fn captured_traces() -> Arc<Mutex<Vec<u8>>> {
    #[derive(Clone)]
    struct Sink(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Sink {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Sink {
        type Writer = Sink;
        fn make_writer(&'a self) -> Sink {
            self.clone()
        }
    }
    static TRACES: OnceLock<Arc<Mutex<Vec<u8>>>> = OnceLock::new();
    TRACES
        .get_or_init(|| {
            let buffer = Arc::new(Mutex::new(Vec::new()));
            logging::try_init_with(
                BoxMakeWriter::new(Sink(buffer.clone())),
                LogFormat::Json,
                false,
                EnvFilter::new("trace,bollard=trace,bollard::docker=trace,hyper_util=trace"),
            )
            .expect("première installation du journal de ce binaire de test");
            buffer
        })
        .clone()
}

#[tokio::test]
async fn no_variable_value_reaches_a_message_a_response_or_a_trace_and_no_name_or_command_reaches_a_trace()
 {
    let traces = captured_traces();
    // Témoins : sans eux, un tampon vide ou un pont `log` absent rendraient le test aveugle. Une
    // ligne `log` ordinaire doit arriver ; une ligne `log` de `bollard` sous `warn` ne doit pas ;
    // au niveau `warn` elle arrive.
    log::debug!(target: "hearth_witness", "witness-log-line");
    log::debug!(target: "bollard::docker", "witness-bollard-debug");
    log::warn!(target: "bollard::docker", "witness-bollard-warn");
    let env = env().await;
    // Le VRAI adaptateur `bollard` sur un faux démon dont les réponses d'inspection portent une
    // variable d'environnement sentinelle ET une commande sentinelle.
    let daemon = FakeDaemon::start().await;
    let mut container = DaemonContainer::new("secretive", "running");
    container.env = vec![
        "DB_PASSWORD=SENTINEL-VALUE-91c2".to_owned(),
        "SENTINEL_NAME_77=plain".to_owned(),
    ];
    container.cmd = vec![
        "run-server".to_owned(),
        "--token=SENTINEL-CMD-5d1e".to_owned(),
    ];
    container.publish = Some((8080, 80));
    daemon.put(container);
    let engine = BollardEngine::new(
        Target::Http(daemon.address()),
        InstallMarkers::at(Vec::<std::path::PathBuf>::new()),
    );
    let agent = https::start_engined(
        &env,
        EngineParts {
            engine: Arc::new(engine),
            supervisor: Arc::new(FakeSupervisor::new()),
            host: Arc::new(support::engine::TestHost),
            timings: fast_timings(),
        },
    )
    .await;
    let registry = agent.services();
    eventually("le service est lu", || {
        let registry = registry.clone();
        async move { registry.snapshot().services.len() == 1 }
    })
    .await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let readonly = token(&env, &agent, "lucas", Role::ReadOnly).await;

    let mut texts = Vec::new();
    for (round, token) in [&admin, &readonly].into_iter().enumerate() {
        let (state, text) = services_of(&agent, token).await;
        assert_eq!(state.services.len(), 1);
        texts.push(text);
        let recheck = agent
            .request("POST", "/engine/recheck")
            .token(token)
            .send()
            .await;
        texts.push(recheck.text);
        let (mut client, snapshot) = subscribed(&agent, token).await;
        texts.push(serde_json::to_string(&snapshot).unwrap());
        // Un changement du service fait aussi partir un delta : il ne porte rien de plus.
        // Un état différent à chaque tour, pour qu'il y ait toujours un changement à annoncer.
        let status = if round % 2 == 0 { "exited" } else { "running" };
        daemon.update(snapshot.services[0].key.trim_start_matches("c_"), |c| {
            c.status = status.into();
        });
        daemon.emit(snapshot.services[0].key.trim_start_matches("c_"), "die");
        let message = client.next_services().await;
        texts.push(serde_json::to_string(&message).unwrap());
    }
    // Les VALEURS ne voyagent jamais dans la liste, ni dans un delta, ni dans une relance (elles ne se
    // lisent que par `GET /services/{key}/env`). Les NOMS et la commande, eux, sont dans le détail
    // d'un service pour les deux rôles (BR-SVC-007, BR-SVC-008, BR-SVC-020) : on le prouve, pour que
    // l'absence de la valeur ne soit pas un test aveugle.
    for text in &texts {
        assert!(
            !text.contains("SENTINEL-VALUE-91c2"),
            "une valeur dans {text}"
        );
    }
    assert!(texts[0].contains("DB_PASSWORD") && texts[0].contains("SENTINEL_NAME_77"));
    assert!(texts[0].contains("run-server"));
    agent.shutdown().await;
    let captured = String::from_utf8_lossy(&traces.lock().unwrap()).into_owned();
    assert!(
        captured.contains("witness-log-line"),
        "le pont `log` vers `tracing` doit être actif dans ce test : {captured}"
    );
    for forbidden in [
        "SENTINEL-VALUE-91c2",
        "DB_PASSWORD",
        "SENTINEL_NAME_77",
        "SENTINEL-CMD-5d1e",
        "run-server",
    ] {
        assert!(!captured.contains(forbidden), "{forbidden} dans les traces");
    }
    assert!(captured.contains("witness-bollard-warn"));
    assert!(
        !captured.contains("witness-bollard-debug"),
        "`bollard` ne sort pas sous `warn`"
    );
}

// ---------------------------------------------------------------------------------------------
// Détail d'un service, services supervisés, révélation des variables (HRT-63)
// ---------------------------------------------------------------------------------------------

/// Un conteneur avec son détail : noms de variables, commande, délai d'arrêt déclaré.
fn detailed(name: &str, state: RunState) -> hearth_agent::domain::services::Observation {
    let mut observation = container(name, state);
    observation.env_names = vec![Hidden::new("APP_MODE"), Hidden::new("DB_PASSWORD")];
    observation.command = Some(vec![Hidden::new("/entrypoint.sh"), Hidden::new("--serve")]);
    observation.stop_timeout_s = Some(30);
    observation
}

/// Le conteneur d'un autre identifiant que le système recrée à chaque démarrage.
fn recreated(
    name: &str,
    state: RunState,
    id_char: char,
) -> hearth_agent::domain::services::Observation {
    let mut observation = detailed(name, state);
    observation.id = id_char.to_string().repeat(64);
    observation
}

async fn listed(agent: &Agent, count: usize) {
    let registry = agent.services();
    eventually("la liste est lue", || {
        let registry = registry.clone();
        async move { registry.snapshot().services.len() == count }
    })
    .await;
}

/// Compte, résultat, cible, raison, libellé d'une entrée du journal.
type JournalRow = (String, String, Option<String>, Option<String>, String);

fn all_bytes(dir: &std::path::Path) -> Vec<u8> {
    let mut bytes = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            bytes.extend(all_bytes(&path));
        } else if let Ok(content) = std::fs::read(&path) {
            bytes.extend(content);
        }
    }
    bytes
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

#[tokio::test]
async fn the_detail_of_a_service_has_names_command_and_grace_for_both_roles_and_never_a_value() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let web = detailed("web", RunState::Running);
    engine.put_silently(web.clone());
    engine.set_env(
        &web.id,
        &[("APP_MODE", "prod"), ("DB_PASSWORD", "SENTINEL-VALUE-A")],
    );
    let agent = ready_agent(&env, &engine).await;
    listed(&agent, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let readonly = token(&env, &agent, "lucas", Role::ReadOnly).await;
    let mut seen = Vec::new();
    for token in [&admin, &readonly] {
        let (state, text) = services_of(&agent, token).await;
        let item = &state.services[0];
        let names: Vec<&str> = item.env_names.iter().map(Concealed::expose).collect();
        assert_eq!(names, ["APP_MODE", "DB_PASSWORD"]);
        let command: Vec<&str> = item
            .command
            .iter()
            .flatten()
            .map(Concealed::expose)
            .collect();
        assert_eq!(command, ["/entrypoint.sh", "--serve"]);
        assert_eq!(item.grace_s, Some(30));
        assert!(!text.contains("SENTINEL-VALUE-A"), "une valeur dans {text}");
        seen.push(item.clone());
    }
    assert_eq!(seen[0], seen[1], "les deux rôles voient le même détail");
    assert_eq!(
        engine.env_read_count(),
        0,
        "aucune valeur n'est lue sans demande de révélation"
    );
    agent.shutdown().await;
}

#[tokio::test]
async fn a_service_run_by_a_unit_is_served_under_its_unit_and_stays_listed_when_its_container_goes()
{
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let supervisor = Arc::new(FakeSupervisor::new());
    let unit = unit_named("docker-terraria.service");
    let first = recreated("terraria", RunState::Running, 'a');
    supervisor.set_owner(&first.id, &unit);
    supervisor.set_status(&unit, running_unit());
    engine.put_silently(first.clone());
    let agent = https::start_engined(
        &env,
        parts_supervised(engine.clone(), supervisor.clone(), fast_timings()),
    )
    .await;
    listed(&agent, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let (mut client, snapshot) = subscribed(&agent, &admin).await;
    let item = &snapshot.services[0];
    assert_eq!(item.key, "u_docker-terraria.service");
    assert_eq!(item.driver, Some(ServiceDriver::Supervisor));
    assert_eq!(item.unit.as_deref(), Some("docker-terraria.service"));
    assert_eq!(item.restart, RestartMode::Supervisor);
    assert_eq!(item.grace_s, Some(120), "le délai de l'unité, pas 10 s");
    assert_eq!(item.state, ServiceState::Running);

    // Le système arrête le service (`--rm` : le conteneur disparaît) et constate l'échec.
    supervisor.set_status(
        &unit,
        UnitStatus {
            activity: UnitActivity::Failed,
            auto_restart: false,
            restarts: true,
            timeout_stop_s: Some(120),
        },
    );
    engine.remove(&first.id);
    eventually("le service reste listé, en erreur", || {
        let agent = &agent;
        let admin = &admin;
        async move {
            let (state, _) = services_of(agent, admin).await;
            state.services.len() == 1
                && state.services[0].key == "u_docker-terraria.service"
                && state.services[0].state == ServiceState::Error
        }
    })
    .await;
    // Le delta qui l'annonce garde la même clé, jamais une disparition suivie d'une apparition.
    let mut removed_seen = false;
    let mut error_seen = false;
    while !error_seen {
        let ServicesMessage::ServicesDelta(delta) = client.next_services().await else {
            continue;
        };
        removed_seen |= delta.removed.iter().any(|key| key.contains("terraria"));
        error_seen = delta.upserts.iter().any(|item| {
            item.key == "u_docker-terraria.service" && item.state == ServiceState::Error
        });
    }
    assert!(!removed_seen, "le service n'a jamais disparu de la liste");
    let (state, text) = services_of(&agent, &admin).await;
    let gone = &state.services[0];
    assert!(gone.env_names.is_empty() && gone.command.is_none());
    assert_eq!(gone.name, "terraria");
    assert!(!text.contains("DB_PASSWORD"), "rien du détail n'est retenu");

    // Un homonyme créé ensuite n'est pas confondu avec lui (BR-SVC-016).
    engine.put(recreated("terraria", RunState::Running, 'c'), "start");
    listed(&agent, 2).await;
    let (state, _) = services_of(&agent, &admin).await;
    let mut keys: Vec<&str> = state.services.iter().map(|s| s.key.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(keys.len(), 2);
    assert!(keys[0].starts_with("c_") && keys[1] == "u_docker-terraria.service");

    // Le système relance le service : un autre conteneur, la même clé, de nouveau en marche.
    let second = recreated("terraria", RunState::Running, 'b');
    supervisor.set_owner(&second.id, &unit);
    supervisor.set_status(&unit, running_unit());
    engine.put(second.clone(), "start");
    eventually("le service relancé est de nouveau en marche", || {
        let agent = &agent;
        let admin = &admin;
        async move {
            let (state, _) = services_of(agent, admin).await;
            state
                .services
                .iter()
                .any(|s| s.key == "u_docker-terraria.service" && s.state == ServiceState::Running)
        }
    })
    .await;
    agent.shutdown().await;
}

#[tokio::test]
async fn only_an_administrator_reveals_the_values_and_every_attempt_is_journaled_without_them() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let web = detailed("web", RunState::Running);
    engine.put_silently(web.clone());
    engine.set_env(
        &web.id,
        &[("APP_MODE", "prod"), ("DB_PASSWORD", "SENTINEL-VALUE-A")],
    );
    let agent = ready_agent(&env, &engine).await;
    listed(&agent, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let readonly = token(&env, &agent, "lucas", Role::ReadOnly).await;
    let path = format!("/services/c_{}/env", web.id);

    // Un compte en lecture seule est refusé, même client modifié : rien n'est lu dans le moteur.
    let denied = agent.request("GET", &path).token(&readonly).send().await;
    assert_eq!(denied.status, 403, "{:?}", denied.body);
    assert_eq!(denied.code(), "FORBIDDEN_ROLE");
    assert_eq!(
        denied.body["error"]["message"],
        "Réservé aux administrateurs."
    );
    assert!(!denied.text.contains("SENTINEL") && !denied.text.contains("DB_PASSWORD"));
    assert_eq!(engine.env_read_count(), 0);
    // Sans session : refusé avant tout.
    let anonymous = agent.request("GET", &path).send().await;
    assert_eq!(anonymous.status, 401);

    // L'administrateur reçoit les valeurs, dans une réponse que rien ne stocke.
    let revealed = agent.request("GET", &path).token(&admin).send().await;
    assert_eq!(revealed.status, 200, "{:?}", revealed.body);
    assert_eq!(revealed.header("cache-control"), Some("no-store"));
    assert_eq!(revealed.header("pragma"), Some("no-cache"));
    assert_eq!(
        revealed.body,
        json!({ "variables": [
            { "name": "APP_MODE", "value": "prod" },
            { "name": "DB_PASSWORD", "value": "SENTINEL-VALUE-A" }
        ]})
    );
    assert_eq!(engine.env_read_count(), 1);

    // Un service inconnu, une clé mal formée : 404, sans répéter la clé.
    for bad in ["c_00ff", "x_not-a-key", "u_docker-ghost.service"] {
        let reply = agent
            .request("GET", &format!("/services/{bad}/env"))
            .token(&admin)
            .send()
            .await;
        assert_eq!(reply.status, 404, "{bad} : {:?}", reply.body);
        assert!(
            !reply.text.contains(bad),
            "la clé est répétée : {}",
            reply.text
        );
    }
    assert_eq!(engine.env_read_count(), 1, "rien de plus n'est lu");

    // Le journal : le refus (cible : la route) et la réussite (cible : le service), sans nom ni valeur.
    let rows: Vec<JournalRow> = sqlx::query_as(
        "SELECT account, outcome, target, reason, action_label FROM audit_events
         WHERE action = 'service.env.reveal' ORDER BY id",
    )
    .fetch_all(env.db.pool())
    .await
    .unwrap();
    assert_eq!(
        rows,
        vec![
            (
                "lucas".to_owned(),
                "denied".to_owned(),
                Some("/services/{key}/env".to_owned()),
                Some("lecture seule".to_owned()),
                "Valeurs des variables révélées".to_owned()
            ),
            (
                "marie".to_owned(),
                "ok".to_owned(),
                Some("service web".to_owned()),
                None,
                "Valeurs des variables révélées".to_owned()
            ),
        ]
    );
    // Rien de secret nulle part dans les fichiers de la base.
    agent.shutdown().await;
    let bytes = all_bytes(env.dir.path());
    for secret in ["SENTINEL-VALUE-A", "DB_PASSWORD", "APP_MODE"] {
        assert!(!contains(&bytes, secret), "{secret} dans la base");
    }
}

#[tokio::test]
async fn a_revealed_value_reaches_only_the_admin_response_never_a_trace_the_journal_or_the_database()
 {
    let traces = captured_traces();
    let env = env().await;
    // Le VRAI adaptateur `bollard` sur un faux démon dont l'inspection porte une valeur sentinelle.
    let daemon = FakeDaemon::start().await;
    let mut container = DaemonContainer::new("vault", "running");
    container.env = vec![
        "DB_PASSWORD=SENTINEL-REVEAL-VALUE-44ab".to_owned(),
        "SENTINEL_REVEAL_NAME_12=plain".to_owned(),
    ];
    container.cmd = vec![
        "serve".to_owned(),
        "--token=SENTINEL-REVEAL-CMD-9f".to_owned(),
    ];
    let key = format!("c_{}", container.id);
    daemon.put(container);
    let engine = BollardEngine::new(
        Target::Http(daemon.address()),
        InstallMarkers::at(Vec::<std::path::PathBuf>::new()),
    );
    let agent = https::start_engined(
        &env,
        EngineParts {
            engine: Arc::new(engine),
            supervisor: Arc::new(FakeSupervisor::new()),
            host: Arc::new(support::engine::TestHost),
            timings: fast_timings(),
        },
    )
    .await;
    listed(&agent, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let readonly = token(&env, &agent, "lucas", Role::ReadOnly).await;
    let path = format!("/services/{key}/env");

    // Réussite, refus, service inconnu, requête sans session.
    let revealed = agent.request("GET", &path).token(&admin).send().await;
    assert_eq!(revealed.status, 200, "{:?}", revealed.body);
    assert!(
        revealed.text.contains("SENTINEL-REVEAL-VALUE-44ab"),
        "le témoin : l'administrateur reçoit bien la valeur"
    );
    let mut elsewhere = Vec::new();
    for reply in [
        agent.request("GET", &path).token(&readonly).send().await,
        agent
            .request("GET", "/services/c_00ff/env")
            .token(&admin)
            .send()
            .await,
        agent.request("GET", &path).send().await,
        agent
            .request("GET", "/services")
            .token(&readonly)
            .send()
            .await,
    ] {
        elsewhere.push(reply.text);
    }
    let (mut client, snapshot) = subscribed(&agent, &readonly).await;
    elsewhere.push(serde_json::to_string(&snapshot).unwrap());
    daemon.update(key.trim_start_matches("c_"), |c| c.status = "exited".into());
    daemon.emit(key.trim_start_matches("c_"), "die");
    elsewhere.push(serde_json::to_string(&client.next_services().await).unwrap());
    for text in &elsewhere {
        assert!(
            !text.contains("SENTINEL-REVEAL-VALUE-44ab"),
            "une valeur hors de la révélation : {text}"
        );
    }
    agent.shutdown().await;

    // Ni le journal d'activité ni les fichiers de la base ne contiennent valeur, nom ni commande.
    let journal: Vec<String> = sqlx::query_scalar(
        "SELECT COALESCE(account, '') || '|' || COALESCE(origin_name, '') || '|' || action_label || '|'
                || COALESCE(target, '') || '|' || COALESCE(reason, '') || '|' || COALESCE(details, '')
         FROM audit_events",
    )
    .fetch_all(env.db.pool())
    .await
    .unwrap();
    assert!(journal.iter().any(|line| line.contains("service vault")));
    let bytes = all_bytes(env.dir.path());
    for forbidden in [
        "SENTINEL-REVEAL-VALUE-44ab",
        "DB_PASSWORD",
        "SENTINEL_REVEAL_NAME_12",
        "SENTINEL-REVEAL-CMD-9f",
    ] {
        assert!(
            journal.iter().all(|line| !line.contains(forbidden)),
            "{forbidden} dans le journal"
        );
        assert!(!contains(&bytes, forbidden), "{forbidden} dans la base");
    }
    // Les traces, au niveau le plus bavard, pont `log` de `bollard` compris : ni la valeur, ni le
    // nom, ni la commande.
    let captured = String::from_utf8_lossy(&traces.lock().unwrap()).into_owned();
    assert!(!captured.is_empty(), "le tampon des traces n'est pas vide");
    for forbidden in [
        "SENTINEL-REVEAL-VALUE-44ab",
        "DB_PASSWORD",
        "SENTINEL_REVEAL_NAME_12",
        "SENTINEL-REVEAL-CMD-9f",
        "--token=",
    ] {
        if let Some(at) = captured.find(forbidden) {
            let from = captured[..at]
                .char_indices()
                .rev()
                .nth(300)
                .map_or(0, |(i, _)| i);
            panic!(
                "{forbidden} dans les traces : ...{}",
                &captured[from..(at + 200).min(captured.len())]
            );
        }
    }
}
