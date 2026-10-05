//! Flux d'audit vide : tant qu'aucun journal n'est branché (HRT-05), le sujet `audit` du flux
//! temps réel reste muet.

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::broadcast;

use crate::application::ports::AuditFeed;

#[derive(Debug, Default)]
pub struct NoAuditFeed;

impl AuditFeed for NoAuditFeed {
    fn subscribe(&self) -> Option<broadcast::Receiver<Arc<Value>>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_is_nothing_to_subscribe_to() {
        assert!(NoAuditFeed.subscribe().is_none());
    }
}
