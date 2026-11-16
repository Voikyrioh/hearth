//! Le protocole `hearth-illustration` (HRT-239, HRT-240, ADR-0061, BR-CLIENT-015) : la page affiche
//! `<img src="hearth-illustration://localhost/...">` (sous Windows `http://hearth-illustration.localhost/...`) et la
//! COQUILLE répond depuis son cache disque ou va chercher l'image. La page n'ouvre aucune connexion ; la CSP ne contient
//! aucun hôte externe.
//!
//! Deux formes d'adresse (`url.rs`, jugées avant toute E/S) :
//! - `/steam/{app_id}/header` : l'en-tête du magasin Steam (`steam.rs`), seulement si le réglage « Images des jeux depuis
//!   Steam » est activé (lu à CHAQUE requête : une page modifiée ne le contourne pas) ; sinon `404` sans aucune requête ;
//! - `/agent/{server_id}/{fiche_id}/{hex}` : l'illustration embarquée de l'agent, lue par la liaison (`AgentImages`),
//!   vérifiée par son empreinte SHA-256.
//!
//! Réponses : `200 image/jpeg` (`nosniff`), `400` adresse invalide (rien ne part), `404` tout échec (la page passe au visuel
//! neutre), `405` méthode autre que GET. Aucun texte d'erreur : jamais une adresse, un identifiant ni un message reçu.

pub mod cache;
pub mod steam;
pub mod url;

use std::sync::Arc;
use std::time::SystemTime;

use async_trait::async_trait;

use cache::{Cache, Cached, Meta};
use steam::{Fetched, SteamClient};
use url::Target;

/// Le nom du schéma d'URI enregistré auprès de Tauri.
pub const SCHEME: &str = "hearth-illustration";

/// Ce que le protocole répond : un code et les octets (vides sauf `200`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Reply {
    fn status(status: u16) -> Self {
        Self {
            status,
            body: Vec::new(),
        }
    }

    fn image(body: Vec<u8>) -> Self {
        Self { status: 200, body }
    }
}

/// La lecture de l'illustration embarquée d'une fiche chez l'agent d'un serveur du carnet (HRT-240). Les octets rendus
/// sont déjà bornés, typés et vérifiés contre l'empreinte par la liaison ; le protocole les revérifie avant de les garder.
#[async_trait]
pub trait AgentImages: Send + Sync {
    async fn fetch(&self, server_id: &str, fiche_id: &str, hex: &str) -> Option<Vec<u8>>;
}

/// Aucune lecture possible (tests, ou liaison absente).
pub struct NoAgentImages;

#[async_trait]
impl AgentImages for NoAgentImages {
    async fn fetch(&self, _server_id: &str, _fiche_id: &str, _hex: &str) -> Option<Vec<u8>> {
        None
    }
}

/// Le réglage « Images des jeux depuis Steam » : relu à chaque requête.
pub type EnabledFlag = Arc<dyn Fn() -> bool + Send + Sync>;

pub struct Illustrations {
    cache: Cache,
    steam: SteamClient,
    enabled: EnabledFlag,
    agent: Arc<dyn AgentImages>,
}

impl Illustrations {
    pub fn new(
        cache: Cache,
        steam: SteamClient,
        enabled: EnabledFlag,
        agent: Arc<dyn AgentImages>,
    ) -> Self {
        Self {
            cache,
            steam,
            enabled,
            agent,
        }
    }

    /// Oublie les images Steam gardées (le réglage vient d'être coupé).
    pub fn purge_steam(&self) {
        self.cache.purge_steam();
    }

    /// Répond à une requête du protocole.
    pub async fn handle(&self, method: &str, path: &str) -> Reply {
        if !method.eq_ignore_ascii_case("GET") {
            return Reply::status(405);
        }
        match url::parse(path) {
            Err(_) => Reply::status(400),
            Ok(Target::Steam { app_id }) => self.steam_image(app_id).await,
            Ok(Target::Agent {
                server_id,
                fiche_id,
                hex,
            }) => self.agent_image(&server_id, &fiche_id, &hex).await,
        }
    }

    async fn steam_image(&self, app_id: u32) -> Reply {
        // Réglage coupé : aucune requête, aucune lecture du cache (les images Steam gardées ont été oubliées).
        if !(self.enabled)() {
            return Reply::status(404);
        }
        let now = SystemTime::now();
        let cached = self.cache.read_steam(app_id);
        if let Some(Cached { bytes, meta }) = &cached
            && Cache::is_fresh(meta, now)
        {
            return Reply::image(bytes.clone());
        }
        let validator = cached.as_ref().and_then(|c| c.meta.etag.clone());
        match self.steam.fetch(app_id, validator.as_deref()).await {
            Ok(Fetched::NotModified) => match cached {
                Some(Cached { bytes, meta }) => {
                    self.cache.refresh_steam(app_id, &meta, now);
                    Reply::image(bytes)
                }
                None => Reply::status(404),
            },
            Ok(Fetched::Image {
                bytes,
                etag,
                last_modified,
            }) => {
                let fetched_s = now
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let meta = Meta {
                    etag,
                    last_modified,
                    fetched_s,
                };
                let _ = self.cache.write_steam(app_id, &bytes, &meta);
                Reply::image(bytes)
            }
            // Steam injoignable ou refus : la copie ancienne sert (hors ligne), sinon rien.
            Err(_) => cached.map_or_else(|| Reply::status(404), |c| Reply::image(c.bytes)),
        }
    }

    async fn agent_image(&self, server_id: &str, fiche_id: &str, hex: &str) -> Reply {
        if let Some(bytes) = self.cache.read_agent(hex) {
            return Reply::image(bytes);
        }
        let Some(bytes) = self.agent.fetch(server_id, fiche_id, hex).await else {
            return Reply::status(404);
        };
        // Revérification : jamais un fichier dont la forme ou l'empreinte ne correspond pas.
        if steam::check_image(&bytes).is_err() || sha256_hex(&bytes) != hex {
            return Reply::status(404);
        }
        let _ = self.cache.write_agent(hex, &bytes);
        Reply::image(bytes)
    }
}

/// L'empreinte SHA-256 en hexadécimal minuscule.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    use sha2::{Digest, Sha256};
    let mut out = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// La réponse HTTP du protocole : type fixe, `nosniff`, une journée de cache côté WebView, aucun corps hors `200`.
pub fn http_response(reply: Reply) -> tauri::http::Response<Vec<u8>> {
    let mut builder = tauri::http::Response::builder()
        .status(reply.status)
        .header("X-Content-Type-Options", "nosniff");
    if reply.status == 200 {
        builder = builder
            .header("Content-Type", "image/jpeg")
            .header("Cache-Control", "max-age=86400");
    }
    builder
        .body(reply.body)
        .unwrap_or_else(|_| tauri::http::Response::new(Vec::new()))
}

#[cfg(test)]
mod tests {

    /// Un numéro unique par test : deux tests ne partagent jamais un dossier (l'horloge de Windows tourne par 100 ns).
    fn next_id() -> u64 {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use super::steam::test_server::{Canned, jpeg, serve};
    use super::*;

    fn temp_root() -> std::path::PathBuf {
        let target = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.ancestors().nth(3).map(Path::to_path_buf))
            .unwrap_or_else(std::env::temp_dir);
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        target.join("hearth-test-tmp").join(format!(
            "protocole-{}-{nanos}-{}",
            std::process::id(),
            next_id()
        ))
    }

    struct Bench {
        ill: Illustrations,
        server: steam::test_server::Server,
        reply: Arc<Mutex<Canned>>,
        enabled: Arc<AtomicBool>,
        root: std::path::PathBuf,
    }

    struct FakeAgent {
        bytes: Option<Vec<u8>>,
        calls: AtomicUsize,
    }

    #[async_trait]
    impl AgentImages for FakeAgent {
        async fn fetch(&self, _s: &str, _f: &str, _h: &str) -> Option<Vec<u8>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.bytes.clone()
        }
    }

    async fn bench(agent: Arc<dyn AgentImages>) -> Bench {
        let reply = Arc::new(Mutex::new(Canned {
            status: 200,
            headers: vec![
                ("Content-Type".into(), "image/jpeg".into()),
                ("ETag".into(), "\"v1\"".into()),
            ],
            body: jpeg(460, 215, 3000),
        }));
        let server = serve(reply.clone()).await;
        let root = temp_root();
        let enabled = Arc::new(AtomicBool::new(true));
        let flag = enabled.clone();
        let ill = Illustrations::new(
            Cache::new(root.clone()),
            SteamClient::for_test(server.base.clone()),
            Arc::new(move || flag.load(Ordering::SeqCst)),
            agent,
        );
        Bench {
            ill,
            server,
            reply,
            enabled,
            root,
        }
    }

    impl Drop for Bench {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[tokio::test]
    async fn a_disabled_setting_sends_no_request_at_all_and_answers_404() {
        let b = bench(Arc::new(NoAgentImages)).await;
        b.enabled.store(false, Ordering::SeqCst);
        let reply = b.ill.handle("GET", "/steam/105600/header").await;
        assert_eq!(
            reply,
            Reply {
                status: 404,
                body: Vec::new()
            }
        );
        assert_eq!(
            b.server.calls(),
            0,
            "Steam n'est jamais contacté quand le réglage est coupé"
        );
    }

    #[tokio::test]
    async fn a_first_read_goes_to_steam_then_the_cache_serves_for_thirty_days() {
        let b = bench(Arc::new(NoAgentImages)).await;
        let first = b.ill.handle("GET", "/steam/105600/header").await;
        assert_eq!((first.status, first.body.len()), (200, 3000));
        let second = b.ill.handle("GET", "/steam/105600/header").await;
        assert_eq!(second.status, 200);
        assert_eq!(
            b.server.calls(),
            1,
            "la copie fraîche est servie sans relire Steam"
        );
    }

    #[tokio::test]
    async fn an_old_copy_is_revalidated_and_served_when_steam_says_304_or_is_down() {
        let b = bench(Arc::new(NoAgentImages)).await;
        b.ill.handle("GET", "/steam/7/header").await;
        // Vieillit la copie : ses validateurs gardés, `fetched_s` très ancien.
        let old = Meta {
            etag: Some("\"v1\"".into()),
            last_modified: None,
            fetched_s: 1,
        };
        let bytes = std::fs::read(b.root.join("steam").join("7-header.jpg")).unwrap();
        b.ill.cache.write_steam(7, &bytes, &old).unwrap();
        *b.reply.lock().unwrap() = Canned {
            status: 304,
            headers: vec![],
            body: Vec::new(),
        };
        let served = b.ill.handle("GET", "/steam/7/header").await;
        assert_eq!((served.status, served.body.len()), (200, 3000));
        assert!(
            b.server
                .requests
                .lock()
                .unwrap()
                .last()
                .unwrap()
                .to_ascii_lowercase()
                .contains("if-none-match: \"v1\"")
        );
        // Steam injoignable : la copie ancienne sert encore (hors ligne).
        b.ill.cache.write_steam(7, &bytes, &old).unwrap();
        *b.reply.lock().unwrap() = Canned {
            status: 500,
            headers: vec![],
            body: Vec::new(),
        };
        assert_eq!(b.ill.handle("GET", "/steam/7/header").await.status, 200);
        // Aucune copie et Steam refuse : 404.
        assert_eq!(b.ill.handle("GET", "/steam/8/header").await.status, 404);
    }

    #[tokio::test]
    async fn an_invalid_address_is_400_a_wrong_method_is_405_and_nothing_leaves() {
        let b = bench(Arc::new(NoAgentImages)).await;
        for path in [
            "/steam/0/header",
            "/steam/abc/header",
            "/steam/1/library",
            "/agent/x/../y/zz",
        ] {
            assert_eq!(b.ill.handle("GET", path).await.status, 400, "{path}");
        }
        for method in ["POST", "PUT", "DELETE"] {
            assert_eq!(
                b.ill.handle(method, "/steam/1/header").await.status,
                405,
                "{method}"
            );
        }
        assert_eq!(b.server.calls(), 0);
    }

    #[tokio::test]
    async fn turning_the_setting_off_forgets_the_steam_copies_but_keeps_the_agent_ones() {
        let image = jpeg(920, 430, 2000);
        let hex = sha256_hex(&image);
        let agent = Arc::new(FakeAgent {
            bytes: Some(image.clone()),
            calls: AtomicUsize::new(0),
        });
        let b = bench(agent.clone()).await;
        b.ill.handle("GET", "/steam/9/header").await;
        let path = format!("/agent/forge/minecraft/{hex}");
        assert_eq!(b.ill.handle("GET", &path).await.status, 200);
        b.enabled.store(false, Ordering::SeqCst);
        b.ill.purge_steam();
        assert!(!b.root.join("steam").exists());
        // L'illustration de l'agent reste affichée, depuis le cache : l'agent n'est pas relu.
        assert_eq!(b.ill.handle("GET", &path).await.status, 200);
        assert_eq!(agent.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn an_agent_image_with_the_wrong_digest_or_shape_is_refused() {
        let image = jpeg(920, 430, 2000);
        let agent = Arc::new(FakeAgent {
            bytes: Some(image),
            calls: AtomicUsize::new(0),
        });
        let b = bench(agent).await;
        let other = "ab".repeat(32);
        assert_eq!(
            b.ill
                .handle("GET", &format!("/agent/forge/minecraft/{other}"))
                .await
                .status,
            404
        );
        assert!(!b.root.join("agent").join(format!("{other}.jpg")).exists());
        let hostile = Arc::new(FakeAgent {
            bytes: Some(b"<html>".to_vec()),
            calls: AtomicUsize::new(0),
        });
        let b = bench(hostile).await;
        let hex = sha256_hex(b"<html>");
        assert_eq!(
            b.ill
                .handle("GET", &format!("/agent/forge/minecraft/{hex}"))
                .await
                .status,
            404
        );
    }

    #[test]
    fn the_http_response_has_a_fixed_type_nosniff_and_no_body_on_failure() {
        let ok = http_response(Reply::image(vec![1, 2, 3]));
        assert_eq!(ok.status(), 200);
        assert_eq!(ok.headers().get("Content-Type").unwrap(), "image/jpeg");
        assert_eq!(
            ok.headers().get("X-Content-Type-Options").unwrap(),
            "nosniff"
        );
        let missing = http_response(Reply::status(404));
        assert!(missing.body().is_empty());
        assert!(missing.headers().get("Content-Type").is_none());
    }
}
