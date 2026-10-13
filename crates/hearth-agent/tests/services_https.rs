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

/// Adopte en base (HRT-79) tous les conteneurs que le faux moteur connaît maintenant : le registre ne
/// publie que les services créés par Hearth et les adoptés, et la plupart de ces tests parlent de ce
/// que les services montrent, pas de ce qui les rend publiables (voir les tests de publication).
async fn adopt_all(env: &Env, engine: &FakeEngine) {
    for id in engine.container_ids() {
        env.adopt_container(&id, "adopté").await;
    }
}

/// Démarre l'agent après avoir adopté ce que le moteur contient déjà.
async fn started(env: &Env, engine: &Arc<FakeEngine>, engine_parts: EngineParts) -> Agent {
    adopt_all(env, engine).await;
    https::start_engined(env, engine_parts).await
}

/// Adopte maintenant un conteneur apparu après le démarrage et le fait prendre en compte.
async fn adopt_now(
    env: &Env,
    agent: &Agent,
    observation: &hearth_agent::domain::services::Observation,
) {
    env.adopt_container(&observation.id, &observation.name)
        .await;
    agent.services().reload_adopted().await;
}

async fn ready_agent(env: &Env, engine: &Arc<FakeEngine>) -> Agent {
    let agent = started(env, engine, parts(engine.clone())).await;
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
    assert_eq!(
        reply.body["capabilities"],
        json!([
            "services",
            "service-rights",
            "service-logs",
            "service-adoption"
        ])
    );
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

    // Un nouveau conteneur n'est pas publié tant qu'il n'est pas adopté : il n'est que compté.
    let fresh = container("c-fresh", RunState::Running);
    engine.put(fresh.clone(), "start");
    let ServicesMessage::ServicesDelta(counted) = client.next_services().await else {
        panic!("un services_delta était attendu");
    };
    assert!(counted.upserts.is_empty() && counted.removed.is_empty());
    assert_eq!(counted.engine.as_ref().map(|e| e.external_count), Some(1));
    // Une fois adopté, le service apparaît : le delta arrive moins de 500 ms après (BR-SVC-015).
    let started = Instant::now();
    adopt_now(&env, &agent, &fresh).await;
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
    assert!(delta.removed.is_empty());
    assert_eq!(delta.engine.as_ref().map(|e| e.external_count), Some(0));

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
    let agent = started(&env, &engine, parts(engine.clone())).await;
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
    let late = container("late", RunState::Running);
    engine.put_silently(late.clone());
    // Le moteur est absent : aucune liste n'est tenue, l'adoption ne tombe pas (BR-ADOPT-017).
    adopt_now(&env, &agent, &late).await;
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
    let agent = started(&env, &engine, parts_with(engine.clone(), timings)).await;
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
    let two = container("two", RunState::Running);
    engine.put_silently(two.clone());
    let broken_at = Instant::now();
    engine.break_events();
    // Le conteneur n'est pas adopté : la relecture complète le voit, et ne fait que le compter.
    eventually(
        "le deuxième conteneur est vu par la relecture complète",
        || {
            let registry = registry.clone();
            async move { registry.engine_status().external_count == 1 }
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
    adopt_now(&env, &agent, &two).await;
    listed(&agent, 2).await;
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
    for id in &ids {
        env.adopt_container(id, "long").await;
    }
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
                hearth_agent::entrypoint::services_wire::service_item(
                    view,
                    hearth_agent::entrypoint::services_wire::Reader {
                        may_manage_services: true,
                    },
                )
                .state,
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
    // Adopté, pour qu'il soit publié (HRT-79) ; l'administrateur a le droit, il en lit donc le détail.
    env.adopt(&format!("c_{}", container.id), "secretive", "marie")
        .await;
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
            log_brake: hearth_agent::domain::services::BrakeSettings::relaxed(),
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
    env.grant_services("marie").await;

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
    // lisent que par `GET /services/{key}/env`). Les NOMS et la commande d'un service adopté sont dans
    // le détail pour qui a le droit seulement (BR-ADOPT-015) : on le prouve pour le compte qui l'a,
    // pour que l'absence de la valeur ne soit pas un test aveugle.
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

/// Toutes les cellules de toutes les tables de la base, en texte (`quote()` de SQLite) : de quoi chercher une
/// valeur où qu'elle soit écrite, pas seulement dans les tables qu'on pense à regarder.
async fn dump_tables(env: &Env) -> String {
    dump_pool(env.db.pool()).await
}

async fn dump_pool(pool: &sqlx::SqlitePool) -> String {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let mut out = String::new();
    for table in tables {
        let columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info(?)")
            .bind(&table)
            .fetch_all(pool)
            .await
            .unwrap();
        if columns.is_empty() {
            continue;
        }
        let cells = columns
            .iter()
            .map(|column| format!("quote(\"{column}\")"))
            .collect::<Vec<_>>()
            .join(" || ',' || ");
        // Les noms de tables et de colonnes viennent de `sqlite_master` : aucune saisie.
        let rows: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT {cells} FROM \"{table}\""
        )))
        .fetch_all(pool)
        .await
        .unwrap();
        out.push_str(&format!("[{table}] {} lignes\n", rows.len()));
        for row in rows {
            out.push_str(&row);
            out.push('\n');
        }
    }
    out
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

#[tokio::test]
async fn the_command_and_variable_names_of_an_adopted_service_are_for_the_right_only_the_rest_is_for_all_and_no_value_is_sent()
 {
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
    // A : administrateur avec le droit ; B : administrateur sans ; C : lecture seule sans ; D : lecture
    // seule avec (BR-GRANT-001). Le rôle ne décide de rien (BR-ADOPT-015).
    let a = token(&env, &agent, "ada", Role::Admin).await;
    let b = token(&env, &agent, "bob", Role::Admin).await;
    let c = token(&env, &agent, "cleo", Role::ReadOnly).await;
    let d = token(&env, &agent, "dan", Role::ReadOnly).await;
    env.grant_services("ada").await;
    env.grant_services("dan").await;
    let mut seen = Vec::new();
    for (who, token, right) in [
        ("ada", &a, true),
        ("bob", &b, false),
        ("cleo", &c, false),
        ("dan", &d, true),
    ] {
        let (state, text) = services_of(&agent, token).await;
        let item = &state.services[0];
        let names: Vec<&str> = item.env_names.iter().map(Concealed::expose).collect();
        let command: Vec<&str> = item
            .command
            .iter()
            .flatten()
            .map(Concealed::expose)
            .collect();
        if right {
            assert_eq!(names, ["APP_MODE", "DB_PASSWORD"], "{who}");
            assert_eq!(command, ["/entrypoint.sh", "--serve"], "{who}");
            assert!(!item.details_reserved, "{who}");
        } else {
            assert!(names.is_empty() && command.is_empty(), "{who} : {text}");
            assert!(item.details_reserved, "{who}");
            assert!(
                !text.contains("APP_MODE") && !text.contains("entrypoint"),
                "{who} : {text}"
            );
        }
        // Tout le reste du détail et le repère d'adoption sont ouverts à tous.
        assert_eq!(item.grace_s, Some(30));
        assert_eq!(item.adopted.as_ref().map(|a| a.by.as_str()), Some("marie"));
        assert!(!text.contains("SENTINEL-VALUE-A"), "une valeur dans {text}");
        let mut open = item.clone();
        open.env_names.clear();
        open.command = None;
        open.details_reserved = false;
        seen.push(open);
    }
    assert!(
        seen.windows(2).all(|pair| pair[0] == pair[1]),
        "les quatre profils voient le même service, détail réservé mis à part"
    );
    assert_eq!(
        engine.env_read_count(),
        0,
        "aucune valeur n'est lue sans demande de révélation"
    );
    agent.shutdown().await;
}

/// Ce que le message d'un abonné porte du détail réservé d'un service adopté : les noms, la commande.
const DETAIL_TEXTS: [&str; 4] = ["APP_MODE", "DB_PASSWORD", "/entrypoint.sh", "--serve"];

fn assert_detail(who: &str, what: &str, text: &str, present: bool) {
    for needle in DETAIL_TEXTS {
        assert_eq!(
            text.contains(needle),
            present,
            "{who} / {what} : {needle} {} dans {text}",
            if present { "manque" } else { "présent" }
        );
    }
}

/// Le prochain `services_snapshot` d'un abonné (les lots qui le précèdent sont lus et rendus).
async fn next_snapshot(client: &mut WsClient) -> (ServicesState, Vec<ServicesMessage>) {
    let mut before = Vec::new();
    loop {
        match client.try_next_services(Duration::from_secs(3)).await {
            Some(ServicesMessage::ServicesSnapshot(snapshot)) => return (snapshot, before),
            Some(other) => before.push(other),
            None => panic!("aucun instantané dans le délai (lots reçus : {before:?})"),
        }
    }
}

#[tokio::test]
async fn the_stream_is_composed_for_each_reader_so_the_detail_of_an_adopted_service_reaches_only_the_right()
 {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let web = detailed("web", RunState::Running);
    engine.put_silently(web.clone());
    let agent = ready_agent(&env, &engine).await;
    listed(&agent, 1).await;
    // Deux comptes abonnés EN MÊME TEMPS : ada a le droit, bob (administrateur) ne l'a pas.
    let ada = token(&env, &agent, "ada", Role::Admin).await;
    let bob = token(&env, &agent, "bob", Role::Admin).await;
    env.grant_services("ada").await;
    let (mut ws_ada, snapshot_ada) = subscribed(&agent, &ada).await;
    let (mut ws_bob, snapshot_bob) = subscribed(&agent, &bob).await;

    // 1. L'instantané : le détail pour ada, rien pour bob, qui voit pourtant le service.
    let (text_ada, text_bob) = (
        serde_json::to_string(&snapshot_ada).unwrap(),
        serde_json::to_string(&snapshot_bob).unwrap(),
    );
    assert_detail("ada", "instantané", &text_ada, true);
    assert_detail("bob", "instantané", &text_bob, false);
    assert_eq!(names(&snapshot_bob), ["web"], "bob voit le service");
    assert!(
        snapshot_bob.services[0].details_reserved && !snapshot_ada.services[0].details_reserved
    );

    // 2. Un lot : le service change d'état, les deux abonnés reçoivent leur propre composition.
    engine.update(&web.id, |o| {
        o.run_state = RunState::Exited;
        o.exit_code = Some(0);
    });
    engine.emit(&web.id, "die");
    for (who, client, present) in [("ada", &mut ws_ada, true), ("bob", &mut ws_bob, false)] {
        let ServicesMessage::ServicesDelta(delta) = client.next_services().await else {
            panic!("{who} : un services_delta était attendu");
        };
        assert_eq!(delta.upserts.len(), 1, "{who}");
        assert_detail(who, "lot", &serde_json::to_string(&delta).unwrap(), present);
        assert_eq!(delta.upserts[0].details_reserved, !present, "{who}");
    }

    // 3. Le droit d'ada est retiré PENDANT l'abonnement : un instantané entier, sans le détail, puis les
    // lots suivants ne portent plus rien non plus.
    env.revoke_services("ada").await;
    let (snapshot, _) = next_snapshot(&mut ws_ada).await;
    assert_detail(
        "ada",
        "instantané après retrait",
        &serde_json::to_string(&snapshot).unwrap(),
        false,
    );
    assert!(snapshot.services[0].details_reserved);
    engine.update(&web.id, |o| o.run_state = RunState::Running);
    engine.emit(&web.id, "start");
    let ServicesMessage::ServicesDelta(delta) = ws_ada.next_services().await else {
        panic!("un services_delta était attendu après le retrait");
    };
    assert_detail(
        "ada",
        "lot après retrait",
        &serde_json::to_string(&delta).unwrap(),
        false,
    );
    assert!(delta.upserts[0].details_reserved);

    // 4. Le droit donné à bob pendant l'abonnement : le détail arrive d'un coup, puis dans les lots.
    env.grant_services("bob").await;
    let (snapshot, _) = next_snapshot(&mut ws_bob).await;
    assert_detail(
        "bob",
        "instantané après octroi",
        &serde_json::to_string(&snapshot).unwrap(),
        true,
    );
    assert!(!snapshot.services[0].details_reserved);
    engine.update(&web.id, |o| o.run_state = RunState::Exited);
    engine.emit(&web.id, "die");
    let ServicesMessage::ServicesDelta(delta) = ws_bob.next_services().await else {
        panic!("un services_delta était attendu après l'octroi");
    };
    assert_detail(
        "bob",
        "lot après octroi",
        &serde_json::to_string(&delta).unwrap(),
        true,
    );
    drop((ws_ada, ws_bob));
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
    env.adopt("u_docker-terraria.service", "terraria", "marie")
        .await;
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

    // Un homonyme créé ensuite n'est pas confondu avec lui (BR-SVC-016) : il n'hérite pas de
    // l'adoption (BR-ADOPT-017), il est compté comme non géré.
    engine.put(recreated("terraria", RunState::Running, 'c'), "start");
    eventually("l'homonyme est compté", || {
        let registry = agent.services();
        async move { registry.engine_status().external_count == 1 }
    })
    .await;
    let (state, _) = services_of(&agent, &admin).await;
    let keys: Vec<&str> = state.services.iter().map(|s| s.key.as_str()).collect();
    assert_eq!(keys, ["u_docker-terraria.service"]);
    assert_eq!(state.engine.external_count, 1);

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
async fn only_an_account_with_the_right_reveals_the_values_whatever_its_role_and_every_attempt_is_journaled_without_them()
 {
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
    // Les quatre profils de la spec (BR-GRANT-001) : A admin avec le droit, B admin sans, C lecture
    // seule sans, D lecture seule avec.
    let a = token(&env, &agent, "ada", Role::Admin).await;
    let b = token(&env, &agent, "bob", Role::Admin).await;
    let c = token(&env, &agent, "cleo", Role::ReadOnly).await;
    let d = token(&env, &agent, "dan", Role::ReadOnly).await;
    env.grant_services("ada").await;
    env.grant_services("dan").await;
    let path = format!("/services/c_{}/env", web.id);

    // Sans le droit, quel que soit le rôle (même un administrateur) : refusé, même client modifié, et
    // rien n'est lu dans le moteur. Le texte ne dit jamais « administrateur ».
    for (who, token) in [("bob", &b), ("cleo", &c)] {
        let denied = agent.request("GET", &path).token(token).send().await;
        assert_eq!(denied.status, 403, "{who} : {:?}", denied.body);
        assert_eq!(denied.code(), "FORBIDDEN_ROLE");
        assert_eq!(
            denied.body["error"]["message"],
            "Tu n'as pas le droit de gérer les services de ce serveur."
        );
        assert_eq!(
            denied.body["error"]["details"],
            json!({ "missing": "services" })
        );
        assert!(!denied.text.contains("SENTINEL") && !denied.text.contains("DB_PASSWORD"));
        assert!(!denied.text.to_lowercase().contains("administrateur"));
    }
    assert_eq!(engine.env_read_count(), 0);
    // Un service inconnu ou une clé mal formée ne se distingue pas d'un service connu pour un compte
    // sans le droit : le droit est jugé avant la clé (BR-GRANT-004), aucun oracle d'existence.
    let known = agent.request("GET", &path).token(&b).send().await;
    for other in ["c_00ff", "x_not-a-key", "u_docker-ghost.service"] {
        let unknown = agent
            .request("GET", &format!("/services/{other}/env"))
            .token(&b)
            .send()
            .await;
        assert_eq!(unknown.status, known.status, "{other}");
        assert_eq!(unknown.body, known.body, "{other}");
    }
    // Sans session : refusé avant tout.
    let anonymous = agent.request("GET", &path).send().await;
    assert_eq!(anonymous.status, 401);

    // Avec le droit, administrateur ou lecture seule : les valeurs, dans une réponse que rien ne stocke.
    for (who, token) in [("ada", &a), ("dan", &d)] {
        let revealed = agent.request("GET", &path).token(token).send().await;
        assert_eq!(revealed.status, 200, "{who} : {:?}", revealed.body);
        assert_eq!(revealed.header("cache-control"), Some("no-store"));
        assert_eq!(revealed.header("pragma"), Some("no-cache"));
        assert_eq!(
            revealed.body,
            json!({ "variables": [
                { "name": "APP_MODE", "value": "prod" },
                { "name": "DB_PASSWORD", "value": "SENTINEL-VALUE-A" }
            ]})
        );
    }
    assert_eq!(engine.env_read_count(), 2);

    // Un service inconnu, une clé mal formée : 404 pour qui a le droit, sans répéter la clé.
    for bad in ["c_00ff", "x_not-a-key", "u_docker-ghost.service"] {
        let reply = agent
            .request("GET", &format!("/services/{bad}/env"))
            .token(&a)
            .send()
            .await;
        assert_eq!(reply.status, 404, "{bad} : {:?}", reply.body);
        assert!(
            !reply.text.contains(bad),
            "la clé est répétée : {}",
            reply.text
        );
    }
    assert_eq!(engine.env_read_count(), 2, "rien de plus n'est lu");

    // Le journal : les refus (cible : la route, jamais la clé demandée, cause « droits insuffisants »)
    // et les réussites (cible : le service), sans nom ni valeur. Les refus identiques d'un même compte
    // se regroupent dans la minute : on lit la première entrée de chacun.
    for who in ["bob", "cleo"] {
        let first: JournalRow = sqlx::query_as(
            "SELECT account, outcome, target, reason, action_label FROM audit_events
             WHERE action = 'service.env.reveal' AND outcome = 'denied' AND account = ?
             ORDER BY id LIMIT 1",
        )
        .bind(who)
        .fetch_one(env.db.pool())
        .await
        .unwrap();
        assert_eq!(
            first,
            (
                who.to_owned(),
                "denied".to_owned(),
                Some("/services/{key}/env".to_owned()),
                Some("droits insuffisants".to_owned()),
                "Valeurs des variables révélées".to_owned()
            )
        );
    }
    let ok: Vec<JournalRow> = sqlx::query_as(
        "SELECT account, outcome, target, reason, action_label FROM audit_events
         WHERE action = 'service.env.reveal' AND outcome = 'ok' ORDER BY id",
    )
    .fetch_all(env.db.pool())
    .await
    .unwrap();
    assert_eq!(
        ok,
        vec![
            (
                "ada".to_owned(),
                "ok".to_owned(),
                Some("service web".to_owned()),
                None,
                "Valeurs des variables révélées".to_owned()
            ),
            (
                "dan".to_owned(),
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

/// BR-GRANT-010 : un retrait prend effet à la requête suivante, sans fermer la session ; un octroi
/// aussi. Le droit est relu en base à chaque requête (aucun cache de session).
#[tokio::test]
async fn a_withdrawal_takes_effect_on_the_next_request_without_closing_the_session() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let web = detailed("web", RunState::Running);
    engine.put_silently(web.clone());
    engine.set_env(&web.id, &[("DB_PASSWORD", "SENTINEL-VALUE-W")]);
    let agent = ready_agent(&env, &engine).await;
    listed(&agent, 1).await;
    let lucas = token(&env, &agent, "lucas", Role::ReadOnly).await;
    let path = format!("/services/c_{}/env", web.id);

    let refused = agent.request("GET", &path).token(&lucas).send().await;
    assert_eq!(refused.status, 403);
    env.grant_services("lucas").await;
    let granted = agent.request("GET", &path).token(&lucas).send().await;
    assert_eq!(granted.status, 200, "{:?}", granted.body);
    // Le retrait : la requête suivante, avec le MÊME jeton, est refusée ; la session reste ouverte.
    env.revoke_services("lucas").await;
    let after = agent.request("GET", &path).token(&lucas).send().await;
    assert_eq!(after.status, 403, "{:?}", after.body);
    assert_eq!(
        after.body["error"]["details"],
        json!({ "missing": "services" })
    );
    let still_open = agent.request("GET", "/me").token(&lucas).send().await;
    assert_eq!(still_open.status, 200, "la session n'est pas fermée");
    assert_eq!(still_open.body["account"]["role"], "readonly");
    assert_eq!(still_open.body["account"]["rights"], json!([]));
    assert_eq!(
        engine.env_read_count(),
        1,
        "rien n'a été lu après le retrait"
    );
    // Le droit redonné, le même jeton passe de nouveau : rien n'a été fermé ni mémorisé.
    env.grant_services("lucas").await;
    let again = agent.request("GET", &path).token(&lucas).send().await;
    assert_eq!(again.status, 200);
    let me = agent.request("GET", "/me").token(&lucas).send().await;
    assert_eq!(me.body["account"]["rights"], json!(["services"]));
    agent.shutdown().await;
}

/// BR-GRANT-015 : le coupe-circuit l'emporte sur le droit. Test à deux valeurs de `engine.enabled`.
#[tokio::test]
async fn the_kill_switch_wins_over_the_right_with_two_values_of_the_setting() {
    // Pilotage permis : le compte qui a le droit révèle.
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let web = detailed("web", RunState::Running);
    engine.put_silently(web.clone());
    engine.set_env(&web.id, &[("DB_PASSWORD", "SENTINEL-VALUE-K")]);
    let agent = ready_agent(&env, &engine).await;
    listed(&agent, 1).await;
    let marie = token(&env, &agent, "marie", Role::Admin).await;
    env.grant_services("marie").await;
    let path = format!("/services/c_{}/env", web.id);
    let on = agent.request("GET", &path).token(&marie).send().await;
    assert_eq!(on.status, 200, "{:?}", on.body);
    agent.shutdown().await;

    // Pilotage désactivé : le même compte, avec le droit, n'obtient rien et le moteur n'est jamais ouvert.
    let env = self::env().await;
    let engine = Arc::new(FakeEngine::new());
    engine.put_silently(web.clone());
    engine.set_env(&web.id, &[("DB_PASSWORD", "SENTINEL-VALUE-K")]);
    let mut config_parts = parts(engine.clone());
    config_parts.timings = fast_timings();
    let agent = https::start_engine_disabled(&env, config_parts).await;
    let marie = token(&env, &agent, "marie", Role::Admin).await;
    env.grant_services("marie").await;
    let off = agent.request("GET", &path).token(&marie).send().await;
    assert_eq!(off.status, 404, "{:?}", off.body);
    assert!(!off.text.contains("SENTINEL"));
    assert_eq!(engine.env_read_count(), 0);
    assert_eq!(engine.probe_count(), 0, "le socket n'est jamais ouvert");
    // Le droit existe toujours : le coupe-circuit ne le retire pas.
    assert_eq!(env.services_right_of("marie").await, "manage");
    agent.shutdown().await;
}

#[tokio::test]
async fn no_value_leaves_when_the_journal_cannot_be_written_and_each_reveal_is_its_own_entry() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let web = detailed("web", RunState::Running);
    engine.put_silently(web.clone());
    engine.set_env(&web.id, &[("DB_PASSWORD", "SENTINEL-VALUE-J")]);
    let agent = ready_agent(&env, &engine).await;
    listed(&agent, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    env.grant_services("marie").await;
    let path = format!("/services/c_{}/env", web.id);

    // Le journal refuse toute écriture de révélation (base pleine ou verrouillée, simulée).
    sqlx::query(
        "CREATE TRIGGER refuse_reveal BEFORE INSERT ON audit_events
         WHEN NEW.action = 'service.env.reveal'
         BEGIN SELECT RAISE(ABORT, 'journal indisponible'); END",
    )
    .execute(env.db.pool())
    .await
    .unwrap();
    let refused = agent.request("GET", &path).token(&admin).send().await;
    assert_eq!(refused.status, 503, "{:?}", refused.body);
    assert_eq!(refused.code(), "BUSY");
    assert!(
        !refused.text.contains("SENTINEL") && !refused.text.contains("DB_PASSWORD"),
        "une valeur ou un nom est sorti : {}",
        refused.text
    );
    assert!(
        !refused.text.contains("journal indisponible"),
        "le texte de la base remonte : {}",
        refused.text
    );

    // Le journal revient : deux révélations de suite sont deux entrées (pas de regroupement).
    sqlx::query("DROP TRIGGER refuse_reveal")
        .execute(env.db.pool())
        .await
        .unwrap();
    for _ in 0..2 {
        let revealed = agent.request("GET", &path).token(&admin).send().await;
        assert_eq!(revealed.status, 200, "{:?}", revealed.body);
        assert_eq!(revealed.body["variables"][0]["value"], "SENTINEL-VALUE-J");
    }
    let ok: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_events WHERE action = 'service.env.reveal' AND outcome = 'ok'",
    )
    .fetch_one(env.db.pool())
    .await
    .unwrap();
    assert_eq!(ok, 2, "une entrée par révélation réussie");
    agent.shutdown().await;
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
    env.adopt(&key, "vault", "marie").await;
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
            log_brake: hearth_agent::domain::services::BrakeSettings::relaxed(),
        },
    )
    .await;
    listed(&agent, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let readonly = token(&env, &agent, "lucas", Role::ReadOnly).await;
    // Seul un compte qui a le droit de gérer les services révèle une valeur (BR-GRANT-003).
    env.grant_services("marie").await;
    let path = format!("/services/{key}/env");

    // Réussite, refus, service inconnu, requête sans session.
    let revealed = agent.request("GET", &path).token(&admin).send().await;
    assert_eq!(revealed.status, 200, "{:?}", revealed.body);
    assert!(
        revealed.text.contains("SENTINEL-REVEAL-VALUE-44ab"),
        "le témoin : le compte qui a le droit reçoit bien la valeur"
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

// ---------------------------------------------------------------------------------------------
// Publication (HRT-79, ADR-0043) : seuls les services créés par Hearth et les adoptés sortent
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn on_a_server_whose_containers_are_all_outside_hearth_the_list_is_empty_and_they_are_counted()
 {
    // BR-ADOPT-001, BR-ADOPT-002 : trois conteneurs, un arrêté, un jamais démarré, aucun adopté.
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    engine.put_silently(container("web", RunState::Running));
    engine.put_silently(container("db", RunState::Exited));
    engine.put_silently(container("cache", RunState::Created));
    let agent = https::start_engined(&env, parts(engine.clone())).await;
    let registry = agent.services();
    eventually("les trois conteneurs sont comptés", || {
        let registry = registry.clone();
        async move { registry.engine_status().external_count == 3 }
    })
    .await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let readonly = token(&env, &agent, "lucas", Role::ReadOnly).await;
    env.grant_services("marie").await;
    for token in [&admin, &readonly] {
        let (state, text) = services_of(&agent, token).await;
        assert_eq!(state.engine.state, EngineState::Ready);
        assert!(state.services.is_empty(), "{text}");
        assert_eq!(state.engine.external_count, 3);
        for hidden in ["web", "db", "cache", "busybox"] {
            assert!(!text.contains(hidden), "{hidden} dans {text}");
        }
        let (mut client, snapshot) = subscribed(&agent, token).await;
        assert!(snapshot.services.is_empty());
        assert_eq!(snapshot.engine.external_count, 3);
        let recheck = agent
            .request("POST", "/engine/recheck")
            .token(token)
            .send()
            .await;
        assert_eq!(recheck.body["external_count"], 3);
        // Rien d'autre ne part : le monde caché reste muet.
        assert!(
            client
                .try_next_services(Duration::from_millis(300))
                .await
                .is_none()
        );
    }
    agent.shutdown().await;
}

#[tokio::test]
async fn a_request_by_the_key_of_a_container_that_is_not_adopted_gets_the_answer_of_an_unknown_service()
 {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let hidden = container("hidden", RunState::Running);
    let shown = container("shown", RunState::Running);
    engine.put_silently(hidden.clone());
    engine.put_silently(shown.clone());
    engine.set_env(&hidden.id, &[("DB_PASSWORD", "SENTINEL-VALUE-H")]);
    engine.set_env(&shown.id, &[("DB_PASSWORD", "SENTINEL-VALUE-S")]);
    env.adopt_container(&shown.id, "shown").await;
    // Seul `shown` est adopté : `ready_agent` adopterait aussi l'autre.
    let agent = https::start_engined(&env, parts(engine.clone())).await;
    listed(&agent, 1).await;
    let admin = token(&env, &agent, "ada", Role::Admin).await;
    let readonly = token(&env, &agent, "lucas", Role::ReadOnly).await;
    env.grant_services("ada").await;
    // Avec le droit : un conteneur non adopté, une clé jamais vue, une clé d'unité jamais vue et une
    // clé mal formée reçoivent la MÊME réponse (code, code d'erreur, message, corps).
    let hidden_path = format!("/services/c_{}/env", hidden.id);
    let answers = [
        hidden_path.clone(),
        format!("/services/c_{}/env", "f".repeat(64)),
        "/services/u_docker-ghost.service/env".to_owned(),
        "/services/c_not-hex-!/env".to_owned(),
    ];
    let mut replies = Vec::new();
    for path in &answers {
        let reply = agent.request("GET", path).token(&admin).send().await;
        replies.push((reply.status, reply.code().to_owned(), reply.body.clone()));
        assert!(
            !reply.text.contains("hidden") && !reply.text.contains("SENTINEL"),
            "{path} : {}",
            reply.text
        );
    }
    assert_eq!(replies[0].0, 404);
    assert!(
        replies.windows(2).all(|pair| pair[0] == pair[1]),
        "non adopté, inconnu et mal formé se confondent : {replies:?}"
    );
    // Sans le droit : le droit est jugé avant la clé, même réponse pour tout.
    let denied: Vec<_> = {
        let mut out = Vec::new();
        for path in &answers {
            let reply = agent.request("GET", path).token(&readonly).send().await;
            out.push((reply.status, reply.body.clone()));
        }
        out
    };
    assert_eq!(denied[0].0, 403);
    assert!(denied.windows(2).all(|pair| pair[0] == pair[1]));
    assert_eq!(
        engine.env_read_count(),
        0,
        "rien n'est lu dans le moteur pour un conteneur non adopté"
    );
    // Le service adopté, lui, se révèle toujours.
    let revealed = agent
        .request("GET", &format!("/services/c_{}/env", shown.id))
        .token(&admin)
        .send()
        .await;
    assert_eq!(revealed.status, 200, "{:?}", revealed.body);
    agent.shutdown().await;
}

const HIDDEN_NAME: &str = "sentinel-hidden-name-7f3a";
const HIDDEN_IMAGE: &str = "sentinel-hidden-image-3b9d";
const HIDDEN_VARIABLE: &str = "sentinel_hidden_var_52";
const HIDDEN_VALUE: &str = "sentinel-hidden-value-c41e";
const HIDDEN_COMMAND: &str = "sentinel-hidden-cmd-a90b";
const HIDDEN_UNIT: &str = "sentinel-hidden-unit-2e1f";
const HIDDEN_PORT: &str = "47813";
const HIDDEN_CONTAINER_PORT: &str = "31847";
/// Une ligne de journal écrite par un conteneur caché (préfixe sentinelle : `HIDDEN_FORBIDDEN` la couvre).
const HIDDEN_LOG_LINE: &str = "sentinel-hidden-log-line-6b21 jeton=abc";
/// Une ligne du journal du témoin adopté : elle DOIT sortir, pour qui a le droit.
const SHOWN_LOG_LINE: &str = "visible-log-line-91d2";

/// Ouvre une lecture de journaux, rend tout ce qui est reçu jusqu'à la fermeture (texte brut des messages).
async fn logs_text(agent: &Agent, token: &str, key: &str) -> String {
    let mut client = ws::open_logs(agent, key)
        .await
        .expect("socket des journaux");
    client
        .send_value(&json!({ "type": "auth", "token": token }))
        .await;
    client
        .send_value(&hearth_proto::api::service_logs::LogsClientMessage::Open {
            tail: 1000,
            since: None,
        })
        .await;
    let mut text = String::new();
    // Une lecture qui s'ouvre ne finit pas d'elle-même : on s'arrête à la première ligne reçue, à la
    // fermeture ou à un silence.
    while let Some(next) = client.try_next_logs(Duration::from_millis(800)).await {
        match next {
            Ok(message) => {
                text.push_str(&format!("{message:?}\n"));
                // Le `Debug` d'une ligne cache son texte : on le lit ici, pour que la recherche des
                // sentinelles voie ce que le client recevrait.
                if let hearth_proto::api::service_logs::LogsServerMessage::Lines(lines) = message {
                    for item in &lines.items {
                        text.push_str(&item.x);
                        text.push('\n');
                    }
                    break;
                }
            }
            Err(end) => {
                text.push_str(&format!("{end:?}\n"));
                break;
            }
        }
    }
    text
}

/// Tout ce qui, d'un conteneur non adopté, ne doit sortir nulle part (comparé en minuscules). Les
/// noms des autres conteneurs cachés commencent tous par le même préfixe sentinelle.
const HIDDEN_FORBIDDEN: [&str; 8] = [
    HIDDEN_NAME,
    HIDDEN_IMAGE,
    HIDDEN_VARIABLE,
    HIDDEN_VALUE,
    HIDDEN_COMMAND,
    HIDDEN_UNIT,
    "sentinel-hidden",
    "sentinel_hidden",
];

/// Les ports sentinelles, cherchés comme nombres entiers : une date des traces peut contenir ces
/// chiffres sans que ce soit un port.
fn contains_number(text: &str, number: &str) -> bool {
    text.match_indices(number).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + number.len()..].chars().next();
        !before.is_some_and(|c| c.is_ascii_digit()) && !after.is_some_and(|c| c.is_ascii_digit())
    })
}

fn assert_no_hidden(what: &str, text: &str) {
    let lower = text.to_lowercase();
    for forbidden in HIDDEN_FORBIDDEN {
        assert!(
            !lower.contains(forbidden),
            "{forbidden} dans {what} : {text}"
        );
    }
    for port in [HIDDEN_PORT, HIDDEN_CONTAINER_PORT] {
        assert!(
            !contains_number(&lower, port),
            "le port {port} dans {what} : {text}"
        );
    }
}

#[tokio::test]
async fn nothing_of_a_container_that_is_not_adopted_leaves_the_agent_by_any_route_message_or_trace()
{
    let traces = captured_traces();
    log::debug!(target: "hearth_witness", "witness-hidden-line");
    let env = env().await;
    // Le VRAI adaptateur `bollard` sur un faux démon. Un conteneur non adopté porte la valeur
    // sentinelle dans son nom, son image, un port, une variable (nom et valeur) et sa commande ; un
    // second est lancé par une unité dont le nom est sentinelle ; un troisième est arrêté.
    let daemon = FakeDaemon::start().await;
    let mut hidden = DaemonContainer::new(HIDDEN_NAME, "running");
    hidden.image = format!("registry.example/{HIDDEN_IMAGE}:9.9.9");
    hidden.publish = Some((47813, 31847));
    hidden.env = vec![format!("{HIDDEN_VARIABLE}={HIDDEN_VALUE}")];
    hidden.cmd = vec!["serve".to_owned(), format!("--key={HIDDEN_COMMAND}")];
    let hidden_id = hidden.id.clone();
    let mut supervised = DaemonContainer::new("sentinel-hidden-supervised-8d", "running");
    supervised.image = format!("registry.example/{HIDDEN_IMAGE}:1.0");
    let supervised_id = supervised.id.clone();
    let stopped = DaemonContainer::new("sentinel-hidden-stopped-19", "exited");
    let stopped_id = stopped.id.clone();
    // Le témoin : un service adopté, qui DOIT sortir (sinon le test serait aveugle).
    let shown = DaemonContainer::new("visible-control", "running");
    env.adopt(&format!("c_{}", shown.id), "visible-control", "marie")
        .await;
    let shown_id = shown.id.clone();
    for container in [hidden, supervised, stopped, shown] {
        daemon.put(container);
    }
    // Les journaux : chaque conteneur caché écrit une ligne sentinelle, le témoin une ligne qui doit sortir.
    for id in [&hidden_id, &supervised_id, &stopped_id] {
        daemon.push_log(id, 1, 1_791_885_600_000, HIDDEN_LOG_LINE);
        daemon.push_log(id, 2, 1_791_885_600_001, HIDDEN_LOG_LINE);
    }
    daemon.push_log(&shown_id, 1, 1_791_885_600_000, SHOWN_LOG_LINE);
    let supervisor = Arc::new(FakeSupervisor::new());
    let unit = unit_named(&format!("docker-{HIDDEN_UNIT}.service"));
    supervisor.set_owner(&supervised_id, &unit);
    supervisor.set_status(&unit, running_unit());
    let engine = BollardEngine::new(
        Target::Http(daemon.address()),
        InstallMarkers::at(Vec::<std::path::PathBuf>::new()),
    );
    let agent = https::start_engined(
        &env,
        EngineParts {
            engine: Arc::new(engine),
            supervisor,
            host: Arc::new(support::engine::TestHost),
            timings: fast_timings(),
            log_brake: hearth_agent::domain::services::BrakeSettings::relaxed(),
        },
    )
    .await;
    let registry = agent.services();
    eventually(
        "le témoin est publié et les trois autres comptés",
        || {
            let registry = registry.clone();
            async move {
                registry.snapshot().services.len() == 1
                    && registry.engine_status().external_count == 3
            }
        },
    )
    .await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let readonly = token(&env, &agent, "lucas", Role::ReadOnly).await;
    env.grant_services("marie").await;

    let mut seen: Vec<(String, String)> = Vec::new();
    let mut clients = Vec::new();
    for (who, token) in [("marie", &admin), ("lucas", &readonly)] {
        let (state, text) = services_of(&agent, token).await;
        assert_eq!(names(&state), vec!["visible-control"], "{who}");
        assert_eq!(state.engine.external_count, 3, "{who}");
        seen.push((format!("GET /services ({who})"), text));
        let recheck = agent
            .request("POST", "/engine/recheck")
            .token(token)
            .send()
            .await;
        seen.push((format!("POST /engine/recheck ({who})"), recheck.text));
        let (client, snapshot) = subscribed(&agent, token).await;
        seen.push((
            format!("services_snapshot ({who})"),
            serde_json::to_string(&snapshot).unwrap(),
        ));
        clients.push((who, client));
    }
    let rev = registry.snapshot().rev;

    // Le conteneur caché s'arrête, repart, change de port d'état : aucun lot, `rev` ne bouge pas.
    daemon.update(&hidden_id, |c| c.status = "exited".into());
    daemon.emit(&hidden_id, "die");
    daemon.update(&hidden_id, |c| c.status = "running".into());
    daemon.emit(&hidden_id, "start");
    daemon.emit(&supervised_id, "die");
    for (who, client) in &mut clients {
        assert!(
            client
                .try_next_services(Duration::from_millis(400))
                .await
                .is_none(),
            "{who} : un conteneur caché a fait partir un message"
        );
    }
    assert_eq!(
        registry.snapshot().rev,
        rev,
        "rev ne suit pas un conteneur caché"
    );

    // Un quatrième conteneur caché apparaît : un seul lot, qui ne porte que le nombre.
    let mut fourth = DaemonContainer::new("sentinel-hidden-fourth-aa", "running");
    fourth.env = vec![format!("{HIDDEN_VARIABLE}={HIDDEN_VALUE}")];
    let fourth_id = fourth.id.clone();
    daemon.put(fourth);
    daemon.emit(&fourth_id, "start");
    for (who, client) in &mut clients {
        let ServicesMessage::ServicesDelta(delta) = client.next_services().await else {
            panic!("{who} : un services_delta était attendu");
        };
        assert!(
            delta.upserts.is_empty() && delta.removed.is_empty(),
            "{who}"
        );
        assert_eq!(delta.engine.as_ref().map(|e| e.external_count), Some(4));
        seen.push((
            format!("services_delta ({who})"),
            serde_json::to_string(&delta).unwrap(),
        ));
    }

    // Une inspection illisible d'un conteneur caché : la liste est marquée « à vérifier », rien d'autre.
    daemon.break_inspection(&hidden_id);
    daemon.emit(&hidden_id, "die");
    for (who, client) in &mut clients {
        if let Some(message) = client.try_next_services(Duration::from_millis(600)).await {
            let ServicesMessage::ServicesDelta(delta) = message else {
                panic!("{who} : un services_delta était attendu");
            };
            assert!(
                delta.upserts.is_empty() && delta.removed.is_empty(),
                "{who}"
            );
            seen.push((
                format!("services_delta illisible ({who})"),
                serde_json::to_string(&delta).unwrap(),
            ));
        }
    }

    // Détail, valeurs, erreurs : par la clé du conteneur caché, avec et sans le droit.
    let path = format!("/services/c_{hidden_id}/env");
    for (who, token) in [("marie", &admin), ("lucas", &readonly)] {
        let reply = agent.request("GET", &path).token(token).send().await;
        assert!(
            matches!(reply.status, 403 | 404),
            "{who} : {}",
            reply.status
        );
        seen.push((format!("GET env ({who})"), reply.text));
    }
    let revealed = agent
        .request("GET", "/services/c_unknown/env")
        .token(&admin)
        .send()
        .await;
    seen.push(("GET env inconnu".to_owned(), revealed.text));

    // Journaux : la clé d'un conteneur caché (avec le droit ou sans, arrêté, lancé par une unité, apparu
    // plus tard) ne donne pas une ligne, par aucun des deux comptes.
    let hidden_keys = [
        format!("c_{hidden_id}"),
        // La clé d'une unité cachée n'est pas demandée : son nom est la sentinelle, et le chemin que le
        // client écrit lui-même est tracé par la couche HTTP (BR-ADOPT-004, cas limites).
        format!("c_{supervised_id}"),
        format!("c_{stopped_id}"),
        format!("c_{fourth_id}"),
    ];
    for (who, token) in [("marie", &admin), ("lucas", &readonly)] {
        for key in &hidden_keys {
            let text = logs_text(&agent, token, key).await;
            assert!(text.contains("ServiceNotFound"), "{who} {key} : {text}");
            seen.push((format!("logs {key} ({who})"), text));
        }
    }
    // Témoin : le service adopté se lit avec le droit (les lignes sortent, le test n'est pas aveugle), pas
    // sans (même réponse qu'une clé inconnue).
    let shown_key = format!("c_{shown_id}");
    let with_right = logs_text(&agent, &admin, &shown_key).await;
    assert!(with_right.contains(SHOWN_LOG_LINE), "{with_right}");
    let without_right = logs_text(&agent, &readonly, &shown_key).await;
    // Un service adopté est dans la liste de ce compte : le refus est explicite (BR-SLOG-024), à la
    // différence de la clé cachée ou inconnue, qui reçoit « introuvable ».
    assert!(
        without_right.contains("MissingRight") && !without_right.contains(SHOWN_LOG_LINE),
        "{without_right}"
    );
    seen.push(("logs témoin (lucas)".to_owned(), without_right));

    // Un compte sans le droit n'apprend RIEN de la liste des non gérés, ni des actes d'adoption : ses
    // demandes (lecture, adoption de clés cachées, relâche) sont refusées avant qu'une clé soit lue.
    let bob = token(&env, &agent, "bob", Role::Admin).await;
    for (who, token) in [("lucas", &readonly), ("bob", &bob)] {
        let candidates = agent
            .request("GET", "/services/external")
            .token(token)
            .send()
            .await;
        assert_eq!(candidates.status, 403, "{who} : {}", candidates.text);
        seen.push((format!("GET /services/external ({who})"), candidates.text));
        for key in &hidden_keys {
            let adopt = agent
                .request("POST", "/services/adopt")
                .token(token)
                .json(&json!({ "keys": [key] }))
                .send()
                .await;
            assert_eq!(adopt.status, 403, "{who} : {}", adopt.text);
            seen.push((format!("POST adopt {key} ({who})"), adopt.text));
            let release = agent
                .request("POST", &format!("/services/{key}/release"))
                .token(token)
                .json(&json!({}))
                .send()
                .await;
            assert_eq!(release.status, 403, "{who} : {}", release.text);
            seen.push((format!("POST release {key} ({who})"), release.text));
        }
    }
    // Un AUTRE conteneur est adopté par l'acte (avec le droit, la preuve de la clé et le mot de passe) :
    // il apparaît, et les cachés restent invisibles, par aucune route ni aucun message.
    let marie = agent.login_actor("marie").await;
    let fifth = DaemonContainer::new("adoptable-extra", "running");
    let second = format!("c_{}", fifth.id);
    daemon.put(fifth);
    daemon.emit(second.trim_start_matches("c_"), "start");
    for (who, client) in &mut clients {
        // Un cinquième conteneur non géré apparaît : le nombre seul bouge.
        let ServicesMessage::ServicesDelta(delta) = client.next_services().await else {
            panic!("{who} : un services_delta était attendu");
        };
        assert!(delta.upserts.is_empty(), "{who}");
        assert_eq!(delta.engine.as_ref().map(|e| e.external_count), Some(5));
    }
    // Le compte qui a le droit LIT les candidats : c'est le seul code qui tient en main le nom, l'image et
    // l'unité d'un conteneur caché (`ServicesRegistry::candidates`). Sa réponse est le TÉMOIN : elle doit
    // nommer les cachés, donc elle n'entre pas dans `seen` (ce que les comptes sans le droit reçoivent) ;
    // tout ce qui est balayé ensuite (traces, journal, tables, octets, messages des autres abonnés) ne doit
    // pas en avoir gardé un seul mot. Aucune variable, valeur, commande ni port n'y figure jamais.
    let candidates = agent
        .request("GET", "/services/external")
        .token(&admin)
        .send()
        .await;
    assert_eq!(candidates.status, 200, "{}", candidates.text);
    let witness = candidates.text.to_lowercase();
    for named in [
        HIDDEN_NAME,
        HIDDEN_IMAGE,
        HIDDEN_UNIT,
        "sentinel-hidden-supervised-8d",
        "sentinel-hidden-stopped-19",
        "sentinel-hidden-fourth-aa",
        "adoptable-extra",
    ] {
        assert!(
            witness.contains(named),
            "la lecture des candidats doit nommer {named} (témoin) : {witness}"
        );
    }
    for never in [HIDDEN_VARIABLE, HIDDEN_VALUE, HIDDEN_COMMAND] {
        assert!(!witness.contains(never), "{never} dans les candidats");
    }
    for port in [HIDDEN_PORT, HIDDEN_CONTAINER_PORT] {
        assert!(!contains_number(&witness, port), "port {port}");
    }
    // Elle en adopte UN, choisi dans cette liste ; les autres restent cachés.
    let chosen = candidates.body["containers"]
        .as_array()
        .expect("containers")
        .iter()
        .find(|c| c["name"] == "adoptable-extra")
        .expect("le candidat à adopter");
    assert_eq!(chosen["key"], second.as_str());
    let adopted = agent
        .confirmed(
            &marie,
            "POST",
            "/services/adopt",
            json!({ "keys": [second.clone()] }),
        )
        .await
        .send()
        .await;
    assert_eq!(adopted.status, 200, "{}", adopted.text);
    for (who, client) in &mut clients {
        // Le lot de l'adoption : un seul service de plus, et le compte des non gérés baisse d'un.
        let ServicesMessage::ServicesDelta(delta) = client.next_services().await else {
            panic!("{who} : un services_delta était attendu");
        };
        assert_eq!(delta.upserts.len(), 1, "{who}");
        assert_eq!(delta.upserts[0].key, second, "{who}");
        seen.push((
            format!("services_delta après adoption ({who})"),
            serde_json::to_string(&delta).unwrap(),
        ));
    }
    for (who, token) in [("marie", &admin), ("lucas", &readonly)] {
        let (state, text) = services_of(&agent, token).await;
        assert_eq!(state.services.len(), 2, "{who}");
        assert_eq!(state.engine.external_count, 4, "{who}");
        seen.push((format!("GET /services après adoption ({who})"), text));
    }
    // Avec le droit, la preuve et le mot de passe : demander une clé cachée (celle du conteneur d'une unité,
    // qui n'est pas une identité de ce service) ou inventée a la même issue, sans un nom ; relâcher une clé
    // cachée ou inventée ne fait rien.
    for key in [format!("c_{supervised_id}"), "c_inventee0123".to_owned()] {
        let reply = agent
            .confirmed(
                &marie,
                "POST",
                "/services/adopt",
                json!({ "keys": [key.clone()] }),
            )
            .await
            .send()
            .await;
        assert_eq!(reply.status, 200, "{}", reply.text);
        assert_eq!(reply.body["adopted"], 0);
        assert_eq!(reply.body["results"][0]["result"], "not_found");
        seen.push((format!("POST adopt {key} (marie)"), reply.text));
        let reply = agent
            .confirmed(
                &marie,
                "POST",
                &format!("/services/{key}/release"),
                json!({}),
            )
            .await
            .send()
            .await;
        assert_eq!(reply.status, 200, "{}", reply.text);
        assert_eq!(reply.body["released"], false);
        seen.push((format!("POST release {key} (marie)"), reply.text));
    }
    // Les trois autres restent cachés : leurs clés répondent comme des clés inconnues.
    for key in &hidden_keys[..3] {
        let text = logs_text(&agent, &admin, key).await;
        assert!(text.contains("ServiceNotFound"), "{key} : {text}");
        seen.push((format!("logs {key} après adoption (marie)"), text));
    }
    // Le journal d'activité (lu par l'administrateur) et la base : aucune entrée, aucune table ne contient
    // un nom, une image, une variable, une valeur, une commande, une unité ou un port cachés. Le nom du
    // conteneur adopté est le seul nom d'un conteneur qui y entre (BR-ADOPT-019), et seulement lui.
    agent.flush_journal().await;
    let journal: Vec<(String, String, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT action, outcome, target, reason FROM audit_events ORDER BY id")
            .fetch_all(env.db.pool())
            .await
            .unwrap();
    assert!(
        journal
            .iter()
            .any(|(action, outcome, target, _)| action == "service.adopt"
                && outcome == "ok"
                && target.as_deref() == Some("service adoptable-extra")),
        "l'adoption est consignée avec le nom qui vient d'être publié : {journal:?}"
    );
    // La lecture réussie des candidats y est (action au catalogue), sans un seul nom de conteneur. Rien n'est
    // effacé ni remplacé avant la recherche : le texte est balayé tel que la base le porte.
    assert!(
        journal.iter().any(
            |(action, outcome, target, _)| action == "service.external.read"
                && outcome == "ok"
                && target.as_deref() == Some("/services/external")
        ),
        "la lecture des candidats est consignée : {journal:?}"
    );
    let journal_text = journal
        .iter()
        .map(|row| format!("{row:?}"))
        .collect::<Vec<_>>()
        .join("\n");
    seen.push(("journal d'activité".to_owned(), journal_text));
    seen.push(("tables de la base".to_owned(), dump_tables(&env).await));
    // Témoin : la recherche voit une sentinelle écrite dans une table, quelle qu'elle soit. Sans lui, un
    // balayage qui ne lirait aucune table passerait pour un balayage propre. Il se fait sur une base en
    // mémoire à part : une ligne écrite puis effacée reste lisible dans les pages d'un fichier SQLite.
    let scratch = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query("CREATE TABLE sentinel_control (x TEXT)")
        .execute(&scratch)
        .await
        .unwrap();
    sqlx::query("INSERT INTO sentinel_control (x) VALUES (?)")
        .bind(HIDDEN_NAME)
        .execute(&scratch)
        .await
        .unwrap();
    let controlled = dump_pool(&scratch).await;
    assert!(
        std::panic::catch_unwind(|| assert_no_hidden("témoin", &controlled)).is_err(),
        "le balayage des tables doit voir une sentinelle"
    );

    for (what, text) in &seen {
        assert_no_hidden(what, text);
    }
    // Témoin : le service adopté, lui, est bien dans ce qui sort.
    assert!(
        seen.iter()
            .any(|(_, text)| text.contains("visible-control")),
        "le témoin doit sortir : sinon ce test est aveugle"
    );
    drop(clients);
    agent.shutdown().await;
    // Les octets du dossier de données (la base et son journal d'écriture, les fichiers de l'agent) : les
    // sentinelles des conteneurs restés cachés n'y sont pas. Le nom du conteneur adopté y est, lui.
    let bytes = all_bytes(env.dir.path());
    let stored = String::from_utf8_lossy(&bytes).to_lowercase();
    // Les deux préfixes génériques couvrent tous les conteneurs restés cachés (noms, images, unités,
    // variables, valeurs, commandes, lignes de journal) sans rien oublier.
    for forbidden in ["sentinel-hidden", "sentinel_hidden"] {
        assert!(
            !stored.contains(forbidden),
            "{forbidden} dans le dossier de données"
        );
    }
    let captured = String::from_utf8_lossy(&traces.lock().unwrap()).into_owned();
    assert!(
        captured.contains("witness-hidden-line"),
        "le pont `log` vers `tracing` doit être actif dans ce test"
    );
    assert!(
        captured.len() > 2_000,
        "les traces de l'agent, au niveau le plus bavard, doivent être capturées ({} octets)",
        captured.len()
    );
    assert_no_hidden("les traces", &captured);
}

#[tokio::test]
async fn an_adoption_by_unit_outlives_the_container_and_the_agent_and_one_without_a_unit_falls_with_its_container()
 {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let supervisor = Arc::new(FakeSupervisor::new());
    let unit = unit_named("docker-terraria.service");
    let first = recreated("terraria", RunState::Running, 'a');
    supervisor.set_owner(&first.id, &unit);
    supervisor.set_status(&unit, running_unit());
    engine.put_silently(first.clone());
    let plain = container("plain", RunState::Running);
    engine.put_silently(plain.clone());
    env.adopt("u_docker-terraria.service", "terraria", "marie")
        .await;
    env.adopt_container(&plain.id, "plain").await;
    let agent = https::start_engined(
        &env,
        parts_supervised(engine.clone(), supervisor.clone(), fast_timings()),
    )
    .await;
    listed(&agent, 2).await;
    // Le système recrée le conteneur de l'unité, un tiers recrée « plain » sous un autre identifiant.
    engine.remove(&first.id);
    let second = recreated("terraria", RunState::Running, 'b');
    supervisor.set_owner(&second.id, &unit);
    engine.put(second.clone(), "start");
    engine.remove(&plain.id);
    let mut namesake = container("plain", RunState::Running);
    namesake.id = "9".repeat(64);
    engine.put(namesake, "start");
    eventually("l'homonyme est compté, l'unité reste publiée", || {
        let registry = agent.services();
        async move {
            let snapshot = registry.snapshot();
            snapshot.engine.external_count == 1
                && snapshot.services.len() == 1
                && snapshot.services[0].key.as_str() == "u_docker-terraria.service"
        }
    })
    .await;
    // La base suit : l'adoption sans unité est oubliée, celle de l'unité reste.
    eventually("l'adoption de l'ancien conteneur est effacée", || {
        let env = &env;
        async move { env.adopted_keys().await == vec!["u_docker-terraria.service".to_owned()] }
    })
    .await;
    agent.shutdown().await;

    // Le serveur redémarre : l'agent repart sur la même base, le conteneur de l'unité n'est pas encore
    // revenu. Le service reste listé d'après son unité ; l'homonyme reste non géré.
    engine.remove(&second.id);
    let restarted = https::start_engined(
        &env,
        parts_supervised(engine.clone(), supervisor.clone(), fast_timings()),
    )
    .await;
    listed(&restarted, 1).await;
    let service = restarted.services().snapshot().services.remove(0);
    assert_eq!(service.key.as_str(), "u_docker-terraria.service");
    assert!(service.adoption.is_some());
    assert_eq!(restarted.services().snapshot().engine.external_count, 1);
    restarted.shutdown().await;
}
