//! HRT-17, ADR-0021 : la cible de l'agent dans le flux de versions. D'abord le service du client
//! avec des ports simulés (une lecture de la cible par vérification, jamais seule ni en plus du
//! quota, silence sur échec, cible refusée jamais retenue, fichier d'état modifié à la main), puis
//! l'adaptateur réel avec le vrai greffon contre un serveur local (section `agent` du `latest.json`, UNE requête, client à jour jamais proposé, section absente ou trop grosse, section
//! bornées, aucune requête sans adresse configurée). Horloge injectée : aucun test n'attend.
#![allow(clippy::unwrap_used, clippy::expect_used)] // tests : les helpers peuvent paniquer

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use hearth_desktop_lib::agent_update::domain::{
    AGENT_PLATFORM, AgentCandidate, AgentTargetRecord, MANIFEST_MAX_BYTES,
};
use hearth_desktop_lib::update::domain::{
    Candidate, DownloadPolicy, MAX_AUTOMATIC_ATTEMPTS_PER_DAY, UpdateRecord,
};
use hearth_desktop_lib::update::dto::UpdateStateDto;
use hearth_desktop_lib::update::feed::TauriFeed;
use hearth_desktop_lib::update::ports::{
    Clock, DownloadError, Feed, FeedError, StateSink, UpdateStore, VerifiedInstaller,
};
use hearth_desktop_lib::update::service::UpdateService;
use serde_json::json;
use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;
use url::Url;

const HOUR: i64 = 60 * 60 * 1000;
const START: i64 = 1_800_000_000_000;
const URL: &str =
    "https://github.com/Voikyrioh/hearth/releases/download/v0.2.0/hearth-agent-linux-x86_64";
const SIGNATURE: &str =
    "untrusted comment: signature from minisign secret key\nRUQabc\ntrusted comment: x\nabc\n";

fn candidate() -> AgentCandidate {
    AgentCandidate {
        version: "0.2.0".into(),
        url: URL.into(),
        signature: SIGNATURE.into(),
        sha256: "ab".repeat(32),
    }
}

// ---- le service, avec des ports simulés -----------------------------------------------------------

struct FakeClock(AtomicI64);

impl Clock for FakeClock {
    fn now_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Default)]
struct MemStore(Mutex<UpdateRecord>);

impl UpdateStore for MemStore {
    fn load(&self) -> UpdateRecord {
        self.0.lock().unwrap().clone()
    }
    fn save(&self, record: &UpdateRecord) -> Result<(), String> {
        *self.0.lock().unwrap() = record.clone();
        Ok(())
    }
}

struct Quiet;

impl StateSink for Quiet {
    fn publish(&self, _: &UpdateStateDto) {}
}

#[derive(Default)]
struct FakeFeed {
    checks: Mutex<VecDeque<Result<Option<Candidate>, FeedError>>>,
    agents: Mutex<VecDeque<Result<Option<AgentCandidate>, FeedError>>>,
    check_calls: AtomicUsize,
    agent_calls: AtomicUsize,
}

#[async_trait]
impl Feed for FakeFeed {
    async fn check(&self) -> Result<Option<Candidate>, FeedError> {
        self.check_calls.fetch_add(1, Ordering::SeqCst);
        self.checks.lock().unwrap().pop_front().unwrap_or(Ok(None))
    }

    async fn check_agent(&self) -> Result<Option<AgentCandidate>, FeedError> {
        self.agent_calls.fetch_add(1, Ordering::SeqCst);
        self.agents.lock().unwrap().pop_front().unwrap_or(Ok(None))
    }

    async fn download(
        &self,
        _: &str,
        _: &mut (dyn FnMut(u64, Option<u64>) + Send),
    ) -> Result<VerifiedInstaller, DownloadError> {
        Err(DownloadError::Failed("non utilisé".into()))
    }

    fn install(&self, _: &str, _: VerifiedInstaller) -> Result<(), DownloadError> {
        Ok(())
    }
}

struct Rig {
    service: UpdateService,
    feed: Arc<FakeFeed>,
    store: Arc<MemStore>,
    clock: Arc<FakeClock>,
}

fn rig() -> Rig {
    let clock = Arc::new(FakeClock(AtomicI64::new(START)));
    let store = Arc::new(MemStore::default());
    let feed = Arc::new(FakeFeed::default());
    let service = UpdateService::new(
        clock.clone(),
        store.clone(),
        feed.clone(),
        Arc::new(Quiet),
        DownloadPolicy::github_releases(),
        "0.1.0",
    );
    Rig {
        service,
        feed,
        store,
        clock,
    }
}

impl Rig {
    fn script_agent(&self, outcome: Result<Option<AgentCandidate>, FeedError>) {
        self.feed.agents.lock().unwrap().push_back(outcome);
    }
}

#[tokio::test]
async fn a_check_reads_the_agent_target_in_the_same_attempt_and_retains_it_validated() {
    let rig = rig();
    rig.script_agent(Ok(Some(candidate())));
    rig.service.check_now().await;
    assert_eq!(rig.feed.check_calls.load(Ordering::SeqCst), 1);
    assert_eq!(rig.feed.agent_calls.load(Ordering::SeqCst), 1);
    let target = rig.service.agent_target().expect("une cible");
    assert_eq!(target.version().to_string(), "0.2.0");
    assert_eq!(target.url().as_str(), URL);
    // Retenue dans l'état du client, pas ailleurs.
    assert!(rig.store.load().agent.is_some());
}

#[tokio::test]
async fn the_agent_file_is_read_once_per_check_never_alone_and_never_beyond_the_quota() {
    let rig = rig();
    // Première vérification automatique : une lecture du flux du client, une de la cible.
    rig.service.check_if_due().await;
    assert_eq!(rig.feed.check_calls.load(Ordering::SeqCst), 1);
    assert_eq!(rig.feed.agent_calls.load(Ordering::SeqCst), 1);
    // Les heures suivantes (battement horaire) : ni l'un ni l'autre avant 24 h.
    for _ in 0..23 {
        rig.clock.0.fetch_add(HOUR, Ordering::SeqCst);
        rig.service.check_if_due().await;
    }
    assert_eq!(rig.feed.check_calls.load(Ordering::SeqCst), 1);
    assert_eq!(rig.feed.agent_calls.load(Ordering::SeqCst), 1);
    // Le lendemain : une de chaque.
    rig.clock.0.fetch_add(HOUR, Ordering::SeqCst);
    rig.service.check_if_due().await;
    assert_eq!(rig.feed.check_calls.load(Ordering::SeqCst), 2);
    assert_eq!(rig.feed.agent_calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn the_agent_file_follows_the_client_attempts_one_for_one_under_the_daily_cap() {
    // Un échec APRÈS émission consomme le quota : prochaine tentative le lendemain. Le plafond dur
    // (3 tentatives par 24 h glissantes) est celui du client : la cible de l'agent ne le double pas.
    let rig = rig();
    for _ in 0..(MAX_AUTOMATIC_ATTEMPTS_PER_DAY + 3) {
        rig.feed
            .checks
            .lock()
            .unwrap()
            .push_back(Err(FeedError::failed("réponse en erreur")));
        rig.service.check_if_due().await;
        rig.clock.0.fetch_add(HOUR, Ordering::SeqCst);
    }
    let checks = rig.feed.check_calls.load(Ordering::SeqCst);
    assert!(checks <= MAX_AUTOMATIC_ATTEMPTS_PER_DAY, "{checks}");
    assert_eq!(rig.feed.agent_calls.load(Ordering::SeqCst), checks);
}

#[tokio::test]
async fn without_any_request_for_the_client_the_agent_file_is_not_read_either() {
    let rig = rig();
    rig.feed
        .checks
        .lock()
        .unwrap()
        .push_back(Err(FeedError::offline("pas de réseau")));
    rig.service.check_if_due().await;
    assert_eq!(rig.feed.check_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        rig.feed.agent_calls.load(Ordering::SeqCst),
        0,
        "aucune requête n'est partie : rien non plus pour l'agent"
    );
}

#[tokio::test]
async fn a_failed_read_keeps_the_previous_target_and_a_missing_entry_clears_it() {
    let rig = rig();
    rig.script_agent(Ok(Some(candidate())));
    rig.service.check_now().await;
    assert!(rig.service.agent_target().is_some());
    // Panne de lecture (serveur muet, 5xx) : silence, la cible reste.
    rig.clock.0.fetch_add(HOUR, Ordering::SeqCst);
    rig.script_agent(Err(FeedError::failed("délai dépassé")));
    rig.service.check_now().await;
    assert!(rig.service.agent_target().is_some());
    // La release n'annonce plus d'agent : plus de cible.
    rig.clock.0.fetch_add(HOUR, Ordering::SeqCst);
    rig.script_agent(Ok(None));
    rig.service.check_now().await;
    assert!(rig.service.agent_target().is_none());
}

#[tokio::test]
async fn a_target_the_client_refuses_is_never_retained_and_replaces_nothing_dangerous() {
    for bad in [
        AgentCandidate {
            url: "http://github.com/Voikyrioh/hearth/releases/download/v0.2.0/a".into(),
            ..candidate()
        },
        AgentCandidate {
            url: "https://exemple.org/hearth-agent".into(),
            ..candidate()
        },
        AgentCandidate {
            url: "https://127.0.0.1/Voikyrioh/hearth/releases/download/v0.2.0/a".into(),
            ..candidate()
        },
        AgentCandidate {
            version: "0.2.0-rc.1".into(),
            ..candidate()
        },
        AgentCandidate {
            sha256: "zz".into(),
            ..candidate()
        },
    ] {
        let rig = rig();
        rig.script_agent(Ok(Some(candidate())));
        rig.service.check_now().await;
        assert!(rig.service.agent_target().is_some());
        rig.clock.0.fetch_add(HOUR, Ordering::SeqCst);
        rig.script_agent(Ok(Some(bad.clone())));
        rig.service.check_now().await;
        assert!(
            rig.service.agent_target().is_none(),
            "refusée, donc effacée : {bad:?}"
        );
        assert!(rig.store.load().agent.is_none());
    }
}

#[tokio::test]
async fn a_state_file_edited_by_hand_cannot_make_the_client_send_another_address() {
    let clock = Arc::new(FakeClock(AtomicI64::new(START)));
    let store = Arc::new(MemStore::default());
    store
        .save(&UpdateRecord {
            agent: Some(AgentTargetRecord {
                version: "0.2.0".into(),
                url: "https://exemple.org/hearth-agent".into(),
                signature: SIGNATURE.into(),
                sha256: "ab".repeat(32),
            }),
            ..UpdateRecord::default()
        })
        .unwrap();
    let service = UpdateService::new(
        clock,
        store,
        Arc::new(FakeFeed::default()),
        Arc::new(Quiet),
        DownloadPolicy::github_releases(),
        "0.1.0",
    );
    assert!(service.agent_target().is_none());
}

#[tokio::test]
async fn the_interface_state_of_the_client_update_carries_nothing_of_the_agent_target() {
    let rig = rig();
    rig.script_agent(Ok(Some(candidate())));
    let state = rig.service.check_now().await;
    let text = serde_json::to_string(&state).unwrap();
    assert!(!text.contains("hearth-agent"), "{text}");
    assert!(!text.contains(&"ab".repeat(32)), "{text}");
}

#[tokio::test]
async fn without_a_registered_server_the_agent_file_is_not_read() {
    let clock = Arc::new(FakeClock(AtomicI64::new(START)));
    let feed = Arc::new(FakeFeed::default());
    let service = UpdateService::new(
        clock,
        Arc::new(MemStore::default()),
        feed.clone(),
        Arc::new(Quiet),
        DownloadPolicy::github_releases(),
        "0.1.0",
    )
    .with_agent_wanted(|| false);
    service.check_now().await;
    assert_eq!(feed.check_calls.load(Ordering::SeqCst), 1);
    assert_eq!(feed.agent_calls.load(Ordering::SeqCst), 0);
}

fn service_on(store: &Arc<MemStore>) -> UpdateService {
    UpdateService::new(
        Arc::new(FakeClock(AtomicI64::new(START))),
        store.clone(),
        Arc::new(FakeFeed::default()),
        Arc::new(Quiet),
        DownloadPolicy::github_releases(),
        "0.1.0",
    )
}

fn known(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| (*id).to_owned()).collect()
}

#[tokio::test]
async fn an_announced_result_is_noted_by_its_date_and_survives_a_restart_of_the_client() {
    use hearth_desktop_lib::update::service::AckRefusal;
    let store = Arc::new(MemStore::default());
    let service = service_on(&store);
    let carnet = known(&["forge", "salon"]);
    assert_eq!(service.agent_result_seen("forge"), None);
    // La coquille lit le résultat chez l'agent, puis seulement celui-là peut être acquitté.
    service.note_agent_result_read("forge", "2026-10-06T10:00:00Z");
    service
        .ack_agent_result("forge", "2026-10-06T10:00:00Z", &carnet)
        .unwrap();
    assert_eq!(
        service.agent_result_seen("forge").as_deref(),
        Some("2026-10-06T10:00:00Z")
    );
    // Le client redémarre : la note est lue sur le disque, par serveur.
    let again = service_on(&store);
    assert_eq!(
        again.agent_result_seen("forge").as_deref(),
        Some("2026-10-06T10:00:00Z")
    );
    assert_eq!(again.agent_result_seen("salon"), None);
    // Un résultat suivant (autre date, même issue) est un autre résultat, lu puis acquitté.
    again.note_agent_result_read("forge", "2026-10-07T10:00:00Z");
    again
        .ack_agent_result("forge", "2026-10-07T10:00:00Z", &carnet)
        .unwrap();
    assert_eq!(
        again.agent_result_seen("forge").as_deref(),
        Some("2026-10-07T10:00:00Z")
    );
    // Une date plus ancienne lue ensuite ne fait pas reculer la note.
    again.note_agent_result_read("forge", "2026-10-05T10:00:00Z");
    again
        .ack_agent_result("forge", "2026-10-05T10:00:00Z", &carnet)
        .unwrap();
    assert_eq!(
        again.agent_result_seen("forge").as_deref(),
        Some("2026-10-07T10:00:00Z")
    );
    let _ = AckRefusal::UnknownServer;
}

#[tokio::test]
async fn an_acknowledgement_that_is_not_the_result_the_shell_read_is_refused_and_writes_nothing() {
    use hearth_desktop_lib::update::service::AckRefusal;
    let store = Arc::new(MemStore::default());
    let service = service_on(&store);
    let carnet = known(&["forge"]);
    service.note_agent_result_read("forge", "2026-10-06T10:00:00Z");
    let before = store.load();
    // Serveur inconnu du carnet.
    assert_eq!(
        service.ack_agent_result("inconnu", "2026-10-06T10:00:00Z", &carnet),
        Err(AckRefusal::UnknownServer)
    );
    // Dates invalides.
    for bad in ["", "9999", "hier", "2026-10-06", "2026-13-45T00:00:00Z"] {
        assert_eq!(
            service.ack_agent_result("forge", bad, &carnet),
            Err(AckRefusal::InvalidDate),
            "{bad}"
        );
    }
    // Date valide qui n'est PAS celle du résultat lu (une date « 9999 » ne peut pas empoisonner la note).
    for other in [
        "9999-12-31T23:59:59Z",
        "2026-10-06T10:00:01Z",
        "2000-01-01T00:00:00Z",
    ] {
        assert_eq!(
            service.ack_agent_result("forge", other, &carnet),
            Err(AckRefusal::NotTheResultRead),
            "{other}"
        );
    }
    // Un serveur dont aucun résultat n'a été lu : rien à acquitter.
    assert_eq!(
        service.ack_agent_result("salon", "2026-10-06T10:00:00Z", &known(&["forge", "salon"])),
        Err(AckRefusal::NotTheResultRead)
    );
    assert_eq!(store.load(), before, "rien n'a été écrit");
    assert_eq!(service.agent_result_seen("forge"), None);
}

#[tokio::test]
async fn the_notes_file_never_grows_beyond_the_registered_servers_and_forgets_removed_ones() {
    let store = Arc::new(MemStore::default());
    let service = service_on(&store);
    let at = "2026-10-06T10:00:00Z";
    for id in ["a", "b", "c"] {
        service.note_agent_result_read(id, at);
        service
            .ack_agent_result(id, at, &known(&["a", "b", "c"]))
            .unwrap();
    }
    assert_eq!(store.load().agent_results_seen.len(), 3);
    // Des identifiants inventés par une page ne sont pas du carnet : refusés, aucune entrée de plus.
    for n in 0..200 {
        let id = format!("faux{n}");
        service.note_agent_result_read(&id, at);
        assert!(
            service
                .ack_agent_result(&id, at, &known(&["a", "b", "c"]))
                .is_err()
        );
    }
    assert_eq!(store.load().agent_results_seen.len(), 3);
    // Le serveur « c » est supprimé : son entrée part au prochain acquittement.
    service.note_agent_result_read("a", "2026-10-07T10:00:00Z");
    service
        .ack_agent_result("a", "2026-10-07T10:00:00Z", &known(&["a", "b"]))
        .unwrap();
    let kept: Vec<String> = store.load().agent_results_seen.keys().cloned().collect();
    assert_eq!(kept, ["a", "b"]);
}

// ---- l'adaptateur réel contre un serveur local ----------------------------------------------------

#[derive(Clone)]
enum Reply {
    Ok(Vec<u8>),
    Status(u16),
}

struct Server {
    port: u16,
    routes: Arc<Mutex<HashMap<String, Reply>>>,
    hits: Arc<Mutex<Vec<String>>>,
}

impl Server {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let routes: Arc<Mutex<HashMap<String, Reply>>> = Arc::default();
        let hits: Arc<Mutex<Vec<String>>> = Arc::default();
        let (shared_routes, shared_hits) = (routes.clone(), hits.clone());
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                let (routes, hits) = (shared_routes.clone(), shared_hits.clone());
                tokio::spawn(async move {
                    let mut buffer = Vec::new();
                    let mut chunk = [0u8; 1024];
                    while !buffer.windows(4).any(|window| window == b"\r\n\r\n") {
                        match stream.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
                        }
                    }
                    let head = String::from_utf8_lossy(&buffer).into_owned();
                    let path = head.split_whitespace().nth(1).unwrap_or("/").to_owned();
                    hits.lock().unwrap().push(path.clone());
                    let reply = routes.lock().unwrap().get(&path).cloned();
                    let response: Vec<u8> = match reply {
                        None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
                        Some(Reply::Status(code)) => format!(
                            "HTTP/1.1 {code} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        )
                        .into_bytes(),
                        Some(Reply::Ok(body)) => {
                            let mut out = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                body.len()
                            )
                            .into_bytes();
                            out.extend(body);
                            out
                        }
                    };
                    let _ = stream.write_all(&response).await;
                    let _ = stream.shutdown().await;
                });
            }
        });
        Self { port, routes, hits }
    }

    fn url(&self, path: &str) -> Url {
        Url::parse(&format!("http://127.0.0.1:{}{path}", self.port)).unwrap()
    }

    fn serve(&self, path: &str, reply: Reply) {
        self.routes.lock().unwrap().insert(path.to_owned(), reply);
    }

    fn hits(&self) -> usize {
        self.hits.lock().unwrap().len()
    }
}

/// Application simulée avec le VRAI greffon de mise à jour, configuré comme dans `tauri.conf.json` (dont
/// `requireSignedVersion`) : la version du client y est 0.1.0 (`mock_context`).
fn app() -> tauri::App<MockRuntime> {
    use hearth_desktop_lib::update::feed::plugin_with_key;
    let public = minisign::KeyPair::generate_unencrypted_keypair()
        .unwrap()
        .pk
        .to_box()
        .unwrap()
        .to_string();
    let mut context = mock_context(noop_assets());
    let config: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json"),
        )
        .unwrap(),
    )
    .unwrap();
    context
        .config_mut()
        .plugins
        .0
        .insert("updater".to_owned(), config["plugins"]["updater"].clone());
    mock_builder()
        .plugin(plugin_with_key::<MockRuntime>(&public))
        .build(context)
        .unwrap()
}

fn agent_section() -> serde_json::Value {
    json!({
        "version": "0.2.0",
        "platforms": { AGENT_PLATFORM: { "url": URL, "signature": SIGNATURE, "sha256": "ab".repeat(32) } }
    })
}

/// Le `latest.json` d'un client en `version`, avec ou sans section de l'agent.
fn latest(server: &Server, version: &str, agent: Option<serde_json::Value>) -> Vec<u8> {
    let mut value = json!({
        "version": version,
        "notes": "Notes.",
        "pub_date": "2026-10-06T10:00:00Z",
        "platforms": { "windows-x86_64": { "signature": "c2ln", "url": server.url("/Hearth.exe").to_string() } }
    });
    if let Some(agent) = agent {
        value["agent"] = agent;
    }
    value.to_string().into_bytes()
}

fn feed_of(app: &tauri::App<MockRuntime>, server: &Server, path: &str) -> TauriFeed<MockRuntime> {
    TauriFeed::with_endpoint(
        app.handle().clone(),
        server.url(path),
        DownloadPolicy::local_for_tests(server.port),
    )
}

#[tokio::test]
async fn one_request_gives_the_client_announcement_and_the_agent_section() {
    let server = Server::start().await;
    server.serve(
        "/latest.json",
        Reply::Ok(latest(&server, "1.1.0", Some(agent_section()))),
    );
    let app = app();
    let feed = feed_of(&app, &server, "/latest.json");
    let announced = feed
        .check()
        .await
        .unwrap()
        .expect("une version du client plus récente");
    assert_eq!(announced.version, "1.1.0");
    assert_eq!(feed.check_agent().await.unwrap(), Some(candidate()));
    assert_eq!(
        server.hits(),
        1,
        "UNE requête par vérification : le client ET l'agent"
    );
}

#[tokio::test]
async fn a_client_that_is_up_to_date_is_never_proposed_a_version_but_the_agent_section_is_read() {
    // Le comparateur du greffon rend le manifeste toujours ; le client, lui, ignore une version égale
    // ou inférieure (jamais de mise à jour ni d'annonce à tort, jamais de rétrogradation).
    for version in ["0.1.0", "0.0.9", "0.0.1"] {
        let server = Server::start().await;
        server.serve(
            "/latest.json",
            Reply::Ok(latest(&server, version, Some(agent_section()))),
        );
        let app = app();
        let feed = feed_of(&app, &server, "/latest.json");
        assert!(feed.check().await.unwrap().is_none(), "client en {version}");
        assert_eq!(
            feed.check_agent().await.unwrap(),
            Some(candidate()),
            "client en {version}"
        );
        assert_eq!(server.hits(), 1);
        // Rien n'est retenu pour être téléchargé.
        let mut progress = |_: u64, _: Option<u64>| {};
        assert!(matches!(
            feed.download(version, &mut progress).await,
            Err(DownloadError::NotStaged)
        ));
    }
}

#[tokio::test]
async fn a_manifest_without_an_agent_section_offers_nothing_for_the_agent() {
    let server = Server::start().await;
    server.serve("/latest.json", Reply::Ok(latest(&server, "1.1.0", None)));
    let app = app();
    let feed = feed_of(&app, &server, "/latest.json");
    assert!(feed.check().await.unwrap().is_some());
    assert_eq!(feed.check_agent().await.unwrap(), None);
}

#[tokio::test]
async fn a_broken_or_oversized_agent_section_is_an_error_that_does_not_touch_the_client_check() {
    for section in [
        json!({ "version": "0.2.0", "platforms": { AGENT_PLATFORM: { "url": URL } } }),
        json!({ "version": "0.2.0", "padding": "x".repeat(MANIFEST_MAX_BYTES + 1) }),
    ] {
        let server = Server::start().await;
        server.serve(
            "/latest.json",
            Reply::Ok(latest(&server, "1.1.0", Some(section))),
        );
        let app = app();
        let feed = feed_of(&app, &server, "/latest.json");
        assert!(
            feed.check().await.unwrap().is_some(),
            "le client est annoncé quand même"
        );
        assert!(feed.check_agent().await.is_err());
    }
}

#[tokio::test]
async fn a_failed_client_check_leaves_no_agent_section_to_use() {
    let server = Server::start().await;
    server.serve("/latest.json", Reply::Status(500));
    let app = app();
    let feed = feed_of(&app, &server, "/latest.json");
    assert!(feed.check().await.is_err());
    assert!(
        feed.check_agent().await.is_err(),
        "erreur : la cible précédente est gardée"
    );
    // Et une lecture de l'agent avant toute vérification n'invente rien non plus.
    let fresh = feed_of(&app, &server, "/latest.json");
    assert!(fresh.check_agent().await.is_err());
}

#[tokio::test]
async fn the_agent_section_of_an_older_manifest_is_not_reused_after_a_new_check() {
    let server = Server::start().await;
    server.serve(
        "/latest.json",
        Reply::Ok(latest(&server, "1.1.0", Some(agent_section()))),
    );
    let app = app();
    let feed = feed_of(&app, &server, "/latest.json");
    feed.check().await.unwrap();
    assert!(feed.check_agent().await.unwrap().is_some());
    server.serve("/latest.json", Reply::Ok(latest(&server, "1.1.0", None)));
    feed.check().await.unwrap();
    assert_eq!(feed.check_agent().await.unwrap(), None);
}

// ---- le VRAI code de publication (`cargo xtask agent-manifest`) lu par le client -------------------

#[allow(dead_code)]
#[path = "../../../../xtask/src/release_core.rs"]
mod release_core;

#[tokio::test]
async fn the_section_made_by_the_publication_code_is_read_and_retained_by_the_client() {
    use hearth_desktop_lib::agent_update::domain::validate_target;
    let keys = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let public = keys.pk.to_box().unwrap().to_string();
    let binary = b"hearth-agent 0.2.0";
    let signature = minisign::sign(
        None,
        &keys.sk,
        std::io::Cursor::new(binary),
        Some("trusted"),
        Some("signature de test"),
    )
    .unwrap()
    .to_string();
    // Ce que l'agent exigera : la signature se vérifie contre SA clé.
    release_core::verify_agent_signature(&public, &signature, binary).unwrap();
    let sha = "ab".repeat(32);
    let section = release_core::agent_section(
        "https://github.com/Voikyrioh/hearth/releases/download/",
        "0.2.0",
        &signature,
        &sha,
        URL,
    )
    .unwrap();
    // Le manifeste du client (celui de `client-manifest`), puis la section ajoutée : UN seul fichier.
    let server = Server::start().await;
    let client = String::from_utf8(latest(&server, "1.1.0", None)).unwrap();
    let merged = release_core::add_agent_section(&client, section).unwrap();
    server.serve("/latest.json", Reply::Ok(merged.into_bytes()));
    let app = app();
    let feed = feed_of(&app, &server, "/latest.json");
    assert_eq!(feed.check().await.unwrap().unwrap().version, "1.1.0");
    let found = feed
        .check_agent()
        .await
        .unwrap()
        .expect("section de l'agent");
    let target = validate_target(&found, &DownloadPolicy::github_releases()).unwrap();
    assert_eq!(target.version().to_string(), "0.2.0");
    assert_eq!(target.url().as_str(), URL);
    assert_eq!(target.sha256(), sha);
    assert!(target.signature().starts_with("untrusted comment:"));
    assert_eq!(server.hits(), 1);
    assert_ne!(release_core::AGENT_TARGET, release_core::TARGET);
}
