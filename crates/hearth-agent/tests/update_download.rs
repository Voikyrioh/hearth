//! Le téléchargeur réel contre un vrai serveur HTTPS local (certificat auto-signé donné comme seule
//! autorité) : octets et progression, statut d'erreur, redirection hors HTTPS refusée, taille
//! plafonnée, certificat inconnu refusé, adresse en clair refusée.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use hearth_agent::application::ports::{Downloader, FetchError};
use hearth_agent::infrastructure::update::HttpsDownloader;
use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

const BODY: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";

struct Server {
    port: u16,
    root: CertificateDer<'static>,
}

/// Sert : `/ok` (le corps), `/missing` (404), `/redirect` (302 vers HTTP), `/big` (annonce 10 Mo).
async fn serve() -> Server {
    let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
    let cert = certified.cert.der().clone();
    let key = PrivateKeyDer::try_from(certified.signing_key.serialize_der()).unwrap();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![cert.clone()], key)
        .unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(stream).await else {
                    return;
                };
                let mut request = Vec::new();
                let mut buffer = [0_u8; 1024];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    match tls.read(&mut buffer).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => request.extend_from_slice(&buffer[..n]),
                    }
                }
                let text = String::from_utf8_lossy(&request).into_owned();
                let path = text.split_whitespace().nth(1).unwrap_or("/").to_owned();
                let response: Vec<u8> = match path.as_str() {
                    "/ok" => [
                        format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", BODY.len())
                            .into_bytes(),
                        BODY.to_vec(),
                    ]
                    .concat(),
                    "/redirect" => b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
                    "/big" => b"HTTP/1.1 200 OK\r\nContent-Length: 10485760\r\nConnection: close\r\n\r\n".to_vec(),
                    _ => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
                };
                let _ = tls.write_all(&response).await;
                let _ = tls.shutdown().await;
            });
        }
    });
    Server { port, root: cert }
}

fn url(server: &Server, path: &str) -> String {
    format!("https://localhost:{}{path}", server.port)
}

#[tokio::test]
async fn the_bytes_arrive_whole_with_a_progress_ending_at_the_total() {
    let server = serve().await;
    let downloader = HttpsDownloader::with_roots(vec![server.root.clone()], true);
    let seen = Mutex::new(Vec::new());
    let bytes = downloader
        .fetch(&url(&server, "/ok"), 1024, &|received, total| {
            seen.lock().unwrap().push((received, total));
        })
        .await
        .unwrap();
    assert_eq!(bytes, BODY);
    let seen = seen.into_inner().unwrap();
    assert_eq!(seen.first(), Some(&(0, Some(BODY.len() as u64))));
    assert_eq!(
        seen.last(),
        Some(&(BODY.len() as u64, Some(BODY.len() as u64)))
    );
    assert!(
        seen.windows(2).all(|pair| pair[0].0 <= pair[1].0),
        "jamais de recul"
    );
}

#[tokio::test]
async fn an_error_status_is_a_failure_not_an_unreachable_server() {
    let server = serve().await;
    let downloader = HttpsDownloader::with_roots(vec![server.root.clone()], true);
    let result = downloader
        .fetch(&url(&server, "/missing"), 1024, &|_, _| {})
        .await;
    assert!(matches!(result, Err(FetchError::Failed(_))), "{result:?}");
}

#[tokio::test]
async fn a_redirect_to_clear_text_is_refused() {
    let server = serve().await;
    let downloader = HttpsDownloader::with_roots(vec![server.root.clone()], true);
    let result = downloader
        .fetch(&url(&server, "/redirect"), 1024, &|_, _| {})
        .await;
    assert!(result.is_err(), "{result:?}");
}

#[tokio::test]
async fn a_file_over_the_limit_is_refused_from_its_announced_size() {
    let server = serve().await;
    let downloader = HttpsDownloader::with_roots(vec![server.root.clone()], true);
    let result = downloader
        .fetch(&url(&server, "/big"), 1024, &|_, _| {})
        .await;
    assert!(matches!(result, Err(FetchError::TooLarge)), "{result:?}");
    // Et par le nombre d'octets reçus, si la taille n'était pas annoncée honnêtement.
    let small = downloader.fetch(&url(&server, "/ok"), 10, &|_, _| {}).await;
    assert!(matches!(small, Err(FetchError::TooLarge)), "{small:?}");
}

#[tokio::test]
async fn a_certificate_the_system_does_not_trust_is_refused() {
    let server = serve().await;
    // Aucune autorité donnée : le certificat auto-signé du serveur n'est pas approuvé.
    let downloader = HttpsDownloader::with_roots(Vec::new(), true);
    let result = downloader
        .fetch(&url(&server, "/ok"), 1024, &|_, _| {})
        .await;
    assert!(result.is_err(), "{result:?}");
}

#[tokio::test]
async fn local_and_private_addresses_are_refused_by_name_and_by_literal_unless_allowed() {
    let server = serve().await;
    // `localhost` se résout en bouclage : refusé à la résolution, avant toute connexion.
    let strict = HttpsDownloader::with_roots(vec![server.root.clone()], false);
    let by_name = strict.fetch(&url(&server, "/ok"), 1024, &|_, _| {}).await;
    assert!(by_name.is_err(), "{by_name:?}");
    // Le même serveur, une fois les adresses locales permises (tests de bout en bout seulement).
    let open = HttpsDownloader::with_roots(vec![server.root.clone()], true);
    assert!(
        open.fetch(&url(&server, "/ok"), 1024, &|_, _| {})
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn exotic_spellings_of_a_local_address_are_refused_before_any_connection() {
    let server = serve().await;
    let strict = HttpsDownloader::with_roots(vec![server.root.clone()], false);
    // Autant de façons d'écrire 127.0.0.1 : `reqwest` les lit toutes comme l'adresse de bouclage,
    // le filtre doit les lire de la même façon (un littéral ne passe jamais par le résolveur).
    for host in [
        "2130706433",
        "127.1",
        "0x7f.0.0.1",
        "0x7f000001",
        "0177.0.0.1",
        "%31%32%37.0.0.1",
        "[::ffff:127.0.0.1]",
        "[::127.0.0.1]",
        "127.0.0.1.",
    ] {
        let url = format!("https://{host}:{}/ok", server.port);
        let result = strict.fetch(&url, 1024, &|_, _| {}).await;
        assert!(
            matches!(&result, Err(FetchError::Failed(detail)) if detail.contains("locale ou privée")),
            "{host} : {result:?}"
        );
    }
}

/// Le sous-processus de `an_environment_proxy_is_never_used_...` : il tourne avec `HTTPS_PROXY`,
/// `HTTP_PROXY` et `ALL_PROXY` posés vers un « proxy » qui compte les connexions (l'environnement
/// d'un processus ne se modifie pas en Rust sûr : le test se relance lui-même avec cet
/// environnement). Lancé seul, il ne fait rien.
#[tokio::test]
async fn proxy_child_probe() {
    if std::env::var("HEARTH_PROXY_CHILD").as_deref() != Ok("1") {
        return;
    }
    assert!(
        std::env::var("HTTPS_PROXY").is_ok(),
        "le proxy doit être posé dans cet environnement"
    );
    let server = serve().await;
    // Adresses locales permises (bout en bout) : le serveur est joint DIRECTEMENT. Un client qui
    // suivrait HTTPS_PROXY enverrait sa demande au proxy et échouerait.
    let open = HttpsDownloader::with_roots(vec![server.root.clone()], true);
    let direct = open.fetch(&url(&server, "/ok"), 1024, &|_, _| {}).await;
    assert_eq!(direct.unwrap(), BODY, "joint sans passer par le proxy");
    // Filtre actif : l'adresse locale est refusée avant toute connexion, proxy ou non.
    let strict = HttpsDownloader::with_roots(vec![server.root.clone()], false);
    let literal = format!("https://127.0.0.1:{}/ok", server.port);
    let refused = strict.fetch(&literal, 1024, &|_, _| {}).await;
    assert!(
        matches!(&refused, Err(FetchError::Failed(detail)) if detail.contains("locale ou privée")),
        "{refused:?}"
    );
}

// FIX:01M47XJXQ0GHV77FN4J6R1NXPZ
#[test]
fn an_environment_proxy_is_never_used_so_the_address_filter_cannot_be_bypassed() {
    // BR-UPDATE-027 : avec un proxy, le nom de l'hôte partirait au proxy qui le résoudrait lui-même,
    // et le filtre d'adresses appliqué après résolution ne verrait rien. Le client n'utilise
    // aucun proxy d'environnement.
    let proxy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    proxy.set_nonblocking(true).unwrap();
    let address = format!("http://{}", proxy.local_addr().unwrap());
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "proxy_child_probe", "--nocapture"])
        .env("HEARTH_PROXY_CHILD", "1")
        .env("HTTPS_PROXY", &address)
        .env("https_proxy", &address)
        .env("HTTP_PROXY", &address)
        .env("ALL_PROXY", &address)
        .env("http_proxy", &address)
        .env("all_proxy", &address)
        .env_remove("NO_PROXY")
        .env_remove("no_proxy")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "le sous-processus a bien tourné : {}",
        String::from_utf8_lossy(&output.stdout)
    );
    match proxy.accept() {
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
        other => panic!("une connexion est partie vers le proxy : {other:?}"),
    }
}
