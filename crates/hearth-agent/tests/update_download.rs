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
