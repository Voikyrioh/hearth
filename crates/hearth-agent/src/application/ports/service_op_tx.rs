//! Le journal des opérations de service DANS l'unité de travail (HRT-144, ADR-0045) : de quoi lire une
//! opération, la juger et changer son statut dans la MÊME transaction que l'entrée du journal d'activité
//! de l'acte. Annuler et décider sont les deux seuls appelants (`ServiceOpAccess`) : pas d'annulation
//! sans trace, pas de trace d'une annulation qui n'a pas eu lieu.
//!
//! Les écritures de l'exécuteur d'une opération (étapes, progression, fin) passent par `OpJournal`, sur le
//! bassin : SQLite sérialise les deux voies, et l'écriture d'une ligne du journal d'étapes vérifie le
//! statut dans sa propre instruction (`StepGuard`).

use async_trait::async_trait;
use time::OffsetDateTime;

use super::{DecisionOffer, JournalError, OpRecord, StoreError};
use crate::domain::ops::{Cause, OpStatus, StepRecord};

#[async_trait]
pub trait ServiceOpTx: Send {
    /// Une opération, telle qu'elle est dans cette transaction.
    async fn get(&mut self, id: &str) -> Result<Option<OpRecord>, StoreError>;

    /// Son journal d'étapes. Une ligne illisible est une erreur, jamais omise.
    async fn steps(&mut self, id: &str) -> Result<Vec<StepRecord>, StoreError>;

    /// Change le statut si et seulement s'il vaut `from` (comparaison-et-échange), comme
    /// `OpJournal::transition`.
    async fn transition(
        &mut self,
        id: &str,
        from: OpStatus,
        to: OpStatus,
        cause: Option<Cause>,
        decision: Option<&DecisionOffer>,
        at: OffsetDateTime,
    ) -> Result<(), JournalError>;

    /// Pose, après coup, une cause sur une réussite qui n'en a pas, comme `OpJournal::annotate_cause`.
    /// `false` : rien n'a changé.
    async fn annotate_cause(
        &mut self,
        id: &str,
        cause: Cause,
        at: OffsetDateTime,
    ) -> Result<bool, JournalError>;
}
