//! Interroge l'agent qui vient de démarrer : `GET /api/v1/hello` en HTTPS (TLS 1.3), et relève
//! l'empreinte du certificat **réellement servi**.
//!
//! Le certificat de l'agent est auto-signé : il n'y a rien à vérifier contre une autorité. Ce
//! client le prend tel quel, **n'envoie aucun identifiant ni aucun secret** (une seule requête
//! publique, sans en-tête d'autorisation) et rend l'empreinte, que l'installation compare à celle
//! de l'identité qu'elle vient de créer (BR-INSTALL-004). Ce code n'est jamais utilisé pour une
//! connexion de compte : le client réel est `hearth-link`, qui épingle l'empreinte.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use hearth_proto::fingerprint::Fingerprint;
use hearth_proto::headers::API_VERSION;
use hearth_proto::product::PRODUCT_NAME;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, ClientConnection, DigitallySignedStruct, SignatureScheme, StreamOwned};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProbeError {
    #[error("connexion à {addr} impossible : {source}")]
    Connect {
        addr: SocketAddr,
        source: std::io::Error,
    },
    #[error("échange TLS avec {addr} impossible : {detail}")]
    Tls { addr: SocketAddr, detail: String },
    #[error("réponse inattendue de {addr} : {detail}")]
    Response { addr: SocketAddr, detail: String },
}

/// Prend le certificat présenté et en garde l'empreinte ; ne vérifie ni chaîne ni nom.
#[derive(Debug)]
struct CaptureVerifier {
    provider: Arc<CryptoProvider>,
    seen: Mutex<Option<Fingerprint>>,
}

impl ServerCertVerifier for CaptureVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        *self.seen.lock().unwrap_or_else(PoisonError::into_inner) =
            Some(Fingerprint::of_certificate_der(end_entity.as_ref()));
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::PeerIncompatible(
            rustls::PeerIncompatible::Tls12NotOffered,
        ))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Une demande de `/api/v1/hello` : l'empreinte du certificat servi si l'agent répond bien.
pub fn hello(addr: SocketAddr, timeout: Duration) -> Result<Fingerprint, ProbeError> {
    let tls_error = |detail: String| ProbeError::Tls { addr, detail };
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = Arc::new(CaptureVerifier {
        provider: provider.clone(),
        seen: Mutex::new(None),
    });
    let config = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| tls_error(e.to_string()))?
        .dangerous()
        .with_custom_certificate_verifier(verifier.clone())
        .with_no_client_auth();
    let connection =
        ClientConnection::new(Arc::new(config), ServerName::IpAddress(addr.ip().into()))
            .map_err(|e| tls_error(e.to_string()))?;

    let tcp = TcpStream::connect_timeout(&addr, timeout)
        .map_err(|source| ProbeError::Connect { addr, source })?;
    tcp.set_read_timeout(Some(timeout))
        .and_then(|()| tcp.set_write_timeout(Some(timeout)))
        .map_err(|source| ProbeError::Connect { addr, source })?;
    let mut stream = StreamOwned::new(connection, tcp);

    let request = format!(
        "GET /api/v1/hello HTTP/1.1\r\nHost: {addr}\r\n{API_VERSION}: 1\r\nAccept: application/json\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|e| tls_error(e.to_string()))?;
    let mut raw = Vec::new();
    // La fin de flux sans `close_notify` est une erreur pour rustls : la réponse est déjà lue.
    let _ = stream.take(64 * 1024).read_to_end(&mut raw);

    let text = String::from_utf8_lossy(&raw);
    let status_ok = text
        .lines()
        .next()
        .is_some_and(|line| line.split_whitespace().nth(1) == Some("200"));
    let is_hearth = text.contains(&format!("\"product\":\"{PRODUCT_NAME}\""));
    if !status_ok || !is_hearth {
        let first = text.lines().next().unwrap_or("(aucune réponse)");
        return Err(ProbeError::Response {
            addr,
            detail: first.chars().take(80).collect(),
        });
    }
    let seen = *verifier.seen.lock().unwrap_or_else(PoisonError::into_inner);
    seen.ok_or_else(|| tls_error("aucun certificat présenté".to_owned()))
}
