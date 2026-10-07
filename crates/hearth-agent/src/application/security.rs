//! État de sécurité du compte connecté et alerte « attaque probable » (HRT-24, ADR-0024,
//! BR-TRUST-008).
//!
//! **L'alerte se déduit du compteur par identifiant** (`identifier_slowdown::is_alert`), jamais d'une
//! table d'état : ce service lit, il ne décide de rien. Il consigne deux événements au journal, une
//! fois chacun par épisode (début, fin), et dit au flux qu'« il y a du nouveau ».
//!
//! Qui voit quoi (BR-TRUST-008) : le titulaire de l'identifiant visé voit l'alerte sur le sien, quel
//! que soit son rôle ; un administrateur voit en plus **combien** d'autres comptes sont visés, jamais
//! leurs noms ; un autre compte n'apprend jamais qu'un autre identifiant est visé. Un identifiant qui
//! n'existe pas ne déclenche rien : ni alerte ni entrée au journal.

use std::collections::HashMap;
use std::sync::Arc;

use time::{Duration, OffsetDateTime};

use super::accounts::AccountView;
use super::audit::{AuditTrail, Pending};
use super::ports::{
    AccountRepo, Clock, DeviceRepo, LoginAttemptRepo, SecurityFeed, Store, StoreError,
};
use crate::domain::accounts::Username;
use crate::domain::audit::{
    Actor, AlertPhase, AuditAction, AuditEvent, Origin, Outcome, Reason, Target,
};
use crate::domain::identifier_slowdown::{self, Slowdown};
use crate::domain::lockout::{AttemptKey, retry_after_seconds};
use crate::domain::sessions::SessionId;

/// L'alerte, telle que le compte connecté a le droit de la voir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertState {
    /// L'identifiant du compte connecté est visé en ce moment.
    pub own: bool,
    /// Début de l'épisode, quand `own` est vrai.
    pub since: Option<OffsetDateTime>,
    /// Administrateur seulement : combien d'AUTRES comptes existants sont visés.
    pub others: Option<u32>,
}

/// Ce que le service sait dire d'un compte connecté.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecuritySnapshot {
    pub alert: AlertState,
    /// La session a été ouverte ou prouvée par la clé d'un poste inscrit.
    pub device_proven: bool,
}

pub struct SecurityService {
    attempts: Arc<dyn LoginAttemptRepo>,
    accounts: Arc<dyn AccountRepo>,
    devices: Arc<dyn DeviceRepo>,
    store: Arc<dyn Store>,
    clock: Arc<dyn Clock>,
    trail: Arc<AuditTrail>,
    feed: Arc<dyn SecurityFeed>,
}

impl SecurityService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        attempts: Arc<dyn LoginAttemptRepo>,
        accounts: Arc<dyn AccountRepo>,
        devices: Arc<dyn DeviceRepo>,
        store: Arc<dyn Store>,
        clock: Arc<dyn Clock>,
        trail: Arc<AuditTrail>,
        feed: Arc<dyn SecurityFeed>,
    ) -> Self {
        Self {
            attempts,
            accounts,
            devices,
            store,
            clock,
            trail,
            feed,
        }
    }

    /// Un abonnement aux tics de changement (le flux relit alors son état).
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<()> {
        self.feed.subscribe()
    }

    /// L'état de sécurité de ce compte, tel que son rôle le lui montre.
    pub async fn state_for(
        &self,
        account: &AccountView,
        session: &SessionId,
    ) -> Result<SecuritySnapshot, StoreError> {
        let now = self.clock.now();
        let rows: HashMap<String, Slowdown> = self.attempts.alerting().await?.into_iter().collect();
        let own_key = AttemptKey::identifier(account.username.as_str());
        let own_row = rows.get(own_key.as_str());
        let own = own_row.is_some_and(|row| identifier_slowdown::is_alert(row, now));
        let since = own_row
            .filter(|_| own)
            .and_then(|row| row.alerted_at.or(row.last_failure_at));
        // Le nombre des autres : calculé seulement pour un administrateur, et seulement sur des
        // comptes qui existent (un identifiant inexistant ne compte pas).
        let others = if account.role.can_manage_accounts() {
            let alerting_others = rows
                .iter()
                .filter(|(key, row)| {
                    key.as_str() != own_key.as_str() && identifier_slowdown::is_alert(row, now)
                })
                .count();
            if alerting_others == 0 {
                Some(0)
            } else {
                let existing: Vec<String> = self
                    .accounts
                    .list()
                    .await?
                    .iter()
                    .map(|other| {
                        AttemptKey::identifier(other.username.as_str())
                            .as_str()
                            .to_owned()
                    })
                    .collect();
                let count = rows
                    .iter()
                    .filter(|(key, row)| {
                        key.as_str() != own_key.as_str()
                            && identifier_slowdown::is_alert(row, now)
                            && existing.iter().any(|name| name == *key)
                    })
                    .count();
                Some(u32::try_from(count).unwrap_or(u32::MAX))
            }
        } else {
            None
        };
        let device_proven = self.devices.of_session(session).await?.is_some();
        Ok(SecuritySnapshot {
            alert: AlertState { own, since, others },
            device_proven,
        })
    }

    /// Une attaque probable vient de commencer à viser ce compte : une entrée au journal et un tic
    /// au flux. Appelé une fois par épisode, après la transaction de la tentative qui l'a ouvert
    /// (jamais pour un identifiant qui n'existe pas : l'appelant n'a pas de compte à nommer).
    pub async fn alert_started(
        &self,
        account: &Username,
        origin: Origin,
        wait: Option<Duration>,
    ) -> Result<(), StoreError> {
        let outcome = Outcome::Denied(Reason::TooManyAttempts {
            retry_after_s: wait.map_or(0, retry_after_seconds),
        });
        self.journal(account, origin, AlertPhase::Started, outcome)
            .await
    }

    /// L'épisode précédent est fini parce que le compteur est reparti de zéro.
    pub async fn alert_ended(&self, account: &Username, origin: Origin) -> Result<(), StoreError> {
        self.journal(account, origin, AlertPhase::Ended, Outcome::Succeeded)
            .await
    }

    async fn journal(
        &self,
        account: &Username,
        origin: Origin,
        phase: AlertPhase,
        outcome: Outcome,
    ) -> Result<(), StoreError> {
        let mut tx = self.store.begin().await?;
        let mut journal = Pending::default();
        journal
            .record(
                &mut *tx,
                AuditEvent::new(
                    self.clock.now(),
                    Actor::new(Some(account.clone()), origin),
                    AuditAction::SecurityAlert,
                    Target::Alert(phase),
                    outcome,
                ),
            )
            .await?;
        tx.commit().await?;
        journal.publish(&self.trail);
        self.feed.publish();
        Ok(())
    }

    /// Finit les épisodes dont les conditions ne tiennent plus (30 minutes sans échec) : une entrée
    /// « fin de l'alerte » par épisode et par compte existant, puis un tic au flux. À appeler
    /// régulièrement ; sans effet quand rien n'est fini. Rend le nombre d'épisodes finis.
    pub async fn sweep(&self) -> Result<usize, StoreError> {
        let now = self.clock.now();
        let ended: Vec<(String, OffsetDateTime)> = self
            .attempts
            .alerting()
            .await?
            .into_iter()
            .filter(|(_, row)| identifier_slowdown::ended(row, now))
            .filter_map(|(key, row)| row.alerted_at.map(|since| (key, since)))
            .collect();
        if ended.is_empty() {
            return Ok(0);
        }
        let accounts = self.accounts.list().await?;
        let mut count = 0;
        for (key, since) in ended {
            let mut tx = self.store.begin().await?;
            if !tx.login_attempts().end_alert(&key, since).await? {
                // Un autre passage l'a déjà fini, ou un nouvel épisode a commencé.
                continue;
            }
            let mut journal = Pending::default();
            if let Some(account) = accounts
                .iter()
                .find(|account| AttemptKey::identifier(account.username.as_str()).as_str() == key)
            {
                journal
                    .record(
                        &mut *tx,
                        AuditEvent::new(
                            now,
                            Actor::new(Some(account.username.clone()), Origin::client(None, "")),
                            AuditAction::SecurityAlert,
                            Target::Alert(AlertPhase::Ended),
                            Outcome::Succeeded,
                        ),
                    )
                    .await?;
            }
            tx.commit().await?;
            journal.publish(&self.trail);
            count += 1;
        }
        if count > 0 {
            self.feed.publish();
        }
        Ok(count)
    }
}
