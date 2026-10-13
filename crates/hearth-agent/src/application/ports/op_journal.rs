//! Le journal des opérations de service (`service_ops`, `service_op_steps`, HRT-83, ADR-0045) et les deux
//! ports qui l'entourent au démarrage : ce que le moteur dit du service, et le retrait des objets que
//! l'opération avait créés pour elle-même.
//!
//! Rien ici ne porte un secret : un plan, des étapes, des causes en liste fermée.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;

use super::StoreError;
use crate::domain::ops::{
    Cause, Choice, CreatedObject, Observed, OpKind, OpPlan, OpStatus, StepName, StepRecord,
    StepState, UndoInfo,
};
use crate::domain::services::key::ServiceKey;

/// Compteurs de progression (`progress_json`). Tous facultatifs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items_done: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes_done: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes_total: Option<u64>,
}

/// Les choix offerts au compte, et depuis quand ils attendent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionOffer {
    pub choices: Vec<Choice>,
    pub since: OffsetDateTime,
}

/// Une opération à accepter.
#[derive(Debug, Clone)]
pub struct NewOp {
    pub id: String,
    /// Clé d'opération du client (rejeu après coupure).
    pub op_key: String,
    pub service: ServiceKey,
    /// Le nom du service, figé à l'acceptation.
    pub service_name: String,
    pub kind: OpKind,
    /// Identifiant du compte, figé.
    pub account: String,
    pub plan: OpPlan,
    pub forced: bool,
    pub at: OffsetDateTime,
}

/// Une opération relue.
#[derive(Debug, Clone)]
pub struct OpRecord {
    pub id: String,
    pub op_key: String,
    pub service: ServiceKey,
    pub service_name: String,
    pub kind: OpKind,
    pub account: String,
    pub plan: OpPlan,
    pub status: OpStatus,
    pub step: Option<StepName>,
    pub decision: Option<DecisionOffer>,
    pub progress: Progress,
    pub cause: Option<Cause>,
    pub forced: bool,
    pub started_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub finished_at: Option<OffsetDateTime>,
}

#[derive(Debug, Error)]
pub enum JournalError {
    /// Une opération non terminée tient déjà ce service (ceinture de la base), ou l'identifiant existe.
    #[error("une opération est déjà en cours sur ce service")]
    ServiceBusy,
    /// Le statut n'est plus celui que l'appelant attendait (une autre voie a déjà bougé l'opération).
    #[error("l'opération n'est plus dans le statut attendu")]
    Stale,
    #[error("opération inconnue")]
    Unknown,
    #[error(transparent)]
    Store(#[from] StoreError),
}

#[async_trait]
pub trait OpJournal: Send + Sync {
    /// Accepte l'opération (statut `accepted`).
    async fn insert(&self, op: &NewOp) -> Result<(), JournalError>;

    async fn get(&self, id: &str) -> Result<Option<OpRecord>, StoreError>;

    /// Les opérations non terminales (qui tiennent un verrou), les plus anciennes d'abord.
    async fn unfinished(&self) -> Result<Vec<OpRecord>, StoreError>;

    /// Le journal d'une opération, dans l'ordre. Une ligne illisible est une erreur, jamais omise : une
    /// ligne perdue ferait croire qu'un geste n'a pas eu lieu.
    async fn steps(&self, id: &str) -> Result<Vec<StepRecord>, StoreError>;

    /// Ajoute une ligne au journal et met l'étape courante de l'opération. Avec `durable`, la ligne est
    /// validée SUR LE DISQUE (`synchronous=FULL`) avant le retour : c'est le cas de tout `intent`.
    async fn append_step(
        &self,
        id: &str,
        step: &StepName,
        state: StepState,
        undo: &UndoInfo,
        at: OffsetDateTime,
        durable: bool,
    ) -> Result<u32, JournalError>;

    /// Change le statut si et seulement s'il vaut `from` (compare-and-set). `decision` remplace les choix
    /// offerts (`None` les efface). Un statut terminal pose `finished_at`.
    async fn transition(
        &self,
        id: &str,
        from: OpStatus,
        to: OpStatus,
        cause: Option<Cause>,
        decision: Option<&DecisionOffer>,
        at: OffsetDateTime,
    ) -> Result<(), JournalError>;

    async fn set_progress(
        &self,
        id: &str,
        progress: &Progress,
        at: OffsetDateTime,
    ) -> Result<(), JournalError>;

    /// Les opérations en attente de décision depuis `older_than` ou avant.
    async fn expired_decisions(
        &self,
        older_than: OffsetDateTime,
    ) -> Result<Vec<OpRecord>, StoreError>;

    /// Efface les opérations terminées avant `before` et leur journal. Une opération `needs_attention`
    /// n'est jamais effacée (elle n'est pas terminée). Rend leur nombre.
    async fn purge_finished(&self, before: OffsetDateTime) -> Result<u64, StoreError>;
}

/// Ce que le moteur dit du service d'une opération non terminée, jugé par l'adaptateur du moteur. Le
/// domaine ne sait que classer le résultat (`domain::ops::recover`).
#[async_trait]
pub trait ServiceObserver: Send + Sync {
    async fn observe(&self, op: &OpRecord) -> Observed;
}

/// Ce que devient un objet que l'opération avait créé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoveOutcome {
    Removed,
    /// Il n'existait déjà plus (reprise d'un nettoyage interrompu).
    AlreadyGone,
    /// L'emplacement existe mais ce n'est plus l'objet de l'opération (autre numéro d'inode, lien, autre
    /// genre de fichier) : intact.
    NotOurs,
    /// L'inode n'a pas pu être vérifié (plate-forme sans numéro d'inode, erreur) : intact.
    Unverifiable,
}

/// Retire un objet créé par une opération, après avoir revérifié que c'est bien lui.
#[async_trait]
pub trait CreatedObjects: Send + Sync {
    async fn remove(&self, object: &CreatedObject) -> RemoveOutcome;
}
