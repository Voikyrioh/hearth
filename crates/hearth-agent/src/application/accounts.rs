//! Cas d'usage des comptes : créer, lister, changer le rôle, définir ou changer un mot de passe,
//! supprimer, fermer les sessions (BR-ACCT-*).
//!
//! Les règles sont celles de `domain::accounts` et `domain::sessions` ; ce module les enchaîne
//! et demande au stockage d'exécuter. Toute opération qui écrit en plusieurs étapes passe par
//! une seule transaction : un échec n'en laisse aucune trace.

use std::sync::Arc;

use thiserror::Error;
use time::OffsetDateTime;

use super::audit::Pending;
use super::ports::{
    AccountRepo, AuditFeed, Clock, HashError, IdGen, PasswordHasher, SessionRepo, Store, StoreError,
};
use crate::domain::accounts::{
    Account, AccountId, ConfirmationMismatch, LastAdminError, PasswordRejected, PlainPassword,
    Role, Username, UsernameError, check_removal, check_role_change, confirm_self_deletion,
};
use crate::domain::audit::{Actor, AuditAction, AuditEvent, Outcome, Target};
use crate::domain::secret::Secret;
use crate::domain::sessions::{
    PasswordChange, SessionId, closure_on_account_deletion, closure_on_password_change,
    closure_on_revocation, is_open,
};

#[derive(Debug, Error)]
pub enum AccountError {
    #[error(transparent)]
    Username(#[from] UsernameError),
    #[error(transparent)]
    WeakPassword(PasswordRejected),
    #[error("Cet identifiant est déjà utilisé")]
    UsernameTaken,
    #[error("Ce compte n'existe pas")]
    NotFound,
    #[error(transparent)]
    LastAdmin(#[from] LastAdminError),
    #[error("L'ancien mot de passe est incorrect")]
    OldPasswordIncorrect,
    #[error("Le mot de passe a été modifié entre-temps, réessaye")]
    PasswordChangedMeanwhile,
    #[error(transparent)]
    SelfDeletion(#[from] ConfirmationMismatch),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Hash(#[from] HashError),
}

/// Ce que les cas d'usage rendent d'un compte : jamais le haché du mot de passe (BR-ACCT-006).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountView {
    pub id: AccountId,
    pub username: Username,
    pub role: Role,
    pub created_at: OffsetDateTime,
    pub password_changed_at: OffsetDateTime,
    pub last_login_at: Option<OffsetDateTime>,
}

impl From<&Account> for AccountView {
    fn from(account: &Account) -> Self {
        Self {
            id: account.id.clone(),
            username: account.username.clone(),
            role: account.role,
            created_at: account.created_at,
            password_changed_at: account.password_changed_at,
            last_login_at: account.last_login_at,
        }
    }
}

/// Un compte et ce que la liste affiche à son sujet.
#[derive(Debug)]
pub struct AccountSummary {
    pub account: AccountView,
    /// Sessions dont l'expiration est dans le futur.
    pub sessions_open: usize,
}

pub struct AccountService {
    accounts: Arc<dyn AccountRepo>,
    sessions: Arc<dyn SessionRepo>,
    store: Arc<dyn Store>,
    hasher: Arc<dyn PasswordHasher>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGen>,
    feed: Arc<dyn AuditFeed>,
}

impl AccountService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        accounts: Arc<dyn AccountRepo>,
        sessions: Arc<dyn SessionRepo>,
        store: Arc<dyn Store>,
        hasher: Arc<dyn PasswordHasher>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGen>,
        feed: Arc<dyn AuditFeed>,
    ) -> Self {
        Self {
            accounts,
            sessions,
            store,
            hasher,
            clock,
            ids,
            feed,
        }
    }

    /// L'entrée du journal d'une action de gestion des comptes réussie (BR-AUDIT-003), écrite
    /// dans la transaction de l'action (BR-ACCT-016).
    fn succeeded(&self, by: &Actor, action: AuditAction, target: Target) -> AuditEvent {
        AuditEvent::new(
            self.clock.now(),
            by.clone(),
            action,
            target,
            Outcome::Succeeded,
        )
    }

    /// BR-ACCT-002 : contrôle le format d'un identifiant sans rien lire ni écrire. Permet de
    /// refuser avant de demander un mot de passe.
    pub fn validate_username(username: &str) -> Result<(), AccountError> {
        Username::parse(username)?;
        Ok(())
    }

    /// BR-ACCT-001 à BR-ACCT-005 : crée un compte. `by` : qui le demande (journal d'activité).
    pub async fn create(
        &self,
        username: &str,
        password: Secret,
        role: Role,
        by: &Actor,
    ) -> Result<AccountView, AccountError> {
        let username = Username::parse(username)?;
        let password =
            PlainPassword::new(password, &username).map_err(AccountError::WeakPassword)?;
        let hash = self.hasher.hash(&password).await?;
        let now = self.clock.now();
        let account = Account {
            id: AccountId::new(self.ids.new_id()),
            username,
            role,
            password_hash: hash,
            created_at: now,
            password_changed_at: now,
            last_login_at: None,
        };

        let mut tx = self.store.begin().await?;
        if tx
            .accounts()
            .find_by_username(&account.username)
            .await?
            .is_some()
        {
            return Err(AccountError::UsernameTaken);
        }
        tx.accounts()
            .insert(&account)
            .await
            .map_err(|error| match error {
                StoreError::Duplicate { .. } => AccountError::UsernameTaken,
                other => AccountError::Store(other),
            })?;
        let mut journal = Pending::default();
        let event = self.succeeded(
            by,
            AuditAction::AccountCreate,
            Target::Account(account.username.clone()),
        );
        journal.record(&mut *tx, event).await?;
        tx.commit().await?;
        journal.publish(&*self.feed);
        Ok(AccountView::from(&account))
    }

    /// Retrouve un compte par l'identifiant saisi (insensible à la casse).
    pub async fn find(&self, username: &str) -> Result<AccountView, AccountError> {
        let username = Username::parse(username)?;
        let account = self
            .accounts
            .find_by_username(&username)
            .await?
            .ok_or(AccountError::NotFound)?;
        Ok(AccountView::from(&account))
    }

    /// L'identifiant d'un compte par son identifiant technique, `None` s'il n'existe pas (ou
    /// plus) : pour nommer la cible d'une action dans le journal.
    pub async fn username_of(&self, id: &AccountId) -> Result<Option<Username>, AccountError> {
        Ok(self
            .accounts
            .find_by_id(id)
            .await?
            .map(|account| account.username))
    }

    /// Les comptes avec leur nombre de sessions ouvertes, du plus ancien au plus récent.
    pub async fn list(&self) -> Result<Vec<AccountSummary>, AccountError> {
        let now = self.clock.now();
        let mut summaries = Vec::new();
        for account in self.accounts.list().await? {
            let expiries = self.sessions.expiries_of(&account.id).await?;
            let sessions_open = expiries
                .into_iter()
                .filter(|&expires_at| is_open(expires_at, now))
                .count();
            summaries.push(AccountSummary {
                account: AccountView::from(&account),
                sessions_open,
            });
        }
        Ok(summaries)
    }

    /// BR-ACCT-007 : change le rôle, sauf pour rétrograder le dernier administrateur.
    pub async fn change_role(
        &self,
        id: &AccountId,
        role: Role,
        by: &Actor,
    ) -> Result<(), AccountError> {
        let mut tx = self.store.begin().await?;
        let account = tx
            .accounts()
            .find(id)
            .await?
            .ok_or(AccountError::NotFound)?;
        let admins = tx.accounts().count_admins().await?;
        check_role_change(account.role, role, admins)?;
        tx.accounts().set_role(id, role).await?;
        let mut journal = Pending::default();
        let event = self.succeeded(
            by,
            AuditAction::AccountRole,
            Target::AccountRole(account.username.clone(), role),
        );
        journal.record(&mut *tx, event).await?;
        tx.commit().await?;
        journal.publish(&*self.feed);
        Ok(())
    }

    /// BR-ACCT-008 : définit le mot de passe d'un compte (administrateur ou ligne de commande)
    /// et ferme toutes ses sessions. Rend le nombre de sessions fermées.
    pub async fn set_password(
        &self,
        id: &AccountId,
        password: Secret,
        by: &Actor,
    ) -> Result<u64, AccountError> {
        let account = self.require(id).await?;
        let hash = self.hash_for(&account, password).await?;
        self.apply_password(
            id,
            &hash,
            PasswordChange::ByAdmin,
            None,
            (by, AuditAction::AccountPassword),
        )
        .await
    }

    /// BR-ACCT-009 : le titulaire change son mot de passe. Vérifie l'ancien, ferme les autres
    /// sessions et garde `current_session`. Rend le nombre de sessions fermées.
    pub async fn change_own_password(
        &self,
        id: &AccountId,
        old_password: Secret,
        new_password: Secret,
        current_session: Option<SessionId>,
        by: &Actor,
    ) -> Result<u64, AccountError> {
        let account = self.require(id).await?;
        if !self
            .hasher
            .verify(&old_password, &account.password_hash)
            .await?
        {
            return Err(AccountError::OldPasswordIncorrect);
        }
        let hash = self.hash_for(&account, new_password).await?;
        self.apply_password(
            id,
            &hash,
            PasswordChange::Own { current_session },
            Some(&account.password_hash),
            (by, AuditAction::OwnPassword),
        )
        .await
    }

    /// BR-ACCT-007, BR-ACCT-010, BR-ACCT-012 : supprime un compte et ferme ses sessions.
    /// `acting` est le compte qui demande la suppression (absent en ligne de commande) ; s'il
    /// supprime son propre compte, `confirmation` doit être son identifiant retapé.
    /// Rend le nombre de sessions fermées.
    pub async fn delete(
        &self,
        id: &AccountId,
        acting: Option<&AccountId>,
        confirmation: Option<&str>,
        by: &Actor,
    ) -> Result<u64, AccountError> {
        let mut tx = self.store.begin().await?;
        let account = tx
            .accounts()
            .find(id)
            .await?
            .ok_or(AccountError::NotFound)?;
        if acting == Some(id) {
            confirm_self_deletion(&account.username, confirmation.unwrap_or(""))?;
        }
        let admins = tx.accounts().count_admins().await?;
        check_removal(account.role, admins)?;
        let closed = tx
            .sessions()
            .close(id, &closure_on_account_deletion(), self.clock.now())
            .await?;
        tx.accounts().delete(id).await?;
        let mut journal = Pending::default();
        let event = self.succeeded(
            by,
            AuditAction::AccountDelete,
            Target::Account(account.username.clone()),
        );
        journal.record(&mut *tx, event).await?;
        tx.commit().await?;
        journal.publish(&*self.feed);
        Ok(closed)
    }

    /// BR-ACCT-011 : ferme toutes les sessions du compte sans toucher au mot de passe.
    /// Rend le nombre de sessions fermées.
    pub async fn revoke_sessions(&self, id: &AccountId, by: &Actor) -> Result<u64, AccountError> {
        let mut tx = self.store.begin().await?;
        let account = tx
            .accounts()
            .find(id)
            .await?
            .ok_or(AccountError::NotFound)?;
        let closed = tx
            .sessions()
            .close(id, &closure_on_revocation(), self.clock.now())
            .await?;
        let mut journal = Pending::default();
        let event = self.succeeded(
            by,
            AuditAction::SessionsRevoke,
            Target::Account(account.username.clone()),
        );
        journal.record(&mut *tx, event).await?;
        tx.commit().await?;
        journal.publish(&*self.feed);
        Ok(closed)
    }

    async fn require(&self, id: &AccountId) -> Result<Account, AccountError> {
        self.accounts
            .find_by_id(id)
            .await?
            .ok_or(AccountError::NotFound)
    }

    async fn hash_for(&self, account: &Account, password: Secret) -> Result<Secret, AccountError> {
        let password =
            PlainPassword::new(password, &account.username).map_err(AccountError::WeakPassword)?;
        Ok(self.hasher.hash(&password).await?)
    }

    /// Change le haché et ferme les sessions dans la même transaction.
    async fn apply_password(
        &self,
        id: &AccountId,
        hash: &Secret,
        change: PasswordChange,
        verified_hash: Option<&Secret>,
        (by, action): (&Actor, AuditAction),
    ) -> Result<u64, AccountError> {
        let mut tx = self.store.begin().await?;
        let current = tx
            .accounts()
            .find(id)
            .await?
            .ok_or(AccountError::NotFound)?;
        // Ce qui a été vérifié doit être ce qu'on remplace : si le mot de passe a changé depuis,
        // on refuse plutôt que d'écraser un changement fait entre-temps.
        if let Some(verified) = verified_hash
            && current.password_hash.expose() != verified.expose()
        {
            return Err(AccountError::PasswordChangedMeanwhile);
        }
        let now = self.clock.now();
        tx.accounts().set_password(id, hash, now).await?;
        let closed = tx
            .sessions()
            .close(id, &closure_on_password_change(change), now)
            .await?;
        let mut journal = Pending::default();
        let event = self.succeeded(by, action, Target::Account(current.username.clone()));
        journal.record(&mut *tx, event).await?;
        tx.commit().await?;
        journal.publish(&*self.feed);
        Ok(closed)
    }
}
