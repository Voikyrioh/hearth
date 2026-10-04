//! Cas d'usage des sessions : connexion (avec verrouillage), reconnaissance d'un jeton présenté
//! (expiration glissante), déconnexion (BR-CONN-006, 007, 013 ; BR-RESIL-012, 014).
//!
//! Les règles sont celles de `domain::{lockout, sessions, session_token}` ; ce module les
//! enchaîne et demande au stockage d'exécuter.

use std::sync::Arc;

use thiserror::Error;
use time::{Duration, OffsetDateTime};

use super::accounts::AccountView;
use super::ports::{
    AccountRepo, Clock, HashError, IdGen, LoginAttemptRepo, PasswordHasher, SessionRepo, Store,
    StoreError, TokenGen, TokenGenError,
};
use crate::domain::accounts::Username;
use crate::domain::lockout::{AttemptKey, LockoutDecision, LockoutEvent, step};
use crate::domain::secret::Secret;
use crate::domain::session_token::SessionToken;
use crate::domain::sessions::{Session, SessionEnd, SessionId, check, expiry_from, renewed_expiry};

/// Qui se connecte : le poste (`X-Hearth-Client`) et l'adresse de la connexion.
#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub name: String,
    /// Adresse IP du client, telle que vue par l'agent (jamais lue d'un en-tête).
    pub addr: String,
}

/// Connexion réussie. Le jeton n'existe en clair que dans ce résultat.
#[derive(Debug)]
pub struct LoginOutcome {
    pub token: SessionToken,
    pub session_id: SessionId,
    pub expires_at: OffsetDateTime,
    pub account: AccountView,
}

#[derive(Debug, Error)]
pub enum LoginError {
    /// Identifiant inconnu ou mot de passe faux : volontairement indiscernables (BR-CONN-013).
    #[error("Identifiant ou mot de passe incorrect")]
    InvalidCredentials,
    #[error("Trop de tentatives, attends avant de réessayer")]
    TooManyAttempts { retry_after: Duration },
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Hash(#[from] HashError),
    #[error(transparent)]
    Token(#[from] TokenGenError),
}

#[derive(Debug, Error)]
pub enum AuthError {
    /// Jeton absent de la requête ou illisible.
    #[error("Jeton de session absent ou illisible")]
    Malformed,
    /// La session n'est plus utilisable : expirée, ou fermée par l'administration.
    #[error("La session n'est plus valable")]
    Ended(SessionEnd),
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Ce que l'on sait de la session qui a présenté le jeton.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentSession {
    pub account: AccountView,
    pub session_id: SessionId,
    pub expires_at: OffsetDateTime,
}

pub struct SessionService {
    accounts: Arc<dyn AccountRepo>,
    sessions: Arc<dyn SessionRepo>,
    attempts: Arc<dyn LoginAttemptRepo>,
    store: Arc<dyn Store>,
    hasher: Arc<dyn PasswordHasher>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGen>,
    tokens: Arc<dyn TokenGen>,
}

impl SessionService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        accounts: Arc<dyn AccountRepo>,
        sessions: Arc<dyn SessionRepo>,
        attempts: Arc<dyn LoginAttemptRepo>,
        store: Arc<dyn Store>,
        hasher: Arc<dyn PasswordHasher>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGen>,
        tokens: Arc<dyn TokenGen>,
    ) -> Self {
        Self {
            accounts,
            sessions,
            attempts,
            store,
            hasher,
            clock,
            ids,
            tokens,
        }
    }

    /// Ouvre une session. Identifiant inconnu et mot de passe faux suivent exactement le même
    /// chemin : une vérification Argon2 (contre un haché factice si le compte n'existe pas), un
    /// échec compté, une transaction (BR-CONN-013).
    pub async fn login(
        &self,
        username: &str,
        password: Secret,
        client: &ClientInfo,
    ) -> Result<LoginOutcome, LoginError> {
        let key = AttemptKey::new(username, &client.addr);

        // 1. Admission : pendant une attente, on ne vérifie même pas le mot de passe.
        let state = self.attempts.get(&key).await?;
        if let (_, LockoutDecision::Blocked { retry_after }) =
            step(state, LockoutEvent::Attempt, self.clock.now())
        {
            return Err(LoginError::TooManyAttempts { retry_after });
        }

        // 2. Vérification, hors transaction (Argon2 est lent et ne doit pas tenir le verrou
        //    d'écriture). Un identifiant illisible est traité comme un identifiant inconnu.
        let account = match Username::parse(username) {
            Ok(username) => self.accounts.find_by_username(&username).await?,
            Err(_) => None,
        };
        let hash = account
            .as_ref()
            .map_or_else(|| self.hasher.decoy_hash(), |found| &found.password_hash);
        let verified = self.hasher.verify(&password, hash).await?;
        let account = account.filter(|_| verified);

        // 3. Issue, dans une seule transaction : compteur, et pour un succès la session et la
        //    date de dernière connexion. L'état est relu dans la transaction : des tentatives
        //    concurrentes ne peuvent pas dépasser le palier.
        let mut tx = self.store.begin().await?;
        let now = self.clock.now();
        let state = tx.login_attempts().get(&key).await?;
        if let (_, LockoutDecision::Blocked { retry_after }) =
            step(state, LockoutEvent::Attempt, now)
        {
            return Err(LoginError::TooManyAttempts { retry_after });
        }
        let Some(account) = account else {
            let (next, decision) = step(state, LockoutEvent::Failed, now);
            tx.login_attempts().save(&key, &next, now).await?;
            tx.commit().await?;
            return Err(match decision {
                LockoutDecision::Blocked { retry_after } => {
                    LoginError::TooManyAttempts { retry_after }
                }
                LockoutDecision::Allowed => LoginError::InvalidCredentials,
            });
        };

        let (reset, _) = step(state, LockoutEvent::Succeeded, now);
        tx.login_attempts().save(&key, &reset, now).await?;
        let token = self.tokens.generate()?;
        let session = Session {
            id: SessionId::new(self.ids.new_id()),
            account: account.id.clone(),
            token_hash: token.hash(),
            client_name: client.name.clone(),
            client_addr: client.addr.clone(),
            created_at: now,
            last_seen_at: now,
            expires_at: expiry_from(now),
        };
        tx.sessions().insert(&session).await?;
        tx.accounts().record_login(&account.id, now).await?;
        tx.commit().await?;

        let mut view = AccountView::from(&account);
        view.last_login_at = Some(now);
        Ok(LoginOutcome {
            token,
            session_id: session.id,
            expires_at: session.expires_at,
            account: view,
        })
    }

    /// Reconnaît la session du jeton présenté et repousse son expiration (expiration
    /// glissante, au plus une écriture toutes les `RENEWAL_INTERVAL`).
    pub async fn authenticate(&self, token: &str) -> Result<CurrentSession, AuthError> {
        let token = SessionToken::parse(token).map_err(|_| AuthError::Malformed)?;
        let hash = token.hash();
        let now = self.clock.now();
        let found = self.sessions.find_by_token_hash(&hash).await?;
        let revoked = match found {
            Some(_) => false,
            None => self.sessions.is_revoked(&hash).await?,
        };
        let session = check(found.as_ref(), revoked, now).map_err(AuthError::Ended)?;
        let account = self
            .accounts
            .find_by_id(&session.account)
            .await?
            .ok_or(AuthError::Ended(SessionEnd::Revoked))?;

        let expires_at = match renewed_expiry(session.last_seen_at, now) {
            Some(expires_at) => {
                let mut tx = self.store.begin().await?;
                tx.sessions().renew(&session.id, now, expires_at).await?;
                tx.commit().await?;
                expires_at
            }
            None => session.expires_at,
        };
        Ok(CurrentSession {
            account: AccountView::from(&account),
            session_id: session.id.clone(),
            expires_at,
        })
    }

    /// Déconnexion explicite : supprime la session courante.
    pub async fn logout(&self, session: &SessionId) -> Result<(), StoreError> {
        let mut tx = self.store.begin().await?;
        tx.sessions().delete(session).await?;
        tx.commit().await
    }
}
