//! Adaptateur du greffon de mise à jour contre un serveur de versions LOCAL de test : lecture du
//! manifeste `latest.json`, signature minisign vérifiée contre la clé publique donnée au greffon,
//! signature d'une autre clé ou fichier altéré refusés, téléchargement coupé (interrompu), serveur
//! muet. Les paires de clés sont jetées à chaque test : aucune clé réelle, aucun secret.
//!
//! Le service complet est aussi essayé de bout en bout sur ce serveur (sans installer : l'installateur
//! n'est lancé que sous Windows, par `Update::install`, jamais ici).
#![allow(clippy::unwrap_used, clippy::expect_used)] // tests : les helpers peuvent paniquer

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use hearth_desktop_lib::update::domain::{Candidate, DownloadPolicy, MAX_REDIRECTS, UpdateRecord};
use hearth_desktop_lib::update::dto::{UpdateFailure, UpdatePhase, UpdateStateDto};
use hearth_desktop_lib::update::feed::{TARGET, TauriFeed, plugin_with_key};
use hearth_desktop_lib::update::ports::{
    Clock, DownloadError, Feed, FeedError, StateSink, UpdateStore, VerifiedInstaller,
};
use hearth_desktop_lib::update::service::UpdateService;
use serde_json::json;
use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;
use url::Url;

// ---- serveur de versions local -------------------------------------------------------------------

#[derive(Clone)]
enum Reply {
    Ok(Vec<u8>),
    /// Annonce `declared` octets, en envoie `sent`, puis coupe la connexion.
    Cut {
        declared: usize,
        sent: Vec<u8>,
    },
    Status(u16),
    /// Redirige (302) vers cette adresse.
    Redirect(String),
    /// Accepte la connexion et ne répond jamais.
    Hang,
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
                    let path = head
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or("/")
                        .split('?')
                        .next()
                        .unwrap_or("/")
                        .to_owned();
                    hits.lock().unwrap().push(path.clone());
                    let reply = routes.lock().unwrap().get(&path).cloned();
                    if matches!(reply, Some(Reply::Hang)) {
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        return;
                    }
                    let response: Vec<u8> = match reply {
                        None | Some(Reply::Status(404)) => {
                            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                                .to_vec()
                        }
                        Some(Reply::Hang) => unreachable!(),
                        Some(Reply::Redirect(to)) => format!(
                            "HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        )
                        .into_bytes(),
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
                        Some(Reply::Cut { declared, sent }) => {
                            let mut out = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {declared}\r\nConnection: close\r\n\r\n"
                            )
                            .into_bytes();
                            out.extend(sent);
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

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    fn serve(&self, path: &str, reply: Reply) {
        self.routes.lock().unwrap().insert(path.to_owned(), reply);
    }

    /// `n` redirections de `start` à `end` : start, start~1, …, start~(n-1), end.
    fn chain(&self, start: &str, n: usize, end: &str) {
        for step in 0..n {
            let from = if step == 0 {
                start.to_owned()
            } else {
                format!("{start}~{step}")
            };
            let to = if step + 1 == n {
                end.to_owned()
            } else {
                format!("{start}~{}", step + 1)
            };
            self.serve(&from, Reply::Redirect(self.url(&to)));
        }
    }

    fn hits(&self, path: &str) -> usize {
        self.hits
            .lock()
            .unwrap()
            .iter()
            .filter(|p| *p == path)
            .count()
    }

    fn total_hits(&self) -> usize {
        self.hits.lock().unwrap().len()
    }
}

// ---- clés et signatures de test (jetées, jamais écrites) -----------------------------------------

struct TestKey {
    pair: minisign::KeyPair,
}

impl TestKey {
    fn new() -> Self {
        Self {
            pair: minisign::KeyPair::generate_unencrypted_keypair().unwrap(),
        }
    }

    /// Le contenu d'un fichier `.pub`.
    fn public_file(&self) -> String {
        self.pair.pk.to_box().unwrap().to_string()
    }

    /// La signature comme la publie Tauri : le contenu du `.sig` en base64, commentaire de
    /// confiance `timestamp:…\tfile:…\tversion:…`.
    fn sign(&self, data: &[u8], version: &str) -> String {
        self.sign_with_comment(
            data,
            &format!("timestamp:1\tfile:Hearth_{version}_x64-setup.exe\tversion:{version}"),
        )
    }

    /// Une signature sans version (`tauri signer sign` sans `--app-version`).
    fn sign_without_version(&self, data: &[u8]) -> String {
        self.sign_with_comment(data, "timestamp:1\tfile:Hearth_x64-setup.exe")
    }

    fn sign_with_comment(&self, data: &[u8], comment: &str) -> String {
        let signature = minisign::sign(
            Some(&self.pair.pk),
            &self.pair.sk,
            Cursor::new(data),
            Some(comment),
            Some("signature de test"),
        )
        .unwrap();
        STANDARD.encode(signature.to_string())
    }
}

fn installer(size: usize) -> Vec<u8> {
    // Un « exécutable » de test : jamais lancé.
    let mut bytes = b"MZ".to_vec();
    bytes.extend((0..size).map(|n| (n % 251) as u8));
    bytes
}

fn manifest(version: &str, url: &str, signature: &str) -> Vec<u8> {
    json!({
        "version": version,
        "notes": "Corrections et nouveautés.",
        "pub_date": "2026-10-05T12:00:00Z",
        "platforms": { TARGET: { "signature": signature, "url": url } }
    })
    .to_string()
    .into_bytes()
}

/// Application simulée : Tauri y fixe la version du client à 0.1.0 (`mock_context`), c'est donc
/// la « version en cours » du greffon dans ces tests.
fn app(key: &TestKey) -> tauri::App<MockRuntime> {
    app_for_public_file(&key.public_file())
}

fn app_for_public_file(public_file: &str) -> tauri::App<MockRuntime> {
    let mut context = mock_context(noop_assets());
    // La configuration du greffon est celle de `tauri.conf.json` (dont `requireSignedVersion`) et
    // non une copie : ce que les tests éprouvent est ce qui est livré.
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
        .plugin(plugin_with_key::<MockRuntime>(public_file))
        .build(context)
        .unwrap()
}

fn feed_for(app: &tauri::App<MockRuntime>, server: &Server) -> TauriFeed<MockRuntime> {
    TauriFeed::with_endpoint(
        app.handle().clone(),
        Url::parse(&server.url("/latest.json")).unwrap(),
        DownloadPolicy::local_for_tests(server.port),
    )
}

async fn no_progress(
    feed: &TauriFeed<MockRuntime>,
    version: &str,
) -> Result<Vec<u8>, DownloadError> {
    feed.download(version, &mut |_, _| {})
        .await
        .map(VerifiedInstaller::into_bytes_for_tests)
}

// ---- manifeste ------------------------------------------------------------------------------------

#[tokio::test]
async fn the_manifest_is_read_and_announces_the_newer_version() {
    let key = TestKey::new();
    let server = Server::start().await;
    let file = installer(2_000);
    server.serve("/setup.exe", Reply::Ok(file.clone()));
    server.serve(
        "/latest.json",
        Reply::Ok(manifest(
            "1.1.0",
            &server.url("/setup.exe"),
            &key.sign(&file, "1.1.0"),
        )),
    );
    let app = app(&key);
    let feed = feed_for(&app, &server);

    let candidate = feed.check().await.unwrap().unwrap();

    assert_eq!(candidate.version, "1.1.0");
    assert_eq!(
        candidate.notes.as_deref(),
        Some("Corrections et nouveautés.")
    );
    assert_eq!(candidate.download_url, server.url("/setup.exe"));
    assert!(!candidate.signature.is_empty());
    // La vérification ne télécharge pas l'installateur.
    assert_eq!(server.hits("/setup.exe"), 0);
    assert_eq!(server.hits("/latest.json"), 1);
}

#[tokio::test]
async fn the_same_or_an_older_manifest_version_offers_nothing() {
    let key = TestKey::new();
    let server = Server::start().await;
    let file = installer(100);
    for version in ["0.1.0", "0.0.9"] {
        server.serve(
            "/latest.json",
            Reply::Ok(manifest(
                version,
                &server.url("/setup.exe"),
                &key.sign(&file, version),
            )),
        );
        let app = app(&key);
        assert!(
            feed_for(&app, &server).check().await.unwrap().is_none(),
            "{version}"
        );
    }
}

#[tokio::test]
async fn a_mute_unreachable_or_broken_feed_is_an_error_the_service_swallows() {
    let key = TestKey::new();
    let server = Server::start().await;
    let app = app(&key);
    let feed = feed_for(&app, &server);

    // 404 (aucune release publiée), 503, JSON cassé, plateforme absente.
    assert!(feed.check().await.is_err());
    server.serve("/latest.json", Reply::Status(503));
    let answered = feed.check().await.unwrap_err();
    assert!(!answered.no_request_sent, "{answered:?}"); // la requête est partie : quota consommé
    server.serve("/latest.json", Reply::Ok(b"pas du json".to_vec()));
    assert!(feed.check().await.is_err());
    server.serve(
        "/latest.json",
        Reply::Ok(
            json!({"version": "1.1.0", "platforms": {"linux-x86_64": {"signature": "s", "url": "http://x/y"}}})
                .to_string()
                .into_bytes(),
        ),
    );
    assert!(feed.check().await.is_err());

    // Rien n'écoute : refus de connexion, donc aucune requête partie (pas de réseau).
    let closed = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let dead_feed = TauriFeed::with_endpoint(
        app.handle().clone(),
        Url::parse(&format!("http://127.0.0.1:{closed}/latest.json")).unwrap(),
        DownloadPolicy::local_for_tests(closed),
    );
    let offline: FeedError = dead_feed.check().await.unwrap_err();
    assert!(offline.no_request_sent, "{offline:?}");
}

// ---- téléchargement et signature ------------------------------------------------------------------

async fn staged(
    key: &TestKey,
    server: &Server,
    app: &tauri::App<MockRuntime>,
    signature: &str,
) -> (TauriFeed<MockRuntime>, Candidate) {
    server.serve(
        "/latest.json",
        Reply::Ok(manifest("1.1.0", &server.url("/setup.exe"), signature)),
    );
    let _ = key;
    let feed = feed_for(app, server);
    let candidate = feed.check().await.unwrap().unwrap();
    (feed, candidate)
}

#[tokio::test]
async fn a_download_with_a_valid_signature_is_returned_with_its_progress() {
    let key = TestKey::new();
    let server = Server::start().await;
    let file = installer(300_000);
    server.serve("/setup.exe", Reply::Ok(file.clone()));
    let app = app(&key);
    let (feed, _) = staged(&key, &server, &app, &key.sign(&file, "1.1.0")).await;

    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    let bytes = feed
        .download("1.1.0", &mut |received, total| {
            sink.lock().unwrap().push((received, total));
        })
        .await
        .unwrap();

    assert_eq!(bytes.into_bytes_for_tests(), file);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.last().unwrap().0, file.len() as u64, "reçus cumulés");
    assert_eq!(seen.last().unwrap().1, Some(file.len() as u64));
    assert!(seen.windows(2).all(|pair| pair[0].0 <= pair[1].0));
}

#[tokio::test]
async fn a_signature_from_another_key_is_refused_as_corrupted() {
    let key = TestKey::new();
    let attacker = TestKey::new();
    let server = Server::start().await;
    let file = installer(5_000);
    server.serve("/setup.exe", Reply::Ok(file.clone()));
    let app = app(&key);
    let (feed, _) = staged(&key, &server, &app, &attacker.sign(&file, "1.1.0")).await;

    let error = no_progress(&feed, "1.1.0").await.unwrap_err();

    assert!(matches!(error, DownloadError::Corrupted(_)), "{error:?}");
}

#[tokio::test]
async fn a_file_altered_after_signing_is_refused_as_corrupted() {
    let key = TestKey::new();
    let server = Server::start().await;
    let file = installer(5_000);
    let signature = key.sign(&file, "1.1.0");
    let mut altered = file.clone();
    altered[100] ^= 0xff;
    server.serve("/setup.exe", Reply::Ok(altered));
    let app = app(&key);
    let (feed, _) = staged(&key, &server, &app, &signature).await;

    let error = no_progress(&feed, "1.1.0").await.unwrap_err();

    assert!(matches!(error, DownloadError::Corrupted(_)), "{error:?}");
}

#[tokio::test]
async fn a_complete_but_shorter_file_is_refused_as_corrupted() {
    let key = TestKey::new();
    let server = Server::start().await;
    let file = installer(5_000);
    let signature = key.sign(&file, "1.1.0");
    server.serve("/setup.exe", Reply::Ok(file[..2_000].to_vec()));
    let app = app(&key);
    let (feed, _) = staged(&key, &server, &app, &signature).await;

    let error = no_progress(&feed, "1.1.0").await.unwrap_err();

    assert!(matches!(error, DownloadError::Corrupted(_)), "{error:?}");
}

#[tokio::test]
async fn a_garbage_signature_is_refused_as_corrupted() {
    let key = TestKey::new();
    let server = Server::start().await;
    let file = installer(500);
    server.serve("/setup.exe", Reply::Ok(file));
    let app = app(&key);
    let (feed, _) = staged(&key, &server, &app, "pas-une-signature").await;

    let error = no_progress(&feed, "1.1.0").await.unwrap_err();

    assert!(matches!(error, DownloadError::Corrupted(_)), "{error:?}");
}

#[tokio::test]
async fn a_download_cut_halfway_is_interrupted_and_can_be_run_again() {
    let key = TestKey::new();
    let server = Server::start().await;
    let file = installer(100_000);
    let signature = key.sign(&file, "1.1.0");
    server.serve(
        "/setup.exe",
        Reply::Cut {
            declared: file.len(),
            sent: file[..30_000].to_vec(),
        },
    );
    let app = app(&key);
    let (feed, _) = staged(&key, &server, &app, &signature).await;

    let error = no_progress(&feed, "1.1.0").await.unwrap_err();
    assert!(matches!(error, DownloadError::Interrupted(_)), "{error:?}");

    // La connexion revient : la même annonce se télécharge en entier.
    server.serve("/setup.exe", Reply::Ok(file.clone()));
    assert_eq!(no_progress(&feed, "1.1.0").await.unwrap(), file);
    assert_eq!(server.hits("/setup.exe"), 2);
}

#[tokio::test]
async fn a_failing_download_server_is_interrupted_not_corrupted() {
    let key = TestKey::new();
    let server = Server::start().await;
    let file = installer(500);
    let signature = key.sign(&file, "1.1.0");
    server.serve("/setup.exe", Reply::Status(500));
    let app = app(&key);
    let (feed, _) = staged(&key, &server, &app, &signature).await;

    let error = no_progress(&feed, "1.1.0").await.unwrap_err();

    assert!(matches!(error, DownloadError::Interrupted(_)), "{error:?}");
}

#[tokio::test]
async fn nothing_is_downloaded_for_a_version_that_was_not_announced() {
    let key = TestKey::new();
    let server = Server::start().await;
    let file = installer(500);
    server.serve("/setup.exe", Reply::Ok(file.clone()));
    let app = app(&key);
    let (feed, _) = staged(&key, &server, &app, &key.sign(&file, "1.1.0")).await;

    assert_eq!(
        no_progress(&feed, "9.9.9").await.unwrap_err(),
        DownloadError::NotStaged
    );
    assert_eq!(server.hits("/setup.exe"), 0);
    // Sans annonce du tout (rien de plus récent) : rien à télécharger non plus.
    let fresh = feed_for(&app, &server);
    assert_eq!(
        no_progress(&fresh, "1.1.0").await.unwrap_err(),
        DownloadError::NotStaged
    );
}

#[tokio::test]
async fn an_announcement_from_a_source_the_policy_refuses_is_never_staged() {
    let key = TestKey::new();
    let server = Server::start().await;
    let file = installer(500);
    server.serve("/setup.exe", Reply::Ok(file.clone()));
    server.serve(
        "/latest.json",
        Reply::Ok(manifest(
            "1.1.0",
            &server.url("/setup.exe"),
            &key.sign(&file, "1.1.0"),
        )),
    );
    let app = app(&key);
    // Le port du serveur n'est pas celui que la politique permet.
    let feed = TauriFeed::with_endpoint(
        app.handle().clone(),
        Url::parse(&server.url("/latest.json")).unwrap(),
        DownloadPolicy::local_for_tests(server.port.wrapping_add(1)),
    );
    assert!(feed.check().await.unwrap().is_some());

    assert_eq!(
        no_progress(&feed, "1.1.0").await.unwrap_err(),
        DownloadError::NotStaged
    );
    assert_eq!(server.hits("/setup.exe"), 0);
}

// ---- le service complet sur ce serveur ------------------------------------------------------------

struct Fixed(i64);

impl Clock for Fixed {
    fn now_ms(&self) -> i64 {
        self.0
    }
}

#[derive(Default)]
struct Memory(Mutex<UpdateRecord>);

impl UpdateStore for Memory {
    fn load(&self) -> UpdateRecord {
        self.0.lock().unwrap().clone()
    }
    fn save(&self, record: &UpdateRecord) -> Result<(), String> {
        *self.0.lock().unwrap() = record.clone();
        Ok(())
    }
}

#[derive(Default)]
struct Collect(Mutex<Vec<UpdateStateDto>>);

impl StateSink for Collect {
    fn publish(&self, state: &UpdateStateDto) {
        self.0.lock().unwrap().push(state.clone());
    }
}

/// Un faux installateur : la même coquille que le vrai, mais qui ne lance rien.
struct RecordingInstaller<F: Feed> {
    inner: F,
    installed: AtomicUsize,
}

#[async_trait::async_trait]
impl<F: Feed> Feed for RecordingInstaller<F> {
    async fn check(
        &self,
    ) -> Result<Option<Candidate>, hearth_desktop_lib::update::ports::FeedError> {
        self.inner.check().await
    }
    async fn download(
        &self,
        version: &str,
        progress: &mut (dyn FnMut(u64, Option<u64>) + Send),
    ) -> Result<VerifiedInstaller, DownloadError> {
        self.inner.download(version, progress).await
    }
    fn install(&self, _version: &str, _installer: VerifiedInstaller) -> Result<(), DownloadError> {
        self.installed.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// Le clic.
async fn install(
    service: &UpdateService,
) -> Result<(), hearth_desktop_lib::update::service::InstallRefusal> {
    service.begin_install()?;
    service.run_install().await;
    Ok(())
}

fn service_on(
    app: &tauri::App<MockRuntime>,
    server: &Server,
) -> (
    Arc<RecordingInstaller<TauriFeed<MockRuntime>>>,
    UpdateService,
    Arc<Collect>,
) {
    let feed = Arc::new(RecordingInstaller {
        inner: feed_for(app, server),
        installed: AtomicUsize::new(0),
    });
    let sink = Arc::new(Collect::default());
    let service = UpdateService::new(
        Arc::new(Fixed(1_800_000_000_000)),
        Arc::new(Memory::default()),
        feed.clone(),
        sink.clone(),
        DownloadPolicy::local_for_tests(server.port),
        "1.0.0",
    );
    (feed, service, sink)
}

#[tokio::test]
async fn end_to_end_check_then_click_installs_only_a_verified_file() {
    let key = TestKey::new();
    let server = Server::start().await;
    let file = installer(80_000);
    server.serve("/setup.exe", Reply::Ok(file.clone()));
    server.serve(
        "/latest.json",
        Reply::Ok(manifest(
            "1.1.0",
            &server.url("/setup.exe"),
            &key.sign(&file, "1.1.0"),
        )),
    );
    let app = app(&key);
    let (feed, service, _) = service_on(&app, &server);

    let state = service.check_if_due().await;
    assert!(state.banner_visible);
    assert_eq!(
        server.total_hits(),
        1,
        "une vérification = une requête, rien d'autre"
    );
    assert_eq!(feed.installed.load(Ordering::SeqCst), 0);

    install(&service).await.unwrap();

    assert_eq!(feed.installed.load(Ordering::SeqCst), 1);
    assert_eq!(server.hits("/setup.exe"), 1);
    assert_eq!(service.state().phase, UpdatePhase::Installing);
}

#[tokio::test]
async fn end_to_end_a_tampered_installer_is_refused_with_the_corrupted_failure() {
    let key = TestKey::new();
    let server = Server::start().await;
    let file = installer(80_000);
    let signature = key.sign(&file, "1.1.0");
    let mut altered = file.clone();
    altered[10] ^= 1;
    server.serve("/setup.exe", Reply::Ok(altered));
    server.serve(
        "/latest.json",
        Reply::Ok(manifest("1.1.0", &server.url("/setup.exe"), &signature)),
    );
    let app = app(&key);
    let (feed, service, _) = service_on(&app, &server);
    service.check_if_due().await;

    install(&service).await.unwrap();

    let state = service.state();
    assert_eq!(state.failure, Some(UpdateFailure::Corrupted));
    assert_eq!(state.phase, UpdatePhase::Idle);
    assert_eq!(
        feed.installed.load(Ordering::SeqCst),
        0,
        "rien n'est installé"
    );
    assert_eq!(state.current_version, "1.0.0");
}

#[tokio::test]
async fn end_to_end_a_cut_download_then_a_second_try_succeeds() {
    let key = TestKey::new();
    let server = Server::start().await;
    let file = installer(120_000);
    server.serve(
        "/setup.exe",
        Reply::Cut {
            declared: file.len(),
            sent: file[..40_000].to_vec(),
        },
    );
    server.serve(
        "/latest.json",
        Reply::Ok(manifest(
            "1.1.0",
            &server.url("/setup.exe"),
            &key.sign(&file, "1.1.0"),
        )),
    );
    let app = app(&key);
    let (feed, service, _) = service_on(&app, &server);
    service.check_if_due().await;

    install(&service).await.unwrap();
    assert_eq!(service.state().failure, Some(UpdateFailure::Interrupted));
    assert_eq!(feed.installed.load(Ordering::SeqCst), 0);

    server.serve("/setup.exe", Reply::Ok(file));
    install(&service).await.unwrap();
    assert_eq!(feed.installed.load(Ordering::SeqCst), 1);
    assert_eq!(service.state().failure, None);
}

#[tokio::test]
async fn end_to_end_with_a_different_key_embedded_nothing_ever_verifies() {
    // Le cas du dépôt tant que Voiky n'a pas mis sa clé : la clé de développement n'a pas de secret.
    let signing = TestKey::new();
    let embedded = TestKey::new();
    let server = Server::start().await;
    let file = installer(2_000);
    server.serve("/setup.exe", Reply::Ok(file.clone()));
    server.serve(
        "/latest.json",
        Reply::Ok(manifest(
            "1.1.0",
            &server.url("/setup.exe"),
            &signing.sign(&file, "1.1.0"),
        )),
    );
    let app = app(&embedded);
    let (feed, service, _) = service_on(&app, &server);
    service.check_if_due().await;

    install(&service).await.unwrap();

    assert_eq!(service.state().failure, Some(UpdateFailure::Corrupted));
    assert_eq!(feed.installed.load(Ordering::SeqCst), 0);
}

// ---- version signée : rejeu d'un ancien installateur, signature sans version ---------------------

#[tokio::test]
async fn an_old_installer_validly_signed_for_another_version_is_refused_when_announced_newer() {
    // Rejeu : l'installateur de la 1.0.5, bel et bien signé par la clé, est servi par un manifeste
    // qui annonce la 1.1.0. La signature est bonne, mais elle porte `version:1.0.5` : refusé AVANT
    // toute écriture exécutable (`requireSignedVersion` de tauri.conf.json).
    let key = TestKey::new();
    let server = Server::start().await;
    let old = installer(4_000);
    server.serve("/setup.exe", Reply::Ok(old.clone()));
    let app = app(&key);
    let (feed, _) = staged(&key, &server, &app, &key.sign(&old, "1.0.5")).await;

    let error = no_progress(&feed, "1.1.0").await.unwrap_err();

    assert!(matches!(error, DownloadError::Corrupted(_)), "{error:?}");
    assert!(error.to_string().contains("1.0.5"), "{error}");
    // Rien n'a été installé : le flux de bout en bout aboutit au même refus.
    let (installing, service, _) = service_on(&app, &server);
    service.check_if_due().await;
    install(&service).await.unwrap();
    assert_eq!(service.state().failure, Some(UpdateFailure::Corrupted));
    assert_eq!(installing.installed.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_signature_without_a_version_is_refused() {
    // Une signature produite sans `--app-version` resterait rejouable à vie.
    let key = TestKey::new();
    let server = Server::start().await;
    let file = installer(4_000);
    server.serve("/setup.exe", Reply::Ok(file.clone()));
    let app = app(&key);
    let (feed, _) = staged(&key, &server, &app, &key.sign_without_version(&file)).await;

    let error = no_progress(&feed, "1.1.0").await.unwrap_err();

    assert!(matches!(error, DownloadError::Corrupted(_)), "{error:?}");
}

#[tokio::test]
async fn the_signed_version_may_be_spelled_with_a_leading_v() {
    let key = TestKey::new();
    let server = Server::start().await;
    let file = installer(4_000);
    server.serve("/setup.exe", Reply::Ok(file.clone()));
    let app = app(&key);
    let (feed, _) = staged(&key, &server, &app, &key.sign(&file, "v1.1.0")).await;
    assert_eq!(no_progress(&feed, "1.1.0").await.unwrap(), file);
}

// ---- redirections ---------------------------------------------------------------------------------

#[tokio::test]
async fn a_redirect_of_the_installer_to_plain_http_is_not_followed() {
    let key = TestKey::new();
    let server = Server::start().await;
    let file = installer(2_000);
    let signature = key.sign(&file, "1.1.0");
    server.serve("/real.exe", Reply::Ok(file.clone()));
    server.serve("/setup.exe", Reply::Redirect(server.url("/real.exe")));
    server.serve(
        "/latest.json",
        Reply::Ok(manifest("1.1.0", &server.url("/setup.exe"), &signature)),
    );
    let app = app(&key);
    // Règle de production sur les sauts : HTTPS à chaque saut ; ici le saut est en http://.
    let feed = TauriFeed::with_endpoint(
        app.handle().clone(),
        Url::parse(&server.url("/latest.json")).unwrap(),
        DownloadPolicy::local_strict_redirects_for_tests(server.port),
    );
    feed.check().await.unwrap().unwrap();

    let error = no_progress(&feed, "1.1.0").await.unwrap_err();

    assert!(matches!(error, DownloadError::Interrupted(_)), "{error:?}");
    assert_eq!(
        server.hits("/real.exe"),
        0,
        "la redirection en clair n'a pas été suivie"
    );
}

#[tokio::test]
async fn a_redirect_is_followed_when_the_policy_allows_the_hop() {
    let key = TestKey::new();
    let server = Server::start().await;
    let file = installer(2_000);
    server.serve("/real.exe", Reply::Ok(file.clone()));
    server.serve("/setup.exe", Reply::Redirect(server.url("/real.exe")));
    let app = app(&key);
    let (feed, _) = staged(&key, &server, &app, &key.sign(&file, "1.1.0")).await;
    assert_eq!(no_progress(&feed, "1.1.0").await.unwrap(), file);
    assert_eq!(server.hits("/real.exe"), 1);
}

#[tokio::test]
async fn the_installer_follows_exactly_the_maximum_number_of_redirects() {
    let key = TestKey::new();
    let file = installer(2_000);
    for (redirects, accepted) in [(MAX_REDIRECTS, true), (MAX_REDIRECTS + 1, false), (2, true)] {
        let server = Server::start().await;
        server.serve("/real.exe", Reply::Ok(file.clone()));
        server.chain("/setup.exe", redirects, "/real.exe");
        let app = app(&key);
        let (feed, _) = staged(&key, &server, &app, &key.sign(&file, "1.1.0")).await;
        let outcome = no_progress(&feed, "1.1.0").await;
        assert_eq!(
            outcome.is_ok(),
            accepted,
            "{redirects} redirections : {outcome:?}"
        );
        assert_eq!(server.hits("/real.exe") == 1, accepted, "{redirects}");
    }
}

#[tokio::test]
async fn the_manifest_follows_the_same_bound_and_works_behind_githubs_two_redirects() {
    let key = TestKey::new();
    let file = installer(500);
    let signature = key.sign(&file, "1.1.0");
    for (redirects, accepted) in [(2, true), (MAX_REDIRECTS, true), (MAX_REDIRECTS + 1, false)] {
        let server = Server::start().await;
        server.serve(
            "/manifest.json",
            Reply::Ok(manifest("1.1.0", &server.url("/setup.exe"), &signature)),
        );
        server.chain("/latest.json", redirects, "/manifest.json");
        let app = app(&key);
        let outcome = feed_for(&app, &server).check().await;
        assert_eq!(
            outcome.is_ok(),
            accepted,
            "{redirects} redirections : {:?}",
            outcome.map(|c| c.map(|c| c.version))
        );
    }
}

#[tokio::test]
async fn a_redirect_to_another_host_over_the_allowed_scheme_is_followed() {
    // `localhost` n'est pas `127.0.0.1` : un autre hôte, suivi (aucune liste d'hôtes).
    let key = TestKey::new();
    let server = Server::start().await;
    let file = installer(1_000);
    server.serve("/real.exe", Reply::Ok(file.clone()));
    server.serve(
        "/setup.exe",
        Reply::Redirect(format!("http://localhost:{}/real.exe", server.port)),
    );
    let app = app(&key);
    let (feed, _) = staged(&key, &server, &app, &key.sign(&file, "1.1.0")).await;
    assert_eq!(no_progress(&feed, "1.1.0").await.unwrap(), file);
}

#[tokio::test]
async fn a_plain_http_redirect_is_refused_for_the_manifest_too() {
    let key = TestKey::new();
    let server = Server::start().await;
    server.serve("/real.json", Reply::Ok(b"{}".to_vec()));
    server.serve("/latest.json", Reply::Redirect(server.url("/real.json")));
    let app = app(&key);
    let feed = TauriFeed::with_endpoint(
        app.handle().clone(),
        Url::parse(&server.url("/latest.json")).unwrap(),
        DownloadPolicy::local_strict_redirects_for_tests(server.port),
    );
    assert!(feed.check().await.is_err());
    assert_eq!(server.hits("/real.json"), 0);
}

// ---- classement « aucune requête partie » : strictement le premier hôte, TCP ou nom -----------------

#[tokio::test]
async fn only_a_failed_name_or_connection_to_the_first_host_gives_the_quota_back() {
    let key = TestKey::new();
    let app = app(&key);
    let server = Server::start().await;
    let closed = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };

    // Premier hôte injoignable : hors ligne (quota rendu).
    let feed = TauriFeed::with_endpoint(
        app.handle().clone(),
        Url::parse(&format!("http://127.0.0.1:{closed}/latest.json")).unwrap(),
        DownloadPolicy::local_for_tests(closed),
    );
    assert!(feed.check().await.unwrap_err().no_request_sent);

    // Le premier hôte répond, c'est le SECOND saut qui ne se connecte pas : quota consommé.
    server.serve(
        "/latest.json",
        Reply::Redirect(format!("http://127.0.0.1:{closed}/x")),
    );
    let error = feed_for(&app, &server).check().await.unwrap_err();
    assert!(!error.no_request_sent, "{error:?}");

    // Échec TLS (poignée de main contre un serveur qui parle en clair) : quota consommé.
    let tls = TauriFeed::with_endpoint(
        app.handle().clone(),
        Url::parse(&format!("https://127.0.0.1:{}/latest.json", server.port)).unwrap(),
        DownloadPolicy::local_for_tests(server.port),
    );
    let error = tls.check().await.unwrap_err();
    assert!(!error.no_request_sent, "{error:?}");

    // Délai dépassé après connexion : quota consommé.
    server.serve("/hang.json", Reply::Hang);
    let slow = TauriFeed::with_endpoint(
        app.handle().clone(),
        Url::parse(&server.url("/hang.json")).unwrap(),
        DownloadPolicy::local_for_tests(server.port),
    )
    .with_check_timeout(std::time::Duration::from_millis(300));
    let error = slow.check().await.unwrap_err();
    assert!(!error.no_request_sent, "{error:?}");

    // Réponse invalide : quota consommé.
    server.serve("/bad.json", Reply::Ok(b"pas du json".to_vec()));
    let bad = TauriFeed::with_endpoint(
        app.handle().clone(),
        Url::parse(&server.url("/bad.json")).unwrap(),
        DownloadPolicy::local_for_tests(server.port),
    );
    assert!(!bad.check().await.unwrap_err().no_request_sent);
}

// ---- le VRAI code de publication (`cargo xtask client-sign` / `client-manifest`) ------------------------

#[allow(dead_code)]
#[path = "../../../../xtask/src/release_core.rs"]
mod release_core;

/// Fabrique une release comme le flux : clé chiffrée (comme celle de `tauri signer generate`),
/// signature par `sign_installer`, manifeste par `manifest_for` (la même fonction que
/// `client-manifest`, avec la racine d'adresses locale).
fn publish_like_the_workflow(
    server: &Server,
    secret_file: &str,
    signed_version: &str,
    announced_version: &str,
    file: &[u8],
) {
    let signature = release_core::sign_installer(
        secret_file,
        "mot de passe jetable",
        file,
        &format!("Hearth_{signed_version}_x64-setup.exe"),
        signed_version,
        1_800_000_000,
    )
    .unwrap();
    let prefix = server.url("/releases/download/");
    let url = server.url(&format!(
        "/releases/download/v{announced_version}/Hearth_{announced_version}_x64-setup.exe"
    ));
    let text = release_core::manifest_for(
        &prefix,
        announced_version,
        "Notes.",
        &signature,
        &url,
        "2026-10-05T20:00:00Z",
    )
    .unwrap();
    server.serve("/latest.json", Reply::Ok(text.into_bytes()));
    server.serve(
        &format!(
            "/releases/download/v{announced_version}/Hearth_{announced_version}_x64-setup.exe"
        ),
        Reply::Ok(file.to_vec()),
    );
}

#[tokio::test]
async fn a_release_made_by_the_publication_code_is_accepted_by_the_real_plugin() {
    let pair =
        minisign::KeyPair::generate_encrypted_keypair(Some("mot de passe jetable".to_owned()))
            .unwrap();
    let secret_file = pair.sk.to_box(None).unwrap().to_string();
    let public_file = pair.pk.to_box().unwrap().to_string();
    let file = installer(20_000);
    let server = Server::start().await;
    publish_like_the_workflow(&server, &secret_file, "1.1.0", "1.1.0", &file);
    let app = app_for_public_file(&public_file);
    let feed = feed_for(&app, &server);

    let candidate = feed.check().await.unwrap().unwrap();
    assert_eq!(candidate.version, "1.1.0");
    assert_eq!(no_progress(&feed, "1.1.0").await.unwrap(), file);
}

#[tokio::test]
async fn the_same_release_with_a_signed_version_that_does_not_match_is_refused() {
    let pair =
        minisign::KeyPair::generate_encrypted_keypair(Some("mot de passe jetable".to_owned()))
            .unwrap();
    let secret_file = pair.sk.to_box(None).unwrap().to_string();
    let public_file = pair.pk.to_box().unwrap().to_string();
    let file = installer(20_000);
    let server = Server::start().await;
    // Signé pour la 1.0.9, annoncé 1.1.0 par le manifeste.
    publish_like_the_workflow(&server, &secret_file, "1.0.9", "1.1.0", &file);
    let app = app_for_public_file(&public_file);
    let feed = feed_for(&app, &server);
    feed.check().await.unwrap().unwrap();

    let error = no_progress(&feed, "1.1.0").await.unwrap_err();

    assert!(matches!(error, DownloadError::Corrupted(_)), "{error:?}");
}
