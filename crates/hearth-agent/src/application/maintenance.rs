//! Purge périodique : sessions expirées, traces de révocation anciennes, compteurs de connexion
//! inactifs, opérations de plus de 24 h, journal d'activité au-delà de 90 jours puis de 50 000
//! entrées. Une seule transaction : la purge est tout ou rien. Les durées de conservation sont des
//! règles du domaine (`lockout`, `sessions`, `operations`, `audit`).

use std::sync::Arc;

use super::ports::{Clock, Store, StoreError};
use crate::domain::audit::{excess_entries, retention_cutoff};
use crate::domain::lockout::ATTEMPT_RETENTION;
use crate::domain::operations::RETENTION as OPERATION_RETENTION;
use crate::domain::sessions::REVOCATION_RETENTION;

/// Ce que la purge a supprimé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PurgeReport {
    pub sessions: u64,
    pub revocations: u64,
    pub login_attempts: u64,
    pub operations: u64,
    pub audit_events: u64,
}

pub struct MaintenanceService {
    store: Arc<dyn Store>,
    clock: Arc<dyn Clock>,
}

impl MaintenanceService {
    pub fn new(store: Arc<dyn Store>, clock: Arc<dyn Clock>) -> Self {
        Self { store, clock }
    }

    pub async fn purge(&self) -> Result<PurgeReport, StoreError> {
        let now = self.clock.now();
        let mut tx = self.store.begin().await?;
        let report = PurgeReport {
            sessions: tx.sessions().purge_expired(now).await?,
            revocations: tx
                .sessions()
                .purge_revocations(now - REVOCATION_RETENTION)
                .await?,
            login_attempts: tx
                .login_attempts()
                .purge_inactive(now - ATTEMPT_RETENTION, now)
                .await?,
            operations: tx.operations().purge(now - OPERATION_RETENTION).await?,
            // Journal : l'âge d'abord, puis le nombre sur ce qui reste (la première limite
            // atteinte joue, BR-AUDIT-008).
            audit_events: {
                let aged = tx.audit().purge_before(retention_cutoff(now)).await?;
                let count = tx.audit().count().await?;
                aged + tx.audit().purge_oldest(excess_entries(count)).await?
            },
        };
        tx.commit().await?;
        Ok(report)
    }
}
