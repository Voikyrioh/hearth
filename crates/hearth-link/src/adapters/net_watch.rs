//! Adresses locales du poste, lues avec `if-addrs` (Windows et Linux).

use std::collections::BTreeSet;
use std::net::IpAddr;

use async_trait::async_trait;

use crate::ports::net_watcher::{NetError, NetWatcher};

pub struct SystemNetWatcher;

#[async_trait]
impl NetWatcher for SystemNetWatcher {
    async fn addresses(&self) -> Result<BTreeSet<IpAddr>, NetError> {
        tokio::task::spawn_blocking(|| {
            if_addrs::get_if_addrs().map(|interfaces| {
                interfaces
                    .into_iter()
                    .filter(|interface| !interface.is_loopback())
                    .map(|interface| interface.ip())
                    .collect()
            })
        })
        .await
        .map_err(|e| NetError(e.to_string()))?
        .map_err(|e| NetError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_local_addresses_can_be_read_without_loopback() {
        let addresses = SystemNetWatcher.addresses().await.unwrap();
        assert!(addresses.iter().all(|ip| !ip.is_loopback()));
    }
}
