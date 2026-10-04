//! Suivi des opérations par clé (BR-RESIL-010) : enregistrer une requête qui modifie avant de
//! l'exécuter, retenir son résultat, rejouer le premier résultat pour la même clé, et laisser le
//! client relire l'état d'une opération après une coupure.

use std::sync::Arc;

use super::ports::{Clock, OperationRepo, Store, StoreError};
use crate::domain::accounts::AccountId;
use crate::domain::operations::{Operation, OperationKey, OperationStatus, Replay, classify};

/// Ce que la couche HTTP doit faire d'une requête qui porte une clé.
#[derive(Debug)]
pub enum Begin {
    /// Clé nouvelle, enregistrée « en cours » : exécuter, puis `finish` (ou `discard`).
    Execute,
    /// Clé déjà terminée : rendre ce résultat sans exécuter.
    Replay(Operation),
    /// Clé déjà reçue, exécution pas finie.
    InProgress,
    /// Clé déjà utilisée par un autre compte.
    ForeignKey,
}

pub struct OperationService {
    operations: Arc<dyn OperationRepo>,
    store: Arc<dyn Store>,
    clock: Arc<dyn Clock>,
}

impl OperationService {
    pub fn new(
        operations: Arc<dyn OperationRepo>,
        store: Arc<dyn Store>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            operations,
            store,
            clock,
        }
    }

    /// Enregistre la clé (état « en cours ») si elle est nouvelle, dans une transaction : deux
    /// requêtes simultanées de même clé ne s'exécutent jamais toutes les deux.
    pub async fn begin(
        &self,
        key: &OperationKey,
        account: &AccountId,
        kind: &str,
    ) -> Result<Begin, StoreError> {
        let mut tx = self.store.begin().await?;
        let existing = tx.operations().find(key).await?;
        match classify(existing.as_ref(), account) {
            Replay::Execute => {
                let operation = Operation {
                    key: key.clone(),
                    account: account.clone(),
                    kind: kind.to_owned(),
                    status: OperationStatus::Running,
                    result_json: None,
                    created_at: self.clock.now(),
                    finished_at: None,
                };
                tx.operations().insert(&operation).await?;
                tx.commit().await?;
                Ok(Begin::Execute)
            }
            Replay::Return(_) => {
                drop(tx);
                Ok(existing.map_or(Begin::InProgress, Begin::Replay))
            }
            Replay::InProgress => Ok(Begin::InProgress),
            Replay::ForeignKey => Ok(Begin::ForeignKey),
        }
    }

    /// Retient le résultat (JSON de la réponse) : la clé rejouée le rendra.
    pub async fn finish(
        &self,
        key: &OperationKey,
        succeeded: bool,
        result_json: &str,
    ) -> Result<(), StoreError> {
        let status = if succeeded {
            OperationStatus::Succeeded
        } else {
            OperationStatus::Failed
        };
        let mut tx = self.store.begin().await?;
        tx.operations()
            .finish(key, status, result_json, self.clock.now())
            .await?;
        tx.commit().await
    }

    /// Oublie la clé : une erreur interne ne se rejoue pas, le client peut relancer.
    pub async fn discard(&self, key: &OperationKey) -> Result<(), StoreError> {
        let mut tx = self.store.begin().await?;
        tx.operations().delete(key).await?;
        tx.commit().await
    }

    /// L'opération de ce compte, pour `GET /operations/{id}`. Celle d'un autre compte n'existe
    /// pas pour lui.
    pub async fn find(
        &self,
        key: &OperationKey,
        account: &AccountId,
    ) -> Result<Option<Operation>, StoreError> {
        Ok(self
            .operations
            .find(key)
            .await?
            .filter(|operation| &operation.account == account))
    }
}
