//! Lire, annuler et décider une opération de service, pour un compte (HRT-144, ADR-0045, BR-LIFE-013,
//! BR-LIFE-020, BR-EDIT-031, BR-JEU-006).
//!
//! Ce module juge ce que le journal des opérations (`ServiceOps`) ne juge pas :
//!
//! - **qui voit quoi** : une opération n'est lisible que par qui peut voir son service
//!   (`domain::ops::may_see`). Pour les autres elle N'EXISTE PAS : même réponse qu'une opération inconnue.
//!   Le droit « gérer les services » de la route d'annulation et de décision est jugé avant, par la couche
//!   d'accès (`Access::Services`) ; la confirmation d'acte (preuve de clé, mot de passe) par la couche
//!   `reauth` ;
//! - **l'annulation** : avant le point de non-retour du genre seulement (`TooLate` sinon), IDEMPOTENTE
//!   (annuler une opération déjà annulée par un compte rend la même issue sans nouvelle entrée de journal) ;
//! - **la décision** : parmi les choix que l'opération offre, et seulement tant que la décision n'a pas
//!   expiré (15 minutes) ;
//! - **la trace** : le changement de statut et l'entrée du journal d'activité (sous l'action du GENRE de
//!   l'opération, avec l'auteur) s'écrivent dans la MÊME transaction (`ServiceOpTx`), comme l'adoption. La
//!   condition de statut, le point de non-retour et l'écriture ne se séparent donc pas, et l'écriture d'un
//!   `intent` par l'exécuteur (`StepGuard`) se sérialise avec elles : aucune fenêtre entre « annulable » et
//!   « annulée ».
//!
//! Rien ici ne rejoue, ne reprend ni ne défait un geste sur un service. Seuls les objets que l'opération
//! avait créés pour elle-même partent, après le changement de statut (`ServiceOps::clean_after_cancel`).

use std::sync::Arc;

use thiserror::Error;
use tokio::sync::broadcast;

use super::audit::{AuditTrail, Pending};
use super::ports::{Clock, JournalError, Store, StoreError};
use super::service_ops::{OpSnapshot, ServiceOps};
use super::services::{AdoptionLookup, ServicesRegistry};
use crate::domain::audit::{Actor, AuditEvent, ClientName, OperationGesture, Outcome, Target};
use crate::domain::ops::{
    CancelVerdict, Cause, Choice, ChoiceRefused, OpStatus, ServiceOrigin, apply_choice,
    cancel_verdict, decision_expired, may_see, origin_of,
};
use crate::domain::services::ServiceKey;

/// Qui regarde : le seul fait du compte qui change ce qu'il voit d'une opération.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewer {
    /// Le compte a le droit « gérer les services » (ADR-0041), quel que soit son rôle.
    pub may_manage_services: bool,
}

/// Pourquoi une décision est refusée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionRefusal {
    /// L'opération n'attend aucune décision.
    NotWaiting,
    /// Ce choix n'est pas parmi ceux que l'opération offre.
    NotOffered,
    /// Ce choix ne s'applique pas à son statut.
    NotApplicable,
    /// La décision attendait depuis 15 minutes ou plus : elle est expirée.
    Expired,
}

#[derive(Debug, Error)]
pub enum AccessError {
    /// Opération inconnue, purgée, ou sur un service que ce compte ne peut pas voir : on ne distingue pas.
    #[error("opération inconnue")]
    NotFound,
    /// Le point de non-retour est passé.
    #[error("trop tard pour annuler")]
    TooLate,
    /// L'opération n'est pas dans un statut qui s'annule.
    #[error("l'opération ne s'annule pas dans ce statut")]
    NotCancellable(OpStatus),
    #[error("décision refusée")]
    Decision(DecisionRefusal),
    /// L'opération a bougé pendant la demande : à relire.
    #[error("l'opération a changé")]
    Changed,
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl From<JournalError> for AccessError {
    fn from(error: JournalError) -> Self {
        match error {
            JournalError::Unknown => Self::NotFound,
            JournalError::Stale | JournalError::ServiceBusy => Self::Changed,
            JournalError::Store(error) => Self::Store(error),
        }
    }
}

impl From<super::service_ops::OpError> for AccessError {
    fn from(error: super::service_ops::OpError) -> Self {
        use super::service_ops::OpError;
        match error {
            OpError::Unknown => Self::NotFound,
            OpError::Storage(message) => Self::Store(StoreError::Unavailable {
                resource: "service_ops",
                message,
            }),
            _ => Self::Changed,
        }
    }
}

/// Ce que l'annulation a fait.
#[derive(Debug, Clone)]
pub enum CancelDone {
    /// L'opération est annulée.
    Cancelled(OpSnapshot),
    /// L'opération court : elle passe en `cancelling`, son exécuteur la clôt.
    Cancelling(OpSnapshot),
    /// Une demande d'annulation avait déjà été faite : rien de nouveau, rien de consigné.
    Already(OpSnapshot),
}

pub struct ServiceOpAccess {
    ops: Arc<ServiceOps>,
    registry: Arc<ServicesRegistry>,
    store: Arc<dyn Store>,
    clock: Arc<dyn Clock>,
    trail: Arc<AuditTrail>,
}

impl ServiceOpAccess {
    pub fn new(
        ops: Arc<ServiceOps>,
        registry: Arc<ServicesRegistry>,
        store: Arc<dyn Store>,
        clock: Arc<dyn Clock>,
        trail: Arc<AuditTrail>,
    ) -> Self {
        Self {
            ops,
            registry,
            store,
            clock,
            trail,
        }
    }

    pub fn ops(&self) -> &Arc<ServiceOps> {
        &self.ops
    }

    /// Ce compte voit-il l'opération de ce service ? Un seul point de décision pour la lecture, le flux, la
    /// décision et l'annulation.
    pub fn can_see(&self, service: &ServiceKey, viewer: Viewer) -> bool {
        let adopted = matches!(
            self.registry.adoption_lookup(service),
            AdoptionLookup::Adopted { .. }
        );
        may_see(origin_of(service, adopted), viewer.may_manage_services)
    }

    /// L'origine du service d'une opération, pour ceux qui jugent plusieurs fois de suite.
    pub fn origin(&self, service: &ServiceKey) -> ServiceOrigin {
        let adopted = matches!(
            self.registry.adoption_lookup(service),
            AdoptionLookup::Adopted { .. }
        );
        origin_of(service, adopted)
    }

    /// `GET /operations/services/{id}` : l'opération et son journal d'étapes, pour qui voit le service.
    pub async fn read(&self, id: &str, viewer: Viewer) -> Result<OpSnapshot, AccessError> {
        let snapshot = self.ops.snapshot(id).await?.ok_or(AccessError::NotFound)?;
        if !self.can_see(&snapshot.record.service, viewer) {
            return Err(AccessError::NotFound);
        }
        Ok(snapshot)
    }

    /// Les opérations en cours que ce compte voit : ce que le flux envoie à l'abonnement.
    pub async fn open_for(&self, viewer: Viewer) -> Result<Vec<OpSnapshot>, AccessError> {
        Ok(self
            .ops
            .open_snapshots()
            .await?
            .into_iter()
            .filter(|snapshot| self.can_see(&snapshot.record.service, viewer))
            .collect())
    }

    /// Les changements d'opération à partir de maintenant, TOUS (le flux filtre par abonné).
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<OpSnapshot>> {
        self.ops.subscribe()
    }

    fn audit_event(
        &self,
        actor: &Actor,
        snapshot: &OpSnapshot,
        gesture: OperationGesture,
    ) -> AuditEvent {
        let target =
            ClientName::parse(&snapshot.record.service_name).map_or(Target::None, |service| {
                Target::ServiceOperation { service, gesture }
            });
        AuditEvent::new(
            self.clock.now(),
            actor.clone(),
            snapshot.record.kind.audit_action(),
            target,
            Outcome::Succeeded,
        )
    }

    /// `DELETE /operations/services/{id}` : annule l'opération. Voir la description du module.
    pub async fn cancel(
        &self,
        id: &str,
        actor: &Actor,
        viewer: Viewer,
    ) -> Result<CancelDone, AccessError> {
        // La visibilité d'abord : une opération qu'on ne voit pas n'existe pas, quel que soit son état.
        self.read(id, viewer).await?;
        let mut tx = self.store.begin().await?;
        let record = tx
            .service_ops()
            .get(id)
            .await?
            .ok_or(AccessError::NotFound)?;
        let steps = tx.service_ops().steps(id).await?;
        let before = OpSnapshot { record, steps };
        match cancel_verdict(before.record.status, &before.record.plan, &before.steps) {
            CancelVerdict::TooLate => return Err(AccessError::TooLate),
            CancelVerdict::NotCancellable => {
                // Idempotence : une opération déjà annulée par un compte répond comme à la première demande.
                return if matches!(
                    before.record.status,
                    OpStatus::Cancelling | OpStatus::Cancelled
                ) && before.record.cause == Some(Cause::CancelledByAccount)
                {
                    Ok(CancelDone::Already(before))
                } else {
                    Err(AccessError::NotCancellable(before.record.status))
                };
            }
            CancelVerdict::Cancel => {}
        }
        let to = if before.record.status == OpStatus::Running {
            OpStatus::Cancelling
        } else {
            OpStatus::Cancelled
        };
        tx.service_ops()
            .transition(
                id,
                before.record.status,
                to,
                Some(Cause::CancelledByAccount),
                None,
                self.clock.now(),
            )
            .await?;
        let mut journal = Pending::default();
        journal
            .record(
                &mut *tx,
                self.audit_event(actor, &before, OperationGesture::Cancelled),
            )
            .await?;
        tx.commit().await?;
        journal.publish(&self.trail);
        self.ops.settle(id).await;
        let after = self.ops.snapshot(id).await?.ok_or(AccessError::NotFound)?;
        if to == OpStatus::Cancelled {
            self.ops.clean_after_cancel(&before).await;
            Ok(CancelDone::Cancelled(after))
        } else {
            Ok(CancelDone::Cancelling(after))
        }
    }

    /// `POST /operations/services/{id}/decision` : applique le choix du compte à une opération qui attend.
    pub async fn decide(
        &self,
        id: &str,
        choice: Choice,
        actor: &Actor,
        viewer: Viewer,
    ) -> Result<OpSnapshot, AccessError> {
        self.read(id, viewer).await?;
        let mut tx = self.store.begin().await?;
        let record = tx
            .service_ops()
            .get(id)
            .await?
            .ok_or(AccessError::NotFound)?;
        let steps = tx.service_ops().steps(id).await?;
        let before = OpSnapshot { record, steps };
        let now = self.clock.now();
        let offered = before
            .record
            .decision
            .as_ref()
            .map(|offer| offer.choices.as_slice())
            .unwrap_or_default();
        // Une décision sans réponse depuis 15 minutes est expirée, que le balayage soit passé ou non
        // (BR-JEU-006). Une opération `needs_attention` n'expire jamais.
        if before.record.status == OpStatus::AwaitingDecision
            && before
                .record
                .decision
                .as_ref()
                .is_some_and(|offer| decision_expired(offer.since, now))
        {
            return Err(AccessError::Decision(DecisionRefusal::Expired));
        }
        let (next, cause) =
            apply_choice(before.record.status, offered, choice).map_err(|refusal| {
                AccessError::Decision(match refusal {
                    ChoiceRefused::NotWaiting => DecisionRefusal::NotWaiting,
                    ChoiceRefused::NotOffered => DecisionRefusal::NotOffered,
                    ChoiceRefused::NotApplicable => DecisionRefusal::NotApplicable,
                })
            })?;
        let cause = (next != OpStatus::Running).then_some(cause);
        tx.service_ops()
            .transition(id, before.record.status, next, cause, None, now)
            .await?;
        let mut journal = Pending::default();
        journal
            .record(
                &mut *tx,
                self.audit_event(
                    actor,
                    &before,
                    OperationGesture::Decided(choice.audit_label()),
                ),
            )
            .await?;
        tx.commit().await?;
        journal.publish(&self.trail);
        self.ops.settle(id).await;
        if next == OpStatus::Cancelled {
            self.ops.clean_after_cancel(&before).await;
        }
        self.ops.snapshot(id).await?.ok_or(AccessError::NotFound)
    }
}
