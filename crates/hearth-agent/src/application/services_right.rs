//! Le droit « gérer les services » se donne et se retire ICI et nulle part ailleurs (BR-GRANT-005,
//! BR-GRANT-008, BR-GRANT-009, ADR-0041).
//!
//! Appelé par la sous-commande `account services` et par la question de l'installation, jamais par une
//! route : ce service n'est pas dans `app::Services`, que le serveur HTTP reçoit. Le changement et son
//! entrée au journal d'activité (origine « ligne de commande du serveur ») sont écrits dans la MÊME
//! transaction : l'appelant n'annonce un succès qu'après leur validation commune. Un compte déjà dans
//! l'état demandé n'est pas écrit et n'ajoute aucune entrée (BR-GRANT-008).

use std::sync::Arc;

use thiserror::Error;

use super::audit::{AuditTrail, Pending};
use super::ports::{AccountRepo, Clock, Store, StoreError};
use crate::domain::accounts::{ServicesRight, Username, UsernameError};
use crate::domain::audit::{Actor, AuditAction, AuditEvent, Outcome, Target};

/// Ce que la demande a fait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RightChange {
    /// Le droit a changé, l'entrée de journal est écrite.
    Changed,
    /// Le compte était déjà dans l'état demandé : rien n'est écrit, aucune entrée de journal.
    Unchanged,
}

#[derive(Debug, Error)]
pub enum RightError {
    /// Aucun compte de cet identifiant (ou un identifiant qui ne peut pas en être un).
    #[error("Ce compte n'existe pas")]
    NotFound,
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl From<UsernameError> for RightError {
    fn from(_: UsernameError) -> Self {
        // Un identifiant qui ne respecte pas le format ne désigne aucun compte.
        Self::NotFound
    }
}

pub struct ServicesRightService {
    accounts: Arc<dyn AccountRepo>,
    store: Arc<dyn Store>,
    clock: Arc<dyn Clock>,
    trail: Arc<AuditTrail>,
}

impl ServicesRightService {
    pub fn new(
        accounts: Arc<dyn AccountRepo>,
        store: Arc<dyn Store>,
        clock: Arc<dyn Clock>,
        trail: Arc<AuditTrail>,
    ) -> Self {
        Self {
            accounts,
            store,
            clock,
            trail,
        }
    }

    /// L'identifiant canonique d'un compte existant, ou `NotFound`. Ne lit que la base.
    pub async fn existing(&self, username: &str) -> Result<Username, RightError> {
        let username = Username::parse(username)?;
        self.accounts
            .find_by_username(&username)
            .await?
            .map(|account| account.username)
            .ok_or(RightError::NotFound)
    }

    /// Met le droit du compte à `wanted` (BR-GRANT-008). `by` : l'origine, toujours la ligne de
    /// commande du serveur. Le compte est relu dans la transaction : deux commandes simultanées ne
    /// se marchent pas dessus (la dernière écrite fait foi, chacune journalise ce qu'elle a changé).
    pub async fn set(
        &self,
        username: &str,
        wanted: ServicesRight,
        by: &Actor,
    ) -> Result<RightChange, RightError> {
        let username = Username::parse(username)?;
        let mut tx = self.store.begin().await?;
        let account = tx
            .accounts()
            .find_by_username(&username)
            .await?
            .ok_or(RightError::NotFound)?;
        if account.services_right == wanted {
            return Ok(RightChange::Unchanged);
        }
        tx.services_right()
            .set_services_right(&account.id, wanted)
            .await?;
        let action = if wanted.can_manage_services() {
            AuditAction::AccountServicesGrant
        } else {
            AuditAction::AccountServicesRevoke
        };
        let event = AuditEvent::new(
            self.clock.now(),
            by.clone(),
            action,
            Target::Account(account.username.clone()),
            Outcome::Succeeded,
        );
        let mut journal = Pending::default();
        journal.record(&mut *tx, event).await?;
        tx.commit().await?;
        journal.publish(&self.trail);
        Ok(RightChange::Changed)
    }
}
