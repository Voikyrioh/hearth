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

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use thiserror::Error;
use time::OffsetDateTime;

use super::ports::{
    Clock, CreatedObjects, DecisionOffer, IdGen, JournalError, NewOp, OpJournal, OpRecord,
    Progress, RemoveOutcome, ServiceObserver, StoreError,
};
use crate::domain::ops::{
    CancelVerdict, Cause, Choice, ChoiceRefused, LockTable, OpKind, OpPlan, OpStatus, Refusal,
    StepName, StepState, UndoInfo, apply_choice, cancel_verdict, decision_expired, decision_expiry,
    latest_state, recover, recover_unreadable_journal,
};
use crate::domain::services::key::ServiceKey;
use crate::domain::services::spec::fold_name;
use hearth_proto::api::service_ops::FINISHED_RETENTION_S;

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
    Unknown,
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
            JournalError::Unknown => Self::Unknown,
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

/// Ce que le démarrage a fait de chaque opération non terminée.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecoverySummary {
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
}

impl ServiceOps {
    pub fn new(
        journal: Arc<dyn OpJournal>,
        locks: Arc<OpLocks>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGen>,
        objects: Arc<dyn CreatedObjects>,
    ) -> Self {
        Self {
            journal,
            locks,
            clock,
            ids,
            objects,
        }
    }

    pub fn locks(&self) -> &Arc<OpLocks> {
        &self.locks
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
        Ok(self
            .journal
            .append_step(id, &name, StepState::Intent, undo, self.clock.now(), true)
            .await?)
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
        Ok(self
            .journal
            .append_step(id, &name, state, undo, self.clock.now(), false)
            .await?)
    }

    async fn step_of(&self, id: &str, step: &str) -> Result<(OpRecord, StepName, bool), OpError> {
        let op = self.journal.get(id).await?.ok_or(OpError::Unknown)?;
        let name = StepName::parse(step).ok_or_else(|| OpError::UnknownStep(step.to_owned()))?;
        let planned = op
            .plan
            .step(&name)
            .ok_or_else(|| OpError::UnknownStep(step.to_owned()))?;
        let destructive = planned.destructive;
        Ok((op, name, destructive))
    }

    pub async fn set_progress(&self, id: &str, progress: &Progress) -> Result<(), OpError> {
        Ok(self
            .journal
            .set_progress(id, progress, self.clock.now())
            .await?)
    }

    /// Clôt l'opération dans un statut terminal et rend ses verrous.
    pub async fn finish(
        &self,
        id: &str,
        to: OpStatus,
        cause: Option<Cause>,
    ) -> Result<(), OpError> {
        if !to.is_terminal() {
            return Err(OpError::IllegalTransition);
        }
        let op = self.journal.get(id).await?.ok_or(OpError::Unknown)?;
        if !op.status.can_become(to) {
            return Err(OpError::IllegalTransition);
        }
        self.move_to(id, op.status, to, cause).await
    }

    /// Demande une décision au compte : `running` → `awaiting_decision` avec les choix offerts.
    pub async fn ask_decision(&self, id: &str, choices: &[Choice]) -> Result<(), OpError> {
        if choices.is_empty() {
            return Err(OpError::Choice(ChoiceRefused::NotOffered));
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
        Ok(())
    }

    /// Applique le choix d'un compte à une opération qui attend. La vérification du DROIT du compte est
    /// celle de la route qui appellera ceci (HRT-64 et suivants).
    pub async fn decide(&self, id: &str, choice: Choice) -> Result<OpStatus, OpError> {
        let op = self.journal.get(id).await?.ok_or(OpError::Unknown)?;
        let offered = op
            .decision
            .as_ref()
            .map(|offer| offer.choices.as_slice())
            .unwrap_or_default();
        let (next, cause) = apply_choice(op.status, offered, choice).map_err(OpError::Choice)?;
        let cause = (next != OpStatus::Running).then_some(cause);
        self.move_to(id, op.status, next, cause).await?;
        Ok(next)
    }

    /// Demande d'annulation (acte `OperationCancel`, ajouté avec sa route). Avant le point de non-retour du
    /// genre ; après, `TooLate`.
    pub async fn cancel(&self, id: &str) -> Result<CancelOutcome, OpError> {
        let op = self.journal.get(id).await?.ok_or(OpError::Unknown)?;
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
                    let observed = observer.observe(&op).await;
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
                _ => summary.interrupted += 1,
            }
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
                }
                // Décidée entre-temps : rien à faire.
                Err(JournalError::Stale) => {}
                Err(error) => return Err(error.into()),
            }
        }
        let retention =
            time::Duration::seconds(i64::try_from(FINISHED_RETENTION_S).unwrap_or(86_400));
        summary.purged = self.journal.purge_finished(now - retention).await?;
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
}
