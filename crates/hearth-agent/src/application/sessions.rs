//! Cas d'usage des sessions : connexion (avec verrouillage), reconnaissance d'un jeton présenté
//! (expiration glissante), déconnexion (BR-CONN-006, 007, 013 ; BR-RESIL-012, 014).
//!
//! Les règles sont celles de `domain::{lockout, sessions, session_token}` ; ce module les
//! enchaîne et demande au stockage d'exécuter.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use thiserror::Error;
use time::{Duration, OffsetDateTime};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

use super::accounts::AccountView;
use super::audit::Pending;
use super::ports::{
    AccountRepo, AuditFeed, Clock, HashError, IdGen, LoginAttemptRepo, PasswordHasher, SessionRepo,
    Store, StoreError, TokenGen, TokenGenError,
};
use crate::domain::accounts::Username;
use crate::domain::audit::{Actor, AuditAction, AuditEvent, Origin, Outcome, Reason, Target};
use crate::domain::lockout::{
    AttemptKey, LockoutDecision, LockoutEvent, LockoutState, admits_in_queue, retry_after_seconds,
    step, step_address,
};
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
    /// Trop de connexions en attente pour cette adresse : refus immédiat, sans compter d'échec.
    #[error("Trop de connexions en attente pour cette adresse")]
    Busy,
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
    feed: Arc<dyn AuditFeed>,
    turns: Turns,
}

/// Tours de parole par adresse : une seule connexion à la fois pour une même adresse, et une file
/// d'attente bornée (`domain::lockout::admits_in_queue`).
#[derive(Default)]
struct Turns(Mutex<HashMap<String, Slot>>);

/// Une adresse : son verrou, et combien de connexions elle a d'admises (en cours et en attente).
#[derive(Default)]
struct Slot {
    mutex: Arc<AsyncMutex<()>>,
    in_flight: usize,
}

/// Place dans la file d'une adresse : rendue (et l'entrée oubliée si plus personne n'attend) à
/// l'abandon, même si l'appelant est annulé pendant l'attente.
struct Turn<'a> {
    turns: &'a Turns,
    key: String,
    guard: Option<OwnedMutexGuard<()>>,
}

impl Turns {
    /// Prend place dans la file de `key` et attend son tour ; `None` si la file de l'adresse est
    /// pleine (refus immédiat).
    async fn lock(&self, key: &str) -> Option<Turn<'_>> {
        let mutex = {
            let mut map = self.0.lock().unwrap_or_else(PoisonError::into_inner);
            let slot = map.entry(key.to_owned()).or_default();
            if !admits_in_queue(slot.in_flight) {
                return None;
            }
            slot.in_flight += 1;
            slot.mutex.clone()
        };
        let mut turn = Turn {
            turns: self,
            key: key.to_owned(),
            guard: None,
        };
        turn.guard = Some(mutex.lock_owned().await);
        Some(turn)
    }
}

impl Drop for Turn<'_> {
    fn drop(&mut self) {
        // Rend le tour, puis la place ; oublie l'entrée si plus personne ne l'attend.
        self.guard.take();
        let mut map = self.turns.0.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(slot) = map.get_mut(&self.key) {
            slot.in_flight = slot.in_flight.saturating_sub(1);
            if slot.in_flight == 0 {
                map.remove(&self.key);
            }
        }
    }
}

/// Attente la plus longue imposée par l'une des décisions, s'il y en a une.
fn longest_wait(decisions: &[LockoutDecision]) -> Option<Duration> {
    decisions
        .iter()
        .filter_map(|decision| match decision {
            LockoutDecision::Blocked { retry_after } => Some(*retry_after),
            LockoutDecision::Allowed => None,
        })
        .max()
}

/// La tentative est-elle admise ? Sinon, l'attente à annoncer (la plus longue des deux compteurs).
fn admission(pair: LockoutState, address: LockoutState, now: OffsetDateTime) -> Option<Duration> {
    longest_wait(&[
        step(pair, LockoutEvent::Attempt, now).1,
        step_address(address, LockoutEvent::Attempt, now).1,
    ])
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
        feed: Arc<dyn AuditFeed>,
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
            feed,
            turns: Turns::default(),
        }
    }

    /// Ouvre une session. Identifiant inconnu et mot de passe faux suivent exactement le même
    /// chemin : une vérification Argon2 (contre un haché factice si le compte n'existe pas), un
    /// échec compté, une transaction (BR-CONN-013).
    ///
    /// Les connexions d'une même adresse sont traitées l'une après l'autre : le palier de
    /// verrouillage se joue sur des états à jour (dix tentatives simultanées ne font pas dix
    /// vérifications). Au plus huit attendent leur tour par adresse ; au-delà, la connexion est
    /// refusée tout de suite (`LoginError::Busy`) et son mot de passe n'est pas gardé.
    pub async fn login(
        &self,
        username: &str,
        password: Secret,
        client: &ClientInfo,
    ) -> Result<LoginOutcome, LoginError> {
        let pair = AttemptKey::new(username, &client.addr);
        let address = AttemptKey::address(&client.addr);
        // Un seul tour, par adresse : le couple contient l'adresse, deux connexions du même couple
        // sont donc déjà sérialisées par le tour de leur adresse.
        let Some(_turn) = self.turns.lock(address.as_str()).await else {
            return Err(LoginError::Busy);
        };
        let result = self
            .login_in_turn(username, password, client, &pair, &address)
            .await;
        // Trace des refus : adresse et raison, jamais l'identifiant saisi (ce peut être un mot de
        // passe tapé au mauvais endroit, BR-AUDIT-005) ni le mot de passe. Le journal d'activité
        // consigne connexions et verrouillages (écrits dans la transaction de la tentative).
        match &result {
            Err(LoginError::InvalidCredentials) => tracing::warn!(
                addr = %client.addr,
                reason = "invalid_credentials",
                "connexion refusée"
            ),
            Err(LoginError::TooManyAttempts { retry_after }) => tracing::warn!(
                addr = %client.addr,
                reason = "locked",
                retry_after_s = retry_after_seconds(*retry_after),
                "connexion refusée : verrouillage"
            ),
            _ => {}
        }
        result
    }

    async fn login_in_turn(
        &self,
        username: &str,
        password: Secret,
        client: &ClientInfo,
        pair: &AttemptKey,
        address: &AttemptKey,
    ) -> Result<LoginOutcome, LoginError> {
        // 1. Admission : pendant une attente, on ne vérifie même pas le mot de passe.
        let now = self.clock.now();
        let pair_state = self.attempts.get(pair).await?;
        let address_state = self.attempts.get(address).await?;
        if let Some(retry_after) = admission(pair_state, address_state, now) {
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
        let verified_account = account.filter(|_| verified);

        // 3. Issue, dans une seule transaction : compteurs, et pour un succès la session et la
        //    date de dernière connexion. Les états sont relus dans la transaction ; le haché
        //    vérifié est comparé à celui d'aujourd'hui (même garde que `change_own_password`) :
        //    un mot de passe changé entre-temps ne connecte pas.
        let mut tx = self.store.begin().await?;
        let now = self.clock.now();
        let pair_state = tx.login_attempts().get(pair).await?;
        let address_state = tx.login_attempts().get(address).await?;
        if let Some(retry_after) = admission(pair_state, address_state, now) {
            return Err(LoginError::TooManyAttempts { retry_after });
        }
        let account =
            match verified_account {
                Some(account) => tx.accounts().find(&account.id).await?.filter(|current| {
                    current.password_hash.expose() == account.password_hash.expose()
                }),
                None => None,
            };
        let Some(account) = account else {
            let (pair_next, pair_decision) = step(pair_state, LockoutEvent::Failed, now);
            let (address_next, address_decision) =
                step_address(address_state, LockoutEvent::Failed, now);
            tx.login_attempts().save(pair, &pair_next, now).await?;
            tx.login_attempts()
                .save(address, &address_next, now)
                .await?;
            // Journal (BR-AUDIT-003, 005, 006, 007), dans la transaction des compteurs : la
            // tentative refusée, sans compte ni identifiant saisi (la raison est la même que
            // l'identifiant existe ou non), puis le blocage qu'elle a éventuellement déclenché.
            let wait = longest_wait(&[pair_decision, address_decision]);
            let actor = Actor::new(None, Origin::client(Some(&client.name), &client.addr));
            let reason = if Username::parse(username).is_err() {
                Reason::InvalidIdentifier
            } else {
                Reason::InvalidCredentials
            };
            let mut journal = Pending::default();
            let denied = AuditEvent::new(
                now,
                actor.clone(),
                AuditAction::Login,
                Target::None,
                Outcome::Denied(reason),
            );
            journal.record(&mut *tx, denied).await?;
            if let Some(retry_after) = wait {
                let locked = AuditEvent::new(
                    now,
                    actor,
                    AuditAction::LoginLocked,
                    Target::None,
                    Outcome::Denied(Reason::TooManyAttempts {
                        retry_after_s: retry_after_seconds(retry_after),
                    }),
                );
                journal.record(&mut *tx, locked).await?;
            }
            tx.commit().await?;
            journal.publish(&*self.feed);
            return Err(match wait {
                Some(retry_after) => LoginError::TooManyAttempts { retry_after },
                None => LoginError::InvalidCredentials,
            });
        };

        let (reset, _) = step(pair_state, LockoutEvent::Succeeded, now);
        tx.login_attempts().save(pair, &reset, now).await?;
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
        let mut journal = Pending::default();
        let succeeded = AuditEvent::new(
            now,
            Actor::new(
                Some(account.username.clone()),
                Origin::client(Some(&client.name), &client.addr),
            ),
            AuditAction::Login,
            Target::None,
            Outcome::Succeeded,
        );
        journal.record(&mut *tx, succeeded).await?;
        tx.commit().await?;
        journal.publish(&*self.feed);

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

    /// Déconnexion explicite : supprime la session courante. `by` : le compte et l'origine de la
    /// requête (journal d'activité, BR-AUDIT-003).
    pub async fn logout(&self, session: &SessionId, by: &Actor) -> Result<(), StoreError> {
        let mut tx = self.store.begin().await?;
        tx.sessions().delete(session).await?;
        let mut journal = Pending::default();
        let event = AuditEvent::new(
            self.clock.now(),
            by.clone(),
            AuditAction::Logout,
            Target::None,
            Outcome::Succeeded,
        );
        journal.record(&mut *tx, event).await?;
        tx.commit().await?;
        journal.publish(&*self.feed);
        Ok(())
    }
}
