//! HRT-17, ADR-0021 : la cible de l'agent dans le flux de versions. D'abord le service du client
//! avec des ports simulés (une lecture de la cible par vérification, jamais seule ni en plus du
//! quota, silence sur échec, cible refusée jamais retenue, fichier d'état modifié à la main), puis
//! l'adaptateur réel contre un serveur local (fichier `agent.json`, 404, trop gros, redirections
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
    Candidate, DownloadPolicy, MAX_AUTOMATIC_ATTEMPTS_PER_DAY, MAX_REDIRECTS, UpdateRecord,
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

#[tokio::test]
async fn an_announced_result_is_noted_by_its_date_and_survives_a_restart_of_the_client() {
    let store = Arc::new(MemStore::default());
    let make = |store: &Arc<MemStore>| {
        UpdateService::new(
            Arc::new(FakeClock(AtomicI64::new(START))),
            store.clone(),
            Arc::new(FakeFeed::default()),
            Arc::new(Quiet),
            DownloadPolicy::github_releases(),
            "0.1.0",
        )
    };
    let service = make(&store);
    assert_eq!(service.agent_result_seen("forge"), None);
    service.ack_agent_result("forge", "2026-10-06T10:00:00Z");
    // Une date plus ancienne ne remplace pas la plus récente.
    service.ack_agent_result("forge", "2026-10-05T10:00:00Z");
    assert_eq!(
        service.agent_result_seen("forge").as_deref(),
        Some("2026-10-06T10:00:00Z")
    );
    // Le client redémarre : la note est lue sur le disque, par serveur.
    let again = make(&store);
    assert_eq!(
        again.agent_result_seen("forge").as_deref(),
        Some("2026-10-06T10:00:00Z")
    );
    assert_eq!(again.agent_result_seen("salon"), None);
    // Un résultat suivant (autre date, même issue) est un autre résultat.
    again.ack_agent_result("forge", "2026-10-07T10:00:00Z");
    assert_eq!(
        again.agent_result_seen("forge").as_deref(),
        Some("2026-10-07T10:00:00Z")
    );
}

// ---- l'adaptateur réel contre un serveur local ----------------------------------------------------

#[derive(Clone)]
enum Reply {
    Ok(Vec<u8>),
    Status(u16),
    Redirect(String),
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
                        Some(Reply::Redirect(to)) => format!(
                            "HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
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

fn app() -> tauri::App<MockRuntime> {
    mock_builder().build(mock_context(noop_assets())).unwrap()
}

fn manifest() -> Vec<u8> {
    json!({
        "version": "0.2.0",
        "pub_date": "2026-10-06T10:00:00Z",
        "platforms": { AGENT_PLATFORM: { "url": URL, "signature": SIGNATURE, "sha256": "ab".repeat(32) } }
    })
    .to_string()
    .into_bytes()
}

fn feed(app: &tauri::App<MockRuntime>, server: &Server, path: &str) -> TauriFeed<MockRuntime> {
    TauriFeed::with_endpoint(
        app.handle().clone(),
        server.url("/latest.json"),
        DownloadPolicy::local_for_tests(server.port),
    )
    .with_agent_endpoint(server.url(path))
}

/// Avec la règle de production sur les redirections : HTTPS à chaque saut.
fn strict_feed(
    app: &tauri::App<MockRuntime>,
    server: &Server,
    path: &str,
) -> TauriFeed<MockRuntime> {
    TauriFeed::with_endpoint(
        app.handle().clone(),
        server.url("/latest.json"),
        DownloadPolicy::local_strict_redirects_for_tests(server.port),
    )
    .with_agent_endpoint(server.url(path))
}

#[tokio::test]
async fn the_real_adapter_reads_the_agent_file_of_the_release() {
    let server = Server::start().await;
    server.serve("/agent.json", Reply::Ok(manifest()));
    let app = app();
    let found = feed(&app, &server, "/agent.json")
        .check_agent()
        .await
        .unwrap();
    assert_eq!(found, Some(candidate()));
    assert_eq!(server.hits(), 1, "une seule requête");
}

#[tokio::test]
async fn a_release_without_the_agent_file_is_not_a_failure_and_other_statuses_are() {
    let server = Server::start().await;
    let app = app();
    // 404 : rien à proposer.
    assert_eq!(
        feed(&app, &server, "/absent.json")
            .check_agent()
            .await
            .unwrap(),
        None
    );
    // 500 : une erreur (la cible précédente est gardée par le service).
    server.serve("/broken.json", Reply::Status(500));
    assert!(
        feed(&app, &server, "/broken.json")
            .check_agent()
            .await
            .is_err()
    );
    // Illisible, ou trop gros : une erreur, jamais une cible.
    server.serve("/junk.json", Reply::Ok(b"pas du json".to_vec()));
    assert!(
        feed(&app, &server, "/junk.json")
            .check_agent()
            .await
            .is_err()
    );
    server.serve("/huge.json", Reply::Ok(vec![b' '; MANIFEST_MAX_BYTES + 10]));
    assert!(
        feed(&app, &server, "/huge.json")
            .check_agent()
            .await
            .is_err()
    );
    // Le manifeste du client seul (sans entrée de l'agent) : rien à proposer.
    server.serve(
        "/client-only.json",
        Reply::Ok(
            json!({ "version": "0.2.0", "platforms": { "windows-x86_64": { "url": "https://x", "signature": "s" } } })
                .to_string()
                .into_bytes(),
        ),
    );
    assert_eq!(
        feed(&app, &server, "/client-only.json")
            .check_agent()
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn without_a_configured_address_no_request_is_ever_made() {
    let server = Server::start().await;
    let app = app();
    let feed = TauriFeed::with_endpoint(
        app.handle().clone(),
        server.url("/latest.json"),
        DownloadPolicy::local_strict_redirects_for_tests(server.port),
    );
    assert_eq!(feed.check_agent().await.unwrap(), None);
    assert_eq!(server.hits(), 0);
}

#[tokio::test]
async fn redirections_are_bounded_and_must_stay_https_like_the_client_feed() {
    let server = Server::start().await;
    let app = app();
    // MAX_REDIRECTS sauts : suivis jusqu'au fichier ; un de plus : refusé.
    let chain = |start: &str, n: usize| {
        for step in 0..n {
            let from = if step == 0 {
                start.to_owned()
            } else {
                format!("{start}~{step}")
            };
            let to = if step + 1 == n {
                "/agent.json".to_owned()
            } else {
                format!("{start}~{}", step + 1)
            };
            server.serve(&from, Reply::Redirect(server.url(&to).to_string()));
        }
    };
    server.serve("/agent.json", Reply::Ok(manifest()));
    chain("/ok", MAX_REDIRECTS);
    assert_eq!(
        feed(&app, &server, "/ok").check_agent().await.unwrap(),
        Some(candidate())
    );
    chain("/too-many", MAX_REDIRECTS + 1);
    assert!(
        feed(&app, &server, "/too-many")
            .check_agent()
            .await
            .is_err()
    );
    // Un saut vers `http://` est refusé par la politique de production sur les redirections.
    server.serve(
        "/to-http",
        Reply::Redirect(server.url("/agent.json").to_string()),
    );
    assert!(
        strict_feed(&app, &server, "/to-http")
            .check_agent()
            .await
            .is_err(),
        "redirection en clair refusée"
    );
}

// ---- le VRAI code de publication (`cargo xtask agent-manifest`) lu par le client -------------------

#[allow(dead_code)]
#[path = "../../../../xtask/src/release_core.rs"]
mod release_core;

#[test]
fn the_manifest_made_by_the_publication_code_is_read_and_retained_by_the_client() {
    use hearth_desktop_lib::agent_update::domain::{parse_manifest, validate_target};
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
    let text = release_core::agent_manifest(
        "https://github.com/Voikyrioh/hearth/releases/download/",
        "0.2.0",
        &signature,
        &sha,
        URL,
        "2026-10-06T10:00:00Z",
    )
    .unwrap();
    // Le client lit ce même fichier, et retient la cible avec la politique de production.
    let candidate = parse_manifest(text.as_bytes())
        .unwrap()
        .expect("entrée de l'agent");
    let target = validate_target(&candidate, &DownloadPolicy::github_releases()).unwrap();
    assert_eq!(target.version().to_string(), "0.2.0");
    assert_eq!(target.url().as_str(), URL);
    assert_eq!(target.sha256(), sha);
    assert!(target.signature().starts_with("untrusted comment:"));
    // L'entrée du manifeste du client n'est pas celle de l'agent : les deux fichiers ne se mélangent pas.
    assert_ne!(release_core::AGENT_TARGET, release_core::TARGET);
}
