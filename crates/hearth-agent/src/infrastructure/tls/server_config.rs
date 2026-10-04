use std::sync::Arc;

use rustls::ServerConfig;
use thiserror::Error;

use super::identity::FileIdentityStore;
use crate::application::ports::IdentityError;

#[derive(Debug, Error)]
pub enum TlsError {
    #[error("configuration TLS invalide : {0}")]
    Config(#[from] rustls::Error),
    #[error(transparent)]
    Identity(#[from] IdentityError),
}

/// Configuration serveur : TLS 1.3 seul, fournisseur cryptographique `ring`, pas d'auth client.
/// La clé privée est lue ici et ne sort pas de ce module.
pub fn server_config(store: &FileIdentityStore) -> Result<Arc<ServerConfig>, TlsError> {
    let material = store.read_tls_material()?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());

    let mut config = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_no_client_auth()
        .with_single_cert(vec![material.certificate], material.private_key)?;
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}
