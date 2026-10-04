//! Purge périodique : sessions expirées, traces de révocation anciennes, compteurs de connexion
//! inactifs, opérations de plus de 24 h, journal d'activité au-delà de 90 jours puis de 50 000
//! entrées. Une seule transaction : la purge est tout ou rien. Les durées de conservation sont des
//! règles du domaine (`lockout`, `sessions`, `operations`, `audit`) ; le journal, lui, se purge à
//! part, par lots.

use std::sync::Arc;

use super::ports::{Clock, Store, StoreError};
use crate::domain::audit::{PURGE_BATCH, excess_entries, retention_cutoff};
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

    /// Journal d'activité (BR-AUDIT-008) : l'âge d'abord, puis le nombre sur ce qui reste (la
    /// première limite atteinte joue), **par lots de `PURGE_BATCH`, chacun dans sa propre
    /// transaction** : la suppression ne tient jamais longtemps le verrou d'écriture.
    ///
    /// **Le seul endroit qui purge le journal** : la purge horaire l'appelle, et le contrôle du
    /// plafond à l'écriture (`audit::AuditTrail`) aussi.
    pub async fn purge_journal(&self) -> Result<u64, StoreError> {
        let cutoff = retention_cutoff(self.clock.now());
        let mut total = 0;
        loop {
            let mut tx = self.store.begin().await?;
            let removed = tx.audit().purge_before(cutoff, PURGE_BATCH).await?;
            tx.commit().await?;
            total += removed;
            if removed < PURGE_BATCH {
                break;
            }
        }
        loop {
            let mut tx = self.store.begin().await?;
            let count = tx.audit().count().await?;
            let batch = excess_entries(count).min(PURGE_BATCH);
            if batch == 0 {
                break;
            }
            let removed = tx.audit().purge_oldest(batch).await?;
            tx.commit().await?;
            total += removed;
            if removed == 0 {
                break;
            }
        }
        Ok(total)
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
            // Le journal se purge à part, par lots (voir `purge_journal`).
            audit_events: 0,
        };
        tx.commit().await?;
        Ok(PurgeReport {
            audit_events: self.purge_journal().await?,
            ..report
        })
    }
}
