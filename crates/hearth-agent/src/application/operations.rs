//! Suivi des opérations par clé (BR-RESIL-010) : enregistrer une requête qui modifie avant de
//! l'exécuter, retenir son résultat, rejouer le premier résultat pour la même clé, et laisser le
//! client relire l'état d'une opération après une coupure. La décision est celle du domaine
//! (`domain::operations::classify`), rendue telle quelle.

use std::sync::Arc;

use super::ports::{Clock, OperationRepo, Store, StoreError};
use crate::domain::accounts::AccountId;
use crate::domain::operations::{
    Operation, OperationKey, OperationStatus, Replay, RequestFingerprint, classify,
};

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

    /// Décide du sort d'une requête qui porte une clé. Pour `Replay::Execute`, la clé est
    /// enregistrée « en cours » dans la même transaction : deux requêtes simultanées de même clé
    /// ne s'exécutent jamais toutes les deux.
    pub async fn begin(
        &self,
        key: &OperationKey,
        account: &AccountId,
        kind: &str,
        request: &RequestFingerprint,
    ) -> Result<Replay, StoreError> {
        let mut tx = self.store.begin().await?;
        let existing = tx.operations().find(account, key).await?;
        let decision = classify(existing, request);
        if decision == Replay::Execute {
            let operation = Operation {
                key: key.clone(),
                account: account.clone(),
                kind: kind.to_owned(),
                request: request.clone(),
                status: OperationStatus::Running,
                result_json: None,
                created_at: self.clock.now(),
                finished_at: None,
            };
            tx.operations().insert(&operation).await?;
            tx.commit().await?;
        }
        Ok(decision)
    }

    /// Retient le résultat (JSON de la réponse) : la clé rejouée le rendra.
    pub async fn finish(
        &self,
        account: &AccountId,
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
            .finish(account, key, status, result_json, self.clock.now())
            .await?;
        tx.commit().await
    }

    /// Oublie la clé : une erreur interne ne se rejoue pas, le client peut relancer.
    pub async fn discard(&self, account: &AccountId, key: &OperationKey) -> Result<(), StoreError> {
        let mut tx = self.store.begin().await?;
        tx.operations().delete(account, key).await?;
        tx.commit().await
    }

    /// Au démarrage de l'agent : toute opération restée « en cours » est interrompue (le client
    /// lira « résultat inconnu »). Rend leur nombre.
    pub async fn interrupt_running(&self) -> Result<u64, StoreError> {
        let mut tx = self.store.begin().await?;
        let count = tx.operations().interrupt_running(self.clock.now()).await?;
        tx.commit().await?;
        Ok(count)
    }

    /// L'opération de ce compte, pour `GET /operations/{id}`.
    pub async fn find(
        &self,
        key: &OperationKey,
        account: &AccountId,
    ) -> Result<Option<Operation>, StoreError> {
        self.operations.find(account, key).await
    }
}
