//! Purge périodique : sessions expirées, traces de révocation anciennes, compteurs de connexion
//! inactifs, opérations de plus de 24 h. Une seule transaction : la purge est tout ou rien.

use std::sync::Arc;

use time::Duration;

use super::ports::{Clock, Store, StoreError};
use crate::domain::operations::RETENTION as OPERATION_RETENTION;

/// Durée pendant laquelle l'empreinte d'un jeton révoqué est retenue (BR-RESIL-014).
pub const REVOCATION_RETENTION: Duration = Duration::days(90);

/// Un compteur de tentatives sans activité depuis ce délai (et sans attente en cours) est oublié.
pub const ATTEMPT_RETENTION: Duration = Duration::hours(24);

/// Ce que la purge a supprimé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PurgeReport {
    pub sessions: u64,
    pub revocations: u64,
    pub login_attempts: u64,
    pub operations: u64,
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
        };
        tx.commit().await?;
        Ok(report)
    }
}
