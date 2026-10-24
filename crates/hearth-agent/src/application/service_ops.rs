//! Le journal des opérations de service, côté cas d'usage : accepter, verrouiller, avancer étape par étape,
//! annuler, décider, et au démarrage reprendre ou signaler (HRT-83, ADR-0045).
//!
//! **Cette tranche n'exécute AUCUNE opération réelle** : pas de suppression, de modification ni de
//! sauvegarde. Elle fournit le modèle sur lequel s'écrivent HRT-64 et HRT-68 : un geste destructif écrit
//! son `intent` AVANT de s'exécuter ([`ServiceOps::intent`], validé sur le disque), son `done` après.
//!
//! **Au démarrage** ([`ServiceOps::recover_at_startup`]), l'agent classe chaque opération non terminée
//! (`domain::ops::recover`), retire seulement les objets que l'opération avait créés pour elle-même, puis
//! la marque `interrupted` ou `needs_attention`. Il ne reprend, ne rejoue et ne défait jamais seul un
//! geste sur un service ou ses données.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use thiserror::Error;
use time::OffsetDateTime;
use tokio::sync::{Mutex as AsyncMutex, broadcast};

use super::audit::{AuditTrail, Pending};
use super::ports::{
    Clock, CreatedObjects, DecisionOffer, IdGen, JournalError, NewOp, OpJournal, OpRecord,
    Progress, RemoveOutcome, ServiceObserver, StepGuard, Store, StoreError,
};
use crate::domain::audit::{Actor, AuditAction, AuditEvent, Outcome, Target};
use crate::domain::ops::{
    CancelVerdict, Cause, Choice, ChoiceRefused, LockTable, OpKind, OpPlan, OpStatus, OpSummary,
    OpsIndex, Refusal, StepName, StepRecord, StepState, UndoInfo, apply_choice, cancel_verdict,
    cleanup_on_cancel, decision_expired, decision_expiry, latest_state, recover,
    recover_unreadable_journal,
};
use crate::domain::services::key::ServiceKey;
use crate::domain::services::spec::fold_name;
use hearth_proto::api::service_ops::{FINISHED_RETENTION_S, MAX_RETAINED_OPERATIONS};

/// Combien de changements d'opération le canal garde pour un abonné lent. Un abonné qui en perd relit les
/// opérations en cours (`open_snapshots`) : il ne bloque jamais l'agent (HRT-144).
const FEED_CAPACITY: usize = 256;
/// Deux publications de progression d'une même opération sont séparées d'au moins cela : la progression
/// d'un tirage d'image arrive par rafales, chaque publication relit le journal. Un changement d'état n'est
/// jamais retenu. (La cadence de 500 ms par opération et par abonné est celle du flux, `domain::ops::flush`.)
const PROGRESS_PUBLISH_MIN: time::Duration = time::Duration::milliseconds(100);
/// Combien d'opérations suivies par la cadence de progression avant de la remettre à zéro.
const PROGRESS_GATE_MAX: usize = 1_024;
/// Combien d'opérations non terminées sont rendues à un abonné qui arrive.
const MAX_OPEN_SNAPSHOTS: usize = 500;

/// Une opération et son journal d'étapes, tels que le suivi les montre : ce que le message `operation` du
/// flux et `GET /operations/services/{id}` portent. Aucun secret : un plan, des étapes en codes.
#[derive(Debug, Clone)]
pub struct OpSnapshot {
    pub record: OpRecord,
    pub steps: Vec<StepRecord>,
}

impl OpSnapshot {
    /// L'annulation est encore possible : avant le point de non-retour du genre, dans un statut qui s'annule.
    pub fn cancellable(&self) -> bool {
        cancel_verdict(self.record.status, &self.record.plan, &self.steps) == CancelVerdict::Cancel
    }
}

fn summary_of(record: &OpRecord) -> OpSummary {
    OpSummary {
        id: record.id.clone(),
        kind: record.kind,
        account: record.account.clone(),
        status: record.status,
        cause: record.cause,
        forced: record.forced,
        started_at: record.started_at,
        finished_at: record.finished_at,
    }
}

/// Les verrous en mémoire : une table pure derrière un `Mutex` (sections critiques sans `await`).
#[derive(Default)]
pub struct OpLocks {
    table: Mutex<LockTable>,
}

impl OpLocks {
    pub fn new() -> Self {
        Self::default()
    }

    /// Accès à la table. Un `Mutex` empoisonné (panique ailleurs) ne bloque pas l'agent : la table n'a pas
    /// d'invariant qu'une panique pourrait casser à mi-chemin.
    pub fn with<R>(&self, job: impl FnOnce(&mut LockTable) -> R) -> R {
        let mut guard: MutexGuard<'_, LockTable> =
            self.table.lock().unwrap_or_else(PoisonError::into_inner);
        job(&mut guard)
    }
}

#[derive(Debug, Error)]
pub enum OpError {
    /// Un verrou refuse (service pris, nom réservé, place des opérations lourdes prise).
    #[error("{0:?}")]
    Locked(Refusal),
    #[error("une opération est déjà en cours sur ce service")]
    ServiceBusy,
    #[error("opération inconnue")]
    NoSuchOperation,
    #[error("l'étape {0} n'est pas dans le plan")]
    UnknownStep(String),
    /// L'ordre du journal est violé : `done` sans `intent` pour un geste destructif, ou un geste sur une
    /// opération qui n'est plus en cours.
    #[error("écriture hors ordre dans le journal")]
    OutOfOrder,
    #[error("l'opération n'est plus dans le statut attendu")]
    Stale,
    #[error("transition de statut interdite")]
    IllegalTransition,
    #[error("ce choix n'est pas permis ici : {0:?}")]
    Choice(ChoiceRefused),
    #[error("stockage indisponible : {0}")]
    Storage(String),
}

impl From<JournalError> for OpError {
    fn from(error: JournalError) -> Self {
        match error {
            JournalError::ServiceBusy => Self::ServiceBusy,
            JournalError::Stale => Self::Stale,
            JournalError::NoSuchOperation => Self::NoSuchOperation,
            JournalError::Store(error) => Self::Storage(error.to_string()),
        }
    }
}

impl From<StoreError> for OpError {
    fn from(error: StoreError) -> Self {
        Self::Storage(error.to_string())
    }
}

/// Une opération à accepter.
pub struct AcceptRequest {
    pub op_key: String,
    pub service: ServiceKey,
    pub service_name: String,
    pub kind: OpKind,
    pub account: String,
    pub plan: OpPlan,
    pub forced: bool,
    /// Le nom à réserver (replié) : suppression ou déploiement (BR-DEL-019).
    pub reserve_name: Option<String>,
    /// L'opération prend la place des opérations lourdes (sauvegarde, restauration, reprise de monde).
    pub heavy_io: bool,
}

/// Ce que l'annulation a fait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelOutcome {
    /// L'opération n'avait rien commencé : annulée.
    Cancelled,
    /// L'opération court : elle passe en `cancelling`, l'exécuteur retire ce qu'elle a créé et la clôt.
    Cancelling,
    /// Le point de non-retour est passé (`TOO_LATE_TO_CANCEL`).
    TooLate,
    /// L'opération n'est pas dans un statut qui s'annule.
    NotCancellable,
}

/// Une action de cycle de vie (démarrer, arrêter, redémarrer) que l'arrêt de l'agent a interrompue (HRT-64,
/// BR-LIFE-020) : le compte, le nom du service au moment de l'action et le geste. Le démarrage de l'agent les
/// donne pour qu'elles soient consignées au journal d'activité ; il ne les reprend, ne les rejoue ni ne les
/// défait.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterruptedAction {
    pub account: String,
    pub service_name: String,
    pub kind: OpKind,
}

/// Ce que le démarrage a fait de chaque opération non terminée.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecoverySummary {
    /// Les actions de cycle de vie et les déploiements interrompus, de la plus ancienne à la plus récente.
    pub interrupted_actions: Vec<InterruptedAction>,
    /// Les déploiements interrompus AVANT la création de leur conteneur (HRT-68, BR-DEPLOY-019) : rien n'a été
    /// créé, leur description écrite à l'acceptation ne doit pas rester (elle garderait le nom). Le démarrage
    /// l'efface (`ServiceSpecs::forget`) et retire leur dossier de données.
    pub abandoned_deploys: Vec<ServiceKey>,
    pub interrupted: u32,
    pub needs_attention: u32,
    /// Opérations `needs_attention` déjà là au démarrage, dont le verrou a été repris.
    pub locks_restored: u32,
    pub objects_removed: u32,
    /// Objets laissés en place (autre inode, lien, vérification impossible).
    pub objects_left: u32,
    /// Opérations dont le journal des étapes ne se relit pas : classées « à vérifier », rien n'est rejoué.
    pub unreadable: u32,
    /// Opérations dont le nouveau statut n'a pas pu être écrit (l'agent démarre quand même).
    pub unwritten: u32,
}

pub struct ServiceOps {
    journal: Arc<dyn OpJournal>,
    locks: Arc<OpLocks>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGen>,
    objects: Arc<dyn CreatedObjects>,
    /// Chaque changement d'opération, pour le flux. Un abonné lent perd les plus anciens et relit l'état.
    feed: broadcast::Sender<Arc<OpSnapshot>>,
    /// `busy` et `last_action` de la liste des services : des projections du journal (BR-LIFE-013).
    index: Mutex<OpsIndex>,
    /// Quand la progression de chaque opération a été publiée pour la dernière fois.
    progress_gate: Mutex<HashMap<String, OffsetDateTime>>,
    /// Sérialise les publications : lecture du journal, projection et envoi se font d'un seul tenant. Deux
    /// publications concurrentes (l'exécuteur d'un côté, l'annulation de l'autre) ne peuvent donc pas
    /// s'envoyer dans l'ordre inverse de leur lecture : un abonné ne reçoit jamais un état plus ancien
    /// APRÈS un plus récent (revue de la PR 94).
    publishing: AsyncMutex<()>,
    /// Le magasin et la diffusion du journal d'activité : une clôture ou un constat écrit son entrée dans la
    /// transaction de l'opération (jamais regroupée : ce n'est pas un refus répété, BR-AUDIT-007).
    store: Arc<dyn Store>,
    trail: Arc<AuditTrail>,
}

/// Un événement du journal d'activité sans sa date : la date est celle de la transaction qui l'écrit avec
/// l'issue ou la cause (`finish_with`, `annotate_with`).
pub struct AuditDraft {
    pub actor: Actor,
    pub action: AuditAction,
    pub target: Target,
    pub outcome: Outcome,
}

impl AuditDraft {
    fn event(&self, at: OffsetDateTime) -> AuditEvent {
        AuditEvent::new(
            at,
            self.actor.clone(),
            self.action,
            self.target.clone(),
            self.outcome,
        )
    }
}

impl ServiceOps {
    pub fn new(
        journal: Arc<dyn OpJournal>,
        locks: Arc<OpLocks>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGen>,
        objects: Arc<dyn CreatedObjects>,
        store: Arc<dyn Store>,
        trail: Arc<AuditTrail>,
    ) -> Self {
        let (feed, _) = broadcast::channel(FEED_CAPACITY);
        Self {
            journal,
            locks,
            clock,
            ids,
            objects,
            feed,
            index: Mutex::new(OpsIndex::new()),
            progress_gate: Mutex::new(HashMap::new()),
            publishing: AsyncMutex::new(()),
            store,
            trail,
        }
    }

    pub fn locks(&self) -> &Arc<OpLocks> {
        &self.locks
    }

    // -----------------------------------------------------------------------------------------
    // Suivi : lire une opération, la suivre en direct, projeter `busy` et `last_action`
    // -----------------------------------------------------------------------------------------

    /// Les changements d'opération à partir de maintenant. S'abonner AVANT de lire l'état
    /// (`open_snapshots`) : aucun changement ne se perd entre les deux.
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<OpSnapshot>> {
        self.feed.subscribe()
    }

    /// Une opération et son journal d'étapes ; `None` si elle n'existe pas (ou plus : purgée).
    pub async fn snapshot(&self, id: &str) -> Result<Option<OpSnapshot>, OpError> {
        let Some(record) = self.journal.get(id).await? else {
            return Ok(None);
        };
        let steps = self.journal.steps(id).await?;
        Ok(Some(OpSnapshot { record, steps }))
    }

    /// Les opérations non terminées (au plus [`MAX_OPEN_SNAPSHOTS`]), de quoi renseigner un abonné qui
    /// arrive ou qui a perdu des changements. Une opération dont le journal ne se relit pas est ignorée
    /// (tracée) : elle reste tenue par son verrou.
    pub async fn open_snapshots(&self) -> Result<Vec<OpSnapshot>, OpError> {
        let mut out = Vec::new();
        for record in self.journal.unfinished().await? {
            if out.len() >= MAX_OPEN_SNAPSHOTS {
                break;
            }
            match self.journal.steps(&record.id).await {
                Ok(steps) => out.push(OpSnapshot { record, steps }),
                Err(error) => {
                    tracing::warn!(op = %record.id, %error, "opération en cours dont le journal ne se relit pas : non montrée");
                }
            }
        }
        Ok(out)
    }

    /// Lit la projection `busy` et `last_action` des services (sous un verrou court : ne pas attendre avec).
    pub fn with_index<R>(&self, read: impl FnOnce(&OpsIndex) -> R) -> R {
        let guard = self.index.lock().unwrap_or_else(PoisonError::into_inner);
        read(&guard)
    }

    /// Rebâtit la projection depuis le journal : les opérations non terminées, puis les dernières terminées.
    pub async fn rebuild_index(&self) -> Result<(), OpError> {
        let finished = self
            .journal
            .recent_finished(MAX_RETAINED_OPERATIONS)
            .await?;
        let open = self.journal.unfinished().await?;
        let mut index = OpsIndex::new();
        // Les plus anciennes d'abord : la plus récente terminée gagne, comme à l'usage.
        for record in finished.iter().rev().chain(open.iter()) {
            index.apply(&record.service, summary_of(record));
        }
        *self.index.lock().unwrap_or_else(PoisonError::into_inner) = index;
        Ok(())
    }

    /// À appeler après un changement d'opération fait HORS de ce service (l'annulation et la décision écrivent
    /// leur statut dans la transaction de leur entrée du journal d'activité, `ServiceOpAccess`) : rend le
    /// verrou d'une opération devenue terminale, met la projection à jour et prévient les abonnés.
    pub async fn settle(&self, id: &str) {
        if let Ok(Some(record)) = self.journal.get(id).await
            && record.status.is_terminal()
        {
            self.locks.with(|table| table.release_all_of(id));
        }
        self.publish(id).await;
    }

    /// Retire ce qu'une opération ANNULÉE avait créé pour elle-même (archive partielle, dossier d'import ou
    /// d'attente : `cleanup_on_cancel`), objet par objet, revérifié par son numéro d'inode. Après le
    /// changement de statut, jamais avant : une annulation refusée n'a rien détruit. Idempotent ; un objet
    /// qui n'est plus lui est laissé (tracé). Aucun geste sur le service ni sur ses données.
    pub async fn clean_after_cancel(&self, snapshot: &OpSnapshot) {
        for object in cleanup_on_cancel(&snapshot.record.plan, &snapshot.steps) {
            match self.objects.remove(&object).await {
                RemoveOutcome::Removed | RemoveOutcome::AlreadyGone => {}
                RemoveOutcome::NotOurs | RemoveOutcome::Unverifiable => tracing::warn!(
                    op = %snapshot.record.id,
                    kind = ?object.kind,
                    "objet d'une opération annulée laissé en place : ce n'est plus lui, ou il n'a pas pu être vérifié"
                ),
            }
        }
    }

    /// Relit l'opération, met la projection à jour et prévient les abonnés. Un échec de lecture est tracé
    /// et ne change rien pour l'appelant : le suivi n'est jamais ce qui fait échouer un geste.
    async fn publish(&self, id: &str) {
        let _one_at_a_time = self.publishing.lock().await;
        match self.snapshot(id).await {
            Ok(Some(snapshot)) => {
                self.index
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .apply(&snapshot.record.service, summary_of(&snapshot.record));
                if snapshot.record.status.is_terminal() {
                    self.progress_gate
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .remove(id);
                }
                // Personne n'écoute : ce n'est pas une erreur.
                let _ = self.feed.send(Arc::new(snapshot));
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(op = %id, %error, "changement d'opération non annoncé : journal illisible");
            }
        }
    }

    /// Une progression se publie au plus toutes les 100 ms par opération (voir [`PROGRESS_PUBLISH_MIN`]).
    fn progress_due(&self, id: &str) -> bool {
        let now = self.clock.now();
        let mut gate = self
            .progress_gate
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if gate
            .get(id)
            .is_some_and(|last| now >= *last && now - *last < PROGRESS_PUBLISH_MIN)
        {
            return false;
        }
        if gate.len() >= PROGRESS_GATE_MAX {
            gate.clear();
        }
        gate.insert(id.to_owned(), now);
        true
    }

    /// Accepte une opération : prend les verrous (service exclusif, nom, place lourde), écrit la ligne
    /// `accepted`. Si l'écriture échoue, les verrous sont rendus. Rend l'identifiant.
    pub async fn accept(&self, request: AcceptRequest) -> Result<String, OpError> {
        let id = self.ids.new_id();
        let service = request.service.as_str();
        self.locks
            .with(|table| -> Result<(), Refusal> {
                table.acquire_exclusive(service, &id, request.kind)?;
                if let Some(name) = &request.reserve_name
                    && let Err(refusal) = table.reserve_name(name, &id, request.kind)
                {
                    table.release_all_of(&id);
                    return Err(refusal);
                }
                if request.heavy_io
                    && let Err(refusal) = table.acquire_heavy_io(service, &id)
                {
                    table.release_all_of(&id);
                    return Err(refusal);
                }
                Ok(())
            })
            .map_err(OpError::Locked)?;
        let new = NewOp {
            id: id.clone(),
            op_key: request.op_key,
            service: request.service,
            service_name: request.service_name,
            kind: request.kind,
            account: request.account,
            plan: request.plan,
            forced: request.forced,
            at: self.clock.now(),
        };
        if let Err(error) = self.journal.insert(&new).await {
            self.locks.with(|table| table.release_all_of(&id));
            return Err(error.into());
        }
        self.publish(&id).await;
        Ok(id)
    }

    /// `accepted` → `running`.
    pub async fn start(&self, id: &str) -> Result<(), OpError> {
        self.move_to(id, OpStatus::Accepted, OpStatus::Running, None)
            .await
    }

    /// Écrit l'`intent` d'un geste, validé sur le disque, AVANT le geste. Pour une étape destructive,
    /// l'opération doit être en cours.
    pub async fn intent(&self, id: &str, step: &str, undo: &UndoInfo) -> Result<u32, OpError> {
        let (op, name, destructive) = self.step_of(id, step).await?;
        if destructive && op.status != OpStatus::Running && op.status != OpStatus::RollingBack {
            return Err(OpError::OutOfOrder);
        }
        if !undo.is_well_formed() {
            return Err(OpError::OutOfOrder);
        }
        // Le statut est revérifié par l'instruction qui écrit : une annulation qui passe l'opération en
        // `cancelling` entre la lecture ci-dessus et l'écriture ne laisse pas s'écrire l'`intent` d'un geste
        // destructif (`Stale`).
        let guard = if destructive {
            StepGuard::Executing
        } else {
            StepGuard::Unfinished
        };
        let seq = self
            .journal
            .append_step(
                id,
                &name,
                StepState::Intent,
                undo,
                self.clock.now(),
                true,
                guard,
            )
            .await?;
        self.publish(id).await;
        Ok(seq)
    }

    /// Écrit le `done` d'un geste fait. Pour une étape destructive, son `intent` doit être la dernière ligne.
    pub async fn done(&self, id: &str, step: &str, undo: &UndoInfo) -> Result<u32, OpError> {
        self.close_step(id, step, StepState::Done, undo).await
    }

    /// Écrit l'échec d'un geste.
    pub async fn failed(&self, id: &str, step: &str) -> Result<u32, OpError> {
        self.close_step(id, step, StepState::Failed, &UndoInfo::default())
            .await
    }

    /// Écrit qu'un geste a été défait (retour arrière réussi).
    pub async fn undone(&self, id: &str, step: &str) -> Result<u32, OpError> {
        self.close_step(id, step, StepState::Undone, &UndoInfo::default())
            .await
    }

    async fn close_step(
        &self,
        id: &str,
        step: &str,
        state: StepState,
        undo: &UndoInfo,
    ) -> Result<u32, OpError> {
        let (op, name, destructive) = self.step_of(id, step).await?;
        if !undo.is_well_formed() {
            return Err(OpError::OutOfOrder);
        }
        if op.status.is_terminal() {
            return Err(OpError::OutOfOrder);
        }
        if destructive {
            let records = self.journal.steps(id).await?;
            let expected = if state == StepState::Undone {
                StepState::Done
            } else {
                StepState::Intent
            };
            if latest_state(&records, &name) != Some(expected) {
                return Err(OpError::OutOfOrder);
            }
        }
        let seq = self
            .journal
            .append_step(
                id,
                &name,
                state,
                undo,
                self.clock.now(),
                false,
                StepGuard::Unfinished,
            )
            .await?;
        self.publish(id).await;
        Ok(seq)
    }

    async fn step_of(&self, id: &str, step: &str) -> Result<(OpRecord, StepName, bool), OpError> {
        let op = self
            .journal
            .get(id)
            .await?
            .ok_or(OpError::NoSuchOperation)?;
        let name = StepName::parse(step).ok_or_else(|| OpError::UnknownStep(step.to_owned()))?;
        let planned = op
            .plan
            .step(&name)
            .ok_or_else(|| OpError::UnknownStep(step.to_owned()))?;
        let destructive = planned.destructive;
        Ok((op, name, destructive))
    }

    pub async fn set_progress(&self, id: &str, progress: &Progress) -> Result<(), OpError> {
        self.journal
            .set_progress(id, progress, self.clock.now())
            .await?;
        if self.progress_due(id) {
            self.publish(id).await;
        }
        Ok(())
    }

    /// Note que l'opération a forcé (HRT-64, BR-LIFE-009). Avant sa fin.
    pub async fn mark_forced(&self, id: &str) -> Result<(), OpError> {
        self.journal.mark_forced(id, self.clock.now()).await?;
        self.publish(id).await;
        Ok(())
    }

    /// Pose, après coup, une cause sur une réussite qui n'en a pas (HRT-64, BR-LIFE-005, BR-LIFE-011) et
    /// prévient les abonnés. La cause et l'entrée du journal d'activité de ce constat sont écrites dans UNE
    /// transaction, puis l'état est annoncé (flux et projection) : `GET /operations/...` lit le journal des
    /// opérations et ne voit la cause qu'avec son entrée (HRT-101). `false` : rien n'a changé, aucune entrée
    /// n'est écrite.
    pub async fn annotate_with(
        &self,
        id: &str,
        cause: Cause,
        draft: AuditDraft,
    ) -> Result<bool, OpError> {
        let now = self.clock.now();
        let mut tx = self.store.begin().await?;
        if !tx.service_ops().annotate_cause(id, cause, now).await? {
            return Ok(false);
        }
        let mut pending = Pending::default();
        pending.record(&mut *tx, draft.event(now)).await?;
        tx.commit().await?;
        pending.publish(&self.trail);
        self.publish(id).await;
        Ok(true)
    }

    /// Clôt l'opération dans un statut terminal, sans entrée au journal d'activité, et rend ses verrous. Le
    /// même chemin que [`Self::finish_with`].
    pub async fn finish(
        &self,
        id: &str,
        to: OpStatus,
        cause: Option<Cause>,
    ) -> Result<(), OpError> {
        self.finish_with(id, to, cause, None).await
    }

    /// Clôt l'opération dans un statut terminal avec l'entrée du journal d'activité de l'issue. La transition
    /// et l'entrée sont écrites dans UNE transaction (une transition refusée n'écrit rien) : `GET
    /// /operations/...`, qui lit le journal des opérations, ne voit l'issue qu'avec son entrée. Puis, dans cet
    /// ordre : les verrous sont rendus, l'issue est annoncée (flux et projection), comme `move_to` et `settle`
    /// (HRT-101).
    ///
    /// Si l'écriture de l'entrée échoue, l'opération n'est pas laissée `running` avec son verrou alors que le
    /// geste a eu lieu : elle est close sans entrée, dans une seconde transaction, et l'échec est tracé.
    pub async fn finish_with(
        &self,
        id: &str,
        to: OpStatus,
        cause: Option<Cause>,
        draft: Option<AuditDraft>,
    ) -> Result<(), OpError> {
        if !to.is_terminal() {
            return Err(OpError::IllegalTransition);
        }
        match self.close(id, to, cause, draft.as_ref()).await {
            Err(OpError::Storage(error)) if draft.is_some() => {
                tracing::error!(
                    %error,
                    operation = id,
                    "entrée du journal d'activité non écrite : l'opération est close sans elle"
                );
                self.close(id, to, cause, None).await
            }
            other => other,
        }
    }

    /// La clôture dans une transaction : statut, entrée éventuelle, puis verrous et annonce.
    async fn close(
        &self,
        id: &str,
        to: OpStatus,
        cause: Option<Cause>,
        draft: Option<&AuditDraft>,
    ) -> Result<(), OpError> {
        let now = self.clock.now();
        let mut tx = self.store.begin().await?;
        let op = tx
            .service_ops()
            .get(id)
            .await?
            .ok_or(OpError::NoSuchOperation)?;
        if !op.status.can_become(to) {
            return Err(OpError::IllegalTransition);
        }
        tx.service_ops()
            .transition(id, op.status, to, cause, None, now)
            .await?;
        let mut pending = Pending::default();
        if let Some(draft) = draft {
            pending.record(&mut *tx, draft.event(now)).await?;
        }
        tx.commit().await?;
        pending.publish(&self.trail);
        self.locks.with(|table| table.release_all_of(id));
        self.publish(id).await;
        Ok(())
    }

    /// Demande une décision au compte : `running` → `awaiting_decision` avec les choix offerts.
    pub async fn ask_decision(&self, id: &str, choices: &[Choice]) -> Result<(), OpError> {
        if choices.is_empty() {
            return Err(OpError::Choice(ChoiceRefused::NotOffered));
        }
        // « Annuler » ne s'offre jamais au-delà du point de non-retour du genre : la décision n'est pas une
        // porte dérobée pour annuler là où `DELETE` répond « trop tard » (revue de la PR 94).
        if choices.contains(&Choice::Cancel) {
            let op = self
                .journal
                .get(id)
                .await?
                .ok_or(OpError::NoSuchOperation)?;
            let steps = self.journal.steps(id).await?;
            if cancel_verdict(op.status, &op.plan, &steps) == CancelVerdict::TooLate {
                return Err(OpError::Choice(ChoiceRefused::NotApplicable));
            }
        }
        let now = self.clock.now();
        let offer = DecisionOffer {
            choices: choices.to_vec(),
            since: now,
        };
        self.journal
            .transition(
                id,
                OpStatus::Running,
                OpStatus::AwaitingDecision,
                None,
                Some(&offer),
                now,
            )
            .await?;
        self.publish(id).await;
        Ok(())
    }

    /// Applique le choix d'un compte à une opération qui attend. La vérification du DROIT du compte est
    /// celle de la route qui appellera ceci (HRT-64 et suivants).
    pub async fn decide(&self, id: &str, choice: Choice) -> Result<OpStatus, OpError> {
        let op = self
            .journal
            .get(id)
            .await?
            .ok_or(OpError::NoSuchOperation)?;
        let offered = op
            .decision
            .as_ref()
            .map(|offer| offer.choices.as_slice())
            .unwrap_or_default();
        if choice == Choice::Cancel {
            let steps = self.journal.steps(id).await?;
            if cancel_verdict(op.status, &op.plan, &steps) == CancelVerdict::TooLate {
                return Err(OpError::Choice(ChoiceRefused::NotApplicable));
            }
        }
        let (next, cause) = apply_choice(op.status, offered, choice).map_err(OpError::Choice)?;
        let cause = (next != OpStatus::Running).then_some(cause);
        self.move_to(id, op.status, next, cause).await?;
        Ok(next)
    }

    /// Demande d'annulation (acte `OperationCancel`, ajouté avec sa route). Avant le point de non-retour du
    /// genre ; après, `TooLate`.
    pub async fn cancel(&self, id: &str) -> Result<CancelOutcome, OpError> {
        let op = self
            .journal
            .get(id)
            .await?
            .ok_or(OpError::NoSuchOperation)?;
        let records = self.journal.steps(id).await?;
        match cancel_verdict(op.status, &op.plan, &records) {
            CancelVerdict::TooLate => Ok(CancelOutcome::TooLate),
            CancelVerdict::NotCancellable => Ok(CancelOutcome::NotCancellable),
            CancelVerdict::Cancel if op.status == OpStatus::Running => {
                self.move_to(
                    id,
                    OpStatus::Running,
                    OpStatus::Cancelling,
                    Some(Cause::CancelledByAccount),
                )
                .await?;
                Ok(CancelOutcome::Cancelling)
            }
            CancelVerdict::Cancel => {
                self.move_to(
                    id,
                    op.status,
                    OpStatus::Cancelled,
                    Some(Cause::CancelledByAccount),
                )
                .await?;
                Ok(CancelOutcome::Cancelled)
            }
        }
    }

    async fn move_to(
        &self,
        id: &str,
        from: OpStatus,
        to: OpStatus,
        cause: Option<Cause>,
    ) -> Result<(), OpError> {
        if !from.can_become(to) {
            return Err(OpError::IllegalTransition);
        }
        self.journal
            .transition(id, from, to, cause, None, self.clock.now())
            .await?;
        if to.is_terminal() {
            self.locks.with(|table| table.release_all_of(id));
        }
        self.publish(id).await;
        Ok(())
    }

    // -----------------------------------------------------------------------------------------
    // Démarrage de l'agent
    // -----------------------------------------------------------------------------------------

    /// Classe chaque opération non terminée au démarrage. À appeler AVANT d'accepter la moindre requête.
    ///
    /// - une opération `needs_attention` déjà classée : son verrou est repris, rien d'autre ;
    /// - les autres : `domain::ops::recover` décide `interrupted` ou `needs_attention` ; les objets créés
    ///   par l'opération pour elle-même sont retirés d'abord (idempotent : un démarrage qui rejoue ce
    ///   nettoyage ne casse rien), puis le statut est posé.
    ///
    /// Aucun geste sur un service ou ses données n'est repris, rejoué ni défait.
    pub async fn recover_at_startup(
        &self,
        observer: &dyn ServiceObserver,
    ) -> Result<RecoverySummary, OpError> {
        let mut summary = RecoverySummary::default();
        for op in self.journal.unfinished().await? {
            if op.status == OpStatus::NeedsAttention {
                self.hold_lock(&op);
                summary.locks_restored += 1;
                continue;
            }
            // Un journal d'étapes illisible n'arrête jamais le démarrage : l'opération est « à vérifier »,
            // son verrou est repris, rien n'est retiré ni rejoué (la trace ne dit que l'identifiant et la
            // colonne fautive, jamais une valeur).
            let recovery = match self.journal.steps(&op.id).await {
                Ok(records) => {
                    let observed = observer.observe(&op, &records).await;
                    recover(op.kind, &op.plan, &records, observed)
                }
                Err(error) => {
                    tracing::error!(op = %op.id, %error, "journal des étapes illisible : opération à vérifier");
                    summary.unreadable += 1;
                    recover_unreadable_journal(op.kind)
                }
            };
            for object in &recovery.cleanup {
                match self.objects.remove(object).await {
                    RemoveOutcome::Removed => summary.objects_removed += 1,
                    RemoveOutcome::AlreadyGone => {}
                    RemoveOutcome::NotOurs | RemoveOutcome::Unverifiable => {
                        summary.objects_left += 1;
                        tracing::warn!(
                            op = %op.id,
                            kind = ?object.kind,
                            "objet créé par une opération interrompue laissé en place : ce n'est plus lui, ou il n'a pas pu être vérifié"
                        );
                    }
                }
            }
            let now = self.clock.now();
            let offer = (!recovery.choices.is_empty()).then(|| DecisionOffer {
                choices: recovery.choices.clone(),
                since: now,
            });
            if let Err(error) = self
                .journal
                .transition(
                    &op.id,
                    op.status,
                    recovery.status,
                    Some(recovery.cause),
                    offer.as_ref(),
                    now,
                )
                .await
            {
                // Le statut n'a pas pu être écrit (base indisponible, ligne changée entre-temps) : l'agent
                // démarre quand même. Une opération à vérifier garde son verrou en mémoire ; l'index unique
                // partiel protège les autres.
                tracing::error!(op = %op.id, %error, "statut de reprise d'une opération non écrit : l'agent démarre");
                summary.unwritten += 1;
            }
            match recovery.status {
                OpStatus::NeedsAttention => {
                    self.hold_lock(&op);
                    summary.needs_attention += 1;
                    tracing::warn!(
                        op = %op.id,
                        kind = op.kind.as_str(),
                        cause = recovery.cause.as_str(),
                        "opération interrompue dans un état à vérifier : le verrou du service est conservé et rien n'est rejoué"
                    );
                }
                _ => {
                    summary.interrupted += 1;
                    if op.kind == OpKind::Deploy && recovery.cause == Cause::NotExecuted {
                        summary.abandoned_deploys.push(op.service.clone());
                    }
                    if op.kind.is_lifecycle() || op.kind == OpKind::Deploy {
                        summary.interrupted_actions.push(InterruptedAction {
                            account: op.account.clone(),
                            service_name: op.service_name.clone(),
                            kind: op.kind,
                        });
                    }
                }
            }
        }
        // La projection `busy` / `last_action` repart du journal tel que la reprise vient de le classer.
        if let Err(error) = self.rebuild_index().await {
            tracing::error!(%error, "projection des opérations non rebâtie : busy et last_action seront vides jusqu'au prochain changement");
        }
        Ok(summary)
    }

    fn hold_lock(&self, op: &OpRecord) {
        let outcome = self
            .locks
            .with(|table| table.restore_held(op.service.as_str(), &op.id, op.kind));
        // Le nom réservé et la place lourde ne sont pas dans le journal : ils se déduisent du genre, comme à
        // l'acceptation. Une suppression « à vérifier » garde son nom réservé après un redémarrage.
        let extra = self.locks.with(|table| -> Result<(), Refusal> {
            if op.kind.reserves_name() {
                table.reserve_name(&fold_name(&op.service_name), &op.id, op.kind)?;
            }
            if op.kind.takes_heavy_io() {
                table.acquire_heavy_io(op.service.as_str(), &op.id)?;
            }
            Ok(())
        });
        if let Err(refusal) = extra {
            tracing::error!(op = %op.id, ?refusal, "nom ou place d'une opération à vérifier impossible à reprendre");
        }
        if let Err(refusal) = outcome {
            // La ceinture de la base (index unique partiel) rend ce cas impossible ; si une base éditée à
            // la main y arrivait, on le dit sans rien détruire.
            tracing::error!(op = %op.id, ?refusal, "verrou d'une opération à vérifier impossible à reprendre");
        }
    }

    /// Balayage périodique : fait expirer les décisions restées sans réponse 15 minutes (vers « annulé »,
    /// BR-JEU-006) et purge les opérations terminées depuis plus de 24 h (jamais une opération à vérifier).
    pub async fn sweep(&self) -> Result<SweepSummary, OpError> {
        let now = self.clock.now();
        let mut summary = SweepSummary::default();
        for op in self
            .journal
            .expired_decisions(now - decision_expiry())
            .await?
        {
            let Some(offer) = &op.decision else { continue };
            if !decision_expired(offer.since, now) {
                continue;
            }
            match self
                .journal
                .transition(
                    &op.id,
                    OpStatus::AwaitingDecision,
                    OpStatus::Cancelled,
                    Some(Cause::DecisionExpired),
                    None,
                    now,
                )
                .await
            {
                Ok(()) => {
                    self.locks.with(|table| table.release_all_of(&op.id));
                    summary.expired += 1;
                    // Ce que l'opération avait créé pour elle-même part avec elle (ADR-0045 : la décision
                    // expirée ne laisse plus d'orphelins), après le changement de statut.
                    if let Ok(steps) = self.journal.steps(&op.id).await {
                        self.clean_after_cancel(&OpSnapshot {
                            record: op.clone(),
                            steps,
                        })
                        .await;
                    }
                    self.publish(&op.id).await;
                }
                // Décidée entre-temps : rien à faire.
                Err(JournalError::Stale) => {}
                Err(error) => return Err(error.into()),
            }
        }
        let retention =
            time::Duration::seconds(i64::try_from(FINISHED_RETENTION_S).unwrap_or(86_400));
        summary.purged = self.journal.purge_finished(now - retention).await?;
        // Le nombre d'opérations gardées est borné, même sous la durée de conservation.
        summary.pruned = self
            .journal
            .prune_finished_beyond(MAX_RETAINED_OPERATIONS)
            .await?;
        Ok(summary)
    }

    /// L'heure, pour les tests de balayage.
    pub fn now(&self) -> OffsetDateTime {
        self.clock.now()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SweepSummary {
    pub expired: u32,
    pub purged: u64,
    /// Opérations terminées effacées parce que le journal en gardait plus que `MAX_RETAINED_OPERATIONS`.
    pub pruned: u64,
}
