//! Client HTTPS de test : vrai TLS 1.3 vers un vrai agent, certificat accepté sans vérification
//! (comme le fait `hearth-link` lors du premier contact), requêtes HTTP/1.1 écrites à la main.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use hearth_agent::app::{self, Adapters, Metering, RunningAgent};
use hearth_agent::infrastructure::config::AgentConfig;
use hearth_agent::infrastructure::ids::UlidGen;
use hearth_agent::infrastructure::random::OsTokenGen;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, ring};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use super::Env;

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

pub fn client_config() -> Arc<ClientConfig> {
    let provider = Arc::new(ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .expect("versions")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAnyCertificate(provider)))
        .with_no_client_auth();
    Arc::new(config)
}

/// Réponse HTTP lue sur le fil.
pub struct Wire {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    /// Le corps lu comme JSON (`null` s'il est vide ou n'est pas du JSON : un export CSV).
    pub body: Value,
    /// Le corps tel quel.
    pub text: String,
}

impl Wire {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn code(&self) -> &str {
        self.body["error"]["code"].as_str().unwrap_or("")
    }
}

/// Agent démarré sur un port libre, sur la base et avec l'horloge de `env`.
pub struct Agent {
    running: RunningAgent,
    pub addr: SocketAddr,
}

pub async fn start(env: &Env) -> Agent {
    start_metered(env, super::probe::metering()).await
}

/// Comme `start`, avec ces sondes (simulées), cette cadence et ces délais de flux.
pub async fn start_metered(env: &Env, metering: Metering) -> Agent {
    let config = AgentConfig {
        listen_addr: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: 0,
        data_dir: env.dir.path().to_owned(),
        managed: false,
    };
    let adapters = Adapters {
        hasher: env.hasher.clone(),
        clock: env.clock.clone(),
        ids: Arc::new(UlidGen),
        tokens: Arc::new(OsTokenGen),
    };
    let running = app::start_with_metering(&config, &env.db, &adapters, metering)
        .await
        .expect("démarrage");
    let addr = running.server.local_addr();
    Agent { running, addr }
}

impl Agent {
    pub async fn shutdown(self) {
        self.running.server.shutdown().await.expect("arrêt");
    }

    /// Requête HTTP/1.1 sur une connexion TLS 1.3 neuve. `X-Hearth-Api: 1` par défaut.
    pub fn request(&self, method: &str, path: &str) -> Request<'_> {
        Request {
            agent: self,
            method: method.to_owned(),
            path: format!("/api/v1{path}"),
            headers: vec![("x-hearth-api".into(), "1".into())],
            body: None,
        }
    }
}

pub struct Request<'a> {
    agent: &'a Agent,
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
}

impl Request<'_> {
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers
            .retain(|(key, _)| !key.eq_ignore_ascii_case(name));
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    pub fn token(self, token: &str) -> Self {
        self.header("authorization", &format!("Bearer {token}"))
    }

    pub fn json(mut self, body: &Value) -> Self {
        self.body = Some(body.to_string());
        self
    }

    /// Envoie la requête puis coupe la connexion sans lire la réponse (client qui perd le lien).
    pub async fn send_and_cut(self) {
        let tcp = TcpStream::connect(self.agent.addr)
            .await
            .expect("connexion");
        let name = ServerName::try_from("localhost").expect("nom");
        let mut tls = TlsConnector::from(client_config())
            .connect(name, tcp)
            .await
            .expect("poignée de main TLS 1.3");
        let body = self.body.clone().unwrap_or_default();
        let mut head = format!(
            "{} {} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n",
            self.method, self.path
        );
        for (name, value) in &self.headers {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        head.push_str(&format!(
            "content-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
            body.len()
        ));
        tls.write_all(head.as_bytes()).await.expect("écriture");
        tls.flush().await.expect("envoi");
        // Laisse l'agent lire la requête avant que la connexion ne tombe.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        drop(tls);
    }

    pub async fn send(self) -> Wire {
        let tcp = TcpStream::connect(self.agent.addr)
            .await
            .expect("connexion");
        let name = ServerName::try_from("localhost").expect("nom");
        let mut tls = TlsConnector::from(client_config())
            .connect(name, tcp)
            .await
            .expect("poignée de main TLS 1.3");
        let mut head = format!(
            "{} {} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n",
            self.method, self.path
        );
        for (name, value) in &self.headers {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        let body = self.body.unwrap_or_default();
        if !body.is_empty() {
            head.push_str(&format!(
                "content-type: application/json\r\ncontent-length: {}\r\n",
                body.len()
            ));
        }
        head.push_str("\r\n");
        head.push_str(&body);
        tls.write_all(head.as_bytes()).await.expect("écriture");

        let mut raw = Vec::new();
        // Une fermeture sans `close_notify` remonte en erreur : on garde ce qui a été lu.
        let _ = tls.read_to_end(&mut raw).await;
        parse(&raw)
    }
}

fn parse(raw: &[u8]) -> Wire {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("fin des en-têtes");
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .expect("statut");
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
        .collect();
    let body = &raw[split + 4..];
    Wire {
        status,
        headers,
        body: if body.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(body).unwrap_or(Value::Null)
        },
        text: String::from_utf8_lossy(body).into_owned(),
    }
}
