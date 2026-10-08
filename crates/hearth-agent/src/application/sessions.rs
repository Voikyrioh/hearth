//! Cas d'usage des sessions : connexion (avec verrouillage), reconnaissance d'un jeton présenté
//! (expiration glissante), déconnexion (BR-CONN-006, 007, 013 ; BR-RESIL-012, 014).
//!
//! Les règles sont celles de `domain::{lockout, sessions, session_token}` ; ce module les
//! enchaîne et demande au stockage d'exécuter.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use hearth_proto::admin_act::AdminAct;
use hearth_proto::api::reauth::{AdminReauthInfo, Reauth, ReauthMode};
use hearth_proto::api::sessions::{DeviceProof, DeviceStatus};
use hearth_proto::device_proof::Binding;
use subtle::ConstantTimeEq;
use thiserror::Error;
use time::{Duration, OffsetDateTime};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

use super::accounts::AccountView;
use super::attack_mode::{AttackModeService, AttackStatus};
use super::audit::{AuditTrail, Pending};
use super::elevation::Elevations;
use super::ports::{
    AccountRepo, AuditSink, Clock, HashError, IdGen, KnownAddressRepo, LoginAttemptRepo,
    PasswordHasher, SessionRepo, Store, StoreError, TokenGen, TokenGenError, UnitOfWork,
};
use super::security::SecurityService;
use super::trust::{
    AttackProofError, DeviceLogin, InFlight, RemoveError, TrustService, VerifiedKey,
};
use crate::domain::accounts::{Account, Username};
use crate::domain::audit::{
    Actor, AuditAction, AuditEvent, ClientName, Origin, Outcome, Reason, Target,
};
use crate::domain::identifier_slowdown::{self, AlertChange, alert_change};
use crate::domain::known_address::{self, canonical, is_known};
use crate::domain::lockout::{AttemptKey, LockoutState, retry_after_seconds};
use crate::domain::login_policy::{
    LoginState, QueueRefusal, Verdict, admit, admit_login, conclude,
};
use crate::domain::secret::Secret;
use crate::domain::session_token::SessionToken;
use crate::domain::sessions::{Session, SessionEnd, SessionId, check, expiry_from, renewed_expiry};
use crate::domain::trust::DeviceId;
use crate::domain::trust::admin_act::covered_by_elevation;
use crate::domain::trust::attack_mode::{Effective, EndHow};
use crate::domain::trust::{
    LoginCriteria, Mode, SessionStanding, TrialKind, judge_login, judge_session, mode_of,
};

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

/// Pourquoi on passe par le chemin de la connexion : ouvrir une session, ou seulement confirmer le
/// mot de passe pour un acte d'administration. Ne change que ce qui s'écrit au journal pour un refus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Purpose {
    Login,
    /// Confirmation du retrait d'un poste ou du mode attaque à plat (contrats livrés) : l'attente imposée
    /// est consignée ici, sous l'action de l'acte.
    Confirmation(AuditAction),
    /// Confirmation par la couche `reauth` (HRT-28) : la route consigne elle-même tout refus, attente
    /// comprise (une seule entrée par refus).
    Act(AuditAction),
}

impl Purpose {
    /// L'action du journal sous laquelle un refus est consigné.
    fn action(self) -> AuditAction {
        match self {
            Self::Login => AuditAction::Login,
            Self::Confirmation(action) | Self::Act(action) => action,
        }
    }

    /// Un mot de passe faux est consigné ici comme une connexion refusée (jamais pour une
    /// confirmation : la route de l'acte le consigne).
    fn journals_wrong_password(self) -> bool {
        self == Self::Login
    }

    /// L'attente imposée par le ralentissement est consignée ici (sauf quand la route le fait).
    fn journals_throttle(self) -> bool {
        !matches!(self, Self::Act(_))
    }
}

/// Une tentative dont le mot de passe est juste et le poste admis : la transaction est ouverte, les
/// compteurs écrits dedans, il reste à ouvrir la session (ou à valider).
struct Passed {
    tx: Box<dyn UnitOfWork>,
    account: Account,
    now: OffsetDateTime,
    key: Option<VerifiedKey>,
    /// Les entrées déjà écrites dans la transaction (l'essai unique du mode attaque) : diffusées une
    /// fois la transaction validée.
    journal: Pending,
}

/// L'action du journal de l'acte (catalogue de l'agent, depuis le code stable du protocole).
fn act_audit_action(act: &AdminAct<'_>) -> AuditAction {
    AuditAction::from_code(act.kind().audit_code()).unwrap_or(AuditAction::Login)
}

/// Trace des refus : adresse et raison, jamais l'identifiant saisi (ce peut être un mot de passe
/// tapé au mauvais endroit, BR-AUDIT-005) ni le mot de passe. Le journal d'activité consigne
/// connexions et verrouillages (le succès dans la transaction de la tentative, les refus par le
/// regroupement).
fn trace_refusal<T>(result: &Result<T, LoginError>, client: &ClientInfo) {
    match result {
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
    /// Mode attaque : la session est valide mais présentée seule (ni adresse retenue ni clé prouvée).
    /// Refusée comme une session expirée, sans être détruite (Q12, Q14 point 2, BR-TRUST-013).
    #[error("La session n'est pas reconnue depuis ce poste")]
    NotRecognized,
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Pourquoi le mode attaque n'a pas été changé (HRT-25).
#[derive(Debug, Error)]
pub enum AttackModeError {
    /// Un compte qui ne gère pas les comptes.
    #[error("Seul un administrateur peut changer le mode attaque")]
    Forbidden,
    /// L'agent n'a pas le mode attaque (sans identité d'appareil).
    #[error("Le mode attaque n'est pas disponible")]
    Unavailable,
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Pourquoi la confirmation d'un acte d'administration est refusée (HRT-28, BR-TRUST-036, 040). Aucune
/// variante ne porte un secret.
#[derive(Debug, Error)]
pub enum ReauthError {
    /// Aucune preuve de clé n'accompagne la confirmation. **Aucun mot de passe n'est essayé.**
    #[error("Aucune preuve de clé n'accompagne la confirmation")]
    ProofMissing,
    /// La preuve n'est pas celle d'une clé inscrite du compte, pour cet acte, cette cible et cette
    /// session, ou elle est périmée ou rejouée. **Aucun mot de passe n'est essayé.**
    #[error("La preuve de la clé de ce poste est invalide")]
    ProofInvalid,
    /// Le mot de passe manque et l'élévation ne couvre pas cet acte.
    #[error("Le mot de passe est requis pour cet acte")]
    PasswordRequired,
    /// Le mot de passe est refusé par le chemin de la connexion (mêmes compteurs, même ralentissement).
    #[error(transparent)]
    Password(Box<LoginError>),
    /// L'agent n'a pas l'identité d'appareil.
    #[error("Les actes confirmés ne sont pas disponibles")]
    Unavailable,
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl From<AttackProofError> for ReauthError {
    fn from(error: AttackProofError) -> Self {
        match error {
            AttackProofError::Missing => Self::ProofMissing,
            AttackProofError::Invalid => Self::ProofInvalid,
            AttackProofError::Store(error) => Self::Store(error),
        }
    }
}

struct ReauthInner {
    /// Le haché du mot de passe réellement vérifié par le chemin de la connexion ; `None` sous élévation.
    verified: Option<Secret>,
    /// Tient le défi pendant l'acte (il est déjà retenu comme consommé : voir `reauthenticate`).
    _reservation: Option<InFlight>,
}

/// Un acte confirmé : le mot de passe (ou l'élévation) et la preuve de clé ont été vérifiés, et le défi
/// est **déjà consommé** (avant l'effet de l'acte : un défi qui ne peut pas être retenu refuse l'acte).
/// Posé sur la requête par la couche `reauth`.
#[derive(Clone)]
pub struct Reauthenticated(Arc<ReauthInner>);

impl Reauthenticated {
    /// Le marqueur d'un acte accepté SANS confirmation, posé par la couche `reauth` pour les seuls bancs
    /// d'essai qui baissent l'exigence. Derrière la fonction cargo `test-support` : un binaire de production
    /// ne le contient pas, et aucun handler d'acte ne travaille sans ce type (BR-TRUST-045).
    #[cfg(feature = "test-support")]
    pub fn unconfirmed() -> Self {
        Self(Arc::new(ReauthInner {
            verified: None,
            _reservation: None,
        }))
    }

    /// Le haché du mot de passe vérifié, `None` si l'élévation a tenu lieu de mot de passe.
    pub fn verified_hash(&self) -> Option<&Secret> {
        self.0.verified.as_ref()
    }
}

impl std::fmt::Debug for Reauthenticated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reauthenticated").finish_non_exhaustive()
    }
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
    /// L'alerte « attaque probable » (HRT-24). Absente : rien n'est signalé, le reste est identique.
    security: Option<Arc<SecurityService>>,
    /// Le mode attaque (HRT-25). Absent : le mode n'existe pas, le comportement est celui d'avant.
    attack: Option<Arc<AttackModeService>>,
    /// L'élévation du mot de passe en administration (HRT-28). Absente : le mot de passe est demandé à
    /// chaque acte, comme avec le réglage `each`.
    elevations: Option<Arc<Elevations>>,
    /// L'agent **exige** la confirmation des actes (`admin_reauth.required`) : VRAI dès la construction,
    /// aucun repli vers « la session suffit ». Seuls les bancs d'essai le baissent, par une méthode au nom
    /// explicite (`accept_unconfirmed_acts_for_tests`).
    reauth_required: AtomicBool,
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
            security: None,
            attack: None,
            elevations: None,
            reauth_required: AtomicBool::new(true),
        }
    }

    /// L'agent exige-t-il la confirmation de chaque acte ?
    pub fn reauth_required(&self) -> bool {
        self.reauth_required.load(Ordering::SeqCst)
    }

    /// **Pour les bancs d'essai seulement** : `true` fait accepter un acte sans `reauth` (le régime d'avant
    /// HRT-30, pour les scénarios qui envoient des actes bruts) ; `false` rétablit l'exigence. Derrière la
    /// fonction cargo `test-support` : un binaire de production ne la contient pas (BR-TRUST-045).
    #[cfg(feature = "test-support")]
    pub fn accept_unconfirmed_acts_for_tests(&self, accept: bool) {
        self.reauth_required.store(!accept, Ordering::SeqCst);
    }

    /// Ajoute l'élévation du mot de passe en administration (HRT-28, BR-TRUST-043).
    #[must_use]
    pub fn with_elevations(mut self, elevations: Arc<Elevations>) -> Self {
        self.elevations = Some(elevations);
        self
    }

    /// L'élévation, si elle est configurée.
    pub fn elevations(&self) -> Option<&Arc<Elevations>> {
        self.elevations.as_ref()
    }

    /// Ajoute le mode attaque : la règle devient « qui passe et qui est bloqué » quand il est actif
    /// (HRT-25).
    #[must_use]
    pub fn with_attack(mut self, attack: Arc<AttackModeService>) -> Self {
        self.attack = Some(attack);
        self
    }

    /// Le mode attaque, s'il est configuré.
    pub fn attack(&self) -> Option<&Arc<AttackModeService>> {
        self.attack.as_ref()
    }

    /// Ajoute l'identité d'appareil : le défi, la preuve de clé, l'inscription des postes.
    #[must_use]
    pub fn with_trust(mut self, trust: Arc<TrustService>) -> Self {
        self.trust = Some(trust);
        self
    }

    /// Ajoute l'alerte : début et fin d'un épisode au journal et au flux (HRT-24).
    #[must_use]
    pub fn with_security(mut self, security: Arc<SecurityService>) -> Self {
        self.security = Some(security);
        self
    }

    /// L'alerte, si elle est configurée.
    pub fn security(&self) -> Option<&Arc<SecurityService>> {
        self.security.as_ref()
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

    /// Comme `login`, avec en option la preuve de la clé d'appareil. La clé est un critère de la règle
    /// « 2 critères sur 3 » (ADR-0024) : elle ne compte, et seulement en ALERTE, que pour échapper au
    /// ralentissement, jamais pour se passer du mot de passe. Absente, illisible, fausse ou rejouée,
    /// la connexion se déroule comme sans elle ; valide, elle inscrit ou date le poste dans la
    /// transaction d'une connexion accordée, et seulement alors (HRT-22).
    pub async fn login_with_device(
        &self,
        username: &str,
        password: Secret,
        client: &ClientInfo,
        device: Option<&DeviceProof>,
    ) -> Result<LoginOutcome, LoginError> {
        let turn = self.take_turn(client).await?;
        let result = match self
            .verify(username, password, client, device, None, Purpose::Login)
            .await
        {
            Ok(passed) => self.open_session(passed, client).await,
            Err(error) => Err(error),
        };
        drop(turn);
        trace_refusal(&result, client);
        self.note_refusal(&result);
        result
    }

    /// Une tentative refusée (mot de passe faux, attente, file pleine) repousse la sortie automatique
    /// du mode attaque (BR-TRUST-019).
    fn note_refusal<T>(&self, result: &Result<T, LoginError>) {
        if let Some(attack) = &self.attack
            && matches!(
                result,
                Err(LoginError::InvalidCredentials
                    | LoginError::TooManyAttempts { .. }
                    | LoginError::Busy)
            )
        {
            attack.note_refusal();
        }
    }

    /// Confirme le mot de passe d'un compte pour un acte d'administration (retrait d'un poste de
    /// confiance, Q16) : **exactement le chemin de la connexion** (tour par adresse, admission, Argon2
    /// contre le vrai haché, compteurs du couple, de l'adresse et de l'identifiant, ralentissement,
    /// journal des refus), sans ouvrir de session ni apprendre d'adresse. Un mot de passe faux ici
    /// n'offre donc aucun moyen de deviner sans limite : il compte comme un échec de connexion.
    ///
    /// `action` : l'action du journal de l'acte confirmé. La route consigne elle-même tout refus
    /// (`Purpose::Act`) : c'est le chemin de l'ancien mot de passe de `PUT /me/password` (HRT-28,
    /// constat C3), qui ne passait par aucun compteur.
    pub async fn confirm_password(
        &self,
        username: &str,
        password: Secret,
        client: &ClientInfo,
        action: AuditAction,
    ) -> Result<Secret, LoginError> {
        self.confirm_password_proven(username, password, client, None, Purpose::Act(action))
            .await
    }

    /// Comme `confirm_password`, quand la preuve de la clé d'un poste inscrit du compte vient d'être
    /// vérifiée par l'acte lui-même (retrait d'un poste, changement du mode attaque) : en mode attaque,
    /// la clé et l'adresse retenue font deux critères et le mot de passe n'est pas jugé comme celui d'un
    /// poste qui n'en aurait qu'un (l'administrateur ne consomme pas d'essai unique, et ne s'enferme pas
    /// dehors en tapant mal une fois).
    async fn confirm_password_proven(
        &self,
        username: &str,
        password: Secret,
        client: &ClientInfo,
        proven: Option<VerifiedKey>,
        purpose: Purpose,
    ) -> Result<Secret, LoginError> {
        let turn = self.take_turn(client).await?;
        let result = match self
            .verify(username, password, client, None, proven, purpose)
            .await
        {
            // Les compteurs sont déjà écrits (le succès remet à zéro celui du couple, comme une
            // connexion) ; ni session, ni adresse apprise, ni entrée de connexion. Rend le haché
            // réellement vérifié (la garde « mot de passe changé entre-temps » de `PUT /me/password`).
            Ok(passed) => {
                let verified = Secret::new(passed.account.password_hash.expose().to_owned());
                passed
                    .tx
                    .commit()
                    .await
                    .map(|()| verified)
                    .map_err(LoginError::from)
            }
            Err(error) => Err(error),
        };
        drop(turn);
        trace_refusal(&result, client);
        self.note_refusal(&result);
        // Le premier mot de passe faux du compte à une confirmation ferme ses élévations, quelle que soit
        // la forme de l'acte (BR-TRUST-043).
        if matches!(result, Err(LoginError::InvalidCredentials))
            && let Some(elevations) = &self.elevations
            && let Ok(name) = Username::parse(username)
            && let Ok(Some(account)) = self.accounts.find_by_username(&name).await
        {
            elevations.close_account(&account.id);
        }
        result
    }

    /// Prend sa place dans la file de l'adresse puis attend son tour. Un seul tour, par adresse
    /// exacte : le couple contient l'adresse, deux connexions du même couple sont donc déjà
    /// sérialisées par le tour de leur adresse. Comme pour le flux, une adresse est d'abord admise
    /// comme inconnue ; seule la saturation fait lire en base si elle est déjà connue (d'un compte
    /// quelconque) et peut prendre une place réservée : la décision ne dépend jamais de
    /// l'identifiant saisi (ADR-0022, BR-CONN-013).
    async fn take_turn(&self, client: &ClientInfo) -> Result<Turn<'_>, LoginError> {
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
        Ok(turn)
    }

    /// Du début de la tentative à son issue : admission, preuve de clé, vérification du mot de passe,
    /// règle de reconnaissance du poste, compteurs. Rend la transaction ouverte quand la connexion
    /// est accordée ; tout refus est déjà écrit (compteurs, journal) et rendu en erreur.
    async fn verify(
        &self,
        username: &str,
        password: Secret,
        client: &ClientInfo,
        device: Option<&DeviceProof>,
        proven: Option<VerifiedKey>,
        purpose: Purpose,
    ) -> Result<Passed, LoginError> {
        let keys = Keys {
            pair: AttemptKey::new(username, &client.addr),
            address: AttemptKey::address(&client.addr),
            identifier: AttemptKey::identifier(username),
            username: Username::parse(username)
                .map_or_else(|_| username.to_owned(), |parsed| parsed.as_str().to_owned()),
        };
        // 1. Admission : pendant une attente du couple ou de l'adresse, on ne vérifie même pas le
        //    mot de passe. Le ralentissement par identifiant, lui, refuse APRÈS la vérification
        //    (étape 3) : même chemin et même durée pour un identifiant existant ou non.
        let now = self.clock.now();
        let early = LoginState {
            pair: self.attempts.get(&keys.pair).await?,
            address: self.attempts.get(&keys.address).await?,
            identifier: identifier_slowdown::Slowdown::default(),
            escapes_slowdown: false,
            seen: false,
        };
        if let Some(retry_after) = admit(&early, now) {
            return Err(LoginError::TooManyAttempts { retry_after });
        }

        // 1 bis. Preuve de la clé d'appareil, s'il y en a une : vérifiée sous la clé FOURNIE, donc
        //    le même travail que l'identifiant existe ou non (HRT-22, absence d'oracle).
        let key: Option<VerifiedKey> = match (proven, device, &self.trust) {
            // Déjà vérifiée par l'acte d'administration qui confirme son mot de passe ici.
            (Some(key), _, _) => Some(key),
            (None, Some(proof), Some(trust)) => {
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
        let target_id = account.as_ref().map(|found| found.id.clone());
        let verified_account = account.filter(|_| verified);

        // 3. Issue, dans une seule transaction : compteurs, et pour un succès la session et la
        //    date de dernière connexion. Les états sont relus dans la transaction ; le haché
        //    vérifié est comparé à celui d'aujourd'hui (même garde que `change_own_password`) :
        //    un mot de passe changé entre-temps ne connecte pas.
        let mut tx = self.store.begin().await?;
        let now = self.clock.now();
        // Les lectures de la règle sont TOUTES faites, que l'identifiant existe ou non (liste vide,
        // clé jamais celle d'un compte absent) : le travail ne dit rien de l'existence du compte.
        let retained = {
            let list = tx.known_addresses().of_username(&keys.username).await?;
            is_known(&list, &client.addr, now)
        };
        // Le poste dont la clé est inscrite pour CE compte (jamais pour un compte absent).
        let key_device: Option<DeviceId> = match &key {
            Some(key) => match tx.devices().find_by_key(&key.key_id).await? {
                Some(enrolled)
                    if target_id.as_ref() == Some(&enrolled.account)
                        && bool::from(enrolled.public_key.ct_eq(&key.public_key)) =>
                {
                    Some(enrolled.id)
                }
                _ => None,
            },
            None => None,
        };
        let key_recognized = key_device.is_some();
        // Le mode attaque : une lecture de la ligne unique, seulement si le service existe. Éteint ou
        // suspendu (fenêtre de redémarrage), c'est le régime de l'identifiant qui s'applique.
        let attack_row = match &self.attack {
            Some(_) => Some(tx.attack_mode().load().await?),
            None => None,
        };
        let attacking = match (&self.attack, &attack_row) {
            (Some(service), Some(row)) => service.effective_of(row) == Effective::Active,
            _ => false,
        };
        let before = LoginState {
            pair: tx.login_attempts().get(&keys.pair).await?,
            address: tx.login_attempts().get(&keys.address).await?,
            identifier: tx.login_attempts().identifier(&keys.identifier).await?,
            escapes_slowdown: false,
            // Connue d'un compte quelconque : lue pour tout identifiant, existant ou non.
            seen: tx
                .known_addresses()
                .address_is_known(&canonical(&client.addr), known_address::cutoff(now))
                .await?,
        };
        // La règle « 2 critères sur 3 » (ADR-0024) : fonction pure, deux booléens. « Du premier
        // coup » : le compteur du couple est à zéro.
        // L'essai unique : seulement quand UN critère est présenté seul (mode attaque). Lu dans la
        // transaction `BEGIN IMMEDIATE` : deux tentatives simultanées ne le consomment pas deux fois.
        let trial_subject: Option<(TrialKind, String)> = match (retained, &key_device) {
            (true, None) => Some((TrialKind::Address, canonical(&client.addr))),
            (false, Some(device)) => Some((TrialKind::Key, device.to_string())),
            _ => None,
        };
        let activation = attack_row
            .as_ref()
            .and_then(|row| row.activation_id.clone());
        let trial_used = match (&trial_subject, &activation, &target_id) {
            (Some((kind, subject)), Some(activation), Some(account)) if attacking => {
                tx.attack_mode()
                    .trial_used(activation, account, *kind, subject)
                    .await?
            }
            // Un mode actif sans identifiant d'activation n'existe pas (l'activation écrit les deux) :
            // si la ligne l'était quand même, aucun essai n'est jamais donné (fermé, jamais ouvert).
            (Some(_), None, _) if attacking => true,
            _ => false,
        };
        let mode = if attacking {
            Mode::Attack
        } else {
            mode_of(&before.identifier, now)
        };
        let standing = judge_login(
            mode,
            LoginCriteria {
                address: retained,
                key: key_recognized,
                first_try: before.pair.failures == 0,
                trial_used,
            },
        );
        let before = LoginState {
            escapes_slowdown: standing.escapes_slowdown,
            ..before
        };
        let account =
            match verified_account {
                Some(account) => tx.accounts().find(&account.id).await?.filter(|current| {
                    current.password_hash.expose() == account.password_hash.expose()
                }),
                None => None,
            };
        let (after, verdict) = conclude(before, account.is_some() && standing.password_counts, now);
        // Écrit ce qui a changé ; une ligne neuve peut dépasser la borne de la table.
        let vacant = LoginState {
            pair: LockoutState::default(),
            address: LockoutState::default(),
            identifier: identifier_slowdown::Slowdown::default(),
            escapes_slowdown: before.escapes_slowdown,
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
        let alert = alert_change(&before.identifier, &after.identifier);

        // L'essai unique : seul un essai RATÉ le consomme (BR-TRUST-015) ; un essai réussi laisse le poste
        // reconnu (BR-TRUST-014). Écrit dans la transaction de la tentative, jamais pour une tentative
        // refusée avant d'avoir été comptée (`Blocked`) ; consigné au journal quand la ligne change.
        let mut journal = Pending::default();
        if let (Some(kind), Some((_, subject)), Some(activation), Some(account_id)) =
            (standing.trial, &trial_subject, &activation, &target_id)
            && !matches!(verdict, Verdict::Blocked(_))
        {
            let succeeded = matches!(verdict, Verdict::Granted);
            let changed = tx
                .attack_mode()
                .record_trial(activation, account_id, kind, subject, now, succeeded)
                .await?;
            if changed {
                journal
                    .record(
                        &mut *tx,
                        AuditEvent::new(
                            now,
                            Actor::new(
                                targeted.clone(),
                                Origin::client(Some(&client.name), &client.addr),
                            ),
                            AuditAction::AttackModeTrial,
                            Target::Trial(kind),
                            if succeeded {
                                Outcome::Succeeded
                            } else {
                                Outcome::Denied(Reason::InvalidCredentials)
                            },
                        ),
                    )
                    .await?;
            }
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
                journal.publish(&self.trail);
                if purpose.journals_throttle() {
                    self.journal_refusal(
                        purpose,
                        targeted,
                        client,
                        Outcome::Denied(Reason::TooManyAttempts {
                            retry_after_s: retry_after_seconds(retry_after),
                        }),
                    )
                    .await;
                }
                return Err(LoginError::TooManyAttempts { retry_after });
            }
            Verdict::Failed(wait) => {
                tx.commit().await?;
                journal.publish(&self.trail);
                // Journal (BR-AUDIT-003, 005, 006, 007), une fois les compteurs validés : la
                // tentative refusée, avec le compte visé seulement s'il existe (la raison est la
                // même que l'identifiant existe ou non, et l'identifiant saisi n'est jamais
                // retenu), puis le blocage qu'elle a éventuellement déclenché. **Par le
                // regroupement des refus** (`AuditSink`) : une rafale de refus depuis une même
                // adresse n'écrit qu'un premier refus et des synthèses, et ne chasse pas
                // l'historique du journal.
                // Mode attaque : une connexion bloquée sans essai (aucun critère, ou essai déjà raté)
                // porte la raison « poste non reconnu » au journal (conception 5.9, lisible des seuls
                // administrateurs) ; la réponse au client, elle, est celle d'un mot de passe faux.
                let reason = if Username::parse(username).is_err() {
                    Reason::InvalidIdentifier
                } else if attacking && !standing.password_counts {
                    Reason::NotRecognized
                } else {
                    Reason::InvalidCredentials
                };
                // Un mot de passe faux à la confirmation d'un acte est consigné par la route de
                // l'acte (`device.remove`, « mot de passe actuel incorrect ») : pas une connexion
                // refusée de plus.
                if purpose.journals_wrong_password() {
                    self.journal_refusal(
                        purpose,
                        targeted.clone(),
                        client,
                        Outcome::Denied(reason),
                    )
                    .await;
                }
                if let Some(retry_after) = wait {
                    let actor = Actor::new(
                        targeted.clone(),
                        Origin::client(Some(&client.name), &client.addr),
                    );
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
                // L'épisode d'alerte qui commence ou finit avec cet échec (une fois par épisode,
                // seulement pour un compte qui existe : rien pour un identifiant inexistant).
                self.signal_alert(alert, targeted, client, wait);
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
        Ok(Passed {
            tx,
            account,
            now,
            key,
            journal,
        })
    }

    /// Le mot de passe est juste et le poste admis : cette adresse devient (ou reste) retenue pour
    /// ce compte, et seulement ainsi (ADR-0022), la session s'ouvre, le poste à clé s'inscrit ou se
    /// date. Une seule transaction avec les compteurs de `verify`.
    async fn open_session(
        &self,
        passed: Passed,
        client: &ClientInfo,
    ) -> Result<LoginOutcome, LoginError> {
        let Passed {
            mut tx,
            account,
            now,
            key,
            mut journal,
        } = passed;
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
        Ok(LoginOutcome {
            token,
            session_id: session.id,
            expires_at: session.expires_at,
            account: view,
            device: device_status,
        })
    }

    /// Dit à l'alerte qu'un épisode commence ou finit. **Hors du chemin de la réponse** : une tâche
    /// détachée écrit l'entrée, la tentative ne l'attend jamais. Et la tâche est lancée **de la même
    /// façon que le compte existe ou non** (elle n'écrit rien s'il n'existe pas) : aucun écart de
    /// travail ni de temps sur le chemin de la requête n'est observable (BR-CONN-013).
    fn signal_alert(
        &self,
        change: Option<AlertChange>,
        targeted: Option<Username>,
        client: &ClientInfo,
        wait: Option<Duration>,
    ) {
        let (Some(change), Some(security)) = (change, &self.security) else {
            return;
        };
        security.spawn_signal(
            change,
            targeted,
            Origin::client(Some(&client.name), &client.addr),
            wait,
        );
    }

    /// Écrit un refus de connexion au journal, par le regroupement des refus (une entrée puis des
    /// synthèses au compte exact, BR-AUDIT-007). Ni l'identifiant saisi ni le mot de passe ne sont
    /// retenus ; le compte visé n'est renseigné que s'il existe (BR-AUDIT-006).
    async fn journal_refusal(
        &self,
        purpose: Purpose,
        targeted: Option<Username>,
        client: &ClientInfo,
        outcome: Outcome,
    ) {
        self.sink
            .record(
                Actor::new(targeted, Origin::client(Some(&client.name), &client.addr)),
                purpose.action(),
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
        self.confirm_password_proven(
            session.account.username.as_str(),
            password,
            client,
            Some(proven.clone()),
            Purpose::Confirmation(AuditAction::DeviceRemove),
        )
        .await
        .map_err(|error| RemoveError::Password(Box::new(error)))?;
        trust
            .remove_proven(&session.account.id, &session.session_id, id, by, &proven)
            .await?;
        // Poste retiré : l'élévation liée à ce poste se ferme (BR-TRUST-043).
        if let Some(elevations) = &self.elevations {
            elevations.close_device(&DeviceId::new(id));
        }
        Ok(())
    }

    /// Confirme un acte d'administration (HRT-28, BR-TRUST-036, 039, 040, 041, 043) : la preuve de
    /// possession d'une clé inscrite du compte, liée à l'acte reconstruit, **puis** le mot de passe, par
    /// le chemin de la connexion (mêmes compteurs, même ralentissement), sauf élévation en cours.
    ///
    /// Ordre : (1) la preuve, **avant tout mot de passe** : une session volée, sans clé, n'essaie rien ;
    /// (2) la réservation du défi (une seule requête simultanée par défi) ; (3) l'élévation, ou le mot de
    /// passe. Rien n'est écrit tant que le mot de passe n'est pas vérifié, et le défi n'est consommé que
    /// par [`Reauthenticated::finish`], après la réussite de l'acte.
    pub async fn reauthenticate(
        &self,
        session: &CurrentSession,
        token: &str,
        act: &AdminAct<'_>,
        reauth: &Reauth,
        client: &ClientInfo,
    ) -> Result<Reauthenticated, ReauthError> {
        let Some(trust) = self.trust.as_ref() else {
            return Err(ReauthError::Unavailable);
        };
        let token_hash = SessionToken::parse(token)
            .map_err(|_| ReauthError::ProofInvalid)?
            .hash();
        let (key, device) = trust
            .verify_act(
                &session.account.id,
                session.account.username.as_str(),
                token_hash.as_bytes(),
                act,
                reauth.device.as_ref(),
                &client.addr,
            )
            .await?;
        let reservation = trust.reserve(&key).ok_or(ReauthError::ProofInvalid)?;
        // Le défi est retenu comme consommé AVANT l'effet de l'acte, une fois la confirmation acquise (un
        // mot de passe faux ne le brûle pas) : s'il ne peut pas l'être (déjà pris, ou part du compte
        // pleine), l'acte est refusé, jamais fait avec une preuve rejouable (BR-TRUST-039).
        let confirmed = |verified: Option<Secret>| -> Result<Reauthenticated, ReauthError> {
            if !trust.consume(&session.account.id, &key) {
                return Err(ReauthError::ProofInvalid);
            }
            Ok(Reauthenticated(Arc::new(ReauthInner {
                verified,
                _reservation: Some(reservation),
            })))
        };
        // Pendant le mode attaque (actif ou suspendu), aucune élévation n'existe.
        let attacking = match &self.attack {
            Some(attack) => attack.effective().await? != Effective::Off,
            None => false,
        };
        let windowed = !attacking
            && self.accounts.reauth_mode(&session.account.id).await? == ReauthMode::Window;
        if windowed
            && covered_by_elevation(act)
            && let Some(elevations) = &self.elevations
            && elevations.covers(&session.session_id, &device, &client.addr)
        {
            return confirmed(None);
        }
        if reauth.password.is_empty() {
            return Err(ReauthError::PasswordRequired);
        }
        let purpose = Purpose::Act(act_audit_action(act));
        let verified = self
            .confirm_password_proven(
                session.account.username.as_str(),
                Secret::from(reauth.password.clone()),
                client,
                Some(key.clone()),
                purpose,
            )
            .await
            .map_err(|error| ReauthError::Password(Box::new(error)))?;
        if windowed && let Some(elevations) = &self.elevations {
            elevations.open(
                &session.session_id,
                &session.account.id,
                &device,
                &client.addr,
            );
            // L'ouverture est consignée : qui (le compte), quel poste (le nom annoncé à l'inscription de
            // la clé prouvée), d'où (l'origine), quand (l'horodatage de l'entrée). Jamais le mot de passe
            // (BR-TRUST-053).
            let device_name = trust
                .list(&session.account.id, &session.session_id)
                .await
                .ok()
                .and_then(|devices| devices.into_iter().find(|view| view.id == device))
                .and_then(|view| ClientName::parse(&view.name));
            self.sink
                .record(
                    Actor::new(
                        Some(session.account.username.clone()),
                        Origin::client(Some(&client.name), &client.addr),
                    ),
                    AuditAction::ReauthElevation,
                    device_name.map_or(Target::None, Target::Device),
                    Outcome::Succeeded,
                )
                .await;
        }
        confirmed(Some(verified))
    }

    /// Active ou désactive le mode attaque pour un acte déjà confirmé par la couche `reauth`. Activer
    /// ferme toutes les élévations.
    pub async fn change_attack_mode_confirmed(
        &self,
        session: &CurrentSession,
        active: bool,
        by: &Actor,
    ) -> Result<AttackStatus, AttackModeError> {
        let Some(attack) = self.attack.as_ref() else {
            return Err(AttackModeError::Unavailable);
        };
        if !session.account.role.can_manage_accounts() {
            return Err(AttackModeError::Forbidden);
        }
        Ok(self.change_attack_mode(attack, active, by).await?)
    }

    /// Le seul endroit où le mode attaque change par une route (forme à plat comme forme `reauth`) : il
    /// ferme toutes les élévations, qu'on l'allume ou qu'on l'éteigne (aucune ne revit à l'extinction).
    async fn change_attack_mode(
        &self,
        attack: &AttackModeService,
        active: bool,
        by: &Actor,
    ) -> Result<AttackStatus, StoreError> {
        let status = attack.change(active, by, EndHow::Manual).await?;
        if let Some(elevations) = &self.elevations {
            elevations.close_all();
        }
        Ok(status)
    }

    /// Le réglage de fréquence du mot de passe du compte (HRT-28, BR-TRUST-042). Remettre `each` ferme
    /// les élévations du compte.
    pub async fn set_reauth_mode(
        &self,
        account: &AccountView,
        mode: ReauthMode,
        by: &Actor,
    ) -> Result<(), StoreError> {
        let mut tx = self.store.begin().await?;
        tx.accounts().set_reauth_mode(&account.id, mode).await?;
        let mut journal = Pending::default();
        journal
            .record(
                &mut *tx,
                AuditEvent::new(
                    self.clock.now(),
                    by.clone(),
                    AuditAction::ReauthSetting,
                    Target::None,
                    Outcome::Succeeded,
                ),
            )
            .await?;
        tx.commit().await?;
        journal.publish(&self.trail);
        if mode == ReauthMode::Each
            && let Some(elevations) = &self.elevations
        {
            elevations.close_account(&account.id);
        }
        Ok(())
    }

    /// Ce que `GET /security` annonce de la confirmation des actes pour cette session depuis cette
    /// adresse. `None` : l'agent n'a pas l'identité d'appareil (aucune confirmation possible).
    pub async fn admin_reauth_info(
        &self,
        session: &CurrentSession,
        addr: &str,
        required: bool,
    ) -> Result<Option<AdminReauthInfo>, StoreError> {
        if self.trust.is_none() {
            return Ok(None);
        }
        let mode = self.accounts.reauth_mode(&session.account.id).await?;
        let attacking = match &self.attack {
            Some(attack) => attack.effective().await? != Effective::Off,
            None => false,
        };
        let elevated_for_s = match &self.elevations {
            Some(elevations) if !attacking && mode == ReauthMode::Window => {
                elevations.remaining_s(&session.session_id, addr)
            }
            _ => 0,
        };
        Ok(Some(AdminReauthInfo {
            required,
            factors: vec!["password".to_owned(), "device_key".to_owned()],
            password: mode,
            elevated_for_s,
        }))
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
        // Mode attaque : une session ne sert que si le poste réunit un deuxième critère, l'adresse retenue
        // ou une clé prouvée. Présentée seule, elle est refusée comme une session expirée, sans essai,
        // et sans être détruite (Q12). Éteint ou suspendu : aucune lecture de plus.
        let attacking = match &self.attack {
            Some(attack) => attack.effective().await? == Effective::Active,
            None => false,
        };
        let retained = if attacking {
            match addr {
                Some(addr) => is_known(&self.known.of_account(&account.id).await?, addr, now),
                None => false,
            }
        } else {
            true
        };
        // Sans preuve, la décision est prise avant toute écriture.
        if attacking
            && key.is_none()
            && judge_session(Mode::Attack, retained, false) == SessionStanding::Refused
        {
            return Err(self.refuse_session(&account.username, addr).await);
        }
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
            // Une preuve qui n'a pas servi (clé non inscrite pour ce compte, défi déjà pris) ne fait pas un
            // deuxième critère : la transaction est abandonnée, rien n'est renouvelé ni appris.
            if attacking
                && judge_session(Mode::Attack, retained, proved) == SessionStanding::Refused
            {
                drop(tx);
                return Err(self.refuse_session(&account.username, addr).await);
            }
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

    /// Une session valide présentée seule en mode attaque : la tentative est consignée (regroupée, jamais
    /// une entrée par requête), la sortie automatique repart de zéro, et l'appelant reçoit la réponse
    /// d'une session expirée (`AuthError::NotRecognized`). La session n'est ni supprimée ni marquée.
    async fn refuse_session(&self, account: &Username, addr: Option<&str>) -> AuthError {
        let addr = addr.unwrap_or_default();
        tracing::warn!(%addr, reason = "session_alone", "session refusée en mode attaque");
        if let Some(attack) = &self.attack {
            attack.note_refusal();
        }
        self.sink
            .record(
                Actor::new(Some(account.clone()), Origin::client(None, addr)),
                AuditAction::SessionRefused,
                Target::None,
                Outcome::Denied(Reason::NotRecognized),
            )
            .await;
        AuthError::NotRecognized
    }

    /// Déconnexion explicite : supprime la session courante. `by` : le compte et l'origine de la
    /// requête (journal d'activité, BR-AUDIT-003).
    pub async fn logout(&self, session: &SessionId, by: &Actor) -> Result<(), StoreError> {
        if let Some(elevations) = &self.elevations {
            elevations.close_session(session);
        }
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
