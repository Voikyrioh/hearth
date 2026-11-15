//! Le registre des transferts (`transfers`, HRT-95, ADR-0048). **Le disque dit si un fichier existe, la base dit s'il
//! est à Hearth** : un fichier de la zone d'attente sans ligne n'existe pas pour l'agent (jamais retiré par nom).

use async_trait::async_trait;
use time::OffsetDateTime;

use super::StoreError;
use crate::domain::transfers::TransferId;
use hearth_proto::api::transfers::DIGEST_LEN;

/// Où en est un transfert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferState {
    /// Des morceaux manquent.
    Receiving,
    /// Tout est reçu et vérifié : `<id>.bin` existe.
    Complete,
}

/// Une ligne du registre, relue.
#[derive(Debug, Clone)]
pub struct TransferRow {
    pub id: TransferId,
    /// L'identifiant technique du compte qui a ouvert le dépôt.
    pub account: String,
    pub state: TransferState,
    pub size: u64,
    /// Les morceaux reçus ET écrits, selon la base (le disque fait foi à la reprise).
    pub received_chunks: u64,
    pub dev: u64,
    pub ino: u64,
    /// Pour un transfert complet : l'empreinte de l'ensemble.
    pub tree_digest: Option<[u8; DIGEST_LEN]>,
    /// Pour un transfert complet : la date de modification du fichier publié (nanosecondes).
    pub complete_mtime_ns: i64,
    pub created_at: OffsetDateTime,
    /// La dernière activité : la reprise est possible 24 heures après.
    pub updated_at: OffsetDateTime,
}

/// Un dépôt qui s'ouvre.
#[derive(Debug, Clone)]
pub struct NewTransfer {
    pub id: TransferId,
    pub account: String,
    pub size: u64,
    pub dev: u64,
    pub ino: u64,
    pub now: OffsetDateTime,
}

#[async_trait]
pub trait TransferRepo: Send + Sync {
    async fn insert(&self, new: &NewTransfer) -> Result<(), StoreError>;

    async fn get(&self, id: &TransferId) -> Result<Option<TransferRow>, StoreError>;

    /// Note les morceaux écrits et la dernière activité d'un envoi EN COURS. `false` : la ligne n'existe plus ou n'est
    /// plus en cours.
    async fn note_progress(
        &self,
        id: &TransferId,
        received_chunks: u64,
        now: OffsetDateTime,
    ) -> Result<bool, StoreError>;

    /// L'envoi est complet : inscrit l'empreinte de l'ensemble et l'identité du fichier publié (le dernier mot de
    /// l'agent sur ce fichier). `false` : la ligne n'existe plus ou n'est plus en cours.
    async fn complete(
        &self,
        id: &TransferId,
        tree_digest: &[u8; DIGEST_LEN],
        mtime_ns: i64,
        now: OffsetDateTime,
    ) -> Result<bool, StoreError>;

    async fn delete(&self, id: &TransferId) -> Result<bool, StoreError>;

    /// Les dépôts de ce compte non consommés (reprenables ou complets).
    async fn count_open(&self, account: &str) -> Result<usize, StoreError>;

    /// Les lignes dont la dernière activité est antérieure à `before`, les plus anciennes d'abord.
    async fn expired(
        &self,
        before: OffsetDateTime,
        limit: usize,
    ) -> Result<Vec<TransferRow>, StoreError>;
}
