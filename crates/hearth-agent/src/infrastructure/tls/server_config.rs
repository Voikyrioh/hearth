use std::sync::Arc;

use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use thiserror::Error;

use crate::application::ports::Identity;

#[derive(Debug, Error)]
pub enum TlsError {
    #[error("configuration TLS invalide : {0}")]
    Config(#[from] rustls::Error),
}

/// Configuration serveur : TLS 1.3 seul, fournisseur cryptographique `ring`, pas d'auth client.
pub fn server_config(identity: &Identity) -> Result<Arc<ServerConfig>, TlsError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let certificate = CertificateDer::from(identity.certificate_der.clone());
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.private_key_der.clone()));

    let mut config = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_no_client_auth()
        .with_single_cert(vec![certificate], key)?;
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}
