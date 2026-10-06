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
use super::audit::{AuditTrail, Pending};
use super::ports::{
    AccountRepo, AuditSink, Clock, HashError, IdGen, KnownAddressRepo, LoginAttemptRepo,
    PasswordHasher, SessionRepo, Store, StoreError, TokenGen, TokenGenError,
};
use crate::domain::accounts::Username;
use crate::domain::audit::{Actor, AuditAction, AuditEvent, Origin, Outcome, Reason, Target};
use crate::domain::identifier_slowdown;
use crate::domain::known_address::{self, canonical, is_known};
use crate::domain::lockout::{AttemptKey, LockoutState, retry_after_seconds};
use crate::domain::login_policy::{
    LoginState, QueueRefusal, Verdict, admit, admit_login, conclude,
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
    /// Trop de connexions en attente pour cette adresse : refus immédiat (`429`), sans compter
    /// d'échec.
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
    known: Arc<dyn KnownAddressRepo>,
    store: Arc<dyn Store>,
    hasher: Arc<dyn PasswordHasher>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGen>,
    tokens: Arc<dyn TokenGen>,
    trail: Arc<AuditTrail>,
    /// Les connexions refusées : écrites hors transaction, par le même regroupement que les autres
    /// refus (BR-AUDIT-007), jamais une par une.
    sink: Arc<dyn AuditSink>,
    turns: Turns,
}

/// Tours de parole par adresse : une seule connexion à la fois pour une même adresse, une file
/// d'attente bornée par adresse et un plafond global dont une part est réservée aux adresses déjà
/// connues (`domain::login_policy::admit_login`, ADR-0022).
#[derive(Default)]
struct Turns(Mutex<TurnState>);

#[derive(Default)]
struct TurnState {
    slots: HashMap<String, Slot>,
    /// Connexions admises, toutes adresses confondues (en cours et en attente).
    total: usize,
}

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
    mutex: Arc<AsyncMutex<()>>,
    guard: Option<OwnedMutexGuard<()>>,
}

impl<'a> Turns {
    /// Prend une place dans la file de `key` (sans attendre son tour) ; refusée si la file de
    /// l'adresse ou le plafond global est atteint. `known` : l'adresse peut prendre les places
    /// réservées.
    fn admit(&'a self, key: &str, known: bool) -> Result<Turn<'a>, QueueRefusal> {
        let mut state = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let own = state.slots.get(key).map_or(0, |slot| slot.in_flight);
        admit_login(state.total, own, known)?;
        state.total += 1;
        let slot = state.slots.entry(key.to_owned()).or_default();
        slot.in_flight += 1;
        Ok(Turn {
            turns: self,
            key: key.to_owned(),
            mutex: slot.mutex.clone(),
            guard: None,
        })
    }
}

impl Turn<'_> {
    /// Attend son tour.
    async fn wait(&mut self) {
        self.guard = Some(self.mutex.clone().lock_owned().await);
    }
}

impl Drop for Turn<'_> {
    fn drop(&mut self) {
        // Rend le tour, puis la place ; oublie l'entrée si plus personne ne l'attend.
        self.guard.take();
        let mut state = self.turns.0.lock().unwrap_or_else(PoisonError::into_inner);
        state.total = state.total.saturating_sub(1);
        if let Some(slot) = state.slots.get_mut(&self.key) {
            slot.in_flight = slot.in_flight.saturating_sub(1);
            if slot.in_flight == 0 {
                state.slots.remove(&self.key);
            }
        }
    }
}

/// Les clés des trois compteurs d'une tentative.
struct Keys {
    pair: AttemptKey,
    address: AttemptKey,
    identifier: AttemptKey,
    /// L'identifiant tel que la base le connaît (pour les adresses connues) : même forme que celle
    /// que `Username::parse` donne, sinon la valeur saisie (qui ne correspond à aucun compte).
    username: String,
}

impl SessionService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        accounts: Arc<dyn AccountRepo>,
        sessions: Arc<dyn SessionRepo>,
        attempts: Arc<dyn LoginAttemptRepo>,
        known: Arc<dyn KnownAddressRepo>,
        store: Arc<dyn Store>,
        hasher: Arc<dyn PasswordHasher>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGen>,
        tokens: Arc<dyn TokenGen>,
        trail: Arc<AuditTrail>,
        sink: Arc<dyn AuditSink>,
    ) -> Self {
        Self {
            accounts,
            sessions,
            attempts,
            known,
            store,
            hasher,
            clock,
            ids,
            tokens,
            trail,
            sink,
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
        let keys = Keys {
            pair: AttemptKey::new(username, &client.addr),
            address: AttemptKey::address(&client.addr),
            identifier: AttemptKey::identifier(username),
            username: Username::parse(username)
                .map_or_else(|_| username.to_owned(), |parsed| parsed.as_str().to_owned()),
        };
        // Un seul tour, par adresse exacte : le couple contient l'adresse, deux connexions du même
        // couple sont donc déjà sérialisées par le tour de leur adresse. Comme pour le flux, une
        // adresse est d'abord admise comme inconnue ; seule la saturation fait lire en base si elle
        // est déjà connue (d'un compte quelconque) et peut prendre une place réservée : la décision
        // ne dépend jamais de l'identifiant saisi (ADR-0022, BR-CONN-013).
        let admitted = match self.turns.admit(&canonical(&client.addr), false) {
            Err(QueueRefusal::Saturated)
                if self.address_is_known(&client.addr).await.unwrap_or(false) =>
            {
                self.turns.admit(&canonical(&client.addr), true)
            }
            other => other,
        };
        let Ok(mut turn) = admitted else {
            tracing::warn!(
                addr = %client.addr,
                reason = "queue_full",
                "connexion refusée : trop de connexions en attente"
            );
            return Err(LoginError::Busy);
        };
        turn.wait().await;
        let result = self.login_in_turn(username, password, client, &keys).await;
        // Trace des refus : adresse et raison, jamais l'identifiant saisi (ce peut être un mot de
        // passe tapé au mauvais endroit, BR-AUDIT-005) ni le mot de passe. Le journal d'activité
        // consigne connexions et verrouillages (le succès dans la transaction de la tentative, les refus par le regroupement).
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
        keys: &Keys,
    ) -> Result<LoginOutcome, LoginError> {
        // 1. Admission : pendant une attente du couple ou de l'adresse, on ne vérifie même pas le
        //    mot de passe. Le ralentissement par identifiant, lui, refuse APRÈS la vérification
        //    (étape 3) : même chemin et même durée pour un identifiant existant ou non.
        let now = self.clock.now();
        let early = LoginState {
            pair: self.attempts.get(&keys.pair).await?,
            address: self.attempts.get(&keys.address).await?,
            identifier: identifier_slowdown::Slowdown::default(),
            known: false,
            seen: false,
        };
        if let Some(retry_after) = admit(&early, now) {
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
        // Le compte visé, pour le journal, seulement s'il existe : ce n'est alors pas un mot de
        // passe tapé à la place de l'identifiant (BR-AUDIT-005, 006).
        let targeted = account.as_ref().map(|found| found.username.clone());
        let verified_account = account.filter(|_| verified);

        // 3. Issue, dans une seule transaction : compteurs, et pour un succès la session et la
        //    date de dernière connexion. Les états sont relus dans la transaction ; le haché
        //    vérifié est comparé à celui d'aujourd'hui (même garde que `change_own_password`) :
        //    un mot de passe changé entre-temps ne connecte pas.
        let mut tx = self.store.begin().await?;
        let now = self.clock.now();
        let known = {
            let list = tx.known_addresses().of_username(&keys.username).await?;
            is_known(&list, &client.addr, now)
        };
        let before = LoginState {
            pair: tx.login_attempts().get(&keys.pair).await?,
            address: tx.login_attempts().get(&keys.address).await?,
            identifier: tx.login_attempts().identifier(&keys.identifier).await?,
            known,
            // Connue d'un compte quelconque : lue pour tout identifiant, existant ou non.
            seen: tx
                .known_addresses()
                .address_is_known(&canonical(&client.addr), known_address::cutoff(now))
                .await?,
        };
        let account =
            match verified_account {
                Some(account) => tx.accounts().find(&account.id).await?.filter(|current| {
                    current.password_hash.expose() == account.password_hash.expose()
                }),
                None => None,
            };
        let (after, verdict) = conclude(before, account.is_some(), now);
        // Écrit ce qui a changé ; une ligne neuve peut dépasser la borne de la table.
        let vacant = LoginState {
            pair: LockoutState::default(),
            address: LockoutState::default(),
            identifier: identifier_slowdown::Slowdown::default(),
            known,
            seen: before.seen,
        };
        if after.pair != before.pair || matches!(verdict, Verdict::Granted) {
            tx.login_attempts()
                .save(&keys.pair, &after.pair, now)
                .await?;
        }
        if after.address != before.address {
            tx.login_attempts()
                .save(&keys.address, &after.address, now)
                .await?;
        }
        if after.identifier != before.identifier {
            tx.login_attempts()
                .save_identifier(&keys.identifier, &after.identifier)
                .await?;
        }
        if before.pair == vacant.pair || before.address == vacant.address {
            tx.login_attempts().trim(now).await?;
        }
        if before.identifier == vacant.identifier && after.identifier != before.identifier {
            tx.login_attempts().trim_identifiers(now).await?;
        }

        match verdict {
            Verdict::Granted => {}
            Verdict::Blocked(retry_after) => {
                // Rien n'est compté : on libère la transaction avant tout autre accès.
                tx.commit().await?;
                return Err(LoginError::TooManyAttempts { retry_after });
            }
            Verdict::Slowed(retry_after) => {
                tx.commit().await?;
                self.journal_refusal(
                    targeted,
                    client,
                    Outcome::Denied(Reason::TooManyAttempts {
                        retry_after_s: retry_after_seconds(retry_after),
                    }),
                )
                .await;
                return Err(LoginError::TooManyAttempts { retry_after });
            }
            Verdict::Failed(wait) => {
                tx.commit().await?;
                // Journal (BR-AUDIT-003, 005, 006, 007), une fois les compteurs validés : la
                // tentative refusée, avec le compte visé seulement s'il existe (la raison est la
                // même que l'identifiant existe ou non, et l'identifiant saisi n'est jamais
                // retenu), puis le blocage qu'elle a éventuellement déclenché. **Par le
                // regroupement des refus** (`AuditSink`) : la clé ne contient jamais l'adresse ni
                // une durée. Une rafale de refus depuis de nombreuses adresses n'écrit qu'un
                // premier refus et une synthèse, et ne chasse pas l'historique du journal.
                let reason = if Username::parse(username).is_err() {
                    Reason::InvalidIdentifier
                } else {
                    Reason::InvalidCredentials
                };
                self.journal_refusal(targeted.clone(), client, Outcome::Denied(reason))
                    .await;
                if let Some(retry_after) = wait {
                    let actor =
                        Actor::new(targeted, Origin::client(Some(&client.name), &client.addr));
                    self.sink
                        .record(
                            actor,
                            AuditAction::LoginLocked,
                            Target::None,
                            Outcome::Denied(Reason::TooManyAttempts {
                                retry_after_s: retry_after_seconds(retry_after),
                            }),
                        )
                        .await;
                }
                return Err(match wait {
                    Some(retry_after) => LoginError::TooManyAttempts { retry_after },
                    None => LoginError::InvalidCredentials,
                });
            }
        }
        let Some(account) = account else {
            // Inatteignable : `Granted` suppose un mot de passe vérifié sur un compte existant.
            return Err(LoginError::InvalidCredentials);
        };

        // Connexion réussie : cette adresse devient (ou reste) connue de ce compte, et seulement
        // ainsi (ADR-0022).
        let remembered = tx.known_addresses().of_account(&account.id).await?;
        let remembered = known_address::learn(remembered, &client.addr, now);
        tx.known_addresses()
            .replace(&account.id, &remembered)
            .await?;
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
        journal.publish(&self.trail);

        let mut view = AccountView::from(&account);
        view.last_login_at = Some(now);
        Ok(LoginOutcome {
            token,
            session_id: session.id,
            expires_at: session.expires_at,
            account: view,
        })
    }

    /// Écrit un refus de connexion au journal, par le regroupement des refus (une entrée puis une
    /// synthèse au compte exact, BR-AUDIT-007). Ni l'identifiant saisi ni le mot de passe ne sont
    /// retenus ; le compte visé n'est renseigné que s'il existe (BR-AUDIT-006).
    async fn journal_refusal(
        &self,
        targeted: Option<Username>,
        client: &ClientInfo,
        outcome: Outcome,
    ) {
        self.sink
            .record(
                Actor::new(targeted, Origin::client(Some(&client.name), &client.addr)),
                AuditAction::Login,
                Target::None,
                outcome,
            )
            .await;
    }

    /// Cette adresse a-t-elle déjà une session valide, ou une connexion réussie dans les
    /// 30 derniers jours, pour un compte quelconque ? Sert aux places d'attente du flux
    /// temps réel, réservées en priorité aux adresses connues (ADR-0022, BR-CONN-020).
    pub async fn address_is_known(&self, addr: &str) -> Result<bool, StoreError> {
        let now = self.clock.now();
        let addr = canonical(addr);
        if self.sessions.has_open_session_from(&addr, now).await? {
            return Ok(true);
        }
        self.known
            .address_is_known(&addr, known_address::cutoff(now))
            .await
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
        journal.publish(&self.trail);
        Ok(())
    }
}
