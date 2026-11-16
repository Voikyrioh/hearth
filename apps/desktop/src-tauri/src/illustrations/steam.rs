//! Le client HTTPS vers l'hôte d'images du magasin Steam (HRT-239, ADR-0061, BR-CLIENT-015). C'est le SEUL endroit du client
//! qui contacte Valve, et il ne le fait que pour un `app_id` déjà jugé (`url::Target::Steam`) et seulement quand le
//! réglage « Images des jeux depuis Steam » est activé (vérifié par l'appelant, à chaque requête).
//!
//! Ce qu'il garantit : HTTPS seulement ; hôte fixe compilé (l'adresse est construite ici depuis l'entier, jamais depuis un
//! texte reçu) ; AUCUNE redirection suivie (une `3xx` est un échec) ; aucun cookie ; délai de 10 secondes ; deux lectures
//! simultanées au plus ; corps lu en flux et coupé à 512 Kio ; type `image/jpeg` exigé ; signature JPEG et dimensions
//! (64 à 2048 pixels) vérifiées avant de rendre un seul octet.

use std::time::Duration;

use hearth_proto::jpeg::jpeg_dimensions;
use reqwest::header::{CONTENT_TYPE, ETAG, IF_NONE_MATCH, LAST_MODIFIED, USER_AGENT};
use reqwest::{Client, StatusCode, redirect};
use tokio::sync::Semaphore;

/// L'hôte d'images du magasin Steam : le seul que ce client contacte, compilé.
pub const STEAM_HOST: &str = "https://shared.akamai.steamstatic.com";
/// 512 Kio au plus lus pour une image (les en-têtes pèsent 37 à 70 Ko).
pub const MAX_BODY: usize = 512 * 1024;
/// Côté minimal et maximal d'une image acceptée.
pub const MIN_SIDE: u16 = 64;
pub const MAX_SIDE: u16 = 2048;
const TIMEOUT: Duration = Duration::from_secs(10);
const CONCURRENCY: usize = 2;

/// Ce qu'une lecture rend.
#[derive(Debug)]
pub enum Fetched {
    /// `304` : la copie gardée est toujours la bonne.
    NotModified,
    /// L'image, vérifiée, avec ses validateurs pour la prochaine relecture conditionnelle.
    Image {
        bytes: Vec<u8>,
        etag: Option<String>,
        last_modified: Option<String>,
    },
}

/// Pourquoi une lecture échoue (jamais le texte reçu, jamais l'adresse complète).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SteamError {
    /// Réseau, délai, hôte injoignable.
    Unreachable,
    /// Redirection (jamais suivie), `404`, ou tout autre code que `200` et `304`.
    Refused,
    /// Type autre que `image/jpeg`.
    WrongType,
    /// Plus de 512 Kio.
    TooLarge,
    /// Signature ou dimensions hors règle.
    BadImage,
}

pub struct SteamClient {
    http: Client,
    base: String,
    gate: Semaphore,
}

/// Le client HTTP : TLS 1.3 par le fournisseur `ring` (le reqwest du dépôt n'en embarque aucun), autorités du système
/// pour l'hôte public de Steam, AUCUNE redirection suivie. `system_roots` faux (tests) : aucune autorité, jamais de TLS.
fn build(https_only: bool, system_roots: bool) -> Client {
    let mut roots = rustls::RootCertStore::empty();
    if system_roots {
        let loaded = rustls_native_certs::load_native_certs();
        let (added, _ignored) = roots.add_parsable_certificates(loaded.certs);
        if added == 0 {
            tracing::warn!(
                "aucune autorité de certification : les images Steam ne se chargeront pas"
            );
        }
    }
    let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map(|builder| builder.with_root_certificates(roots).with_no_client_auth());
    let builder = Client::builder()
        .redirect(redirect::Policy::none())
        .https_only(https_only)
        .timeout(TIMEOUT);
    let builder = match config {
        Ok(config) => builder.use_preconfigured_tls(config),
        Err(_) => builder,
    };
    builder.build().unwrap_or_default()
}

impl SteamClient {
    /// Le client de production : l'hôte compilé, HTTPS seulement.
    pub fn production() -> Self {
        Self {
            http: build(true, true),
            base: STEAM_HOST.to_owned(),
            gate: Semaphore::new(CONCURRENCY),
        }
    }

    /// Un client vers un serveur local de TEST (HTTP clair, hôte injecté) : n'existe qu'en test.
    #[cfg(test)]
    pub fn for_test(base: String) -> Self {
        Self {
            http: build(false, false),
            base,
            gate: Semaphore::new(CONCURRENCY),
        }
    }

    /// L'adresse de l'en-tête d'un jeu, construite depuis l'entier validé.
    fn header_url(&self, app_id: u32) -> String {
        format!(
            "{}/store_item_assets/steam/apps/{app_id}/header.jpg",
            self.base
        )
    }

    /// Lit l'en-tête d'un jeu, en conditionnel si on garde déjà une copie (`if_none_match`).
    pub async fn fetch(
        &self,
        app_id: u32,
        if_none_match: Option<&str>,
    ) -> Result<Fetched, SteamError> {
        let Ok(_permit) = self.gate.acquire().await else {
            return Err(SteamError::Unreachable);
        };
        let mut request = self
            .http
            .get(self.header_url(app_id))
            .header(USER_AGENT, concat!("Hearth/", env!("CARGO_PKG_VERSION")));
        if let Some(etag) = if_none_match {
            request = request.header(IF_NONE_MATCH, etag);
        }
        let mut response = request.send().await.map_err(|_| SteamError::Unreachable)?;
        match response.status() {
            StatusCode::NOT_MODIFIED => return Ok(Fetched::NotModified),
            StatusCode::OK => {}
            _ => return Err(SteamError::Refused),
        }
        let is_jpeg = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("image/jpeg"));
        if !is_jpeg {
            return Err(SteamError::WrongType);
        }
        let header = |name| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        };
        let (etag, last_modified) = (header(ETAG), header(LAST_MODIFIED));
        // Le corps est lu en flux et coupé : une réponse sans fin ne remplit jamais la mémoire.
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| SteamError::Unreachable)?
        {
            if bytes.len() + chunk.len() > MAX_BODY {
                return Err(SteamError::TooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        check_image(&bytes)?;
        Ok(Fetched::Image {
            bytes,
            etag,
            last_modified,
        })
    }
}

/// Signature JPEG et dimensions dans les bornes : ce que Hearth accepte de servir et de garder.
pub fn check_image(bytes: &[u8]) -> Result<(), SteamError> {
    let Some((width, height)) = jpeg_dimensions(bytes) else {
        return Err(SteamError::BadImage);
    };
    let inside = |side: u16| (MIN_SIDE..=MAX_SIDE).contains(&side);
    if inside(width) && inside(height) {
        Ok(())
    } else {
        Err(SteamError::BadImage)
    }
}

#[cfg(test)]
pub mod test_server {
    //! Un petit serveur HTTP local qui joue le rôle de Steam en test : il compte ses appels et répond ce qu'on lui dit.
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[derive(Clone)]
    pub struct Canned {
        pub status: u16,
        pub headers: Vec<(String, String)>,
        pub body: Vec<u8>,
    }

    pub struct Server {
        pub base: String,
        pub calls: Arc<AtomicUsize>,
        pub requests: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl Server {
        pub fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    /// Sert `reply` à chaque requête (une connexion = une requête).
    pub async fn serve(reply: Arc<std::sync::Mutex<Canned>>) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("port local");
        let base = format!("http://{}", listener.local_addr().expect("adresse"));
        let calls = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (counter, log) = (calls.clone(), requests.clone());
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                counter.fetch_add(1, Ordering::SeqCst);
                let (reply, log) = (reply.clone(), log.clone());
                tokio::spawn(async move {
                    let mut buffer = vec![0u8; 4096];
                    let read = socket.read(&mut buffer).await.unwrap_or(0);
                    let head = String::from_utf8_lossy(&buffer[..read]).into_owned();
                    if let Ok(mut log) = log.lock() {
                        log.push(head);
                    }
                    let canned = reply.lock().map(|r| r.clone()).expect("verrou");
                    let mut out = format!(
                        "HTTP/1.1 {} X\r\nContent-Length: {}\r\nConnection: close\r\n",
                        canned.status,
                        canned.body.len()
                    );
                    for (name, value) in &canned.headers {
                        out.push_str(&format!("{name}: {value}\r\n"));
                    }
                    out.push_str("\r\n");
                    let _ = socket.write_all(out.as_bytes()).await;
                    let _ = socket.write_all(&canned.body).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        Server {
            base,
            calls,
            requests,
        }
    }

    /// Un JPEG minimal avec les dimensions demandées (assez pour l'en-tête ; pas une image affichable).
    pub fn jpeg(width: u16, height: u16, total: usize) -> Vec<u8> {
        let mut out = vec![
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xC0, 0x00, 0x0B, 0x08,
        ];
        out.extend_from_slice(&height.to_be_bytes());
        out.extend_from_slice(&width.to_be_bytes());
        out.extend_from_slice(&[0x01, 0x01, 0x11, 0x00]);
        if out.len() < total {
            out.resize(total, 0);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::test_server::{Canned, jpeg, serve};
    use super::*;

    fn ok(body: Vec<u8>) -> Canned {
        Canned {
            status: 200,
            headers: vec![
                ("Content-Type".into(), "image/jpeg".into()),
                ("ETag".into(), "\"abc\"".into()),
            ],
            body,
        }
    }

    async fn client(reply: Canned) -> (SteamClient, super::test_server::Server) {
        let server = serve(Arc::new(Mutex::new(reply))).await;
        (SteamClient::for_test(server.base.clone()), server)
    }

    #[tokio::test]
    async fn reads_a_valid_header_with_its_validators_and_sends_the_hearth_user_agent() {
        let (steam, server) = client(ok(jpeg(460, 215, 2000))).await;
        let Fetched::Image { bytes, etag, .. } = steam.fetch(105_600, None).await.unwrap() else {
            panic!("une image");
        };
        assert_eq!(bytes.len(), 2000);
        assert_eq!(etag.as_deref(), Some("\"abc\""));
        let head = server.requests.lock().unwrap()[0].clone();
        assert!(
            head.starts_with("GET /store_item_assets/steam/apps/105600/header.jpg HTTP/1.1"),
            "{head}"
        );
        assert!(
            head.to_ascii_lowercase().contains("user-agent: hearth/"),
            "{head}"
        );
        assert!(!head.to_ascii_lowercase().contains("cookie"), "{head}");
    }

    #[tokio::test]
    async fn a_redirection_is_never_followed_and_is_a_failure() {
        let target = serve(Arc::new(Mutex::new(ok(jpeg(460, 215, 500))))).await;
        let redirect = Canned {
            status: 302,
            headers: vec![("Location".into(), format!("{}/ailleurs.jpg", target.base))],
            body: Vec::new(),
        };
        let (steam, server) = client(redirect).await;
        assert_eq!(steam.fetch(1, None).await.unwrap_err(), SteamError::Refused);
        assert_eq!(server.calls(), 1);
        assert_eq!(target.calls(), 0, "l'autre hôte n'est jamais contacté");
    }

    #[tokio::test]
    async fn a_body_over_512_kib_is_cut() {
        let (steam, _server) = client(ok(jpeg(460, 215, MAX_BODY + 1))).await;
        assert_eq!(
            steam.fetch(1, None).await.unwrap_err(),
            SteamError::TooLarge
        );
        let (steam, _server) = client(ok(jpeg(460, 215, MAX_BODY))).await;
        assert!(steam.fetch(1, None).await.is_ok());
    }

    #[tokio::test]
    async fn only_a_jpeg_of_sane_size_is_accepted() {
        let html = Canned {
            status: 200,
            headers: vec![("Content-Type".into(), "text/html".into())],
            body: b"<html>".to_vec(),
        };
        assert_eq!(
            client(html).await.0.fetch(1, None).await.unwrap_err(),
            SteamError::WrongType
        );
        let png = Canned {
            status: 200,
            headers: vec![("Content-Type".into(), "image/jpeg".into())],
            body: b"\x89PNG\r\n\x1a\n....".to_vec(),
        };
        assert_eq!(
            client(png).await.0.fetch(1, None).await.unwrap_err(),
            SteamError::BadImage
        );
        for (w, h) in [(63, 215), (460, 63), (2049, 215), (460, 2049)] {
            let wrong = client(ok(jpeg(w, h, 500))).await.0.fetch(1, None).await;
            assert_eq!(wrong.unwrap_err(), SteamError::BadImage, "{w}x{h}");
        }
        let missing = Canned {
            status: 404,
            headers: vec![],
            body: Vec::new(),
        };
        assert_eq!(
            client(missing).await.0.fetch(1, None).await.unwrap_err(),
            SteamError::Refused
        );
    }

    #[tokio::test]
    async fn a_conditional_read_sends_if_none_match_and_understands_304() {
        let (steam, server) = client(Canned {
            status: 304,
            headers: vec![],
            body: Vec::new(),
        })
        .await;
        assert!(matches!(
            steam.fetch(7, Some("\"abc\"")).await.unwrap(),
            Fetched::NotModified
        ));
        let head = server.requests.lock().unwrap()[0].to_ascii_lowercase();
        assert!(head.contains("if-none-match: \"abc\""), "{head}");
    }

    #[tokio::test]
    async fn an_unreachable_host_is_a_failure_not_a_panic() {
        let steam = SteamClient::for_test("http://127.0.0.1:1".to_owned());
        assert_eq!(
            steam.fetch(1, None).await.unwrap_err(),
            SteamError::Unreachable
        );
    }
}
