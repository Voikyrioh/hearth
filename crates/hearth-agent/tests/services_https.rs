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
use hearth_agent::domain::services::{ProbeOutcome, RunState};
use hearth_agent::infrastructure::engine::fake::{container, ready_probe};
use hearth_agent::infrastructure::engine::{BollardEngine, FakeEngine, InstallMarkers, Target};
use hearth_proto::api::services::{EngineState, ServiceItem, ServiceState, ServicesState};
use hearth_proto::stream::{ServicesMessage, Topic};
use serde_json::json;
use support::engine::{fast_timings, parts, parts_with};
use support::fake_daemon::{DaemonContainer, FakeDaemon};
use support::https::{self, Agent};
use support::probe::{fast_stream, metering_with};
use support::ws::{self, WsClient};
use support::{Env, PASSWORD, env};

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
    let mut changes = Vec::new();
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(1_200) {
        if let Some(ServicesMessage::ServicesDelta(delta)) =
            client.try_next_services(Duration::from_millis(100)).await
            && let Some(engine) = delta.engine
        {
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
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    engine.put_silently(container("one", RunState::Running));
    let agent = ready_agent(&env, &engine).await;
    let registry = agent.services();
    eventually("un abonné aux événements", || {
        let engine = engine.clone();
        async move { engine.event_subscribers() > 0 }
    })
    .await;
    let probes = engine.probe_count();
    let lists = engine.list_count();
    // Pendant la rupture, le monde change sans événement.
    engine.put_silently(container("two", RunState::Running));
    engine.break_events();
    eventually(
        "le deuxième service est vu par la relecture complète",
        || {
            let registry = registry.clone();
            async move { registry.snapshot().services.len() == 2 }
        },
    )
    .await;
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

/// Les traces de tout le binaire de test, capturées en mémoire (une seule installation globale).
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
            // Tous les niveaux, y compris `trace` : si une valeur devait fuir, elle fuirait là.
            let _ = tracing::subscriber::set_global_default(
                tracing_subscriber::fmt()
                    .with_max_level(tracing::Level::TRACE)
                    .with_writer(Sink(buffer.clone()))
                    .finish(),
            );
            buffer
        })
        .clone()
}

#[tokio::test]
async fn no_variable_value_name_or_command_reaches_a_message_a_response_or_a_trace() {
    let traces = captured_traces();
    let env = env().await;
    // Le VRAI adaptateur `bollard` sur un faux démon dont les réponses d'inspection portent une
    // variable d'environnement sentinelle ET une commande.
    let daemon = FakeDaemon::start().await;
    let mut container = DaemonContainer::new("secretive", "running");
    container.env = vec![
        "DB_PASSWORD=SENTINEL-VALUE-91c2".to_owned(),
        "SENTINEL_NAME_77=plain".to_owned(),
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
    for text in &texts {
        for forbidden in [
            "SENTINEL-VALUE-91c2",
            "DB_PASSWORD",
            "SENTINEL_NAME_77",
            "sleep",
        ] {
            assert!(!text.contains(forbidden), "{forbidden} dans {text}");
        }
    }
    agent.shutdown().await;
    let captured = String::from_utf8_lossy(&traces.lock().unwrap()).into_owned();
    assert!(
        !captured.is_empty(),
        "les traces doivent avoir été capturées"
    );
    for forbidden in ["SENTINEL-VALUE-91c2", "DB_PASSWORD", "SENTINEL_NAME_77"] {
        assert!(!captured.contains(forbidden), "{forbidden} dans les traces");
    }
}
