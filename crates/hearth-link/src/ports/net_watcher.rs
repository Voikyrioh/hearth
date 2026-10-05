//! Adresses réseau locales (détection d'un changement de réseau, BR-RESIL-006).

use std::collections::BTreeSet;
use std::net::IpAddr;

use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("lecture des adresses réseau impossible : {0}")]
pub struct NetError(pub String);

#[async_trait]
pub trait NetWatcher: Send + Sync {
    /// Adresses locales actuelles (hors bouclage).
    async fn addresses(&self) -> Result<BTreeSet<IpAddr>, NetError>;
}
