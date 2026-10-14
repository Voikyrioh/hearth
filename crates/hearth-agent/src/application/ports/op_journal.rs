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

/// Dans quels statuts une ligne du journal d'étapes peut être écrite. La condition est vérifiée PAR
/// L'INSTRUCTION qui écrit, pas avant : une annulation qui passe l'opération en `cancelling` entre la
/// lecture du statut et l'écriture d'un `intent` destructif ne laisse pas cet `intent` s'écrire (HRT-144,
/// ADR-0045 point « atomique »).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepGuard {
    /// L'opération n'est pas terminée : toute ligne ordinaire (`done`, `failed`, `undone`, `intent` d'une
    /// étape qui ne détruit rien), y compris pendant une annulation ou un retour arrière qui rangent.
    Unfinished,
    /// L'opération s'exécute (`running`) ou revient en arrière (`rolling_back`) : l'`intent` d'une étape
    /// destructive. Une opération en annulation, en attente d'une décision ou à vérifier n'écrit plus de
    /// geste destructif.
    Executing,
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
    NoSuchOperation,
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
    /// validée SUR LE DISQUE (`synchronous=FULL`) avant le retour : c'est le cas de tout `intent`. La ligne
    /// n'est écrite que si le statut de l'opération satisfait `guard`, vérifié dans la même instruction
    /// (`Stale` sinon, `Unknown` si l'opération n'existe pas).
    #[allow(clippy::too_many_arguments)]
    async fn append_step(
        &self,
        id: &str,
        step: &StepName,
        state: StepState,
        undo: &UndoInfo,
        at: OffsetDateTime,
        durable: bool,
        guard: StepGuard,
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

    /// Borne le nombre d'opérations GARDÉES : ne laisse que les `keep` opérations terminées les plus
    /// récemment (avec leur journal), efface les autres. Une opération qui n'est pas terminée n'est jamais
    /// comptée ni effacée. Rend le nombre d'opérations effacées.
    async fn prune_finished_beyond(&self, keep: u32) -> Result<u64, StoreError>;

    /// Les `limit` opérations terminées le plus récemment, de la plus récente à la plus ancienne : de quoi
    /// rebâtir « la dernière action » de chaque service au démarrage. Une ligne illisible est ignorée
    /// (tracée), jamais fatale.
    async fn recent_finished(&self, limit: u32) -> Result<Vec<OpRecord>, StoreError>;
}

/// Ce que le moteur dit du service d'une opération non terminée, jugé par l'adaptateur du moteur. Le
/// domaine ne sait que classer le résultat (`domain::ops::recover`).
#[async_trait]
pub trait ServiceObserver: Send + Sync {
    /// `steps` est le journal de l'opération, déjà relu par l'appelant : l'observateur n'a pas besoin du
    /// journal pour juger si le plan est allé au bout de ses gestes destructifs.
    async fn observe(&self, op: &OpRecord, steps: &[StepRecord]) -> Observed;
}

/// Ce que l'agent a CONSIGNÉ d'un service créé par Hearth : de quoi reconnaître ses conteneurs sans croire
/// une étiquette. Une étiquette de conteneur (`io.hearth.service-id`) peut être posée par n'importe qui, y
/// compris par l'auteur d'une image (`LABEL`) ; elle ne vaut jamais identité (revue de la PR 94, BR-OPS-008).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordedService {
    /// Aucun service de cette clé dans la base de l'agent : une étiquette qui le nomme est une affirmation
    /// d'un tiers.
    Unknown,
    /// Le service est connu, mais l'agent n'a consigné AUCUN identifiant de conteneur pour lui (le journal
    /// ne les consigne pas encore) : un conteneur qui porte l'étiquette ne peut être ni reconnu ni écarté.
    KnownWithoutContainers,
    /// Le service est connu, et voici les identifiants de conteneur que l'agent a consignés pour lui (deux
    /// pendant une recréation : l'ancien et le nouveau).
    Containers(Vec<String>),
}

/// Ce que la base de l'agent sait des conteneurs d'un service créé par Hearth.
#[async_trait]
pub trait RecordedContainers: Send + Sync {
    async fn recorded(&self, key: &ServiceKey) -> Result<RecordedService, StoreError>;
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
