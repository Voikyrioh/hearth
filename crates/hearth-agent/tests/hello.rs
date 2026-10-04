//! Intégration en processus : vrai serveur HTTPS sur un port libre, vrai client rustls
//! qui accepte n'importe quel certificat (comme le fera `hearth-link` lors du premier contact).

// Fichier de test : les aides hors `#[test]` peuvent paniquer sans masquer l'échec.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use hearth_agent::app;
use hearth_agent::entrypoint::http::ServerHandle;
use hearth_agent::infrastructure::config::AgentConfig;
use hearth_proto::api::hello::HelloResponse;
use hearth_proto::error::{ErrorBody, ErrorCode};
use hearth_proto::fingerprint::Fingerprint;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, ring};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, ProtocolVersion, SignatureScheme};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

#[derive(Debug)]
struct AcceptAnyCertificate(Arc<CryptoProvider>);

impl ServerCertVerifier for AcceptAnyCertificate {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

fn client_config(versions: &[&'static rustls::SupportedProtocolVersion]) -> Arc<ClientConfig> {
    let provider = Arc::new(ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(versions)
        .expect("versions")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAnyCertificate(provider)))
        .with_no_client_auth();
    Arc::new(config)
}

struct Reply {
    status: u16,
    body: Vec<u8>,
    tls_version: Option<ProtocolVersion>,
    peer_fingerprint: Fingerprint,
}

async fn connect(
    addr: SocketAddr,
    config: Arc<ClientConfig>,
) -> std::io::Result<tokio_rustls::client::TlsStream<TcpStream>> {
    let tcp = TcpStream::connect(addr).await?;
    let name = ServerName::try_from("localhost").expect("nom");
    TlsConnector::from(config).connect(name, tcp).await
}

async fn get(addr: SocketAddr, path: &str) -> Reply {
    let mut tls = connect(addr, client_config(&[&rustls::version::TLS13]))
        .await
        .expect("poignée de main TLS 1.3");
    let request = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    tls.write_all(request.as_bytes()).await.expect("écriture");

    let mut raw = Vec::new();
    // Une fermeture sans `close_notify` remonte en erreur : on garde ce qui a été lu.
    let _ = tls.read_to_end(&mut raw).await;

    let (_, conn) = tls.get_ref();
    let cert = conn.peer_certificates().expect("certificat présenté")[0].clone();
    let tls_version = conn.protocol_version();

    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("fin des en-têtes");
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("statut");
    Reply {
        status,
        body: raw[split + 4..].to_vec(),
        tls_version,
        peer_fingerprint: Fingerprint::of_certificate_der(cert.as_ref()),
    }
}

fn config_for(dir: &Path) -> AgentConfig {
    AgentConfig {
        listen_addr: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: 0,
        data_dir: dir.to_owned(),
        managed: true,
    }
}

async fn start(dir: &Path) -> (ServerHandle, Fingerprint) {
    let running = app::start(&config_for(dir)).await.expect("démarrage");
    (running.server, running.identity.fingerprint)
}

/// Lance le vrai binaire. Un `agent.toml` vide est passé explicitement pour que la config
/// de la machine de test (`/etc/hearth/agent.toml`) n'interfère pas.
fn cli_fingerprint(data: &Path) -> String {
    std::fs::create_dir_all(data).expect("dossier");
    let config = data.join("agent.toml");
    std::fs::write(&config, "").expect("config vide");
    let output = Command::new(env!("CARGO_BIN_EXE_hearth-agent"))
        .args(["fingerprint", "--data-dir"])
        .arg(data)
        .arg("--config")
        .arg(config)
        .output()
        .expect("lancement du binaire");
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout)
        .expect("utf-8")
        .trim()
        .to_owned()
}

#[tokio::test]
async fn hello_is_served_over_tls13_with_the_pinned_certificate() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (server, fingerprint) = start(dir.path()).await;

    let reply = get(server.local_addr(), "/api/v1/hello").await;
    assert_eq!(reply.status, 200);
    assert_eq!(reply.tls_version, Some(ProtocolVersion::TLSv1_3));

    let hello: HelloResponse = serde_json::from_slice(&reply.body).expect("json");
    assert_eq!(hello.product, "hearth");
    assert_eq!(hello.agent_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(hello.api.min, 1);
    assert_eq!(hello.api.max, 1);
    assert!(hello.managed);
    assert!(!hello.machine_name.is_empty());
    assert_eq!(hello.install_id.len(), 32);

    // Le certificat présenté est celui de l'identité, et la commande `fingerprint` l'affiche.
    assert_eq!(reply.peer_fingerprint, fingerprint);
    assert_eq!(cli_fingerprint(dir.path()), reply.peer_fingerprint.short());

    server.shutdown().await.expect("arrêt");
}

#[tokio::test]
async fn unknown_route_answers_the_error_format_over_tls() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (server, _) = start(dir.path()).await;
    let reply = get(server.local_addr(), "/api/v1/inconnue").await;
    assert_eq!(reply.status, 404);
    let body: ErrorBody = serde_json::from_slice(&reply.body).expect("json");
    assert_eq!(body.error.code, ErrorCode::NotFound);
    server.shutdown().await.expect("arrêt");
}

#[tokio::test]
async fn fingerprint_and_install_id_survive_a_restart() {
    let dir = tempfile::tempdir().expect("tempdir");

    let (server, first_fingerprint) = start(dir.path()).await;
    let first = get(server.local_addr(), "/api/v1/hello").await;
    let first_hello: HelloResponse = serde_json::from_slice(&first.body).expect("json");
    server.shutdown().await.expect("arrêt");

    let (server, second_fingerprint) = start(dir.path()).await;
    let second = get(server.local_addr(), "/api/v1/hello").await;
    let second_hello: HelloResponse = serde_json::from_slice(&second.body).expect("json");
    server.shutdown().await.expect("arrêt");

    assert_eq!(first_fingerprint, second_fingerprint);
    assert_eq!(first.peer_fingerprint, second.peer_fingerprint);
    assert_eq!(first_hello.install_id, second_hello.install_id);
}

#[test]
fn fingerprint_command_creates_the_identity_when_absent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let data = dir.path().join("data");
    let first = cli_fingerprint(&data);
    let second = cli_fingerprint(&data);
    assert_eq!(first, second);
    assert_eq!(first.split(' ').count(), 8);
    assert!(data.join("cert.pem").is_file());
}

#[tokio::test]
async fn tls12_connections_are_refused_with_a_protocol_version_alert() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (server, _) = start(dir.path()).await;
    let error = connect(
        server.local_addr(),
        client_config(&[&rustls::version::TLS12]),
    )
    .await
    .map(|_| ())
    .expect_err("une connexion TLS 1.2 ne doit pas aboutir");
    let tls_error = error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<rustls::Error>())
        .expect("erreur rustls");
    assert!(
        matches!(
            tls_error,
            rustls::Error::AlertReceived(rustls::AlertDescription::ProtocolVersion)
        ),
        "alerte attendue : version de protocole, reçu {tls_error:?}"
    );
    server.shutdown().await.expect("arrêt");
}

#[tokio::test]
async fn wrong_method_answers_405_in_the_error_format_over_tls() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (server, _) = start(dir.path()).await;
    let mut tls = connect(
        server.local_addr(),
        client_config(&[&rustls::version::TLS13]),
    )
    .await
    .expect("poignée de main");
    tls.write_all(
        b"POST /api/v1/hello HTTP/1.1
Host: localhost
Content-Length: 0
Connection: close

",
    )
    .await
    .expect("écriture");
    let mut raw = Vec::new();
    let _ = tls.read_to_end(&mut raw).await;
    let text = String::from_utf8_lossy(&raw);
    assert!(text.starts_with("HTTP/1.1 405"), "{text}");
    assert!(text.contains("METHOD_NOT_ALLOWED"), "{text}");
    server.shutdown().await.expect("arrêt");
}
