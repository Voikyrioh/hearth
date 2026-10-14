//! HRT-67 : la lecture des journaux dans la coquille, contre un VRAI agent (TLS 1.3, SQLite, faux moteur) et la vraie
//! liaison. Ce que prouvent ces tests : la forme des événements envoyés à la fenêtre, le plafond de trois lectures,
//! la fenêtre d'accusés (au plus vingt lots sans accusé), la libération d'une session, et que le texte d'une ligne
//! n'entre dans aucun `Debug`. Ce qu'ils ne prouvent PAS : l'émission réelle vers la WebView (`window.emit`), l'écoute
//! (`listen`) et l'appel des commandes par `invoke` : le sink écrit dans un enregistreur à la place de la fenêtre.
//!
//! Règles : BR-SLOG-007, 019, 020, 026.
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "../../../../crates/hearth-link/tests/support/agent.rs"]
mod agent;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent::fake_engine::{FakeEngine, container};
use agent::{PASSWORD, TestAgent};
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::services::RunState;
use hearth_agent::domain::services::logs::LogSource;
use hearth_desktop_lib::link::{LinkRuntime, UiSink};
use hearth_desktop_lib::link_dto::LinkFailure;
use hearth_desktop_lib::services::logs::{
    IDLE_TIMEOUT, KEEP_ALIVE_PERIOD, LogSessions, MAX_SESSIONS_PER_SERVER, MAX_SESSIONS_TOTAL,
    WINDOW,
};
use hearth_desktop_lib::services::logs_dto::LogClosedCauseDto;
use hearth_desktop_lib::vault::{CredentialBackend, CredentialVault};
use hearth_link::LinkConfig;
use hearth_link::domain::state::Thresholds;
use serde_json::Value;

const GUARD: Duration = Duration::from_secs(60);
/// Une minute avant l'horloge de l'agent de test (1 790 000 000 s).
const T0: i64 = 1_789_999_900_000;

#[derive(Default)]
struct Memory(Mutex<HashMap<String, Vec<u8>>>);

#[derive(Clone)]
struct Shared(Arc<Memory>);

impl CredentialBackend for Shared {
    fn read(&self, target: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.0.0.lock().unwrap().get(target).cloned())
    }
    fn write(&self, target: &str, secret: &[u8]) -> Result<(), String> {
        self.0
            .0
            .lock()
            .unwrap()
            .insert(target.into(), secret.into());
        Ok(())
    }
    fn remove(&self, target: &str) -> Result<(), String> {
        self.0.0.lock().unwrap().remove(target);
        Ok(())
    }
}

struct Nowhere;

impl UiSink for Nowhere {
    fn emit(&self, _: &str, _: Value) {}
}

#[derive(Default)]
struct Recorder(Mutex<Vec<(String, Value)>>);

impl UiSink for Recorder {
    fn emit(&self, event: &str, payload: Value) {
        self.0.lock().unwrap().push((event.to_owned(), payload));
    }
}

impl Recorder {
    fn logs(&self, session: &str) -> Vec<Value> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(event, payload)| {
                event == "link://service-logs" && payload["session"] == session
            })
            .map(|(_, payload)| payload["body"].clone())
            .collect()
    }

    fn batches(&self, session: &str) -> usize {
        self.logs(session)
            .iter()
            .filter(|body| body["kind"] == "lines")
            .count()
    }
}

fn config() -> LinkConfig {
    let hour = Duration::from_secs(3_600);
    LinkConfig {
        thresholds: Thresholds {
            silence: hour,
            reconnecting_after: hour,
            offline_after: hour * 2,
            ..Thresholds::scaled(6)
        },
        attempt_timeout: Duration::from_secs(30),
        request_timeout: Duration::from_secs(30),
        persist_timeout: Duration::from_secs(120),
        ..LinkConfig::default()
    }
}

/// Une horloge que le test fait avancer.
#[derive(Clone, Default)]
struct Ticker(Arc<std::sync::atomic::AtomicU64>);

impl Ticker {
    fn clock(&self) -> hearth_desktop_lib::services::logs::Clock {
        let millis = self.0.clone();
        Arc::new(move || Duration::from_millis(millis.load(std::sync::atomic::Ordering::SeqCst)))
    }

    fn advance(&self, by: Duration) {
        self.0.fetch_add(
            u64::try_from(by.as_millis()).unwrap(),
            std::sync::atomic::Ordering::SeqCst,
        );
    }
}

const MAIN: &str = "main";

struct Rig {
    time: Ticker,
    page: u32,
    runtime: Arc<LinkRuntime>,
    engine: Arc<FakeEngine>,
    sessions: Arc<LogSessions>,
    sink: Arc<Recorder>,
    server: String,
    _agent: TestAgent,
}

async fn rig(names: &[&str]) -> Rig {
    let engine = Arc::new(FakeEngine::new());
    for name in names {
        let mut found = container(name, RunState::Running);
        found.created_at = Some(time::OffsetDateTime::from_unix_timestamp(1_789_998_000).unwrap());
        found.started_at = Some(time::OffsetDateTime::from_unix_timestamp(1_789_999_000).unwrap());
        engine.put_silently(found);
    }
    // Les nombres de production : trois lectures par compte.
    let agent = TestAgent::install_braked(
        None,
        Some(engine.clone()),
        None,
        hearth_agent::domain::services::BrakeSettings::production(),
    )
    .await;
    agent.create_account("marie", Role::Admin).await;
    agent.grant_services("marie").await;
    for name in names {
        let id = container(name, RunState::Running).id;
        agent.adopt_container(&id, name, "marie").await;
    }
    let dir = agent::tmp::tempdir().unwrap();
    let vault = Arc::new(CredentialVault::new(Shared(Arc::new(Memory::default()))));
    let runtime = Arc::new(
        LinkRuntime::open_with(dir.path(), vault, "poste-test/0.1", config())
            .await
            .unwrap(),
    );
    let probe = runtime
        .probe("127.0.0.1", Some(agent.addr.port()))
        .await
        .unwrap();
    let server = runtime
        .add_and_login(
            "forge".into(),
            2,
            "127.0.0.1".into(),
            Some(agent.addr.port()),
            &probe.fingerprint,
            probe.mac_addresses,
            "marie",
            PASSWORD.into(),
            false,
            &Nowhere,
        )
        .await
        .unwrap();
    // Le lien s'établit en arrière-plan : on attend « Connecté » (fait observable).
    let id = hearth_link::domain::server::ServerId::parse(&server.id).unwrap();
    tokio::time::timeout(GUARD, async {
        while runtime.manager().state(&id).unwrap().state
            != hearth_link::domain::state::LinkState::Connected
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("lien non établi");
    let time = Ticker::default();
    let sessions = Arc::new(LogSessions::with_clock(time.clock()));
    let page = sessions.announce_page(MAIN);
    Rig {
        time,
        page,
        runtime,
        engine,
        sessions,
        sink: Arc::new(Recorder::default()),
        server: server.id,
        _agent: agent,
    }
}

impl Rig {
    fn open(&self, name: &str) -> Result<String, LinkFailure> {
        self.open_as(MAIN, self.page, self.sink.clone(), name)
    }

    fn open_as(
        &self,
        owner: &str,
        page: u32,
        sink: Arc<dyn UiSink>,
        name: &str,
    ) -> Result<String, LinkFailure> {
        let key = format!("c_{}", container(name, RunState::Running).id);
        self.sessions.open(
            self.runtime.manager(),
            sink,
            owner,
            page,
            &self.server,
            &key,
        )
    }

    fn push(&self, name: &str, n: usize) {
        let id = container(name, RunState::Running).id;
        self.engine
            .push_log(&id, LogSource::Out, T0 + n as i64, &format!("ligne {n}"));
    }
}

async fn until(what: &str, mut check: impl FnMut() -> bool) {
    tokio::time::timeout(GUARD, async {
        while !check() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("délai dépassé en attendant : {what}"));
}

#[tokio::test]
async fn a_session_relays_the_status_then_the_lines_in_the_shape_the_window_reads() {
    let rig = rig(&["web"]).await;
    rig.push("web", 0);
    rig.push("web", 1);
    let session = rig.open("web").unwrap();
    until("les deux lignes", || {
        rig.sink
            .logs(&session)
            .iter()
            .filter(|body| body["kind"] == "lines")
            .map(|body| body["items"].as_array().unwrap().len())
            .sum::<usize>()
            >= 2
    })
    .await;
    let bodies = rig.sink.logs(&session);
    assert_eq!(bodies[0]["kind"], "status");
    assert_eq!(bodies[0]["state"], "live");
    assert_eq!(bodies[0]["mixed"], false);
    let lines = bodies.iter().find(|body| body["kind"] == "lines").unwrap();
    assert_eq!(lines["seq"], 1);
    let first = &lines["items"][0];
    assert_eq!(first["x"], "ligne 0");
    assert_eq!(first["s"], "out");
    assert_eq!(first["t"].as_f64().unwrap(), T0 as f64);
    // Le curseur de reprise reste dans la liaison.
    assert!(lines.get("cursor").is_none());
    rig.sessions.close(&session);
    assert_eq!(rig.sessions.open_count(), 0);
}

#[tokio::test]
async fn a_fourth_session_on_a_server_is_refused_before_the_server_and_a_closed_one_gives_its_place_back()
 {
    let rig = rig(&["a", "b", "c", "d"]).await;
    let mut held = Vec::new();
    for name in ["a", "b", "c"] {
        held.push(rig.open(name).unwrap());
    }
    assert_eq!(rig.sessions.open_count(), MAX_SESSIONS_PER_SERVER);
    assert!(matches!(rig.open("d"), Err(LinkFailure::LogReadsFull)));
    rig.sessions.close(&held.pop().unwrap());
    assert_eq!(rig.sessions.open_count(), MAX_SESSIONS_PER_SERVER - 1);
    // Fermer deux fois, ou fermer une session inconnue, ne fait rien.
    rig.sessions.close("log-inconnue");
    assert!(rig.open("d").is_ok());
    rig.sessions.close_all();
    assert_eq!(rig.sessions.open_count(), 0);
}

#[tokio::test]
async fn at_most_twenty_batches_leave_without_an_acknowledgement() {
    let rig = rig(&["web"]).await;
    let session = rig.open("web").unwrap();
    until("l'état", || !rig.sink.logs(&session).is_empty()).await;
    // Un lot par ligne : on attend chacun avant la suivante pour que l'agent n'en regroupe aucun.
    for n in 0..WINDOW {
        rig.push("web", n);
        until("un lot", || rig.sink.batches(&session) > n).await;
    }
    assert_eq!(rig.sink.batches(&session), WINDOW);
    // La fenêtre est pleine : les lignes suivantes attendent un accusé.
    rig.push("web", WINDOW);
    rig.push("web", WINDOW + 1);
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(
        rig.sink.batches(&session),
        WINDOW,
        "rien ne part sans accusé"
    );
    // Un accusé de trop (plus que de lots en vol) ne gonfle pas la fenêtre.
    rig.sessions.acknowledge(&session, 1_000);
    until("la suite après l'accusé", || {
        rig.sink
            .logs(&session)
            .iter()
            .filter(|body| body["kind"] == "lines")
            .map(|body| body["items"].as_array().unwrap().len())
            .sum::<usize>()
            >= WINDOW + 2
    })
    .await;
    rig.sessions.close(&session);
}

#[tokio::test]
async fn a_refusal_of_the_agent_reaches_the_window_as_an_end_with_its_code_and_frees_the_session() {
    let rig = rig(&["web"]).await;
    // Le compte perd le droit : l'agent refuse la lecture d'un service adopté.
    rig._agent.revoke_services("marie").await;
    let session = rig.open("web").unwrap();
    until("la fin", || {
        rig.sink
            .logs(&session)
            .iter()
            .any(|body| body["kind"] == "ended")
    })
    .await;
    let ended = rig
        .sink
        .logs(&session)
        .into_iter()
        .find(|body| body["kind"] == "ended")
        .unwrap();
    assert_eq!(ended["end"]["kind"], "refused");
    assert_eq!(ended["end"]["refusal"], "missing_right");
    until("la session libérée", || rig.sessions.open_count() == 0).await;
}

#[tokio::test]
async fn the_text_of_a_line_is_in_the_event_for_the_window_and_in_no_debug() {
    let rig = rig(&["web"]).await;
    let id = container("web", RunState::Running).id;
    rig.engine
        .push_log(&id, LogSource::Out, T0, "SENTINEL-TEXTE-91f2");
    let session = rig.open("web").unwrap();
    until("la ligne", || rig.sink.batches(&session) >= 1).await;
    let bodies = rig.sink.logs(&session);
    assert!(
        Value::Array(bodies.clone())
            .to_string()
            .contains("SENTINEL")
    );
    // Le type qui porte la ligne ne la montre dans aucun `Debug`.
    use hearth_desktop_lib::services::logs_dto::ServiceLogBody;
    use hearth_link::LogOutput;
    use hearth_proto::api::service_logs::{LogCursor, LogItem, LogLines};
    let body = ServiceLogBody::from(LogOutput::Lines(LogLines {
        seq: 1,
        items: vec![LogItem {
            t: 1,
            s: hearth_proto::api::service_logs::LogSource::Out,
            x: "SENTINEL-TEXTE-91f2".into(),
            cut: None,
        }],
        cursor: LogCursor { ts: 1, rank: 0 },
    }));
    let shown = format!("{body:?}");
    assert!(!shown.contains("SENTINEL"), "{shown}");
    assert!(shown.contains("chars"), "{shown}");
    rig.sessions.close(&session);
}

fn closed_cause(recorder: &Recorder, session: &str) -> Option<String> {
    recorder
        .logs(session)
        .into_iter()
        .find(|body| body["kind"] == "ended" && body["end"]["kind"] == "closed")
        .map(|body| body["end"]["cause"].as_str().unwrap().to_owned())
}

/// Le plafond de la coquille est celui de l'agent, repris de `hearth_proto` : un seul nombre.
#[test]
fn the_shell_ceiling_per_server_is_the_one_of_the_agent_and_a_guard_sits_above_it() {
    let agent = hearth_agent::domain::services::BrakeSettings::production();
    assert_eq!(MAX_SESSIONS_PER_SERVER, agent.max_reads);
    const { assert!(MAX_SESSIONS_TOTAL > MAX_SESSIONS_PER_SERVER) };
    // Trois battements manqués ne ferment rien.
    assert!(KEEP_ALIVE_PERIOD * 3 < IDLE_TIMEOUT);
}

#[tokio::test]
async fn a_reloaded_page_closes_what_the_previous_one_left_and_an_old_page_cannot_open_again() {
    let rig = rig(&["a", "b", "c", "d"]).await;
    // La première page ouvre trois lectures puis meurt sans les fermer (rechargement ou plantage du rendu).
    for name in ["a", "b", "c"] {
        rig.open(name).unwrap();
    }
    assert_eq!(rig.sessions.open_count(), 3);
    assert!(matches!(rig.open("d"), Err(LinkFailure::LogReadsFull)));
    // La page suivante s'annonce : tout ce qui restait de la génération précédente est fermé.
    let second = rig.sessions.announce_page(MAIN);
    assert_ne!(second, rig.page);
    assert_eq!(rig.sessions.open_count(), 0, "plus rien d'orphelin");
    // La vieille page, si elle vit encore, n'ouvre plus rien ; la neuve ouvre.
    assert!(matches!(rig.open("d"), Err(LinkFailure::LogPageUnknown)));
    assert!(rig.open_as(MAIN, second, rig.sink.clone(), "d").is_ok());
    // Un battement de la vieille page ne voit rien.
    assert!(rig.sessions.keep_alive(MAIN, rig.page).is_empty());
    assert_eq!(rig.sessions.keep_alive(MAIN, second).len(), 1);
    // Une page qui ne s'est jamais annoncée n'ouvre rien.
    assert!(matches!(
        rig.open_as("autre", 1, rig.sink.clone(), "a"),
        Err(LinkFailure::LogPageUnknown)
    ));
}

#[tokio::test]
async fn a_reload_in_one_window_leaves_the_reads_of_another_window_alone() {
    let rig = rig(&["a", "b"]).await;
    let other = rig.sessions.announce_page("autre");
    rig.open("a").unwrap();
    let theirs = rig.open_as("autre", other, rig.sink.clone(), "b").unwrap();
    rig.sessions.announce_page(MAIN);
    assert_eq!(rig.sessions.open_count(), 1);
    assert_eq!(rig.sessions.keep_alive("autre", other), vec![theirs]);
}

#[tokio::test]
async fn hiding_the_window_closes_its_reads_tells_the_page_and_gives_the_places_back() {
    let rig = rig(&["a", "b", "c", "d"]).await;
    let sessions: Vec<String> = ["a", "b", "c"]
        .iter()
        .map(|name| rig.open(name).unwrap())
        .collect();
    until("l'état de chaque lecture", || {
        sessions.iter().all(|s| !rig.sink.logs(s).is_empty())
    })
    .await;
    assert_eq!(
        rig.sessions
            .close_window(MAIN, LogClosedCauseDto::WindowHidden),
        3
    );
    assert_eq!(rig.sessions.open_count(), 0);
    for session in &sessions {
        assert_eq!(
            closed_cause(&rig.sink, session).as_deref(),
            Some("window_hidden")
        );
    }
    // La page, restée la même, ne voit plus ses sessions.
    assert!(rig.sessions.keep_alive(MAIN, rig.page).is_empty());
    // Elle peut reprendre : une place est bien rendue, côté agent aussi.
    let again = rig.open("d").unwrap();
    until("le nouvel état, place rendue chez l'agent", || {
        rig.sink
            .logs(&again)
            .iter()
            .any(|body| body["kind"] == "status")
    })
    .await;
    rig.sessions.close(&again);
}

#[tokio::test]
async fn a_session_whose_page_went_silent_is_closed_after_the_timeout_and_a_beat_keeps_it() {
    let rig = rig(&["a"]).await;
    let session = rig.open("a").unwrap();
    rig.time.advance(IDLE_TIMEOUT - Duration::from_secs(1));
    assert_eq!(rig.sessions.reap(), 0, "pas encore");
    // Un battement de la page repart le délai.
    assert_eq!(
        rig.sessions.keep_alive(MAIN, rig.page),
        vec![session.clone()]
    );
    rig.time.advance(IDLE_TIMEOUT - Duration::from_secs(1));
    assert_eq!(rig.sessions.reap(), 0);
    // Un accusé est aussi un signe de vie.
    rig.sessions.acknowledge(&session, 1);
    rig.time.advance(IDLE_TIMEOUT - Duration::from_secs(1));
    assert_eq!(rig.sessions.reap(), 0);
    // Puis le silence dépasse le délai.
    rig.time.advance(Duration::from_secs(2));
    assert_eq!(rig.sessions.reap(), 1);
    assert_eq!(rig.sessions.open_count(), 0);
    assert_eq!(closed_cause(&rig.sink, &session).as_deref(), Some("idle"));
}

#[tokio::test]
async fn the_global_guard_refuses_beyond_its_number() {
    let rig = rig(&["a", "b", "c"]).await;
    let sessions = Arc::new(LogSessions::with_limits(rig.time.clock(), 3, 2));
    let page = sessions.announce_page(MAIN);
    let open = |name: &str| {
        let key = format!("c_{}", container(name, RunState::Running).id);
        sessions.open(
            rig.runtime.manager(),
            rig.sink.clone(),
            MAIN,
            page,
            &rig.server,
            &key,
        )
    };
    assert!(open("a").is_ok());
    assert!(open("b").is_ok());
    assert!(matches!(open("c"), Err(LinkFailure::LogReadsFull)));
    assert_eq!(sessions.open_on(&rig.server), 2);
    sessions.close_all();
}

#[tokio::test]
async fn disconnecting_or_changing_a_server_closes_only_its_reads_and_says_why() {
    let rig = rig(&["a", "b"]).await;
    let one = rig.open("a").unwrap();
    let two = rig.open("b").unwrap();
    assert_eq!(rig.sessions.open_on(&rig.server), 2);
    assert_eq!(
        rig.sessions
            .close_server("serveur-inconnu", LogClosedCauseDto::ServerChanged),
        0
    );
    assert_eq!(rig.sessions.open_count(), 2);
    assert_eq!(
        rig.sessions
            .close_server(&rig.server, LogClosedCauseDto::ServerChanged),
        2
    );
    assert_eq!(rig.sessions.open_count(), 0);
    for session in [one, two] {
        assert_eq!(
            closed_cause(&rig.sink, &session).as_deref(),
            Some("server_changed")
        );
    }
}

#[tokio::test]
async fn a_destroyed_window_and_the_exit_close_everything_without_a_word() {
    let rig = rig(&["a", "b"]).await;
    let one = rig.open("a").unwrap();
    assert_eq!(rig.sessions.forget_window(MAIN), 1);
    assert_eq!(rig.sessions.open_count(), 0);
    assert!(
        closed_cause(&rig.sink, &one).is_none(),
        "personne à avertir"
    );
    // La fenêtre détruite oublie aussi sa génération.
    assert!(matches!(rig.open("a"), Err(LinkFailure::LogPageUnknown)));
    let page = rig.sessions.announce_page(MAIN);
    rig.open_as(MAIN, page, rig.sink.clone(), "a").unwrap();
    rig.open_as(MAIN, page, rig.sink.clone(), "b").unwrap();
    rig.sessions.close_all();
    assert_eq!(rig.sessions.open_count(), 0);
}

#[tokio::test]
async fn the_lines_of_a_read_go_only_to_the_sink_of_the_window_that_opened_it() {
    let rig = rig(&["a", "b"]).await;
    let other_sink = Arc::new(Recorder::default());
    let other = rig.sessions.announce_page("autre");
    rig.push("a", 0);
    rig.push("b", 0);
    let mine = rig.open("a").unwrap();
    let theirs = rig
        .open_as("autre", other, other_sink.clone(), "b")
        .unwrap();
    until("les lignes des deux", || {
        rig.sink.batches(&mine) >= 1 && other_sink.batches(&theirs) >= 1
    })
    .await;
    assert_eq!(rig.sink.batches(&theirs), 0);
    assert_eq!(other_sink.batches(&mine), 0);
    rig.sessions.close_all();
}
