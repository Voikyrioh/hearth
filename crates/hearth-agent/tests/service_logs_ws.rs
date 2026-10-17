//! Le socket des journaux d'un service de bout en bout (HRT-66) : vrai agent en HTTPS, vrai WebSocket,
//! faux moteur (ou `bollard` sur un faux démon), vraie base. Un socket par lecture, 1 000 dernières lignes
//! puis le direct, lots, anneau borné, perte comptée, nettoyage, reprise, redémarrage, dix lectures.
//!
//! Règles : BR-SLOG-001, 003, 006 à 010, 011, 012, 013, 015 à 023 ; BR-ADOPT-004, 015, 016.
//!
//! Le registre ne publie que les services créés par Hearth et les adoptés (HRT-79) : ces tests adoptent
//! ce que le faux moteur contient (`adopt_all`) et donnent le droit de gérer les services aux comptes de
//! `token` (un service adopté se lit avec le droit seulement). `token_without_right` et les tests de la fin
//! du fichier prouvent le reste.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use hearth_agent::application::ports::Clock;
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::services::logs::LogSource;
use hearth_agent::domain::services::{RunState, UnitActivity, UnitStatus};
use hearth_agent::infrastructure::engine::{BollardEngine, InstallMarkers, Target};
use hearth_agent::infrastructure::logging::{self, LogFormat};
use hearth_proto::api::service_logs::{
    GapReason, LogCursor, LogItem, LogState, LogsClientMessage, LogsErrorCode, LogsServerMessage,
    LogsStatus, MarkerKind,
};
use hearth_proto::stream::{ClientMessage, ServerMessage};
use serde_json::json;
use support::engine::{fast_timings, parts, parts_supervised};
use support::fake_daemon::{DaemonContainer, FakeDaemon};
use support::fake_engine::{FakeEngine, container};
use support::fake_supervisor::{FakeSupervisor, running_unit, unit_named};
use support::https::{self, Agent};
use support::ws::{self, End, WsClient};
use support::{Env, PASSWORD, env};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::writer::BoxMakeWriter;

/// 2026-10-13 10:00:00 UTC en millisecondes : le démarrage des conteneurs de test.
const T0: i64 = 1_791_885_600_000;

/// Un compte qui a le droit de gérer les services (donc lit les journaux d'un service adopté).
async fn token(env: &Env, agent: &Agent, username: &str, role: Role) -> String {
    env.create(username, role).await;
    env.grant_services(username).await;
    login(agent, username).await
}

/// Un compte SANS le droit : il lit les journaux d'un service créé par Hearth, jamais ceux d'un adopté.
async fn token_without_right(env: &Env, agent: &Agent, username: &str, role: Role) -> String {
    env.create(username, role).await;
    login(agent, username).await
}

/// Adopte en base tous les conteneurs que le faux moteur connaît maintenant.
async fn adopt_all(env: &Env, engine: &FakeEngine) {
    for id in engine.container_ids() {
        env.adopt_container(&id, "adopté").await;
    }
}

/// Ouvre une session de ce compte déjà créé.
async fn login(agent: &Agent, username: &str) -> String {
    let reply = agent
        .request("POST", "/sessions")
        .json(&json!({ "username": username, "password": PASSWORD }))
        .send()
        .await;
    assert_eq!(reply.status, 201, "{:?}", reply.body);
    reply.body["token"].as_str().unwrap().to_owned()
}

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

/// L'horloge de l'agent à `T0` + 1 minute : les lignes des tests datent de `T0`, et un curseur de reprise
/// n'est cru que s'il n'est ni trop ancien ni dans le futur de l'agent (BR-SLOG-014).
fn align_clock(env: &Env) {
    let wanted = T0 / 1000 + 60;
    let now = env.clock.now().unix_timestamp();
    env.clock.advance(time::Duration::seconds(wanted - now));
}

async fn ready_agent(env: &Env, engine: &Arc<FakeEngine>, services: usize) -> Agent {
    align_clock(env);
    adopt_all(env, engine).await;
    let agent = https::start_engined(env, parts(engine.clone())).await;
    wait_listed(&agent, services).await;
    agent
}

async fn wait_listed(agent: &Agent, services: usize) {
    let registry = agent.services();
    eventually("le moteur est prêt et la liste lue", || {
        let registry = registry.clone();
        async move {
            registry.engine_status().state == hearth_agent::domain::services::EngineState::Ready
                && registry.snapshot().services.len() == services
        }
    })
    .await;
}

fn key_of(observation: &hearth_agent::domain::services::Observation) -> String {
    format!("c_{}", observation.id)
}

/// Ouvre le socket d'une lecture. Une adresse n'a que deux places d'attente d'authentification
/// (`MAX_PENDING_PER_ADDRESS`), gardées jusqu'au `open` lu par l'agent : un test qui ouvre ses lectures
/// à la file depuis `127.0.0.1` peut voir la place de la précédente rendue après sa propre demande
/// (course d'ordonnancement, vue sous Linux). Le refus `503` est alors normal et l'on réessaie ; une
/// place qui fuirait ferait échouer au délai.
async fn open_socket(agent: &Agent, key: &str) -> WsClient {
    let started = Instant::now();
    loop {
        match ws::open_logs(agent, key).await {
            Ok(client) => return client,
            Err(refused)
                if refused.status == 503 && started.elapsed() < Duration::from_secs(10) =>
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Err(refused) => panic!("socket ouvert : {refused:?}"),
        }
    }
}

/// Ouvre une lecture : socket, `auth`, `open`.
async fn read(
    agent: &Agent,
    token: &str,
    key: &str,
    tail: u32,
    since: Option<LogCursor>,
) -> WsClient {
    let mut client = open_socket(agent, key).await;
    client
        .send_value(&json!({ "type": "auth", "token": token }))
        .await;
    client
        .send_value(&LogsClientMessage::Open { tail, since })
        .await;
    client
}

/// Les messages jusqu'à avoir reçu `count` lignes : (lignes, autres messages dans l'ordre, curseur du
/// dernier lot).
async fn collect(
    client: &mut WsClient,
    count: usize,
) -> (Vec<LogItem>, Vec<LogsServerMessage>, Option<LogCursor>) {
    let (mut items, mut others, mut cursor) = (Vec::new(), Vec::new(), None);
    while items.len() < count {
        match client.next_logs().await.expect("message") {
            LogsServerMessage::Lines(lines) => {
                cursor = Some(lines.cursor);
                items.extend(lines.items);
            }
            other => others.push(other),
        }
    }
    (items, others, cursor)
}

fn texts(items: &[LogItem]) -> Vec<&str> {
    items.iter().map(|item| item.x.as_str()).collect()
}

/// Plus rien pendant ce délai.
async fn quiet(client: &mut WsClient, wait: Duration) {
    assert!(
        client.try_next_logs(wait).await.is_none(),
        "un message est arrivé alors que rien n'était attendu"
    );
}

fn status_of(message: &LogsServerMessage) -> &LogsStatus {
    let LogsServerMessage::Status(status) = message else {
        panic!("un état était attendu : {message:?}");
    };
    status
}

// ---------------------------------------------------------------------------------------------
// Ouverture, direct, lots (BR-SLOG-001, BR-SLOG-003, BR-SLOG-007)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_last_thousand_lines_then_the_live_arrive_in_under_two_seconds() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let web = container("web", RunState::Running);
    for n in 0..1500 {
        engine.push_log(&web.id, LogSource::Out, T0 + n, &format!("ligne {n}"));
    }
    let key = key_of(&web);
    let id = web.id.clone();
    engine.put_silently(web);
    let agent = ready_agent(&env, &engine, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;

    let started = Instant::now();
    let mut client = read(&agent, &admin, &key, 1000, None).await;
    let state = client.next_logs().await.expect("état");
    let status = status_of(&state);
    assert_eq!(status.state, LogState::Live);
    assert_eq!(status.container.as_deref(), Some(&id[..12]));
    assert!(!status.mixed);
    let (items, others, cursor) = collect(&mut client, 1000).await;
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    assert!(others.is_empty());
    assert_eq!(items.first().unwrap().x, "ligne 500");
    assert_eq!(items.last().unwrap().x, "ligne 1499");
    assert_eq!(items.last().unwrap().t, T0 + 1499);
    assert_eq!(
        cursor,
        Some(LogCursor {
            ts: T0 + 1499,
            rank: 0
        })
    );

    // Le direct : une ligne écrite est visible en moins de 2 secondes.
    let written = Instant::now();
    engine.push_log(&id, LogSource::Out, T0 + 2000, "écrite maintenant");
    let (live, _, _) = collect(&mut client, 1).await;
    assert_eq!(live[0].x, "écrite maintenant");
    assert!(
        written.elapsed() < Duration::from_secs(2),
        "{:?}",
        written.elapsed()
    );
    agent.shutdown().await;
}

#[tokio::test]
async fn a_service_that_has_written_less_than_a_thousand_lines_gives_them_all_and_a_silent_one_gives_none()
 {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let web = container("web", RunState::Running);
    let quiet_one = container("silencieux", RunState::Running);
    for n in 0..7 {
        engine.push_log(&web.id, LogSource::Out, T0 + n, &format!("l{n}"));
    }
    let (web_key, quiet_key) = (key_of(&web), key_of(&quiet_one));
    engine.put_silently(web);
    engine.put_silently(quiet_one);
    let agent = ready_agent(&env, &engine, 2).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let mut client = read(&agent, &admin, &web_key, 1000, None).await;
    assert_eq!(
        status_of(&client.next_logs().await.unwrap()).state,
        LogState::Live
    );
    let (items, _, _) = collect(&mut client, 7).await;
    assert_eq!(texts(&items), ["l0", "l1", "l2", "l3", "l4", "l5", "l6"]);
    let mut silent = read(&agent, &admin, &quiet_key, 1000, None).await;
    assert_eq!(
        status_of(&silent.next_logs().await.unwrap()).state,
        LogState::Live
    );
    quiet(&mut silent, Duration::from_millis(400)).await;
    agent.shutdown().await;
}

#[tokio::test]
async fn lines_leave_in_batches_of_a_hundred_milliseconds_and_never_more_than_five_hundred() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let web = container("web", RunState::Running);
    for n in 0..1000 {
        engine.push_log(&web.id, LogSource::Out, T0 + n, &format!("l{n}"));
    }
    let (key, id) = (key_of(&web), web.id.clone());
    engine.put_silently(web);
    let agent = ready_agent(&env, &engine, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let mut client = read(&agent, &admin, &key, 1000, None).await;
    assert_eq!(
        status_of(&client.next_logs().await.unwrap()).state,
        LogState::Live
    );
    let mut sizes = Vec::new();
    let mut seqs = Vec::new();
    while sizes.iter().sum::<usize>() < 1000 {
        let LogsServerMessage::Lines(lines) = client.next_logs().await.unwrap() else {
            panic!("des lignes étaient attendues");
        };
        sizes.push(lines.items.len());
        seqs.push(lines.seq);
    }
    assert!(sizes.iter().all(|size| *size <= 500), "{sizes:?}");
    assert_eq!(sizes[0], 500, "{sizes:?}");
    assert_eq!(
        seqs,
        (1..=u64::try_from(seqs.len()).unwrap()).collect::<Vec<_>>()
    );
    // Trois lignes écrites à la suite partent dans UN lot (la fenêtre de 100 ms).
    for n in 0..3 {
        engine.push_log(&id, LogSource::Out, T0 + 5000 + n, &format!("rafale {n}"));
    }
    let LogsServerMessage::Lines(burst) = client.next_logs().await.unwrap() else {
        panic!("des lignes étaient attendues");
    };
    assert_eq!(texts(&burst.items), ["rafale 0", "rafale 1", "rafale 2"]);
    agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Sorties, terminal, nettoyage (BR-SLOG-012, BR-SLOG-013)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn error_lines_are_marked_and_a_terminal_service_says_its_outputs_are_mixed() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let web = container("web", RunState::Running);
    let mut shell = container("shell", RunState::Running);
    shell.tty = true;
    engine.push_log(&web.id, LogSource::Out, T0, "normale");
    engine.push_log(&web.id, LogSource::Err, T0 + 1, "erreur");
    engine.push_log(&shell.id, LogSource::Out, T0, "invite");
    let (web_key, shell_key) = (key_of(&web), key_of(&shell));
    engine.put_silently(web);
    engine.put_silently(shell);
    let agent = ready_agent(&env, &engine, 2).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;

    let mut client = read(&agent, &admin, &web_key, 1000, None).await;
    assert!(!status_of(&client.next_logs().await.unwrap()).mixed);
    let (items, _, _) = collect(&mut client, 2).await;
    assert_eq!(items[0].s, LogSource::Out);
    assert_eq!(items[1].s, LogSource::Err);

    let mut terminal = read(&agent, &admin, &shell_key, 1000, None).await;
    let state = terminal.next_logs().await.unwrap();
    assert!(status_of(&state).mixed, "un terminal mélange ses sorties");
    let (items, _, _) = collect(&mut terminal, 1).await;
    assert_eq!(items[0].x, "invite");
    agent.shutdown().await;
}

#[tokio::test]
async fn the_text_is_cleaned_cut_and_counted_before_it_leaves_the_agent() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let web = container("web", RunState::Running);
    engine.push_log_raw(
        &web.id,
        LogSource::Out,
        T0,
        b"\x1b[31mrouge\x1b[0m\x00\x07 ok \xff fin \xe2\x80\xae renverse\n",
    );
    engine.push_log_raw(&web.id, LogSource::Out, T0 + 1, b"10%\r50%\r100%\n");
    engine.push_log(&web.id, LogSource::Out, T0 + 2, &"é".repeat(12_000));
    engine.push_log(&web.id, LogSource::Out, T0 + 3, "après");
    let key = key_of(&web);
    engine.put_silently(web);
    let agent = ready_agent(&env, &engine, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let mut client = read(&agent, &admin, &key, 1000, None).await;
    client.next_logs().await.unwrap();
    let (items, _, _) = collect(&mut client, 6).await;
    assert_eq!(items[0].x, "rouge ok \u{fffd} fin  renverse");
    assert_eq!(texts(&items[1..4]), ["10%", "50%", "100%"]);
    assert_eq!(items[4].x.chars().count(), 10_000);
    assert_eq!(items[4].cut, Some(12_000));
    assert_eq!(items[5].x, "après");
    assert_eq!(items[5].cut, None);
    agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Reprise (BR-SLOG-014)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_resume_gives_exactly_what_follows_the_cursor_even_inside_one_millisecond() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let web = container("web", RunState::Running);
    for n in 0..5 {
        engine.push_log(&web.id, LogSource::Out, T0 + 10, &format!("a{n}"));
    }
    engine.push_log(&web.id, LogSource::Out, T0 + 11, "b");
    let (key, id) = (key_of(&web), web.id.clone());
    engine.put_silently(web);
    let agent = ready_agent(&env, &engine, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;

    // Le curseur du dernier lot d'une première lecture.
    let mut first = read(&agent, &admin, &key, 1000, None).await;
    first.next_logs().await.unwrap();
    let (items, _, cursor) = collect(&mut first, 6).await;
    assert_eq!(texts(&items), ["a0", "a1", "a2", "a3", "a4", "b"]);
    let cursor = cursor.unwrap();
    assert_eq!(
        cursor,
        LogCursor {
            ts: T0 + 11,
            rank: 0
        }
    );
    first.drop_connection();

    // Des lignes arrivent pendant la coupure, dont une dans la même milliseconde que la dernière reçue.
    engine.push_log(&id, LogSource::Out, T0 + 11, "b2");
    engine.push_log(&id, LogSource::Out, T0 + 12, "c");
    let mut again = read(&agent, &admin, &key, 1000, Some(cursor)).await;
    again.next_logs().await.unwrap();
    let (items, others, last) = collect(&mut again, 2).await;
    assert_eq!(texts(&items), ["b2", "c"], "ni doublon ni perte");
    assert!(others.is_empty());
    assert_eq!(
        last,
        Some(LogCursor {
            ts: T0 + 12,
            rank: 0
        })
    );
    quiet(&mut again, Duration::from_millis(300)).await;

    // Un curseur au milieu d'un groupe de même milliseconde saute jusqu'à son rang compris.
    let mut middle = read(
        &agent,
        &admin,
        &key,
        1000,
        Some(LogCursor {
            ts: T0 + 10,
            rank: 2,
        }),
    )
    .await;
    middle.next_logs().await.unwrap();
    let (items, _, _) = collect(&mut middle, 5).await;
    assert_eq!(texts(&items), ["a3", "a4", "b", "b2", "c"]);
    agent.shutdown().await;
}

#[tokio::test]
async fn a_resume_whose_line_the_engine_no_longer_has_says_so_and_reloads_the_last_lines() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let web = container("web", RunState::Running);
    for n in 0..4 {
        engine.push_log(&web.id, LogSource::Out, T0 + n, &format!("l{n}"));
    }
    // Le moteur fait tourner ses fichiers : seules les deux dernières lignes restent.
    engine.rotate_logs(&web.id, 2);
    let key = key_of(&web);
    engine.put_silently(web);
    let agent = ready_agent(&env, &engine, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let mut client = read(
        &agent,
        &admin,
        &key,
        1000,
        Some(LogCursor { ts: T0, rank: 0 }),
    )
    .await;
    let (items, others, _) = collect(&mut client, 2).await;
    assert_eq!(texts(&items), ["l2", "l3"]);
    let gaps: Vec<_> = others
        .iter()
        .filter_map(|message| match message {
            LogsServerMessage::Gap(gap) => Some(*gap),
            _ => None,
        })
        .collect();
    assert_eq!(gaps.len(), 1, "{others:?}");
    assert_eq!(gaps[0].reason, GapReason::ResumeLost);
    assert_eq!(gaps[0].count, None);
    agent.shutdown().await;
}

#[tokio::test]
async fn a_resume_across_a_restart_puts_the_marker_between_the_two_runs() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let mut web = container("web", RunState::Running);
    // Le service a redémarré depuis la dernière ligne que le client a reçue.
    web.started_at = Some(support::fake_engine::at(12));
    engine.push_log(&web.id, LogSource::Out, T0 + 1, "ancienne 1");
    engine.push_log(&web.id, LogSource::Out, T0 + 2, "ancienne 2");
    engine.push_log(&web.id, LogSource::Out, T0 + 7_200_001, "nouvelle");
    let key = key_of(&web);
    engine.put_silently(web);
    let agent = ready_agent(&env, &engine, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let mut client = read(
        &agent,
        &admin,
        &key,
        1000,
        Some(LogCursor {
            ts: T0 + 1,
            rank: 0,
        }),
    )
    .await;
    let mut shape = Vec::new();
    while !shape.iter().any(|entry: &String| entry == "lines:nouvelle") {
        shape.push(match client.next_logs().await.unwrap() {
            LogsServerMessage::Status(_) => "status".to_owned(),
            LogsServerMessage::Marker(_) => "marker".to_owned(),
            LogsServerMessage::Lines(lines) => format!("lines:{}", texts(&lines.items).join("+")),
            other => panic!("{other:?}"),
        });
    }
    assert_eq!(
        shape,
        ["status", "lines:ancienne 2", "marker", "lines:nouvelle"],
        "les dernières lignes de l'ancienne exécution précèdent le repère"
    );
    agent.shutdown().await;
}

#[tokio::test]
async fn a_resume_that_is_up_to_date_does_not_resend_anything_when_the_service_stops() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let web = container("web", RunState::Running);
    engine.push_log(&web.id, LogSource::Out, T0 + 1, "l1");
    engine.push_log(&web.id, LogSource::Out, T0 + 2, "l2");
    let (key, id) = (key_of(&web), web.id.clone());
    engine.put_silently(web);
    let agent = ready_agent(&env, &engine, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let mut client = read(
        &agent,
        &admin,
        &key,
        1000,
        Some(LogCursor {
            ts: T0 + 2,
            rank: 0,
        }),
    )
    .await;
    assert_eq!(
        status_of(&client.next_logs().await.unwrap()).state,
        LogState::Live
    );
    quiet(&mut client, Duration::from_millis(300)).await;
    engine.update(&id, |o| {
        o.run_state = RunState::Exited;
        o.exit_code = Some(1);
        o.finished_at = Some(support::fake_engine::at(11));
    });
    engine.emit(&id, "die");
    assert_eq!(
        status_of(&client.next_logs().await.unwrap()).state,
        LogState::Ended
    );
    quiet(&mut client, Duration::from_millis(700)).await;
    agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Service arrêté, redémarré, recréé par le système (BR-SLOG-008, BR-SLOG-009)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_stopped_service_gives_its_last_lines_and_says_it_is_stopped() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let mut web = container("web", RunState::Exited);
    web.exit_code = Some(3);
    engine.push_log(&web.id, LogSource::Out, T0, "avant la panne");
    engine.push_log(
        &web.id,
        LogSource::Err,
        T0 + 1,
        "erreur fatale : disque plein",
    );
    let key = key_of(&web);
    engine.put_silently(web);
    let agent = ready_agent(&env, &engine, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let mut client = read(&agent, &admin, &key, 1000, None).await;
    let state = client.next_logs().await.unwrap();
    let status = status_of(&state);
    assert_eq!(status.state, LogState::Ended);
    assert_eq!(status.exit_code, Some(3));
    assert!(status.ended_at.is_some());
    assert!(!status.not_kept);
    let (items, _, _) = collect(&mut client, 2).await;
    assert_eq!(
        texts(&items),
        ["avant la panne", "erreur fatale : disque plein"]
    );
    quiet(&mut client, Duration::from_millis(400)).await;
    agent.shutdown().await;
}

#[tokio::test]
async fn a_service_that_starts_again_continues_after_a_restart_marker_without_repeating_lines() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let web = container("web", RunState::Running);
    engine.push_log(&web.id, LogSource::Out, T0 + 1, "première exécution");
    let (key, id) = (key_of(&web), web.id.clone());
    engine.put_silently(web);
    let agent = ready_agent(&env, &engine, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let mut client = read(&agent, &admin, &key, 1000, None).await;
    assert_eq!(
        status_of(&client.next_logs().await.unwrap()).state,
        LogState::Live
    );
    let (items, _, _) = collect(&mut client, 1).await;
    assert_eq!(texts(&items), ["première exécution"]);

    // Le service s'arrête en erreur : un état « arrêté » avec le code.
    engine.update(&id, |o| {
        o.run_state = RunState::Exited;
        o.exit_code = Some(1);
        o.finished_at = Some(support::fake_engine::at(11));
    });
    engine.emit(&id, "die");
    let ended = client.next_logs().await.unwrap();
    assert_eq!(status_of(&ended).state, LogState::Ended);
    assert_eq!(status_of(&ended).exit_code, Some(1));

    // Il redémarre (même conteneur, nouveau démarrage) : repère, état, puis seulement les nouvelles lignes.
    engine.update(&id, |o| {
        o.run_state = RunState::Running;
        o.exit_code = None;
        o.started_at = Some(support::fake_engine::at(12));
    });
    engine.push_log(
        &id,
        LogSource::Out,
        T0 + 7_200_000 + 1,
        "deuxième exécution",
    );
    engine.emit(&id, "start");
    let (items, others, _) = collect(&mut client, 1).await;
    assert_eq!(texts(&items), ["deuxième exécution"]);
    assert!(
        others
            .iter()
            .any(|message| matches!(message, LogsServerMessage::Marker(marker) if marker.kind == MarkerKind::Restarted)),
        "{others:?}"
    );
    assert!(
        others.iter().any(
            |message| matches!(message, LogsServerMessage::Status(s) if s.state == LogState::Live)
        ),
        "{others:?}"
    );
    agent.shutdown().await;
}

#[tokio::test]
async fn a_service_run_by_a_unit_is_followed_across_its_containers_by_its_key() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let supervisor = Arc::new(FakeSupervisor::new());
    let unit = unit_named("docker-terraria.service");
    let mut first = container("terraria", RunState::Running);
    first.id = "a".repeat(64);
    supervisor.set_owner(&first.id, &unit);
    supervisor.set_status(&unit, running_unit());
    engine.push_log(&first.id, LogSource::Out, T0 + 1, "monde chargé");
    engine.put_silently(first.clone());
    env.adopt("u_docker-terraria.service", "terraria", "marie")
        .await;
    let agent = https::start_engined(
        &env,
        parts_supervised(engine.clone(), supervisor.clone(), fast_timings()),
    )
    .await;
    wait_listed(&agent, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let key = "u_docker-terraria.service";
    let mut client = read(&agent, &admin, key, 1000, None).await;
    assert_eq!(
        status_of(&client.next_logs().await.unwrap()).state,
        LogState::Live
    );
    let (items, _, _) = collect(&mut client, 1).await;
    assert_eq!(texts(&items), ["monde chargé"]);

    // Le système arrête le service : son conteneur est supprimé, le moteur ne garde rien.
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
    let waiting = loop {
        let message = client.next_logs().await.unwrap();
        if matches!(&message, LogsServerMessage::Status(s) if s.state == LogState::Waiting) {
            break message;
        }
    };
    let status = status_of(&waiting);
    assert!(
        status.not_kept,
        "le moteur n'a pas gardé les journaux de ce service"
    );
    assert_eq!(status.container, None);

    // Le système le relance : un autre conteneur, un autre identifiant, la même clé.
    let mut second = container("terraria", RunState::Running);
    second.id = "b".repeat(64);
    supervisor.set_owner(&second.id, &unit);
    supervisor.set_status(&unit, running_unit());
    engine.push_log(&second.id, LogSource::Out, T0 + 9_000, "monde rechargé");
    engine.put(second.clone(), "create");
    engine.emit(&second.id, "start");
    let (items, others, _) = collect(&mut client, 1).await;
    assert_eq!(
        texts(&items),
        ["monde rechargé"],
        "seules les lignes de la nouvelle exécution"
    );
    let marker = others
        .iter()
        .position(|m| matches!(m, LogsServerMessage::Marker(m) if m.kind == MarkerKind::Restarted));
    let live = others
        .iter()
        .position(|m| matches!(m, LogsServerMessage::Status(s) if s.state == LogState::Live));
    assert!(marker.is_some() && live.is_some(), "{others:?}");
    assert!(
        marker < live,
        "le repère précède le nouvel état : {others:?}"
    );
    agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Erreurs de l'ouverture (BR-SLOG-015 à 017, 020)
// ---------------------------------------------------------------------------------------------

async fn expect_error(client: &mut WsClient, code: LogsErrorCode) {
    match client.next_logs().await.expect("une erreur") {
        LogsServerMessage::Error(error) => assert_eq!(error.code, code),
        other => panic!("l'erreur {code:?} était attendue : {other:?}"),
    }
    assert!(matches!(client.next_logs().await, Err(End::Closed(1000))));
}

#[tokio::test]
async fn unreadable_logs_an_unknown_service_and_a_missing_engine_are_typed_errors() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let mut silent_driver = container("muet", RunState::Running);
    silent_driver.log_driver = Some("none".into());
    let key = key_of(&silent_driver);
    engine.put_silently(silent_driver);
    let agent = ready_agent(&env, &engine, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;

    // Pilote `none`, connu d'avance par l'inspection.
    let mut client = read(&agent, &admin, &key, 1000, None).await;
    expect_error(&mut client, LogsErrorCode::LogsUnreadable).await;
    // Aucun service de cette clé.
    let mut client = read(&agent, &admin, &format!("c_{}", "f".repeat(64)), 1000, None).await;
    expect_error(&mut client, LogsErrorCode::ServiceNotFound).await;
    // Une clé qui n'a aucune forme de clé : refusée avant la mise à niveau.
    assert_eq!(
        ws::open_logs(&agent, "pas-une-cle")
            .await
            .err()
            .map(|r| r.status),
        Some(404)
    );
    // Moteur pas prêt.
    engine.set_outcome(hearth_agent::domain::services::ProbeOutcome::NoAnswer, true);
    let registry = agent.services();
    eventually("le moteur ne répond plus", || {
        let registry = registry.clone();
        async move {
            registry.engine_status().state != hearth_agent::domain::services::EngineState::Ready
        }
    })
    .await;
    let mut client = read(&agent, &admin, &key, 1000, None).await;
    expect_error(&mut client, LogsErrorCode::EngineUnavailable).await;
    agent.shutdown().await;
}

#[tokio::test]
async fn an_engine_that_cannot_read_the_logs_ends_the_reading_with_a_typed_error() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let web = container("web", RunState::Running);
    let (key, id) = (key_of(&web), web.id.clone());
    engine.put_silently(web);
    engine.fail_logs(
        &id,
        Some(hearth_agent::application::ports::EngineError::Unsupported),
    );
    let agent = ready_agent(&env, &engine, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let mut client = read(&agent, &admin, &key, 1000, None).await;
    let state = client.next_logs().await.unwrap();
    assert_eq!(status_of(&state).state, LogState::Live);
    expect_error(&mut client, LogsErrorCode::LogsUnreadable).await;
    engine.fail_logs(
        &id,
        Some(hearth_agent::application::ports::EngineError::Unreachable),
    );
    let mut client = read(&agent, &admin, &key, 1000, None).await;
    client.next_logs().await.unwrap();
    expect_error(&mut client, LogsErrorCode::EngineUnavailable).await;
    agent.shutdown().await;
}

#[tokio::test]
async fn a_service_that_disappears_during_the_reading_ends_it_with_service_not_found() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let web = container("web", RunState::Running);
    let (key, id) = (key_of(&web), web.id.clone());
    engine.push_log(&id, LogSource::Out, T0, "dernière ligne");
    engine.put_silently(web);
    let agent = ready_agent(&env, &engine, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let mut client = read(&agent, &admin, &key, 1000, None).await;
    client.next_logs().await.unwrap();
    let (items, _, _) = collect(&mut client, 1).await;
    assert_eq!(texts(&items), ["dernière ligne"]);
    engine.remove(&id);
    expect_error(&mut client, LogsErrorCode::ServiceNotFound).await;
    agent.shutdown().await;
}

#[tokio::test]
async fn only_ten_readings_are_open_at_once_and_closing_one_gives_its_place_back() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let web = container("web", RunState::Running);
    let key = key_of(&web);
    engine.put_silently(web);
    let agent = ready_agent(&env, &engine, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let readonly = token(&env, &agent, "lucas", Role::ReadOnly).await;
    let mut open = Vec::new();
    for round in 0..10 {
        // Les dix places sont communes à tous les comptes.
        let who = if round % 2 == 0 { &admin } else { &readonly };
        let mut client = read(&agent, who, &key, 1000, None).await;
        assert_eq!(
            status_of(&client.next_logs().await.unwrap()).state,
            LogState::Live
        );
        open.push(client);
    }
    assert_eq!(engine.open_log_reads(), 10);
    let mut eleventh = read(&agent, &admin, &key, 1000, None).await;
    expect_error(&mut eleventh, LogsErrorCode::LogsLimit).await;

    // Une lecture suspendue par une coupure du lien (le client disparaît sans fermer) libère sa place,
    // et la lecture du moteur s'arrête côté serveur.
    open.pop().unwrap().drop_connection();
    eventually("la place est rendue", || {
        let engine = engine.clone();
        async move { engine.open_log_reads() == 9 }
    })
    .await;
    let mut again = read(&agent, &admin, &key, 1000, None).await;
    assert_eq!(
        status_of(&again.next_logs().await.unwrap()).state,
        LogState::Live
    );

    // Tout fermer arrête toutes les lectures du moteur.
    drop(open);
    drop(again);
    eventually("plus aucune lecture ouverte", || {
        let engine = engine.clone();
        async move { engine.open_log_reads() == 0 }
    })
    .await;
    agent.shutdown().await;
}

#[tokio::test]
async fn both_roles_read_the_same_lines_and_a_refused_token_gets_a_typed_error() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let web = container("web", RunState::Running);
    engine.push_log(&web.id, LogSource::Out, T0, "pour tout le monde");
    let key = key_of(&web);
    engine.put_silently(web);
    let agent = ready_agent(&env, &engine, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let readonly = token(&env, &agent, "lucas", Role::ReadOnly).await;
    for who in [&admin, &readonly] {
        let mut client = read(&agent, who, &key, 1000, None).await;
        client.next_logs().await.unwrap();
        let (items, _, _) = collect(&mut client, 1).await;
        assert_eq!(texts(&items), ["pour tout le monde"]);
        // Le battement du client reçoit son écho.
        client.send_value(&LogsClientMessage::Ping { n: 7 }).await;
        assert_eq!(
            client.next_logs().await.unwrap(),
            LogsServerMessage::Pong { n: 7 }
        );
    }
    let mut refused = read(&agent, "pas-un-jeton", &key, 1000, None).await;
    match refused.next_logs().await.unwrap() {
        LogsServerMessage::Error(error) => assert_eq!(error.code, LogsErrorCode::Unauthenticated),
        other => panic!("{other:?}"),
    }
    assert!(matches!(refused.next_logs().await, Err(End::Closed(_))));
    // Un premier message qui n'est pas `auth`.
    let mut rude = ws::open_logs(&agent, &key).await.unwrap();
    rude.send_value(&LogsClientMessage::Ping { n: 1 }).await;
    match rude.next_logs().await.unwrap() {
        LogsServerMessage::Error(error) => assert_eq!(error.code, LogsErrorCode::Unauthenticated),
        other => panic!("{other:?}"),
    }
    agent.shutdown().await;
}

#[tokio::test]
async fn reading_the_logs_writes_nothing_to_the_journal_or_the_operations_queue() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let web = container("web", RunState::Running);
    engine.push_log(&web.id, LogSource::Out, T0, "une ligne");
    let key = key_of(&web);
    engine.put_silently(web);
    let agent = ready_agent(&env, &engine, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let readonly = token(&env, &agent, "lucas", Role::ReadOnly).await;
    let count = |sql: &'static str| {
        let pool = env.db.pool().clone();
        async move {
            sqlx::query_scalar::<_, i64>(sql)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    let (audit, operations) = (
        count("SELECT COUNT(*) FROM audit_events").await,
        count("SELECT COUNT(*) FROM operations").await,
    );
    for who in [&admin, &readonly] {
        let mut client = read(&agent, who, &key, 1000, None).await;
        client.next_logs().await.unwrap();
        collect(&mut client, 1).await;
        client.drop_connection();
    }
    let mut refused = read(&agent, "pas-un-jeton", &key, 1000, None).await;
    refused.next_logs().await.unwrap();
    assert_eq!(
        count("SELECT COUNT(*) FROM audit_events").await,
        audit,
        "lire n'est pas journalisé"
    );
    assert_eq!(count("SELECT COUNT(*) FROM operations").await, operations);
    agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Débit : 1 000 lignes par seconde, lecteur lent (BR-SLOG-007)
// ---------------------------------------------------------------------------------------------

/// Combien de secondes dure le débit soutenu en local : quelques secondes ; la version longue est celle de
/// la CI (`engine_real`).
fn rate_seconds() -> u64 {
    std::env::var("HEARTH_LOG_RATE_SECONDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(3)
}

#[tokio::test]
async fn a_thousand_lines_a_second_reach_a_reader_that_keeps_up_without_loss() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let web = container("web", RunState::Running);
    let (key, id) = (key_of(&web), web.id.clone());
    engine.put_silently(web);
    let agent = ready_agent(&env, &engine, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let mut client = read(&agent, &admin, &key, 1000, None).await;
    client.next_logs().await.unwrap();

    let total = usize::try_from(rate_seconds() * 1000).unwrap();
    let produced_at = Arc::new(Mutex::new(Vec::<Instant>::with_capacity(total)));
    let producer = tokio::spawn({
        let (engine, produced_at) = (engine.clone(), produced_at.clone());
        async move {
            let started = Instant::now();
            let mut sent = 0usize;
            while sent < total {
                // Au rythme de l'horloge : 1 000 lignes par seconde, par paquets (la minuterie de
                // Windows ne tient pas la milliseconde).
                let due = usize::try_from(started.elapsed().as_millis())
                    .unwrap()
                    .min(total);
                while sent < due {
                    produced_at.lock().unwrap().push(Instant::now());
                    engine.push_log(
                        &id,
                        LogSource::Out,
                        T0 + i64::try_from(sent).unwrap(),
                        &format!("n={sent} une ligne de journal ordinaire"),
                    );
                    sent += 1;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }
    });
    let mut received = 0usize;
    let mut worst = Duration::ZERO;
    let mut expected = 0usize;
    while received < total {
        match client.next_logs().await.expect("message") {
            LogsServerMessage::Lines(lines) => {
                let now = Instant::now();
                for item in &lines.items {
                    let n: usize = item
                        .x
                        .strip_prefix("n=")
                        .unwrap()
                        .split(' ')
                        .next()
                        .unwrap()
                        .parse()
                        .unwrap();
                    assert_eq!(n, expected, "ni perte ni désordre");
                    expected += 1;
                    let at = produced_at.lock().unwrap()[n];
                    worst = worst.max(now.duration_since(at));
                }
                received += lines.items.len();
            }
            other => panic!("aucun trou n'était attendu : {other:?}"),
        }
    }
    producer.await.unwrap();
    assert!(
        worst < Duration::from_secs(2),
        "la ligne la plus lente : {worst:?}"
    );
    agent.shutdown().await;
}

#[tokio::test]
async fn a_slow_reader_loses_the_oldest_lines_counted_and_the_stream_goes_on() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let web = container("web", RunState::Running);
    let (key, id) = (key_of(&web), web.id.clone());
    engine.put_silently(web);
    let agent = ready_agent(&env, &engine, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    // Le flux commun, ouvert avant : il doit répondre encore après.
    let mut common = ws::open(&agent).await;
    common.auth(&admin).await;
    common.send(&ClientMessage::Ping { n: 1 }).await;
    assert!(matches!(
        common.expect().await,
        ServerMessage::Pong { n: 1 }
    ));
    // Un client dont la fenêtre de réception est minuscule et qui ne lit pas.
    let mut client = ws::open_logs_small_buffer(&agent, &key).await.unwrap();
    client
        .send_value(&json!({ "type": "auth", "token": admin }))
        .await;
    client
        .send_value(&LogsClientMessage::Open {
            tail: 1000,
            since: None,
        })
        .await;
    eventually("la lecture est ouverte", || {
        let engine = engine.clone();
        async move { engine.open_log_reads() == 1 }
    })
    .await;

    // Le service écrit beaucoup plus vite que le client ne lit : le moteur n'est jamais ralenti.
    let produced = 150_000usize;
    let started = Instant::now();
    for n in 0..produced {
        engine.push_log(
            &id,
            LogSource::Out,
            // Écrites après l'ouverture de la lecture (l'horloge de l'agent est à T0 + 1 minute) : du
            // direct, pas de l'historique.
            T0 + 3_600_000 + i64::try_from(n).unwrap(),
            &format!("n={n} {}", "x".repeat(100)),
        );
    }
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "le producteur n'attend jamais le lecteur : {:?}",
        started.elapsed()
    );
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Le client se réveille : ce qu'il reçoit + ce que les trous comptent = tout ce qui a été écrit.
    let (mut delivered, mut lost, mut gaps) = (0usize, 0u64, 0usize);
    let mut previous: Option<usize> = None;
    let mut seen_all = false;
    while !seen_all {
        let Some(message) = client.try_next_logs(Duration::from_secs(3)).await else {
            break;
        };
        match message.expect("le flux ne se ferme pas") {
            LogsServerMessage::Lines(lines) => {
                for item in &lines.items {
                    let n: usize = item
                        .x
                        .strip_prefix("n=")
                        .unwrap()
                        .split(' ')
                        .next()
                        .unwrap()
                        .parse()
                        .unwrap();
                    assert!(
                        previous.is_none_or(|p| n > p),
                        "l'ordre est celui du moteur"
                    );
                    previous = Some(n);
                    delivered += 1;
                    seen_all |= n == produced - 1;
                }
            }
            LogsServerMessage::Gap(gap) => {
                assert_eq!(gap.reason, GapReason::Slow);
                lost += gap.count.expect("une perte est comptée");
                gaps += 1;
            }
            LogsServerMessage::Status(_) => {}
            other => panic!("{other:?}"),
        }
    }
    assert!(
        gaps > 0 && lost > 0,
        "un lecteur lent perd des lignes : {delivered} reçues"
    );
    assert_eq!(
        u64::try_from(delivered).unwrap() + lost,
        u64::try_from(produced).unwrap(),
        "tout est reçu ou compté"
    );
    // Le flux commun n'a pas été touché : `/stream` répond toujours.
    common.send(&ClientMessage::Ping { n: 2 }).await;
    assert!(matches!(
        common.expect().await,
        ServerMessage::Pong { n: 2 }
    ));
    agent.shutdown().await;
}

#[tokio::test]
async fn the_engine_socket_is_drained_whatever_the_client_does_with_the_real_adapter() {
    // Le VRAI adaptateur sur un faux démon : si l'agent cessait de lire le moteur parce que le client est
    // lent, le démon ne parviendrait pas à écrire ses messages.
    let env = env().await;
    let daemon = FakeDaemon::start().await;
    let container = DaemonContainer::new("bavard", "running");
    let (id, key) = (container.id.clone(), format!("c_{}", container.id));
    daemon.put(container);
    env.adopt_container(&id, "bavard").await;
    let engine = BollardEngine::new(
        Target::Http(daemon.address()),
        InstallMarkers::at(Vec::<std::path::PathBuf>::new()),
    );
    let agent = https::start_engined(
        &env,
        hearth_agent::app::EngineParts {
            engine: Arc::new(engine),
            supervisor: Arc::new(FakeSupervisor::new()),
            host: Arc::new(support::engine::TestHost),
            timings: fast_timings(),
            log_brake: hearth_agent::domain::services::BrakeSettings::relaxed(),
            lifecycle: support::engine::fast_lifecycle(),
        },
    )
    .await;
    wait_listed(&agent, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let mut client = ws::open_logs_small_buffer(&agent, &key).await.unwrap();
    client
        .send_value(&json!({ "type": "auth", "token": admin }))
        .await;
    client
        .send_value(&LogsClientMessage::Open {
            tail: 1000,
            since: None,
        })
        .await;
    eventually("la lecture est ouverte", || {
        let daemon = &daemon;
        async move { daemon.requests().iter().any(|r| r.contains("/logs")) }
    })
    .await;
    let produced = 40_000usize;
    for n in 0..produced {
        daemon.push_log(
            &id,
            1,
            T0 + i64::try_from(n).unwrap(),
            &format!("n={n} {}", "y".repeat(100)),
        );
    }
    eventually(
        "le démon a pu écrire tout ce que le service a écrit",
        || {
            let daemon = &daemon;
            let id = id.clone();
            async move { daemon.served_logs(&id) == produced }
        },
    )
    .await;
    // Le client lit enfin : tout est reçu ou compté.
    let (mut delivered, mut lost) = (0u64, 0u64);
    loop {
        let Some(message) = client.try_next_logs(Duration::from_secs(2)).await else {
            break;
        };
        match message.expect("le flux ne se ferme pas") {
            LogsServerMessage::Lines(lines) => {
                delivered += u64::try_from(lines.items.len()).unwrap()
            }
            LogsServerMessage::Gap(gap) => lost += gap.count.unwrap(),
            _ => {}
        }
    }
    assert!(
        lost > 0,
        "le client ne lisait pas : des lignes ont été perdues"
    );
    assert_eq!(delivered + lost, u64::try_from(produced).unwrap());
    agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Aucune ligne dans une trace, la base, le journal ni la file des opérations (BR-SLOG-019, BR-SLOG-022)
// ---------------------------------------------------------------------------------------------

/// Les traces de tout le binaire de test, capturées en mémoire par le MÊME chemin d'initialisation que la
/// production (`logging::try_init_with`, filtre de sécurité et pont `log` compris), au niveau le plus
/// bavard, avec des directives qui visent le transport lui-même.
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
                EnvFilter::new(
                    "trace,bollard=trace,bollard::docker=trace,hyper_util=trace,tungstenite=trace,tokio_tungstenite=trace",
                ),
            )
            .expect("première installation du journal de ce binaire de test");
            buffer
        })
        .clone()
}

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
async fn no_line_reaches_a_trace_the_database_the_journal_or_the_operations_queue() {
    let traces = captured_traces();
    // Témoins : sans eux, un tampon vide ou un pont `log` absent rendraient le test aveugle.
    log::debug!(target: "hearth_witness", "witness-log-line");
    log::debug!(target: "bollard::docker", "witness-bollard-debug");
    log::warn!(target: "bollard::docker", "witness-bollard-warn");
    let env = env().await;
    align_clock(&env);
    // Le VRAI adaptateur sur un faux démon dont les journaux portent des phrases sentinelles : sur les deux
    // sorties, en couleur, coupées sur plusieurs messages, une ligne géante, et un conteneur dont les
    // journaux ne se relisent pas (l'erreur du moteur ne doit pas fuir non plus).
    let daemon = FakeDaemon::start().await;
    let talkative = DaemonContainer::new("bavard", "running");
    let (id, key) = (talkative.id.clone(), format!("c_{}", talkative.id));
    daemon.put(talkative);
    let mut muted = DaemonContainer::new("muet", "running");
    muted.log_driver = "none".into();
    let muted_key = format!("c_{}", muted.id);
    daemon.put(muted);
    // Un conteneur à terminal (flux brut, une seule sortie), avec une ligne de contrôle et une longue ligne
    // en trois messages de 16 Kio.
    let mut terminal = DaemonContainer::new("terminal", "running");
    terminal.tty = true;
    let (terminal_id, terminal_key) = (terminal.id.clone(), format!("c_{}", terminal.id));
    daemon.put(terminal);
    daemon.push_log(&terminal_id, 1, T0, "SENTINEL-LINE-TTY-1a2b");
    daemon.push_log(
        &terminal_id,
        1,
        T0 + 1,
        "\u{1}\u{1}SENTINEL-LINE-TTY-CTRL-4d5e",
    );
    for part in 0..3 {
        let mut chunk = if part == 0 {
            b"SENTINEL-LINE-TTY-GIANT-".to_vec()
        } else {
            Vec::new()
        };
        chunk.resize(16 * 1024, b'q');
        daemon.push_log_raw(&terminal_id, 1, T0 + 2 + part, &chunk);
    }
    daemon.push_log_raw(&terminal_id, 1, T0 + 5, b"fin\n");
    // Un « moteur » dont les trames mentent (type de flux inconnu), sentinelles déjà dans le tampon.
    let liar = DaemonContainer::new("menteur", "running");
    let (liar_id, liar_key) = (liar.id.clone(), format!("c_{}", liar.id));
    daemon.put(liar);
    daemon.push_log(&liar_id, 9, T0, "SENTINEL-LINE-BADFRAME-6c0d");
    // Un service bavard que son lecteur ne vide pas.
    let slow = DaemonContainer::new("lent", "running");
    let (slow_id, slow_key) = (slow.id.clone(), format!("c_{}", slow.id));
    daemon.put(slow);
    // Un conteneur que personne n'a adopté (HRT-79) : ses journaux portent des sentinelles. Elles ne
    // doivent sortir par aucune route ni aucune trace (BR-ADOPT-001, BR-ADOPT-004).
    let hidden = DaemonContainer::new("cache", "running");
    let (hidden_id, hidden_key) = (hidden.id.clone(), format!("c_{}", hidden.id));
    daemon.put(hidden);
    daemon.push_log(&hidden_id, 1, T0, "SENTINEL-LINE-HIDDEN-d2f9 jeton=cache");
    daemon.push_log(&hidden_id, 2, T0 + 1, "SENTINEL-LINE-HIDDEN-ERR-81ca");
    for adopted in [
        id.as_str(),
        muted_key.trim_start_matches("c_"),
        terminal_id.as_str(),
        liar_id.as_str(),
        slow_id.as_str(),
    ] {
        env.adopt_container(adopted, "adopté").await;
    }
    daemon.push_log(&id, 1, T0, "SENTINEL-LINE-OUT-5e21 mot de passe=hunter2");
    daemon.push_log(&id, 2, T0 + 1, "\u{1b}[31mSENTINEL-LINE-ERR-9a07\u{1b}[0m");
    daemon.push_log_raw(&id, 1, T0 + 2, b"SENTINEL-LINE-SPLIT-");
    daemon.push_log_raw(&id, 1, T0 + 3, b"3c18\n");
    daemon.push_log(
        &id,
        1,
        T0 + 4,
        &format!("SENTINEL-LINE-GIANT-77d4{}", "z".repeat(30_000)),
    );
    let engine = BollardEngine::new(
        Target::Http(daemon.address()),
        InstallMarkers::at(Vec::<std::path::PathBuf>::new()),
    );
    let agent = https::start_engined(
        &env,
        hearth_agent::app::EngineParts {
            engine: Arc::new(engine),
            supervisor: Arc::new(FakeSupervisor::new()),
            host: Arc::new(support::engine::TestHost),
            timings: fast_timings(),
            log_brake: hearth_agent::domain::services::BrakeSettings::relaxed(),
            lifecycle: support::engine::fast_lifecycle(),
        },
    )
    .await;
    wait_listed(&agent, 5).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let readonly = token(&env, &agent, "lucas", Role::ReadOnly).await;

    let mut shown = String::new();
    for who in [&admin, &readonly] {
        let mut client = read(&agent, who, &key, 1000, None).await;
        let (items, others, _) = collect(&mut client, 4).await;
        // Les lignes arrivent bien au client : les sentinelles y sont (le test n'est pas aveugle)...
        let text = texts(&items).join("\n");
        assert!(text.contains("SENTINEL-LINE-OUT-5e21") && text.contains("SENTINEL-LINE-ERR-9a07"));
        assert!(
            text.contains("SENTINEL-LINE-SPLIT-3c18") && text.contains("SENTINEL-LINE-GIANT-77d4")
        );
        shown.push_str(&text);
        shown.push_str(&format!("{others:?}"));
        client.drop_connection();
    }
    let mut unreadable = read(&agent, &admin, &muted_key, 1000, None).await;
    expect_error(&mut unreadable, LogsErrorCode::LogsUnreadable).await;
    // Le mode terminal : les sentinelles arrivent (le test n'est pas aveugle), aucune trace.
    let mut on_terminal = read(&agent, &admin, &terminal_key, 1000, None).await;
    let (items, others, _) = collect(&mut on_terminal, 3).await;
    let text = texts(&items).join("\n");
    assert!(
        text.contains("SENTINEL-LINE-TTY-1a2b") && text.contains("SENTINEL-LINE-TTY-CTRL-4d5e")
    );
    assert!(text.contains("SENTINEL-LINE-TTY-GIANT-"));
    shown.push_str(&format!("{text}{others:?}"));
    on_terminal.drop_connection();
    // Une trame refusée, avec les sentinelles déjà reçues dans le tampon.
    let mut lying = read(&agent, &admin, &liar_key, 1000, None).await;
    loop {
        match lying.next_logs().await {
            Ok(LogsServerMessage::Error(error)) => {
                assert_eq!(error.code, LogsErrorCode::EngineUnavailable);
                break;
            }
            Ok(_) => {}
            Err(end) => panic!("fermé sans erreur typée : {end:?}"),
        }
    }
    // Une reprise : les lignes sautées jusqu'au curseur, puis une reprise perdue.
    let mut resumed = read(
        &agent,
        &admin,
        &key,
        1000,
        Some(LogCursor {
            ts: T0 + 1,
            rank: 0,
        }),
    )
    .await;
    let (items, others, _) = collect(&mut resumed, 2).await;
    let text = texts(&items).join("\n");
    assert!(text.contains("SENTINEL-LINE-SPLIT-3c18") && !text.contains("SENTINEL-LINE-OUT-5e21"));
    shown.push_str(&format!("{text}{others:?}"));
    resumed.drop_connection();
    let mut lost = read(
        &agent,
        &admin,
        &key,
        1000,
        Some(LogCursor { ts: 0, rank: 0 }),
    )
    .await;
    let (items, others, _) = collect(&mut lost, 4).await;
    assert!(others.iter().any(is_resume_lost), "{others:?}");
    shown.push_str(&texts(&items).join("\n"));
    lost.drop_connection();
    // Un lecteur lent : l'anneau jette des lignes pendant que le client ne lit rien.
    let mut slow_client = ws::open_logs_small_buffer(&agent, &slow_key).await.unwrap();
    slow_client
        .send_value(&json!({ "type": "auth", "token": admin }))
        .await;
    slow_client
        .send_value(&LogsClientMessage::Open {
            tail: 1000,
            since: None,
        })
        .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let filler = "z".repeat(240);
    for n in 0..14_000i64 {
        daemon.push_log(
            &slow_id,
            1,
            T0 + 3_600_000 + n,
            &format!("SENTINEL-LINE-SLOW-{n} {filler}"),
        );
    }
    tokio::time::sleep(Duration::from_secs(2)).await;
    slow_client.drop_connection();
    // Un client dont le jeton est faux, et un service inconnu.
    let mut refused = read(&agent, "faux", &key, 1000, None).await;
    refused.next_logs().await.unwrap();
    let mut unknown = read(&agent, &admin, &format!("c_{}", "d".repeat(64)), 1000, None).await;
    expect_error(&mut unknown, LogsErrorCode::ServiceNotFound).await;
    // Le conteneur caché : même réponse que le service inconnu, pour les deux comptes, et pas une ligne.
    for who in [&admin, &readonly] {
        let mut refused_hidden = read(&agent, who, &hidden_key, 1000, None).await;
        let (messages, end) = until_closed(&mut refused_hidden).await;
        assert_eq!(
            messages,
            [LogsServerMessage::Error(
                hearth_proto::api::service_logs::LogsError::new(LogsErrorCode::ServiceNotFound)
            )]
        );
        assert_eq!(end, End::Closed(1000));
        shown.push_str(&format!("{messages:?}"));
    }
    // Le démon tombe en pleine lecture : l'erreur de transport ne cite aucune ligne.
    let mut cut = read(&agent, &admin, &key, 1000, None).await;
    cut.next_logs().await.unwrap();
    daemon.remove(&id);
    daemon.push_log_raw(&id, 1, T0 + 5, b"SENTINEL-LINE-AFTER-0b6f\n");
    tokio::time::sleep(Duration::from_millis(300)).await;
    cut.drop_connection();

    // Les messages d'état et de repère (hors lignes) ne contiennent pas non plus le texte.
    assert!(!shown.is_empty());
    assert!(
        !shown.contains("SENTINEL-LINE-HIDDEN"),
        "une ligne d'un conteneur caché est sortie"
    );
    let data_dir = env.dir.path().to_owned();
    agent.shutdown().await;
    let captured = String::from_utf8_lossy(&traces.lock().unwrap()).into_owned();
    assert!(
        captured.contains("witness-log-line"),
        "le pont `log` doit être actif"
    );
    assert!(captured.contains("witness-bollard-warn"));
    assert!(
        !captured.contains("witness-bollard-debug"),
        "`bollard` ne sort pas sous `warn`"
    );
    assert!(
        captured.contains("lecture de journaux ouverte"),
        "la trace d'ouverture existe : le test n'est pas aveugle sur nos propres traces"
    );
    for forbidden in ["SENTINEL-LINE", "hunter2", "zzzzzzzzzz"] {
        assert!(!captured.contains(forbidden), "{forbidden} dans les traces");
    }
    // La base (fichier, journal d'écriture compris) et les tables du journal et de la file n'en gardent rien.
    let files = all_bytes(&data_dir);
    for forbidden in ["SENTINEL-LINE", "hunter2"] {
        assert!(
            !contains(&files, forbidden),
            "{forbidden} dans les fichiers de l'agent"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Corrections de la review locale : reprise bornée (B1), repères de redémarrage (I3), première page (I4),
// horloge qui recule (I5), contrôle d'accès rejoué (I2), cycle de vie du socket (I6)
// ---------------------------------------------------------------------------------------------

use hearth_agent::application::service_logs::{
    LogSession, LogSettings, OpenRequest, ReadPolicy, ServiceLogs,
};
use hearth_agent::domain::accounts::{AccountId, ServicesRight};
use hearth_agent::domain::services::ServiceKey;
use hearth_agent::domain::services::logs::{LogTarget, ReadRefusal, Viewer};
use std::sync::atomic::{AtomicBool, Ordering};

fn viewer(role: Role) -> Viewer {
    Viewer {
        account: AccountId::new("compte-de-test"),
        role,
        services_right: ServicesRight::Manage,
    }
}

/// Un compte sans le droit de gérer les services.
fn viewer_without_right(role: Role) -> Viewer {
    Viewer {
        services_right: ServicesRight::None,
        ..viewer(role)
    }
}

/// Des cadences courtes pour les tests qui pilotent `ServiceLogs` sans passer par le socket.
fn quick() -> LogSettings {
    LogSettings {
        recheck: Duration::from_millis(50),
        reattach_pause: Duration::from_millis(20),
        restart_grace: Duration::from_millis(1_500),
        access_recheck: Duration::from_millis(100),
        // Ces tests ne portent pas sur le frein par compte : rien ne les arrête.
        brake: hearth_agent::domain::services::BrakeSettings::relaxed(),
        ..LogSettings::default()
    }
}

fn service_key(key: &str) -> ServiceKey {
    ServiceKey::parse(key).expect("clé valide")
}

fn open_request() -> OpenRequest {
    OpenRequest {
        tail: 1000,
        since: None,
    }
}

async fn next_message(session: &mut LogSession) -> LogsServerMessage {
    tokio::time::timeout(Duration::from_secs(10), session.next_message())
        .await
        .expect("aucun message dans le délai")
        .expect("la lecture est finie")
}

fn is_resume_lost(message: &LogsServerMessage) -> bool {
    matches!(message, LogsServerMessage::Gap(gap) if gap.reason == GapReason::ResumeLost)
}

/// Un conteneur qui a écrit `count` lignes, une par milliseconde à partir de `T0`.
fn chatty_engine(count: usize) -> (Arc<FakeEngine>, String, String) {
    let engine = Arc::new(FakeEngine::new());
    let web = container("web", RunState::Running);
    for n in 0..count {
        engine.push_log(
            &web.id,
            LogSource::Out,
            T0 + i64::try_from(n).unwrap(),
            &format!("l{n}"),
        );
    }
    let (key, id) = (key_of(&web), web.id.clone());
    engine.put_silently(web);
    (engine, key, id)
}

#[tokio::test]
async fn a_cursor_that_cannot_be_trusted_is_a_lost_resume_and_never_makes_the_engine_reread_the_journal()
 {
    let env = env().await;
    let (engine, key, _) = chatty_engine(100_000);
    let agent = ready_agent(&env, &engine, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let now = T0 + 60_000;
    let forged = [
        // Forgé à zéro, négatif, très ancien.
        LogCursor { ts: 0, rank: 0 },
        LogCursor { ts: -1, rank: 0 },
        LogCursor {
            ts: now - 6 * 60_000,
            rank: 0,
        },
        // Dans le futur.
        LogCursor {
            ts: now + 3_600_000,
            rank: 0,
        },
        LogCursor {
            ts: i64::MAX,
            rank: 0,
        },
        // Un rang qu'aucune relecture bornée n'atteindrait.
        LogCursor {
            ts: T0 + 5,
            rank: u32::MAX,
        },
    ];
    for cursor in forged {
        let before = engine.rendered_log_lines();
        let mut client = read(&agent, &admin, &key, 1000, Some(cursor)).await;
        let (items, others, _) = collect(&mut client, 1000).await;
        assert!(
            others.iter().any(is_resume_lost),
            "{cursor:?} : un curseur douteux est une reprise perdue : {others:?}"
        );
        assert_eq!(items.first().map(|i| i.x.as_str()), Some("l99000"));
        assert_eq!(items.last().map(|i| i.x.as_str()), Some("l99999"));
        let rendered = engine.rendered_log_lines() - before;
        assert!(
            rendered <= 1_000,
            "{cursor:?} : le moteur a rendu {rendered} lignes, jamais tout le journal"
        );
        client.drop_connection();
    }
    agent.shutdown().await;
}

#[tokio::test]
async fn ten_forged_resumes_at_once_cost_the_engine_ten_first_pages_not_ten_journals() {
    let env = env().await;
    let (engine, key, _) = chatty_engine(50_000);
    let agent = ready_agent(&env, &engine, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let mut clients = Vec::new();
    for _ in 0..10 {
        clients.push(
            read(
                &agent,
                &admin,
                &key,
                1000,
                Some(LogCursor { ts: 0, rank: 0 }),
            )
            .await,
        );
    }
    for client in &mut clients {
        let (items, others, _) = collect(client, 1000).await;
        assert!(others.iter().any(is_resume_lost), "{others:?}");
        assert_eq!(items.len(), 1000);
    }
    let rendered = engine.rendered_log_lines();
    assert!(
        rendered <= 10_000,
        "dix reprises forgées : {rendered} lignes rendues par le moteur"
    );
    agent.shutdown().await;
}

#[tokio::test]
async fn a_cursor_the_journal_never_reaches_is_given_up_after_a_bounded_number_of_lines() {
    let env = env().await;
    // 30 000 lignes en dix millisecondes ; le curseur est plausible (dans la fenêtre) mais plus loin que
    // la dernière ligne : la ligne du curseur ne vient jamais, le suivi ne finit pas.
    let engine = Arc::new(FakeEngine::new());
    let web = container("web", RunState::Running);
    for n in 0..30_000i64 {
        engine.push_log(&web.id, LogSource::Out, T0 + n % 10, &format!("l{n}"));
    }
    let key = key_of(&web);
    engine.put_silently(web);
    let agent = ready_agent(&env, &engine, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let mut client = read(
        &agent,
        &admin,
        &key,
        1000,
        Some(LogCursor {
            ts: T0 + 500,
            rank: 0,
        }),
    )
    .await;
    let (items, others, _) = collect(&mut client, 1000).await;
    assert!(others.iter().any(is_resume_lost), "{others:?}");
    assert_eq!(items.last().map(|i| i.x.as_str()), Some("l29999"));
    agent.shutdown().await;
}

/// Un conteneur qui a écrit `count` lignes, `per_ms` par milliseconde à partir de `T0`.
fn dense_engine(count: usize, per_ms: usize) -> (Arc<FakeEngine>, String) {
    let engine = Arc::new(FakeEngine::new());
    let web = container("web", RunState::Running);
    for n in 0..count {
        engine.push_log(
            &web.id,
            LogSource::Out,
            T0 + i64::try_from(n / per_ms).unwrap(),
            &format!("l{n}"),
        );
    }
    let key = key_of(&web);
    engine.put_silently(web);
    (engine, key)
}

#[tokio::test]
async fn no_request_to_the_engine_carries_since_without_a_bound() {
    let env = env().await;
    let (engine, key) = dense_engine(1_000, 1);
    let agent = ready_agent(&env, &engine, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    // Une reprise valide (premier octet du curseur dans la fenêtre), puis une reprise perdue.
    for (cursor, expected) in [
        (
            LogCursor {
                ts: T0 + 990,
                rank: 0,
            },
            9,
        ),
        (LogCursor { ts: 0, rank: 0 }, 1000),
    ] {
        let mut client = read(&agent, &admin, &key, 1000, Some(cursor)).await;
        let (items, _, _) = collect(&mut client, expected).await;
        assert_eq!(items.len(), expected);
        client.drop_connection();
    }
    let queries = engine.log_queries();
    assert!(
        queries.iter().any(|query| query.since_ms.is_some()),
        "le test ne voit aucune requête de reprise : {queries:?}"
    );
    for query in queries.iter().filter(|query| query.since_ms.is_some()) {
        assert!(
            query.tail.is_some_and(|tail| tail <= 25_000),
            "requête avec since sans borne de fin : {query:?}"
        );
    }
    agent.shutdown().await;
}

#[tokio::test]
async fn a_valid_cursor_on_a_huge_journal_makes_the_engine_render_a_bounded_window() {
    let env = env().await;
    // 100 000 lignes sur 50 secondes (deux par milliseconde) ; un curseur à 20 s : 60 000 lignes
    // le suivent, bien plus que la fenêtre.
    let (engine, key) = dense_engine(100_000, 2);
    let agent = ready_agent(&env, &engine, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let before = engine.rendered_log_lines();
    let mut client = read(
        &agent,
        &admin,
        &key,
        1000,
        Some(LogCursor {
            ts: T0 + 20_000,
            rank: 0,
        }),
    )
    .await;
    let (items, others, _) = collect(&mut client, 1000).await;
    assert!(others.iter().any(is_resume_lost), "{others:?}");
    assert_eq!(items.last().map(|i| i.x.as_str()), Some("l99999"));
    let rendered = engine.rendered_log_lines() - before;
    assert!(
        rendered <= 25_000 + 1_000,
        "le moteur a rendu {rendered} lignes pour une seule ouverture"
    );
    agent.shutdown().await;
}

#[tokio::test]
async fn a_full_window_that_starts_inside_the_cursor_millisecond_is_a_lost_resume() {
    let env = env().await;
    // 50 000 lignes, une par milliseconde : depuis la seconde du curseur, il y en a exactement 25 000,
    // la fenêtre est pleine et sa première ligne est celle du curseur : ses rangs ne sont pas sûrs.
    let (engine, key) = dense_engine(50_000, 1);
    let agent = ready_agent(&env, &engine, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let mut client = read(
        &agent,
        &admin,
        &key,
        1000,
        Some(LogCursor {
            ts: T0 + 25_000,
            rank: 0,
        }),
    )
    .await;
    let (items, others, _) = collect(&mut client, 1000).await;
    assert!(others.iter().any(is_resume_lost), "{others:?}");
    assert_eq!(items.last().map(|i| i.x.as_str()), Some("l49999"));
    agent.shutdown().await;
}

#[tokio::test]
async fn a_restart_the_registry_has_not_published_yet_still_gets_its_marker() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let web = container("web", RunState::Running);
    engine.push_log(&web.id, LogSource::Out, T0 + 1, "première exécution");
    let (key, id) = (key_of(&web), web.id.clone());
    engine.put_silently(web);
    let agent = ready_agent(&env, &engine, 1).await;
    let logs = ServiceLogs::new(agent.services(), engine.clone(), env.clock.clone(), quick());
    let mut session = logs
        .open(viewer(Role::Admin), service_key(&key), open_request())
        .expect("lecture ouverte");
    assert!(matches!(
        next_message(&mut session).await,
        LogsServerMessage::Status(_)
    ));
    assert!(matches!(
        next_message(&mut session).await,
        LogsServerMessage::Lines(_)
    ));
    // La lecture dure depuis un moment : pas de pause de rattachement qui masquerait la course.
    tokio::time::sleep(Duration::from_millis(400)).await;

    // Le conteneur s'arrête et repart aussitôt (politique de redémarrage) : le flux du moteur se ferme, le
    // moteur n'a encore rien annoncé au registre (il regroupe ses événements).
    engine.update(&id, |o| o.started_at = Some(support::fake_engine::at(12)));
    engine.end_log_streams(&id);
    // L'événement du moteur arrive au registre un peu après (il le regroupe avant de relire) ; la nouvelle
    // exécution écrit sa première ligne entre-temps.
    engine.emit(&id, "start");
    tokio::time::sleep(Duration::from_millis(100)).await;
    engine.push_log(&id, LogSource::Out, T0 + 7_200_001, "deuxième exécution");

    let mut saw_marker = false;
    loop {
        match next_message(&mut session).await {
            LogsServerMessage::Marker(marker) if marker.kind == MarkerKind::Restarted => {
                saw_marker = true;
            }
            LogsServerMessage::Lines(lines) => {
                assert_eq!(texts(&lines.items), ["deuxième exécution"]);
                break;
            }
            _ => {}
        }
    }
    assert!(
        saw_marker,
        "le repère de redémarrage précède la première ligne de la nouvelle exécution"
    );
    agent.shutdown().await;
}

#[tokio::test]
async fn a_first_page_of_long_lines_arrives_whole_for_a_reader_that_keeps_up() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let web = container("web", RunState::Running);
    // 1 000 lignes de 4 Ko : 4 Mo, le double de l'anneau.
    for n in 0..1000i64 {
        engine.push_log(
            &web.id,
            LogSource::Out,
            T0 + n,
            &format!("{n:04} {}", "x".repeat(4_000)),
        );
    }
    let key = key_of(&web);
    engine.put_silently(web);
    let agent = ready_agent(&env, &engine, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let mut client = read(&agent, &admin, &key, 1000, None).await;
    let (items, others, _) = collect(&mut client, 1000).await;
    assert_eq!(items.len(), 1000);
    assert!(items[0].x.starts_with("0000 ") && items[999].x.starts_with("0999 "));
    assert!(
        !others
            .iter()
            .any(|message| matches!(message, LogsServerMessage::Gap(_))),
        "un lecteur qui suit ne perd rien de la première page : {others:?}"
    );
    agent.shutdown().await;
}

#[tokio::test]
async fn a_first_page_a_client_does_not_read_is_cut_as_first_page_never_as_slow() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let web = container("web", RunState::Running);
    for n in 0..300i64 {
        engine.push_log(&web.id, LogSource::Out, T0 + n, &format!("p{n}"));
    }
    let key = key_of(&web);
    engine.put_silently(web);
    let agent = ready_agent(&env, &engine, 1).await;
    let settings = LogSettings {
        ring_lines: 100,
        history_wait: Duration::from_millis(100),
        ..quick()
    };
    let logs = ServiceLogs::new(
        agent.services(),
        engine.clone(),
        env.clock.clone(),
        settings,
    );
    let mut session = logs
        .open(viewer(Role::Admin), service_key(&key), open_request())
        .expect("lecture ouverte");
    // Le client ne lit rien pendant que l'agent lit la page.
    tokio::time::sleep(Duration::from_millis(600)).await;
    let (mut lost_first_page, mut lost_slow, mut received) = (0u64, 0u64, 0u64);
    while received + lost_first_page < 300 {
        match next_message(&mut session).await {
            LogsServerMessage::Gap(gap) if gap.reason == GapReason::FirstPage => {
                lost_first_page += gap.count.unwrap();
            }
            LogsServerMessage::Gap(gap) if gap.reason == GapReason::Slow => {
                lost_slow += gap.count.unwrap();
            }
            LogsServerMessage::Lines(lines) => received += lines.items.len() as u64,
            _ => {}
        }
    }
    assert_eq!(lost_slow, 0, "ce n'est pas un lecteur lent du direct");
    assert!(lost_first_page > 0, "la page a été coupée");
    assert_eq!(received + lost_first_page, 300);
    agent.shutdown().await;
}

#[tokio::test]
async fn when_the_engine_clock_goes_back_the_next_read_says_so_instead_of_staying_silent() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let web = container("web", RunState::Running);
    engine.push_log(&web.id, LogSource::Out, T0 + 10_000, "avant le recul");
    let (key, id) = (key_of(&web), web.id.clone());
    engine.put_silently(web);
    let agent = ready_agent(&env, &engine, 1).await;
    let logs = ServiceLogs::new(
        agent.services(),
        engine.clone(),
        env.clock.clone(),
        LogSettings {
            restart_grace: Duration::from_millis(50),
            ..quick()
        },
    );
    let mut session = logs
        .open(viewer(Role::Admin), service_key(&key), open_request())
        .expect("lecture ouverte");
    // L'heure du moteur recule de neuf secondes : la ligne suivante est plus ancienne que la précédente.
    engine.push_log(&id, LogSource::Out, T0 + 1_000, "après le recul");
    let mut seen = Vec::new();
    while seen.len() < 2 {
        if let LogsServerMessage::Lines(lines) = next_message(&mut session).await {
            seen.extend(lines.items.into_iter().map(|item| item.x));
        }
    }
    // Le flux se coupe ; la relecture ne peut pas se fonder sur l'heure : un trou, puis les dernières
    // lignes, y compris celle écrite entre-temps.
    engine.end_log_streams(&id);
    tokio::time::sleep(Duration::from_millis(300)).await;
    engine.push_log(&id, LogSource::Out, T0 + 2_000, "écrite pendant la reprise");
    let (mut lost, mut again) = (false, Vec::new());
    while !again
        .iter()
        .any(|text: &String| text == "écrite pendant la reprise")
    {
        match next_message(&mut session).await {
            message if is_resume_lost(&message) => lost = true,
            LogsServerMessage::Lines(lines) => {
                again.extend(lines.items.into_iter().map(|item| item.x));
            }
            _ => {}
        }
    }
    assert!(lost, "la reprise perdue est annoncée");
    agent.shutdown().await;
}

#[tokio::test]
async fn a_refusal_taken_during_a_reading_closes_it_with_service_not_found() {
    let env = env().await;
    let (engine, key, _) = chatty_engine(3);
    let agent = ready_agent(&env, &engine, 1).await;
    let refuse = Arc::new(AtomicBool::new(false));
    let policy: ReadPolicy = {
        let refuse = refuse.clone();
        Arc::new(move |_viewer: &Viewer, _target: &LogTarget| {
            if refuse.load(Ordering::SeqCst) {
                Err(ReadRefusal::NotEnabled)
            } else {
                Ok(())
            }
        })
    };
    let logs = ServiceLogs::new(agent.services(), engine.clone(), env.clock.clone(), quick())
        .with_policy(policy);
    let mut session = logs
        .open(viewer(Role::ReadOnly), service_key(&key), open_request())
        .expect("lecture ouverte");
    let mut lines = 0;
    while lines < 3 {
        if let LogsServerMessage::Lines(batch) = next_message(&mut session).await {
            lines += batch.items.len();
        }
    }
    // Le service reste lisible tant que la décision dit oui, puis la lecture se ferme avec le code prévu,
    // en moins de deux périodes de contrôle.
    refuse.store(true, Ordering::SeqCst);
    let started = Instant::now();
    loop {
        if let LogsServerMessage::Error(error) = next_message(&mut session).await {
            assert_eq!(error.code, LogsErrorCode::ServiceNotFound);
            break;
        }
    }
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "{:?}",
        started.elapsed()
    );
    assert!(
        session.next_message().await.is_none(),
        "rien après l'erreur"
    );
    agent.shutdown().await;
}

#[tokio::test]
async fn a_change_of_account_seen_by_the_session_replays_the_decision_at_once() {
    let env = env().await;
    let (engine, key, _) = chatty_engine(1);
    let agent = ready_agent(&env, &engine, 1).await;
    // Un rôle en lecture seule perd l'accès (ce que HRT-76 permettra de décider autrement) : la période de
    // contrôle est d'une heure, seul le changement de compte peut fermer la lecture à temps.
    let policy: ReadPolicy = Arc::new(|viewer: &Viewer, _target: &LogTarget| {
        if viewer.role == Role::ReadOnly {
            Err(ReadRefusal::NotEnabled)
        } else {
            Ok(())
        }
    });
    let logs = ServiceLogs::new(
        agent.services(),
        engine.clone(),
        env.clock.clone(),
        LogSettings {
            access_recheck: Duration::from_secs(3_600),
            ..quick()
        },
    )
    .with_policy(policy);
    let mut session = logs
        .open(viewer(Role::Admin), service_key(&key), open_request())
        .expect("lecture ouverte");
    assert!(matches!(
        next_message(&mut session).await,
        LogsServerMessage::Status(_)
    ));
    session.set_viewer(viewer(Role::ReadOnly));
    loop {
        if let LogsServerMessage::Error(error) = next_message(&mut session).await {
            assert_eq!(error.code, LogsErrorCode::ServiceNotFound);
            break;
        }
    }
    // Et un compte qui n'a jamais eu le droit n'ouvre rien.
    assert_eq!(
        logs.open(viewer(Role::ReadOnly), service_key(&key), open_request())
            .err()
            .map(|refused| refused.error.code),
        Some(LogsErrorCode::ServiceNotFound)
    );
    agent.shutdown().await;
}

/// Lit jusqu'à la fin du socket : les messages reçus et la façon dont il s'est fermé.
async fn until_closed(client: &mut WsClient) -> (Vec<LogsServerMessage>, End) {
    let mut messages = Vec::new();
    loop {
        match client.next_logs().await {
            Ok(message) => messages.push(message),
            Err(end) => return (messages, end),
        }
    }
}

/// Attend qu'une place de lecture se libère : ouvre une lecture avec ce jeton jusqu'à ce qu'elle ne soit
/// plus refusée par la limite de dix, puis la referme.
async fn place_is_freed(agent: &Agent, token: &str, key: &str) {
    let started = Instant::now();
    loop {
        let mut client = read(agent, token, key, 10, None).await;
        match client.next_logs().await {
            Ok(LogsServerMessage::Error(error)) if error.code == LogsErrorCode::LogsLimit => {
                assert!(
                    started.elapsed() < Duration::from_secs(10),
                    "la place n'est pas rendue"
                );
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Ok(LogsServerMessage::Status(_)) => {
                client.drop_connection();
                return;
            }
            other => panic!("{other:?}"),
        }
    }
}

const POLICY_CLOSE: u16 = 1008;
const GOING_AWAY: u16 = 1001;

#[tokio::test]
async fn a_revoked_session_closes_its_reading_and_gives_the_place_back() {
    let env = env().await;
    let (engine, key, _) = chatty_engine(1);
    let agent = ready_agent(&env, &engine, 1).await;
    let marie = env.create("marie", Role::Admin).await;
    env.grant_services("marie").await;
    let admin = login(&agent, "marie").await;
    let readonly = token(&env, &agent, "lucas", Role::ReadOnly).await;
    // Neuf lectures d'un autre compte tiennent neuf places.
    let mut others = Vec::new();
    for _ in 0..9 {
        let mut client = read(&agent, &readonly, &key, 10, None).await;
        client.next_logs().await.unwrap();
        others.push(client);
    }
    let mut client = read(&agent, &admin, &key, 10, None).await;
    client.next_logs().await.unwrap();
    env.service
        .revoke_sessions(&marie.id, support::by())
        .await
        .unwrap();
    let (messages, end) = until_closed(&mut client).await;
    assert!(
        messages.iter().any(|message| matches!(
            message,
            LogsServerMessage::Error(error) if error.code == LogsErrorCode::Unauthenticated
        )),
        "{messages:?}"
    );
    assert_eq!(end, End::Closed(POLICY_CLOSE));
    place_is_freed(&agent, &readonly, &key).await;
    drop(others);
    agent.shutdown().await;
}

#[tokio::test]
async fn a_silent_client_loses_its_reading_and_the_agent_shutting_down_closes_it_going_away() {
    let env = env().await;
    let (engine, key, _) = chatty_engine(1);
    align_clock(&env);
    adopt_all(&env, &engine).await;
    let mut stream = support::probe::fast_stream();
    stream.idle_timeout = Duration::from_millis(400);
    let agent = https::start_engined_metered(
        &env,
        parts(engine.clone()),
        support::probe::metering_with(stream),
    )
    .await;
    wait_listed(&agent, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let mut silent = read(&agent, &admin, &key, 10, None).await;
    silent.next_logs().await.unwrap();
    // Aucun `ping` : au bout du délai de silence, l'agent ferme et rend la place.
    let (_, end) = until_closed(&mut silent).await;
    assert_eq!(end, End::Closed(POLICY_CLOSE));
    eventually("la lecture du moteur est fermée", || {
        let engine = engine.clone();
        async move { engine.open_log_reads() == 0 }
    })
    .await;

    // Un client qui parle encore voit l'arrêt de l'agent comme un départ.
    let mut talking = read(&agent, &admin, &key, 10, None).await;
    talking.next_logs().await.unwrap();
    let shutdown = tokio::spawn(agent.shutdown());
    let mut ended = None;
    for n in 0..200u64 {
        talking.send_value(&LogsClientMessage::Ping { n }).await;
        if let Some(Err(end)) = talking.try_next_logs(Duration::from_millis(50)).await {
            ended = Some(end);
            break;
        }
    }
    shutdown.await.unwrap();
    let ended = match ended {
        Some(end) => end,
        None => until_closed(&mut talking).await.1,
    };
    assert!(
        matches!(ended, End::Closed(GOING_AWAY) | End::Dropped),
        "{ended:?}"
    );
}

#[tokio::test]
async fn the_place_of_a_reading_comes_back_whatever_ends_it() {
    let env = env().await;
    let (engine, key, id) = chatty_engine(1);
    let agent = ready_agent(&env, &engine, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let readonly = token(&env, &agent, "lucas", Role::ReadOnly).await;
    let mut background = Vec::new();
    for _ in 0..9 {
        let mut client = read(&agent, &readonly, &key, 10, None).await;
        client.next_logs().await.unwrap();
        background.push(client);
    }

    // 1. Le client ferme proprement (trame de fermeture).
    let mut client = read(&agent, &admin, &key, 10, None).await;
    client.next_logs().await.unwrap();
    client.close().await;
    place_is_freed(&agent, &readonly, &key).await;

    // 2. Le client disparaît sans rien dire.
    let mut client = read(&agent, &admin, &key, 10, None).await;
    client.next_logs().await.unwrap();
    client.drop_connection();
    place_is_freed(&agent, &readonly, &key).await;

    // 3. Une trame binaire : mauvaise demande.
    let mut client = read(&agent, &admin, &key, 10, None).await;
    client.next_logs().await.unwrap();
    client.send_binary(&[1, 2, 3]).await;
    let (messages, _) = until_closed(&mut client).await;
    assert!(messages.iter().any(|message| matches!(
        message,
        LogsServerMessage::Error(error) if error.code == LogsErrorCode::BadRequest
    )));
    place_is_freed(&agent, &readonly, &key).await;

    // 4. Une deuxième ouverture sur la même lecture : mauvaise demande.
    let mut client = read(&agent, &admin, &key, 10, None).await;
    client.next_logs().await.unwrap();
    client
        .send_value(&LogsClientMessage::Open {
            tail: 10,
            since: None,
        })
        .await;
    let (messages, _) = until_closed(&mut client).await;
    assert!(messages.iter().any(|message| matches!(
        message,
        LogsServerMessage::Error(error) if error.code == LogsErrorCode::BadRequest
    )));
    place_is_freed(&agent, &readonly, &key).await;

    // 5. Le service disparaît : erreur terminale du côté de l'agent.
    let mut client = read(&agent, &admin, &key, 10, None).await;
    client.next_logs().await.unwrap();
    engine.remove(&id);
    let (messages, _) = until_closed(&mut client).await;
    assert!(messages.iter().any(|message| matches!(
        message,
        LogsServerMessage::Error(error) if error.code == LogsErrorCode::ServiceNotFound
    )));
    drop(background);
    agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Journaux et publication (HRT-79) : un conteneur non adopté n'a aucun journal (BR-ADOPT-004), un service
// adopté se lit avec le droit de gérer les services (BR-ADOPT-015, BR-SLOG-018)
// ---------------------------------------------------------------------------------------------

const HIDDEN_LINE: &str = "SENTINEL-LINE-HIDDEN-0d71";

/// Un agent dont le moteur porte un conteneur adopté (`visible`) et un conteneur que personne n'a adopté
/// (`cache`), dont le journal contient une ligne sentinelle.
struct Hiding {
    engine: Arc<FakeEngine>,
    agent: Agent,
    hidden_key: String,
    shown_key: String,
}

async fn hiding_agent(env: &Env) -> Hiding {
    let engine = Arc::new(FakeEngine::new());
    let hidden = container("cache", RunState::Running);
    let shown = container("visible", RunState::Running);
    engine.push_log(&hidden.id, LogSource::Out, T0, HIDDEN_LINE);
    engine.push_log(&shown.id, LogSource::Out, T0, "ligne visible");
    let (hidden_key, shown_key) = (key_of(&hidden), key_of(&shown));
    let shown_id = shown.id.clone();
    engine.put_silently(hidden);
    engine.put_silently(shown);
    align_clock(env);
    // Seul `visible` est adopté : `ready_agent` adopterait aussi l'autre.
    env.adopt_container(&shown_id, "visible").await;
    let agent = https::start_engined(env, parts(engine.clone())).await;
    wait_listed(&agent, 1).await;
    Hiding {
        engine,
        agent,
        hidden_key,
        shown_key,
    }
}

fn not_found() -> Vec<LogsServerMessage> {
    vec![LogsServerMessage::Error(
        hearth_proto::api::service_logs::LogsError::new(LogsErrorCode::ServiceNotFound),
    )]
}

#[tokio::test]
async fn a_hidden_container_gets_on_the_logs_route_exactly_the_answer_of_an_unknown_key() {
    let env = env().await;
    let h = hiding_agent(&env).await;
    let granted = token(&env, &h.agent, "marie", Role::Admin).await;
    let plain = token_without_right(&env, &h.agent, "lucas", Role::ReadOnly).await;
    let unknown_container = format!("c_{}", "f".repeat(64));
    let unknown_unit = "u_docker-fantome.service".to_owned();
    for (who, who_token) in [("avec le droit", &granted), ("sans le droit", &plain)] {
        let mut answers = Vec::new();
        for key in [&h.hidden_key, &unknown_container, &unknown_unit] {
            let started = Instant::now();
            let mut client = read(&h.agent, who_token, key, 1000, None).await;
            let (messages, end) = until_closed(&mut client).await;
            answers.push((messages, end, started.elapsed()));
        }
        // Même contenu, même code de fermeture, même ordre : l'erreur est le premier et seul message.
        assert_eq!(answers[0].0, not_found(), "{who}");
        assert_eq!(answers[0].1, End::Closed(1000), "{who}");
        for pair in answers.windows(2) {
            assert_eq!(pair[0].0, pair[1].0, "{who} : message");
            assert_eq!(pair[0].1, pair[1].1, "{who} : fermeture");
        }
        // Même moment : aucune attente propre à la clé cachée.
        for (_, _, elapsed) in &answers {
            assert!(*elapsed < Duration::from_secs(2), "{who} : {elapsed:?}");
        }
        let text = format!("{answers:?}");
        assert!(
            !text.contains("SENTINEL") && !text.contains("cache"),
            "{text}"
        );
    }
    // Le moteur n'a jamais été interrogé pour la clé cachée : aucune lecture de journaux du tout.
    assert_eq!(h.engine.log_read_count(), 0);
    // Témoin : le service adopté, lui, se lit avec le droit (sinon ce test serait aveugle).
    let mut client = read(&h.agent, &granted, &h.shown_key, 1000, None).await;
    client.next_logs().await.unwrap();
    let (items, _, _) = collect(&mut client, 1).await;
    assert_eq!(texts(&items), ["ligne visible"]);
    assert_eq!(h.engine.log_read_count(), 1);
    h.agent.shutdown().await;
}

#[tokio::test]
async fn the_limit_of_ten_readings_does_not_tell_a_hidden_key_from_an_unknown_one() {
    let env = env().await;
    let h = hiding_agent(&env).await;
    let granted = token(&env, &h.agent, "marie", Role::Admin).await;
    let unknown = format!("c_{}", "e".repeat(64));
    // Avant la limite : cachée et inconnue se ressemblent.
    for key in [&h.hidden_key, &unknown] {
        let mut client = read(&h.agent, &granted, key, 10, None).await;
        expect_error(&mut client, LogsErrorCode::ServiceNotFound).await;
    }
    let mut open = Vec::new();
    for _ in 0..10 {
        let mut client = read(&h.agent, &granted, &h.shown_key, 10, None).await;
        client.next_logs().await.unwrap();
        open.push(client);
    }
    // À la limite : la même réponse pour les deux, qui ne dit rien de l'existence de l'une ni de l'autre.
    for key in [&h.hidden_key, &unknown] {
        let mut client = read(&h.agent, &granted, key, 10, None).await;
        expect_error(&mut client, LogsErrorCode::LogsLimit).await;
    }
    // Une place rendue : de nouveau la même réponse « inconnu » pour les deux.
    open.pop().unwrap().drop_connection();
    let engine = h.engine.clone();
    eventually("la place est rendue", || {
        let engine = engine.clone();
        async move { engine.open_log_reads() == 9 }
    })
    .await;
    for key in [&h.hidden_key, &unknown] {
        let mut client = read(&h.agent, &granted, key, 10, None).await;
        expect_error(&mut client, LogsErrorCode::ServiceNotFound).await;
    }
    drop(open);
    h.agent.shutdown().await;
}

fn missing_right() -> Vec<LogsServerMessage> {
    vec![LogsServerMessage::Error(
        hearth_proto::api::service_logs::LogsError::new(LogsErrorCode::MissingRight),
    )]
}

#[tokio::test]
async fn an_adopted_service_is_read_with_the_right_to_manage_services_whatever_the_role() {
    let env = env().await;
    let h = hiding_agent(&env).await;
    let accounts = [
        ("marie", Role::Admin, true),
        ("lucas", Role::ReadOnly, true),
        ("ada", Role::Admin, false),
        ("zoe", Role::ReadOnly, false),
    ];
    for (name, role, has_right) in accounts {
        let who = if has_right {
            token(&env, &h.agent, name, role).await
        } else {
            token_without_right(&env, &h.agent, name, role).await
        };
        let mut client = read(&h.agent, &who, &h.shown_key, 1000, None).await;
        if has_right {
            client.next_logs().await.unwrap();
            let (items, _, _) = collect(&mut client, 1).await;
            assert_eq!(texts(&items), ["ligne visible"], "{name}");
        } else {
            // Refusé même par un client modifié, sans ligne, et dit en clair : le service est dans la liste
            // de ce compte, le refus est explicite (BR-SLOG-018, BR-SLOG-024) et non « introuvable ».
            let (messages, end) = until_closed(&mut client).await;
            assert_eq!(messages, missing_right(), "{name}");
            assert_eq!(end, End::Closed(1000), "{name}");
        }
    }
    h.agent.shutdown().await;
}

#[tokio::test]
async fn a_right_withdrawn_during_a_reading_closes_it_with_the_explicit_refusal() {
    let env = env().await;
    let h = hiding_agent(&env).await;
    let marie = token(&env, &h.agent, "marie", Role::Admin).await;
    let mut client = read(&h.agent, &marie, &h.shown_key, 1000, None).await;
    client.next_logs().await.unwrap();
    let (items, _, _) = collect(&mut client, 1).await;
    assert_eq!(texts(&items), ["ligne visible"]);
    env.revoke_services("marie").await;
    let started = Instant::now();
    let (messages, end) = until_closed(&mut client).await;
    assert_eq!(messages, missing_right());
    assert_eq!(end, End::Closed(1000));
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    h.agent.shutdown().await;
}

#[tokio::test]
async fn an_adoption_given_up_during_a_reading_closes_it_and_the_key_turns_into_an_unknown_one() {
    let env = env().await;
    let h = hiding_agent(&env).await;
    let marie = token(&env, &h.agent, "marie", Role::Admin).await;
    let mut client = read(&h.agent, &marie, &h.shown_key, 1000, None).await;
    client.next_logs().await.unwrap();
    let (items, _, _) = collect(&mut client, 1).await;
    assert_eq!(texts(&items), ["ligne visible"]);
    // L'adoption est relâchée : le conteneur cesse d'être publié.
    env.unadopt(&h.shown_key).await;
    h.agent.services().reload_adopted().await;
    let started = Instant::now();
    let (messages, end) = until_closed(&mut client).await;
    assert_eq!(messages, not_found());
    assert_eq!(end, End::Closed(1000));
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    // Et la clé, maintenant cachée, reçoit la réponse d'une clé inconnue.
    let mut again = read(&h.agent, &marie, &h.shown_key, 1000, None).await;
    expect_error(&mut again, LogsErrorCode::ServiceNotFound).await;
    h.agent.shutdown().await;
}

#[tokio::test]
async fn the_registry_gives_may_read_the_origin_and_nothing_else_of_a_hidden_container() {
    use hearth_agent::domain::services::logs::{LogOrigin, may_read};
    let env = env().await;
    let h = hiding_agent(&env).await;
    let registry = h.agent.services();
    let hidden = registry.log_target(&service_key(&h.hidden_key));
    let shown = registry.log_target(&service_key(&h.shown_key));
    let gone = registry.log_target(&service_key(&format!("c_{}", "d".repeat(64))));
    assert_eq!(hidden.origin(), Some(LogOrigin::External));
    assert_eq!(shown.origin(), Some(LogOrigin::Adopted));
    assert_eq!(gone, LogTarget::Missing);
    // Une cible cachée ne porte aucune donnée du conteneur.
    assert!(
        matches!(hidden, LogTarget::NoContainer { .. }),
        "{hidden:?}"
    );
    for role in [Role::Admin, Role::ReadOnly] {
        assert_eq!(
            may_read(&viewer(role), &hidden),
            Err(ReadRefusal::NotEnabled)
        );
        assert_eq!(may_read(&viewer(role), &shown), Ok(()));
        // Un service adopté lu sans le droit : refus explicite, distinct de la clé cachée ou inconnue.
        assert_eq!(
            may_read(&viewer_without_right(role), &shown),
            Err(ReadRefusal::MissingRight)
        );
    }
    h.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// HRT-107 : trois lectures en direct par compte (BR-SLOG-026) et frein aux ouvertures en rafale
// (BR-SLOG-027). La règle est prouvée table par table dans `domain::services::log_brake` ; ici, la
// pose dans le service des lectures et sur le socket.
// ---------------------------------------------------------------------------------------------

use hearth_agent::application::service_logs::OpenRefused;
use hearth_agent::domain::services::BrakeSettings;

fn account_viewer(account: &str) -> Viewer {
    Viewer {
        account: AccountId::new(account),
        ..viewer(Role::ReadOnly)
    }
}

fn limited(brake: BrakeSettings) -> LogSettings {
    LogSettings { brake, ..quick() }
}

async fn one_service_agent(env: &Env, engine: &Arc<FakeEngine>) -> (Agent, String) {
    let web = container("web", RunState::Running);
    let key = key_of(&web);
    engine.put_silently(web);
    (ready_agent(env, engine, 1).await, key)
}

fn refusal_of(result: Result<LogSession, OpenRefused>) -> OpenRefused {
    match result {
        Ok(_) => panic!("l'ouverture aurait dû être refusée"),
        Err(refused) => refused,
    }
}

#[tokio::test]
async fn a_fourth_live_reading_of_one_account_is_refused_and_a_close_gives_the_place_back() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let (agent, key) = one_service_agent(&env, &engine).await;
    let logs = ServiceLogs::new(
        agent.services(),
        engine.clone(),
        env.clock.clone(),
        limited(BrakeSettings::production()),
    );
    let open = |who: &str| logs.open(account_viewer(who), service_key(&key), open_request());
    let mut marie: Vec<LogSession> = (0..3).map(|_| open("marie").expect("ouverte")).collect();
    assert_eq!(
        refusal_of(open("marie")).error.code,
        LogsErrorCode::LogsAccountLimit
    );
    // Un autre compte garde ses places : le plafond est par compte, pas pour le serveur.
    let lucas = open("lucas").expect("un autre compte lit");
    assert_eq!(logs.open_reads(), 4);
    // Fermer une lecture rend la place tout de suite.
    drop(marie.pop());
    let again = open("marie").expect("la place est rendue");
    assert_eq!(logs.open_reads(), 4);
    drop((again, lucas, marie));
    assert_eq!(logs.open_reads(), 0);
    agent.shutdown().await;
}

#[tokio::test]
async fn a_refusal_by_the_server_cap_does_not_keep_a_place_for_the_account() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let (agent, key) = one_service_agent(&env, &engine).await;
    let logs = ServiceLogs::new(
        agent.services(),
        engine.clone(),
        env.clock.clone(),
        LogSettings {
            max_reads: 1,
            ..limited(BrakeSettings::production())
        },
    );
    let first = logs
        .open(account_viewer("marie"), service_key(&key), open_request())
        .expect("ouverte");
    for _ in 0..3 {
        assert_eq!(
            refusal_of(logs.open(account_viewer("lucas"), service_key(&key), open_request()))
                .error
                .code,
            LogsErrorCode::LogsLimit
        );
    }
    drop(first);
    // lucas a été refusé trois fois par le serveur : ses trois places sont intactes.
    let lucas: Vec<_> = (0..1)
        .map(|_| logs.open(account_viewer("lucas"), service_key(&key), open_request()))
        .collect();
    assert!(lucas.iter().all(Result::is_ok));
    agent.shutdown().await;
}

#[tokio::test]
async fn the_thirteenth_open_in_a_minute_is_braked_with_its_wait_and_other_accounts_are_not() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let (agent, key) = one_service_agent(&env, &engine).await;
    let logs = ServiceLogs::new(
        agent.services(),
        engine.clone(),
        env.clock.clone(),
        limited(BrakeSettings::production()),
    );
    for round in 0..12 {
        let session = logs
            .open(account_viewer("marie"), service_key(&key), open_request())
            .unwrap_or_else(|_| panic!("ouverture {round}"));
        drop(session);
    }
    let refused = refusal_of(logs.open(account_viewer("marie"), service_key(&key), open_request()));
    assert_eq!(refused.error.code, LogsErrorCode::LogsRateLimited);
    let wait = refused.error.retry_after_s.expect("l'attente est dite");
    assert!((4..=5).contains(&wait), "{wait}");
    assert_eq!(refused.throttle_entry, None);
    assert!(
        logs.open(account_viewer("lucas"), service_key(&key), open_request())
            .is_ok()
    );
    agent.shutdown().await;
}

#[tokio::test]
async fn an_open_comes_back_after_the_refill_period() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let (agent, key) = one_service_agent(&env, &engine).await;
    let logs = ServiceLogs::new(
        agent.services(),
        engine.clone(),
        env.clock.clone(),
        limited(BrakeSettings {
            max_reads: 3,
            burst: 2,
            refill_ms: 150,
        }),
    );
    for _ in 0..2 {
        drop(
            logs.open(account_viewer("marie"), service_key(&key), open_request())
                .expect("ouverte"),
        );
    }
    assert_eq!(
        refusal_of(logs.open(account_viewer("marie"), service_key(&key), open_request()))
            .error
            .code,
        LogsErrorCode::LogsRateLimited
    );
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert!(
        logs.open(account_viewer("marie"), service_key(&key), open_request())
            .is_ok()
    );
    agent.shutdown().await;
}

#[tokio::test]
async fn a_reconnection_after_a_cut_counts_as_an_open() {
    // La reprise ouvre un nouveau socket avec un curseur `since` : même route, donc même frein.
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let (agent, key) = one_service_agent(&env, &engine).await;
    let logs = ServiceLogs::new(
        agent.services(),
        engine.clone(),
        env.clock.clone(),
        limited(BrakeSettings {
            max_reads: 3,
            burst: 1,
            refill_ms: 3_600_000,
        }),
    );
    drop(
        logs.open(account_viewer("marie"), service_key(&key), open_request())
            .expect("ouverte"),
    );
    let resume = OpenRequest {
        tail: 0,
        since: Some(LogCursor { ts: T0, rank: 0 }),
    };
    assert_eq!(
        refusal_of(logs.open(account_viewer("marie"), service_key(&key), resume))
            .error
            .code,
        LogsErrorCode::LogsRateLimited
    );
    agent.shutdown().await;
}

#[tokio::test]
async fn only_one_journal_entry_is_asked_for_after_thirty_refusals() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let (agent, key) = one_service_agent(&env, &engine).await;
    let logs = ServiceLogs::new(
        agent.services(),
        engine.clone(),
        env.clock.clone(),
        limited(BrakeSettings::production()),
    );
    for _ in 0..12 {
        drop(
            logs.open(account_viewer("marie"), service_key(&key), open_request())
                .expect("ouverte"),
        );
    }
    let mut entries = Vec::new();
    for round in 0..120 {
        let refused =
            refusal_of(logs.open(account_viewer("marie"), service_key(&key), open_request()));
        assert_eq!(refused.error.code, LogsErrorCode::LogsRateLimited);
        if let Some(count) = refused.throttle_entry {
            entries.push((round, count));
        }
    }
    assert_eq!(entries, vec![(30, 31)]);
    agent.shutdown().await;
}

#[tokio::test]
async fn a_missing_right_stays_visible_when_the_brake_answers_in_its_place() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let (agent, key) = one_service_agent(&env, &engine).await;
    let logs = ServiceLogs::new(
        agent.services(),
        engine.clone(),
        env.clock.clone(),
        limited(BrakeSettings {
            max_reads: usize::MAX,
            burst: 1,
            refill_ms: 3_600_000,
        }),
    )
    .with_policy(Arc::new(|_, _| Err(ReadRefusal::MissingRight)));
    let first = refusal_of(logs.open(account_viewer("marie"), service_key(&key), open_request()));
    assert_eq!(first.error.code, LogsErrorCode::MissingRight);
    assert!(!first.missing_right);
    // le seau est vide : le client lit « freiné », le journal sait que le droit manquait aussi
    let braked = refusal_of(logs.open(account_viewer("marie"), service_key(&key), open_request()));
    assert_eq!(braked.error.code, LogsErrorCode::LogsRateLimited);
    assert!(braked.missing_right);
    agent.shutdown().await;
}

/// Un agent dont le frein est celui-ci : les tests du socket.
async fn braked_agent(
    env: &Env,
    engine: &Arc<FakeEngine>,
    brake: BrakeSettings,
) -> (Agent, String) {
    let web = container("web", RunState::Running);
    let key = key_of(&web);
    engine.put_silently(web);
    align_clock(env);
    adopt_all(env, engine).await;
    let mut engine_parts = parts(engine.clone());
    engine_parts.log_brake = brake;
    let agent = https::start_engined(env, engine_parts).await;
    wait_listed(&agent, 1).await;
    (agent, key)
}

#[tokio::test]
async fn the_socket_answers_the_account_limit_with_its_code_and_no_wait() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let (agent, key) = braked_agent(&env, &engine, BrakeSettings::production()).await;
    let marie = token(&env, &agent, "marie", Role::ReadOnly).await;
    let lucas = token(&env, &agent, "lucas", Role::ReadOnly).await;
    let mut open = Vec::new();
    for _ in 0..3 {
        let mut client = read(&agent, &marie, &key, 10, None).await;
        assert!(matches!(
            client.next_logs().await.unwrap(),
            LogsServerMessage::Status(_)
        ));
        open.push(client);
    }
    // La quatrième lecture de marie : le plafond du compte, sans attente.
    let mut fourth = read(&agent, &marie, &key, 10, None).await;
    match fourth.next_logs().await.unwrap() {
        LogsServerMessage::Error(error) => {
            assert_eq!(error.code, LogsErrorCode::LogsAccountLimit);
            assert_eq!(error.retry_after_s, None);
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(fourth.next_logs().await, Err(End::Closed(1000))));
    // lucas lit sans souci.
    let mut other = read(&agent, &lucas, &key, 10, None).await;
    assert!(matches!(
        other.next_logs().await.unwrap(),
        LogsServerMessage::Status(_)
    ));
    assert_eq!(engine.open_log_reads(), 4);
    agent.shutdown().await;
}

#[tokio::test]
async fn a_braked_account_is_told_to_wait_and_the_journal_gets_one_entry() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let (agent, key) = braked_agent(
        &env,
        &engine,
        BrakeSettings {
            max_reads: 3,
            burst: 1,
            refill_ms: 3_600_000,
        },
    )
    .await;
    let marie = token(&env, &agent, "marie", Role::ReadOnly).await;
    let mut first = read(&agent, &marie, &key, 10, None).await;
    assert!(matches!(
        first.next_logs().await.unwrap(),
        LogsServerMessage::Status(_)
    ));
    for round in 0..36 {
        let mut braked = read(&agent, &marie, &key, 10, None).await;
        match braked.next_logs().await.unwrap() {
            LogsServerMessage::Error(error) => {
                assert_eq!(error.code, LogsErrorCode::LogsRateLimited, "{round}");
                let wait = error.retry_after_s.expect("l'attente est dite");
                assert!((1..=3_600).contains(&wait), "{wait}");
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(braked.next_logs().await, Err(End::Closed(1000))));
    }
    agent.flush_journal().await;
    // (outcome, target, reason, account) de chaque entrée.
    type Row = (String, Option<String>, Option<String>, Option<String>);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT outcome, target, reason, account FROM audit_events
         WHERE action = 'service.logs.throttled' ORDER BY id",
    )
    .fetch_all(env.db.pool())
    .await
    .unwrap();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].0, "denied");
    assert_eq!(rows[0].1.as_deref(), Some("/services/{key}/logs"));
    assert_eq!(
        rows[0].2.as_deref(),
        Some("plus de 30 ouvertures de journaux refusées en 10 minutes")
    );
    assert_eq!(rows[0].3.as_deref(), Some("marie"));
    // Aucun nom de service ni clé dans l'entrée.
    assert!(!format!("{rows:?}").contains(&key));
    agent.shutdown().await;
}

/// Revue de la PR 89 (point 8) : une lecture de journaux DÉJÀ OUVERTE sur un service dont l'adoption passe
/// « à confirmer » est coupée, comme pour une clé inconnue, et la clé ne se rouvre plus.
#[tokio::test]
async fn a_reading_open_on_a_service_whose_adoption_turns_to_confirm_is_closed_like_an_unknown_key()
{
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let supervisor = Arc::new(FakeSupervisor::new());
    let unit = unit_named("docker-zzz.service");
    let mut service = container("zzz", RunState::Running);
    service.id = "a".repeat(64);
    supervisor.set_owner(&service.id, &unit);
    supervisor.set_status(&unit, running_unit());
    engine.push_log(&service.id, LogSource::Out, T0 + 1, "ligne du service");
    engine.put_silently(service.clone());
    let key = "u_docker-zzz.service";
    env.adopt(key, "zzz", "marie").await;
    let agent = https::start_engined(
        &env,
        parts_supervised(engine.clone(), supervisor.clone(), fast_timings()),
    )
    .await;
    wait_listed(&agent, 1).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;
    let mut client = read(&agent, &admin, key, 1000, None).await;
    client.next_logs().await.unwrap();
    let (items, _, _) = collect(&mut client, 1).await;
    assert_eq!(texts(&items), ["ligne du service"]);
    // Plus d'adoptions par unité que l'agent n'en relit : celle-ci (dernière par ordre alphabétique) n'est
    // plus relue au passage suivant, donc « à confirmer ».
    for index in 0..128 {
        env.adopt(&format!("u_docker-a{index:03}.service"), "autre", "marie")
            .await;
    }
    agent.services().reload_adopted().await;
    let (messages, end) = until_closed(&mut client).await;
    assert_eq!(messages, not_found());
    assert_eq!(end, End::Closed(1000));
    assert!(agent.services().is_unconfirmed(&service_key(key)));
    // Et la clé reçoit la réponse d'une clé inconnue.
    let mut again = read(&agent, &admin, key, 1000, None).await;
    expect_error(&mut again, LogsErrorCode::ServiceNotFound).await;
    agent.shutdown().await;
}
