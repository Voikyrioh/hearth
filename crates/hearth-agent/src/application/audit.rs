//! Journal d'activité : lecture filtrée et export (administrateurs seulement), écriture hors
//! transaction (`AuditRecorder`), et l'aide des cas d'usage qui écrivent une entrée **dans leur
//! transaction** (gestion des comptes, connexion) puis la diffusent une fois la transaction
//! validée.
//!
//! Les règles sont celles de `domain::audit` (ce qui est journalisé, ce qui peut y entrer, la
//! conservation, les filtres, le CSV) ; ce module les enchaîne et demande au stockage d'exécuter.
//! **Un échec d'écriture du journal ne fait jamais échouer l'action** : il est tracé en `error`.

use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::broadcast;

use super::ports::{AuditFeed, AuditRepo, AuditSink, Store, StoreError, UnitOfWork};
use crate::domain::accounts::Role;
use crate::domain::audit::{AuditEvent, AuditFilter, AuditRecord, can_read_journal, render_csv};

/// Lignes d'un export, au plus : les plus récentes (le journal entier tient en 50 000 entrées, un
/// export de cette taille n'a plus rien d'un fichier qu'on ouvre dans un tableur).
pub const EXPORT_LIMIT: usize = 10_000;

#[derive(Debug, Error)]
pub enum AuditError {
    /// BR-AUDIT-001 : seul un administrateur lit le journal.
    #[error("Tu n'as pas la permission de lire le journal d'activité")]
    Forbidden,
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Une page du journal : les entrées, de la plus récente à la plus ancienne, et le curseur de la
/// page suivante (`before`) s'il en reste.
#[derive(Debug)]
pub struct AuditPage {
    pub records: Vec<AuditRecord>,
    pub next_before: Option<i64>,
}

/// L'export du résultat filtré.
#[derive(Debug)]
pub struct AuditExport {
    /// Le fichier CSV (UTF-8 avec marque d'ordre des octets).
    pub csv: String,
    /// Le résultat dépassait `EXPORT_LIMIT` : seules les plus récentes sont dans le fichier.
    pub truncated: bool,
}

pub struct AuditService {
    repo: Arc<dyn AuditRepo>,
    feed: Arc<dyn AuditFeed>,
}

impl AuditService {
    pub fn new(repo: Arc<dyn AuditRepo>, feed: Arc<dyn AuditFeed>) -> Self {
        Self { repo, feed }
    }

    fn check(role: Role) -> Result<(), AuditError> {
        if can_read_journal(role) {
            Ok(())
        } else {
            Err(AuditError::Forbidden)
        }
    }

    /// Une page du journal (BR-AUDIT-001, BR-AUDIT-014 à BR-AUDIT-016). Le journal ne se
    /// consulte qu'en lecture : rien ici ne le modifie (BR-AUDIT-009).
    pub async fn search(&self, role: Role, filter: &AuditFilter) -> Result<AuditPage, AuditError> {
        Self::check(role)?;
        // Une entrée de plus que la page : elle dit s'il y en a une suivante.
        let mut records = self.repo.search(filter, filter.limit + 1).await?;
        let next_before = if records.len() > filter.limit {
            records.truncate(filter.limit);
            records.last().map(|record| record.id)
        } else {
            None
        };
        Ok(AuditPage {
            records,
            next_before,
        })
    }

    /// BR-AUDIT-017 : le résultat filtré, en CSV. Le curseur et la taille de page ne comptent
    /// pas : l'export porte sur tout ce qui correspond, jusqu'à `EXPORT_LIMIT` lignes.
    pub async fn export(
        &self,
        role: Role,
        filter: &AuditFilter,
    ) -> Result<AuditExport, AuditError> {
        Self::check(role)?;
        let filter = AuditFilter {
            before: None,
            ..filter.clone()
        };
        let mut records = self.repo.search(&filter, EXPORT_LIMIT + 1).await?;
        let truncated = records.len() > EXPORT_LIMIT;
        records.truncate(EXPORT_LIMIT);
        Ok(AuditExport {
            csv: render_csv(&records),
            truncated,
        })
    }

    /// Les entrées écrites à partir de maintenant, pour le flux temps réel (BR-AUDIT-010) ;
    /// réservé aux administrateurs.
    pub fn subscribe(&self, role: Role) -> Result<broadcast::Receiver<AuditRecord>, AuditError> {
        Self::check(role)?;
        Ok(self.feed.subscribe())
    }
}

/// Écrit une entrée hors transaction (refus et échecs relevés par le routeur).
pub struct AuditRecorder {
    store: Arc<dyn Store>,
    feed: Arc<dyn AuditFeed>,
}

impl AuditRecorder {
    pub fn new(store: Arc<dyn Store>, feed: Arc<dyn AuditFeed>) -> Self {
        Self { store, feed }
    }

    async fn write(&self, event: &AuditEvent) -> Result<AuditRecord, StoreError> {
        let mut tx = self.store.begin().await?;
        let record = tx.audit().record(event).await?;
        tx.commit().await?;
        Ok(record)
    }
}

#[async_trait]
impl AuditSink for AuditRecorder {
    async fn record(&self, event: AuditEvent) {
        match self.write(&event).await {
            Ok(record) => self.feed.publish(record),
            Err(error) => {
                tracing::error!(%error, action = event.action.code(), "entrée du journal non écrite");
            }
        }
    }
}

/// Entrées écrites dans une transaction, à diffuser une fois celle-ci validée.
#[derive(Default)]
pub(super) struct Pending(Vec<AuditRecord>);

impl Pending {
    /// Écrit l'entrée dans la transaction de l'action : elle existe si l'action est validée, et
    /// seulement alors. Un échec d'écriture n'annule pas l'action : tracé en `error`.
    pub(super) async fn record(&mut self, tx: &mut dyn UnitOfWork, event: AuditEvent) {
        match tx.audit().record(&event).await {
            Ok(record) => self.0.push(record),
            Err(error) => {
                tracing::error!(%error, action = event.action.code(), "entrée du journal non écrite");
            }
        }
    }

    /// À appeler une fois la transaction validée.
    pub(super) fn publish(self, feed: &dyn AuditFeed) {
        for record in self.0 {
            feed.publish(record);
        }
    }
}
