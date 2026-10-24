//! Le magasin et la diffusion du journal d'activité qu'une clôture d'opération écrit dans sa transaction.

#![allow(dead_code)]

use std::sync::Arc;

use hearth_agent::application::audit::AuditTrail;
use hearth_agent::application::ports::{AuditFeed, Clock, Store};
use hearth_agent::infrastructure::audit_feed::BroadcastAuditFeed;
use hearth_agent::infrastructure::sqlite::{Database, SqliteStore};

pub fn audit_parts(db: &Database, clock: Arc<dyn Clock>) -> (Arc<dyn Store>, Arc<AuditTrail>) {
    let store: Arc<dyn Store> = Arc::new(SqliteStore::new(db.pool().clone()));
    let feed: Arc<dyn AuditFeed> = Arc::new(BroadcastAuditFeed::new());
    let (_, trail) = hearth_agent::app::audit_trail(&store, clock, feed);
    (store, trail)
}
