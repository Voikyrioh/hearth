//! Cas d'usage des sessions : connexion (avec verrouillage), reconnaissance d'un jeton présenté
//! (expiration glissante), déconnexion (BR-CONN-006, 007, 013 ; BR-RESIL-012, 014).
//!
//! Les règles sont celles de `domain::{lockout, sessions, session_token}` ; ce module les
//! enchaîne et demande au stockage d'exécuter.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use hearth_proto::api::sessions::{DeviceProof, DeviceStatus};
use hearth_proto::device_proof::Binding;
use thiserror::Error;
use time::{Duration, OffsetDateTime};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

use super::accounts::AccountView;
use super::audit::{AuditTrail, Pending};
use super::ports::{
    AccountRepo, AuditSink, Clock, HashError, IdGen, KnownAddressRepo, LoginAttemptRepo,
    PasswordHasher, SessionRepo, Store, StoreError, TokenGen, TokenGenError,
};
use super::trust::{DeviceLogin, RemoveError, TrustService, VerifiedKey};
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
    /// Ce que la connexion a fait de la clé d'appareil présentée (HRT-22) ; `None` : aucune clé
    /// prise en compte (pas de clé, preuve invalide, clé d'un autre compte).
    pub device: Option<DeviceStatus>,
}

/// Ce qu'un passage par le chemin de la connexion accorde.
enum Granted {
    Session(Box<LoginOutcome>),
    /// Mot de passe confirmé, rien d'autre (`confirm_password`).
    Confirmed,
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
    /// L'identité d'appareil (HRT-22). Absente : le service se comporte exactement comme avant, les
    /// routes de défi et de postes répondent `404`.
    trust: Option<Arc<TrustService>>,
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
            trust: None,
        }
    }

    /// Ajoute l'identité d'appareil : le défi, la preuve de clé, l'inscription des postes.
    #[must_use]
    pub fn with_trust(mut self, trust: Arc<TrustService>) -> Self {
        self.trust = Some(trust);
        self
    }

    /// L'identité d'appareil, si elle est configurée.
    pub fn trust(&self) -> Option<&Arc<TrustService>> {
        self.trust.as_ref()
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
        self.login_with_device(username, password, client, None)
            .await
    }

    /// Comme `login`, avec en option la preuve de la clé d'appareil. **Une preuve ne change aucune
    /// décision d'accès** : absente, illisible, fausse ou rejouée, la connexion se déroule comme
    /// sans elle ; valide, elle inscrit ou date le poste dans la transaction d'une connexion
    /// accordée, et seulement alors (HRT-22).
    pub async fn login_with_device(
        &self,
        username: &str,
        password: Secret,
        client: &ClientInfo,
        device: Option<&DeviceProof>,
    ) -> Result<LoginOutcome, LoginError> {
        match self
            .run_login(username, password, client, device, false)
            .await?
        {
            Granted::Session(outcome) => Ok(*outcome),
            // Inatteignable : seul le mode « confirmer » rend `Confirmed`.
            Granted::Confirmed => Err(LoginError::InvalidCredentials),
        }
    }

    /// Confirme le mot de passe d'un compte pour un acte d'administration (retrait d'un poste de
    /// confiance, Q16) : **exactement le chemin de la connexion** (tour par adresse, admission, Argon2
    /// contre le vrai haché, compteurs du couple, de l'adresse et de l'identifiant, ralentissement,
    /// journal des refus), sans ouvrir de session ni apprendre d'adresse. Un mot de passe faux ici
    /// n'offre donc aucun moyen de deviner sans limite : il compte comme un échec de connexion.
    pub async fn confirm_password(
        &self,
        username: &str,
        password: Secret,
        client: &ClientInfo,
    ) -> Result<(), LoginError> {
        self.run_login(username, password, client, None, true)
            .await
            .map(|_| ())
    }

    async fn run_login(
        &self,
        username: &str,
        password: Secret,
        client: &ClientInfo,
        device: Option<&DeviceProof>,
        confirm: bool,
    ) -> Result<Granted, LoginError> {
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
        let result = self
            .login_in_turn(username, password, client, &keys, device, confirm)
            .await;
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
        device: Option<&DeviceProof>,
        confirm: bool,
    ) -> Result<Granted, LoginError> {
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

        // 1 bis. Preuve de la clé d'appareil, s'il y en a une : vérifiée sous la clé FOURNIE, donc
        //    le même travail que l'identifiant existe ou non (HRT-22, absence d'oracle). Elle ne
        //    décide de rien ici : seule une connexion accordée s'en sert, plus bas.
        let key: Option<VerifiedKey> = match (device, &self.trust) {
            (Some(proof), Some(trust)) => {
                trust.verify(proof, Binding::Login, username, &client.addr)
            }
            _ => None,
        };

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
        if confirm {
            // Confirmation d'un acte : les compteurs sont déjà écrits (le succès remet à zéro celui du
            // couple, comme une connexion) ; ni session, ni adresse apprise, ni entrée de connexion.
            tx.commit().await?;
            return Ok(Granted::Confirmed);
        }

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
        // La clé d'appareil, dans la même transaction : inscrite si elle est nouvelle et qu'il y a
        // de la place, sinon datée. C'est ici, et nulle part ailleurs, qu'un poste s'inscrit :
        // jamais sur la seule présentation d'une session.
        let DeviceLogin {
            status: device_status,
            device: device_id,
        } = match (&key, &self.trust) {
            (Some(key), Some(trust)) => {
                trust
                    .on_login(&mut *tx, &mut journal, key, &account, client, now)
                    .await?
            }
            _ => DeviceLogin {
                status: None,
                device: None,
            },
        };
        if let Some(device_id) = &device_id {
            tx.sessions().bind_device(&session.id, device_id).await?;
        }
        tx.commit().await?;
        journal.publish(&self.trail);

        let mut view = AccountView::from(&account);
        view.last_login_at = Some(now);
        Ok(Granted::Session(Box::new(LoginOutcome {
            token,
            session_id: session.id,
            expires_at: session.expires_at,
            account: view,
            device: device_status,
        })))
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

    /// Retire un poste de confiance : **un acte d'administration** (Q16 : mot de passe, plus tard 2FA, ET
    /// clé privée). Une session seule ne suffit pas.
    ///
    /// Ordre : (1) la preuve de possession de la clé du poste courant (usage « retrait », liée au jeton et
    /// à l'identifiant du poste visé) : sans elle, **aucun mot de passe n'est essayé** (une session volée
    /// ne devine rien) ; (2) le mot de passe actuel, par le chemin de la connexion (mêmes compteurs et
    /// ralentissement) ; (3) le retrait ; (4) le défi n'est consommé que si le retrait a réussi.
    #[allow(clippy::too_many_arguments)]
    pub async fn remove_device(
        &self,
        session: &CurrentSession,
        token: &str,
        id: &str,
        password: Secret,
        proof: Option<&DeviceProof>,
        client: &ClientInfo,
        by: &Actor,
    ) -> Result<(), RemoveError> {
        let Some(trust) = self.trust.as_ref() else {
            return Err(RemoveError::NotFound);
        };
        let token_hash = SessionToken::parse(token)
            .map_err(|_| RemoveError::ProofInvalid)?
            .hash();
        let proven = trust
            .verify_removal(
                &session.account.id,
                session.account.username.as_str(),
                &session.session_id,
                token_hash.as_bytes(),
                id,
                proof,
                &client.addr,
            )
            .await?;
        self.confirm_password(session.account.username.as_str(), password, client)
            .await
            .map_err(|error| RemoveError::Password(Box::new(error)))?;
        trust
            .remove_proven(&session.account.id, &session.session_id, id, by, &proven)
            .await
    }

    /// Reconnaît la session du jeton présenté et repousse son expiration (expiration
    /// glissante, au plus une écriture toutes les `RENEWAL_INTERVAL`).
    pub async fn authenticate(&self, token: &str) -> Result<CurrentSession, AuthError> {
        self.authenticate_inner(token, None, None).await
    }

    /// Comme `authenticate`, depuis cette adresse (celle de la connexion TCP). Quand la session
    /// est renouvelée et que l'adresse est déjà retenue pour le compte, sa durée est repoussée :
    /// un poste utilisé chaque jour ne cesse pas d'être connu au bout de 30 jours. L'usage d'une
    /// session **n'apprend jamais** une adresse (ADR-0023).
    pub async fn authenticate_at(
        &self,
        token: &str,
        addr: &str,
    ) -> Result<CurrentSession, AuthError> {
        self.authenticate_inner(token, Some(addr), None).await
    }

    /// Comme `authenticate_at`, avec la preuve de la clé d'appareil du premier message du flux : si
    /// elle est valide et que la clé est inscrite pour ce compte, l'adresse est retenue (session +
    /// clé, BR-TRUST-007). Une preuve invalide est ignorée : le jeton seul fait ce qu'il faisait.
    pub async fn authenticate_proved(
        &self,
        token: &str,
        addr: &str,
        proof: &DeviceProof,
    ) -> Result<CurrentSession, AuthError> {
        self.authenticate_inner(token, Some(addr), Some(proof))
            .await
    }

    async fn authenticate_inner(
        &self,
        token: &str,
        addr: Option<&str>,
        proof: Option<&DeviceProof>,
    ) -> Result<CurrentSession, AuthError> {
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

        // La preuve de clé, liée à ce jeton et à ce compte : une preuve faite pour un autre jeton,
        // un autre identifiant ou un autre usage ne vaut rien ici.
        let key = match (proof, addr, &self.trust) {
            (Some(proof), Some(addr), Some(trust)) => trust.verify(
                proof,
                Binding::Session {
                    token_hash: hash.as_bytes(),
                },
                account.username.as_str(),
                addr,
            ),
            _ => None,
        };
        let renewal = renewed_expiry(session.last_seen_at, now);
        let expires_at = if renewal.is_some() || key.is_some() {
            let mut tx = self.store.begin().await?;
            if let Some(expires_at) = renewal {
                tx.sessions().renew(&session.id, now, expires_at).await?;
            }
            let proved = match (&key, addr, &self.trust) {
                (Some(key), Some(addr), Some(trust)) => {
                    trust
                        .on_session_proof(&mut *tx, key, &account.id, &session.id, addr, now)
                        .await?
                }
                _ => false,
            };
            // Usage d'une session valide depuis une adresse déjà retenue : sa durée est repoussée,
            // rien n'est appris. Aussi quand la preuve présentée n'a pas servi (clé non inscrite,
            // défi déjà pris) : la session, elle, est valide.
            if !proved
                && renewal.is_some()
                && let Some(addr) = addr
            {
                tx.known_addresses()
                    .touch(&account.id, &canonical(addr), now)
                    .await?;
            }
            tx.commit().await?;
            renewal.unwrap_or(session.expires_at)
        } else {
            session.expires_at
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
