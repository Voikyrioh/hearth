//! Le registre des services et l'état du moteur de conteneurs, tenus à jour en direct (HRT-61,
//! `BR-ENGINE-009` à `BR-ENGINE-013`, `BR-SVC-001`, `BR-SVC-015`).
//!
//! Le registre vit en mémoire et se reconstruit depuis le moteur à chaque démarrage de l'agent : il
//! n'écrit rien dans le moteur ni dans la base (lecture seule). Une tâche (`run`) :
//! 1. sonde le moteur (`/_ping` toutes les 2 s quand il est prêt, toutes les 5 s sinon) ;
//! 2. quand il est prêt, relit tous les conteneurs puis suit les événements du moteur : les
//!    événements d'un lot de 250 ms sont regroupés, chaque conteneur touché est relu (un seul) ;
//! 3. si le flux d'événements se rompt, sonde tout de suite puis relit tout.
//!
//! Un conteneur dont l'inspection échoue (disparu, réponse illisible, délai) ne gèle pas la liste :
//! il garde sa dernière observation, ou apparaît « non reconnu » s'il est nouveau, et la relecture
//! est retentée en reculant (2 s, 4 s, 8 s… jusqu'à 60 s). Une liste refusée (401, 403) alors que la
//! sonde répond donne « accès refusé », pas « prêt » avec une liste vide.
//!
//! Services supervisés (HRT-63, ADR-0037) : le propriétaire de chaque NOUVEAU conteneur est cherché
//! une fois (`ServiceSupervisor::owners`, mis en cache pour la vie du conteneur). Un conteneur lancé
//! sous une unité systemd porte la clé `u_<unité>` : le même service quel que soit le conteneur que le
//! système recrée. Son dernier état connu est retenu (`ServiceRepo`, table `supervised_services`) et,
//! quand le conteneur disparaît, le service reste listé d'après l'état de son unité (relue toutes les
//! deux périodes de sonde). Détection seule : aucune action sur une unité.
//!
//! Publication (HRT-79, ADR-0043, `BR-ADOPT-001`) : le registre LIT tous les conteneurs, mais n'en
//! publie que deux sortes : les services créés par Hearth (clé `h_`) et les services adoptés. Les
//! autres sont comptés (`EngineStatus::external_count`), jamais détaillés. Le filtre est appelé à UN
//! seul endroit, `publish`, à la fin de chaque recalcul : l'instantané, les lots de changements et
//! toute route qui cherche un service par sa clé ne voient que ce qui a passé ce filtre. Un
//! changement qui ne touche qu'un conteneur caché n'avance donc pas `rev` et ne fait partir aucun
//! lot ; seul le compte des non gérés, voulu, bouge.
//!
//! L'état du moteur passe par `Debounce` : au plus un changement affiché toutes les 5 secondes.
//! Chaque changement est un lot (`ServicesChange`) diffusé aux abonnés ; un abonné trop lent
//! (`RecvError::Lagged`) relit un instantané complet. `rev` croît à chaque changement et repart de
//! zéro au redémarrage.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use futures_util::{StreamExt, stream};
use time::OffsetDateTime;
use tokio::sync::{Notify, broadcast};
use tokio::time::{Instant, MissedTickBehavior, interval, sleep, sleep_until};

use async_trait::async_trait;

use crate::application::ports::{
    AdoptedRecord, AuditSink, Clock, ContainerEngine, ContainerRef, EngineError, EnvEntry,
    HostDescription, HostEnvironment, MonotonicClock, ServiceRepo, ServiceSupervisor, StoreError,
    SupervisedRecord,
};
use crate::domain::audit::{Actor, AuditAction, ClientName, Outcome, Target};
use crate::domain::services::image::ImageName;
use crate::domain::services::logs::{LogContainer, LogOrigin, LogTarget};
use crate::domain::services::{
    Adoption, Announced, Debounce, Derived, EngineProbe, EngineState, Holder, Observation,
    PortProtocol, ProbeOutcome, RunState, ServiceKey, ServiceStateKind, StartWindow, UnitLookup,
    UnitName, UnitStatus, Visibility, classify_probe, compare_names, confirmation_expired,
    derive_state, derive_supervised, gone_long_enough, lapsed, unit_is_gone, unread_unit_adoptions,
    visibility_of,
};

/// Capacité de la diffusion : un abonné qui prend plus de 64 lots de retard relit un instantané.
const CHANNEL_CAPACITY: usize = 64;
/// Inspections simultanées au plus lors de la relecture des conteneurs touchés par un lot d'événements.
const REFRESH_CONCURRENCY: usize = 8;
/// Délai de la relecture des conteneurs touchés : au-delà, le moteur est à revérifier.
const REFRESH_TIMEOUT: Duration = Duration::from_secs(30);
/// La relecture qui échoue ou laisse des conteneurs illisibles est retentée au plus toutes les
/// `ready_period` x 30 (60 s en production).
const MAX_BACKOFF_FACTOR: u32 = 30;

/// Les cadences du registre. Celles de production sont celles de la conception ; les tests les
/// raccourcissent.
#[derive(Debug, Clone, Copy)]
pub struct EngineTimings {
    /// Entre deux sondes quand le moteur est prêt (`/_ping` toutes les 2 s).
    pub ready_period: Duration,
    /// Entre deux sondes sinon.
    pub other_period: Duration,
    /// Un changement d'état du moteur affiché au plus une fois par fenêtre (`BR-ENGINE-013`).
    pub debounce_window: Duration,
    /// Les événements du moteur reçus pendant cette fenêtre sont regroupés en un lot.
    pub batch_window: Duration,
    /// « Vérifier à nouveau » plus rapproché que cela rend l'état courant sans sonder.
    pub min_recheck: Duration,
}

impl EngineTimings {
    pub const fn production() -> Self {
        Self {
            ready_period: Duration::from_secs(2),
            other_period: Duration::from_secs(5),
            debounce_window: Duration::from_secs(5),
            batch_window: Duration::from_millis(250),
            min_recheck: Duration::from_secs(1),
        }
    }
}

/// L'état du moteur tel que l'agent le publie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineStatus {
    pub state: EngineState,
    /// Nom et version annoncés par le moteur, seulement s'il a répondu.
    pub name: Option<String>,
    pub version: Option<String>,
    pub last_ready_at: Option<OffsetDateTime>,
    pub checked_at: OffsetDateTime,
    /// Dernière lecture COMPLÈTE réussie de la liste (`checked_at` date la sonde, pas la liste).
    /// Information seulement : elle ne dit pas depuis quand la liste est périmée.
    pub list_read_at: Option<OffsetDateTime>,
    /// La liste affichée est peut-être périmée : lecture en échec, conteneurs illisibles, suivi en
    /// direct suspendu, ou liste pas encore relue depuis le retour du moteur. Le client dit
    /// « Impossible de lire les services » et date la liste.
    pub list_degraded: bool,
    /// L'heure de l'agent au moment où cet état a été fabriqué (HRT-72).
    pub at: OffsetDateTime,
    /// Dernier moment où la liste était à jour (HRT-72) : `at` si elle l'est maintenant, sinon
    /// l'instant où l'agent a vu qu'elle cessait de l'être. `None` : aucune liste n'a été lue.
    pub list_current_at: Option<OffsetDateTime>,
    /// Conteneurs que le moteur connaît, arrêtés compris, qui ne sont ni créés par Hearth ni adoptés
    /// (HRT-79, `BR-ADOPT-002`). Un nombre : rien d'autre n'en sort.
    pub external_count: u32,
}

/// Un service tel que la liste le montre : le conteneur observé et ce qu'on en dérive.
#[derive(Debug, Clone, PartialEq)]
pub struct ServiceView {
    pub key: ServiceKey,
    pub observation: Observation,
    pub derived: Derived,
    /// Démarrages non demandés des 10 dernières minutes.
    pub restarts_10m: u32,
    /// Le service est lancé par une unité systemd (clé `u_`) : l'unité, son état, et si son conteneur
    /// existe. `None` pour tout autre service.
    pub supervision: Option<Supervision>,
    /// Présent pour un service adopté (HRT-79) ; absent pour un service créé par Hearth.
    pub adoption: Option<Adoption>,
}

/// Un conteneur non géré proposé à l'adoption (HRT-80, `BR-ADOPT-006`). C'est TOUT ce que la lecture des
/// candidats sait d'un conteneur : de quoi choisir, rien de plus (ni variable, ni commande, ni port, ni
/// dossier, ni identifiant du conteneur). Il n'est jamais gardé : chaque lecture le recalcule depuis le
/// registre, au moment où un compte qui a le droit ouvre la fenêtre.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub key: ServiceKey,
    pub name: String,
    pub image: ImageName,
    /// L'unité du système qui le lance (`u_<unité>`), absente sans unité.
    pub unit: Option<UnitName>,
    pub state: ServiceStateKind,
}

/// Ce que le registre sait d'une clé que l'acte d'adoption reçoit (HRT-80). Une clé que le registre ne
/// voit pas, une clé `h_` (déjà publiée, créée par Hearth) et une clé qu'il ne reconnaît pas sont la même
/// chose pour l'appelant : `Unknown` (`BR-ADOPT-004`).
#[derive(Debug, Clone, PartialEq)]
pub enum AdoptionLookup {
    /// Un conteneur non géré que le moteur connaît en ce moment : adoptable.
    Candidate(Candidate),
    /// Déjà publié comme service adopté, par ce compte.
    Adopted {
        by: String,
    },
    Unknown,
}

/// Ce qui rattache un service à son unité (HRT-63).
#[derive(Debug, Clone, PartialEq)]
pub struct Supervision {
    pub unit: UnitName,
    /// L'état de l'unité, `None` tant que le superviseur n'a rien dit (muet ou pas encore relue).
    pub status: Option<UnitStatus>,
    /// Le conteneur du service existe. `false` : le service est listé d'après son unité, avec ses
    /// derniers réglages connus.
    pub container_present: bool,
}

/// L'état complet : moteur et services (triés par nom sans tenir compte de la casse).
#[derive(Debug, Clone, PartialEq)]
pub struct ServicesSnapshot {
    pub rev: u64,
    pub engine: EngineStatus,
    pub host: HostDescription,
    pub services: Vec<ServiceView>,
}

/// Un lot de changements.
#[derive(Debug, Clone, PartialEq)]
pub struct ServicesChange {
    pub rev: u64,
    /// Présent seulement si l'état du moteur (ou son nom, sa version) a changé.
    pub engine: Option<EngineStatus>,
    pub upserts: Vec<ServiceView>,
    pub removed: Vec<ServiceKey>,
}

/// Ce qui change l'état publié du moteur (hors dates de contrôle, qui changent à chaque sonde).
type EngineKey = (EngineState, Option<String>, Option<String>, bool, usize);

struct Inner {
    rev: u64,
    debounce: Debounce,
    /// Ce que la dernière sonde a classé (l'état publié peut être en retard de la fenêtre).
    observed: EngineState,
    /// Nom et version annoncés par le moteur pour l'état publié.
    shown: Option<Announced>,
    /// Ce que la dernière sonde a annoncé (publié avec l'état qui la suit, pas avant).
    last_announced: Option<Announced>,
    last_ready_at: Option<OffsetDateTime>,
    checked_at: OffsetDateTime,
    /// Instant monotone de la dernière sonde lancée.
    last_probe: Option<Duration>,
    containers: HashMap<String, Observation>,
    windows: HashMap<ServiceKey, StartWindow>,
    published_engine: EngineKey,
    /// Ce qui a passé le filtre de publication (`publish`) : la SEULE vue des services qui sorte du
    /// registre. Les conteneurs cachés ne sont que dans `containers`.
    published: HashMap<ServiceKey, ServiceView>,
    /// Les clés de ce que le filtre a retenu (ni créé par Hearth ni adopté). Ne sert qu'à la décision de
    /// lecture des journaux (`log_target`), qui les rend comme un service hors Hearth non adopté : le
    /// verdict est rendu par `may_read`, le même pour la clé cachée que pour la clé inconnue. Aucune
    /// donnée du conteneur n'y est gardée, et rien d'autre ne le lit.
    hidden: HashSet<ServiceKey>,
    /// Services adoptés (HRT-79), lus en base (`reload_adopted`). Vide tant qu'ils ne sont pas lus :
    /// rien n'est publié par défaut.
    adopted: HashMap<ServiceKey, Adoption>,
    /// Adoptions tombées avec leur conteneur (`BR-ADOPT-017`), à effacer de la base.
    lapsed: HashSet<ServiceKey>,
    /// Adoptions par unité « à confirmer » (`BR-ADOPT-027`), avec le moment où elles le sont devenues :
    /// l'unité n'est plus relue, rien ne dit que c'est encore la même. Elles ne sont PLUS dans
    /// `adopted` (donc invisibles et sans droit) ; leur ligne reste en base jusqu'à la confirmation ou
    /// l'échéance. En mémoire seulement : un redémarrage de l'agent relit l'unité.
    unconfirmed: HashMap<ServiceKey, OffsetDateTime>,
    /// Combien de conteneurs ne sont ni créés par Hearth ni adoptés.
    external: usize,
    /// Conteneurs vus dans la liste mais illisibles à la dernière relecture complète.
    degraded: usize,
    /// La liste a été refusée (401, 403) alors que la sonde répond : l'accès est refusé.
    list_denied: bool,
    /// Sondes en cours (un compteur : la sonde de la boucle et celle d'une relance peuvent se
    /// chevaucher) ; une relance demandée pendant ce temps n'en lance pas une autre.
    probing: u32,
    /// Date de la dernière relecture complète réussie (sans conteneur illisible).
    list_read_at: Option<OffsetDateTime>,
    /// La liste a été relue depuis la dernière fois que le moteur a été OBSERVÉ non prêt, quoi que
    /// l'anti-clignotement ait publié entre-temps (inaccessible compris) : tant que ce n'est pas
    /// fait, ce que l'agent publie sous « prêt » n'est pas présenté comme frais. Source de vérité de
    /// « cette liste date-t-elle d'avant la coupure » ; limite connue : BR-SVC-018.
    list_loaded: bool,
    /// Un état autre que « prêt » a été publié depuis la dernière relecture faite sous « prêt » publié :
    /// le client a vu la liste grisée ou absente, la marque « pas relue » est due à son retour. Ne
    /// décide que de ce qui est MONTRÉ (une coupure retenue par l'anti-clignotement ne fait pas
    /// clignoter la marque) ; la fraîcheur, elle, suit `list_loaded`.
    gap_shown: bool,
    /// Le registre contient une liste lue (faux au départ, et quand il est vidé : moteur absent,
    /// non pris en charge). Sans liste, aucune date de fraîcheur n'existe : « jamais lue ».
    has_list: bool,
    /// Depuis quand la liste n'est plus à jour (posé quand elle cesse de l'être, levé quand elle
    /// l'est de nouveau) : c'est la date que le fil porte, pas celle de la dernière relecture.
    stale_since: Option<OffsetDateTime>,
    /// La dernière relecture de la liste a échoué (tant qu'une relecture n'a pas réussi).
    read_failing: bool,
    /// Le suivi en direct est dégradé : flux d'événements qui casse en boucle, ou conteneur touché
    /// par un événement et illisible. Une relecture complète propre le rétablit.
    tracking_lost: bool,
    /// Propriétaire de chaque conteneur connu (`None` : lancé par personne qu'on sache voir). Cherché
    /// une fois par conteneur ; oublié avec lui.
    owners: HashMap<String, Option<UnitName>>,
    /// Ce que le superviseur dit de chaque unité connue.
    units: HashMap<UnitName, UnitLookup>,
    /// Lectures « introuvable, sans conteneur » consécutives par unité (`BR-ADOPT-017`) : l'adoption ne
    /// tombe qu'à `UNIT_GONE_READS`.
    gone_reads: HashMap<UnitName, u32>,
    /// Les services supervisés retenus, par unité.
    remembered: HashMap<UnitName, Remembered>,
    /// Unités dont la mémoire reste à écrire en base, et unités à oublier.
    dirty: HashSet<UnitName>,
    forgotten: HashSet<UnitName>,
    /// Unités dont le conteneur existait au dernier calcul (pour écrire le moment de la disparition).
    present_units: HashSet<UnitName>,
    /// Dernier code de sortie vu pour une unité, par cet agent : jamais relu du système, absent après
    /// un redémarrage de l'agent (`BR-SVC-005`).
    last_exit: HashMap<UnitName, i64>,
    /// Instant monotone de la dernière relecture de l'état des unités.
    last_unit_poll: Option<Duration>,
}

/// Un service supervisé retenu : ses derniers réglages connus (sans variable ni commande) et le
/// dernier instant où il a été vu.
#[derive(Debug, Clone)]
struct Remembered {
    observation: Observation,
    last_seen: OffsetDateTime,
}

/// Unités relues au plus par passage : un serveur n'en a que quelques-unes ; la borne protège l'agent
/// d'un système qui en lancerait des centaines.
const MAX_UNITS_PER_POLL: usize = 32;
/// Unités ADOPTÉES relues au plus par passage (`BR-ADOPT-027`) : une adoption est un acte humain, il y en a
/// peu, et c'est elle qui ne doit pas cesser d'être relue. Au-delà de la borne, l'adoption dont l'unité
/// n'est pas relue devient « à confirmer ».
const MAX_ADOPTED_UNITS_PER_POLL: usize = 128;
/// Les unités sont relues toutes les `UNIT_POLL_PERIODS` périodes de sonde (4 s en production).
const UNIT_POLL_PERIODS: u32 = 2;

/// L'observation qu'on retient d'un service supervisé : ses réglages, arrêtée, sans nom de variable ni
/// commande.
fn settings_of(observation: &Observation) -> Observation {
    Observation {
        run_state: RunState::Exited,
        exit_code: None,
        started_at: None,
        finished_at: None,
        env_names: Vec::new(),
        env_omitted: 0,
        command: None,
        ..observation.clone()
    }
}

/// Quand le système recrée le conteneur d'un service supervisé, l'ancien peut encore exister un instant :
/// le conteneur qui court l'emporte sur celui qui est arrêté ; à égalité, le plus récent.
fn better(candidate: &Observation, current: &Observation) -> bool {
    let rank = |observation: &Observation| match observation.run_state {
        RunState::Running => 2,
        RunState::Restarting | RunState::Paused | RunState::Removing | RunState::Unknown => 1,
        RunState::Created | RunState::Exited | RunState::Dead => 0,
    };
    rank(candidate)
        .cmp(&rank(current))
        .then_with(|| candidate.created_at.cmp(&current.created_at))
        .then_with(|| candidate.id.cmp(&current.id))
        .is_gt()
}

/// La clé du service d'un conteneur : `u_<unité>` s'il est lancé sous une unité, sinon `c_<id>`.
fn key_of(
    owners: &HashMap<String, Option<UnitName>>,
    observation: &Observation,
) -> Option<ServiceKey> {
    match owners.get(&observation.id) {
        Some(Some(unit)) => Some(ServiceKey::for_unit(unit)),
        _ => ServiceKey::for_container(&observation.id),
    }
}

/// Le dépôt qui ne retient rien : pour un registre sans base (tests de l'application).
struct NoServiceRepo;

#[async_trait]
impl ServiceRepo for NoServiceRepo {
    async fn load_supervised(&self) -> Result<Vec<SupervisedRecord>, StoreError> {
        Ok(Vec::new())
    }

    async fn save_supervised(&self, _record: &SupervisedRecord) -> Result<(), StoreError> {
        Ok(())
    }

    async fn forget_supervised(&self, _unit: &UnitName) -> Result<(), StoreError> {
        Ok(())
    }

    async fn load_adopted(&self) -> Result<Vec<AdoptedRecord>, StoreError> {
        Ok(Vec::new())
    }

    async fn forget_adopted(&self, _key: &ServiceKey) -> Result<(), StoreError> {
        Ok(())
    }
}

/// Aucun superviseur : aucun conteneur n'a de propriétaire.
struct NoSupervision;

#[async_trait]
impl ServiceSupervisor for NoSupervision {
    async fn owners(&self, _containers: &[ContainerRef]) -> HashMap<String, UnitName> {
        HashMap::new()
    }

    async fn unit(&self, _unit: &UnitName) -> UnitLookup {
        UnitLookup::Unavailable
    }
}

/// Une sonde en cours : retirée du compte quand elle finit OU qu'elle est abandonnée en route.
struct ProbingFlag<'a>(&'a ServicesRegistry);

impl Drop for ProbingFlag<'_> {
    fn drop(&mut self) {
        let mut inner = self.0.lock();
        inner.probing = inner.probing.saturating_sub(1);
    }
}

/// Pourquoi les variables d'un service ne sont pas rendues. Aucun texte du moteur n'en fait partie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevealError {
    /// Aucun service de cette clé (disparu, ou jamais vu).
    NotFound,
    /// Le service est listé d'après son unité mais son conteneur n'existe pas : rien à lire.
    NoContainer,
    /// Le moteur ne répond pas ou refuse la lecture.
    EngineUnavailable,
    /// La consignation a échoué : aucune valeur n'est rendue (BR-SVC-009).
    JournalUnavailable,
}

pub struct ServicesRegistry {
    engine: Arc<dyn ContainerEngine>,
    supervisor: Arc<dyn ServiceSupervisor>,
    repo: Arc<dyn ServiceRepo>,
    clock: Arc<dyn Clock>,
    monotonic: Arc<dyn MonotonicClock>,
    host: HostDescription,
    /// `engine.enabled` : à `false`, l'agent n'ouvre jamais le socket.
    enabled: bool,
    timings: EngineTimings,
    state: Mutex<Inner>,
    changes: broadcast::Sender<Arc<ServicesChange>>,
    /// Réveille la tâche (« Vérifier à nouveau »).
    wake: Notify,
}

impl ServicesRegistry {
    /// Un registre sans superviseur ni base : aucun service n'est supervisé, rien n'est retenu.
    pub fn new(
        engine: Arc<dyn ContainerEngine>,
        host: &dyn HostEnvironment,
        clock: Arc<dyn Clock>,
        monotonic: Arc<dyn MonotonicClock>,
        enabled: bool,
        timings: EngineTimings,
    ) -> Arc<Self> {
        Self::with_supervision(
            engine,
            Arc::new(NoSupervision),
            Arc::new(NoServiceRepo),
            host,
            clock,
            monotonic,
            enabled,
            timings,
        )
    }

    /// Le registre complet (HRT-63) : avec le superviseur qui dit à quelle unité appartient un
    /// conteneur, et le dépôt qui retient les services supervisés d'un démarrage à l'autre.
    #[allow(clippy::too_many_arguments)]
    pub fn with_supervision(
        engine: Arc<dyn ContainerEngine>,
        supervisor: Arc<dyn ServiceSupervisor>,
        repo: Arc<dyn ServiceRepo>,
        host: &dyn HostEnvironment,
        clock: Arc<dyn Clock>,
        monotonic: Arc<dyn MonotonicClock>,
        enabled: bool,
        timings: EngineTimings,
    ) -> Arc<Self> {
        let now = monotonic.elapsed().unsigned_abs();
        let mut debounce = Debounce::new(timings.debounce_window, EngineState::Checking);
        // Désactivé : l'état est connu tout de suite, sans sonde, et ne changera plus.
        let observed = if enabled {
            EngineState::Checking
        } else {
            EngineState::Disabled
        };
        if !enabled {
            debounce.observe(now, EngineState::Disabled);
        }
        let published = debounce.published();
        let (changes, _) = broadcast::channel(CHANNEL_CAPACITY);
        Arc::new(Self {
            engine,
            supervisor,
            repo,
            host: host.describe(),
            enabled,
            timings,
            state: Mutex::new(Inner {
                rev: 0,
                debounce,
                observed,
                shown: None,
                last_announced: None,
                last_ready_at: None,
                checked_at: clock.now(),
                last_probe: None,
                containers: HashMap::new(),
                windows: HashMap::new(),
                published_engine: (published, None, None, false, 0),
                published: HashMap::new(),
                hidden: HashSet::new(),
                adopted: HashMap::new(),
                lapsed: HashSet::new(),
                unconfirmed: HashMap::new(),
                external: 0,
                degraded: 0,
                list_denied: false,
                probing: 0,
                list_read_at: None,
                list_loaded: false,
                gap_shown: true,
                has_list: false,
                stale_since: None,
                read_failing: false,
                tracking_lost: false,
                owners: HashMap::new(),
                units: HashMap::new(),
                gone_reads: HashMap::new(),
                remembered: HashMap::new(),
                dirty: HashSet::new(),
                forgotten: HashSet::new(),
                present_units: HashSet::new(),
                last_exit: HashMap::new(),
                last_unit_poll: None,
            }),
            clock,
            monotonic,
            changes,
            wake: Notify::new(),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn mono(&self) -> Duration {
        self.monotonic.elapsed().unsigned_abs()
    }

    /// L'instantané et un récepteur des changements suivants, pris sous le même verrou : aucun
    /// changement ne s'intercale, aucun ne se perd.
    pub fn subscribe(&self) -> (broadcast::Receiver<Arc<ServicesChange>>, ServicesSnapshot) {
        let inner = self.lock();
        let receiver = self.changes.subscribe();
        (receiver, self.snapshot_of(&inner))
    }

    /// L'état complet maintenant (`GET /services`, abonné en retard).
    pub fn snapshot(&self) -> ServicesSnapshot {
        self.snapshot_of(&self.lock())
    }

    /// Ce que l'agent sait du serveur (distribution, init, compte de l'agent).
    pub fn host(&self) -> &HostDescription {
        &self.host
    }

    pub fn engine_status(&self) -> EngineStatus {
        self.status_of(&self.lock())
    }

    fn snapshot_of(&self, inner: &Inner) -> ServicesSnapshot {
        let mut services: Vec<ServiceView> = inner.published.values().cloned().collect();
        sort_views(&mut services);
        ServicesSnapshot {
            rev: inner.rev,
            engine: self.status_of(inner),
            host: self.host.clone(),
            services,
        }
    }

    fn status_of(&self, inner: &Inner) -> EngineStatus {
        let now = self.clock.now();
        EngineStatus {
            state: inner.debounce.published(),
            name: inner.shown.as_ref().map(|shown| shown.name.clone()),
            version: inner.shown.as_ref().map(|shown| shown.version.clone()),
            last_ready_at: inner.last_ready_at,
            checked_at: inner.checked_at,
            list_read_at: inner.list_read_at,
            list_degraded: Self::list_degraded(inner),
            at: now,
            list_current_at: Self::list_current_at(inner, now),
            external_count: u32::try_from(inner.external).unwrap_or(u32::MAX),
        }
    }

    /// FIX:01M4SFCQ7N5TN8VC21KBQR87PP : la liste est marquée aussi tant qu'elle n'a pas été relue
    /// depuis le retour du moteur (`list_loaded`), pas seulement après un échec de lecture.
    fn list_degraded(inner: &Inner) -> bool {
        let state = inner.debounce.published();
        // Seulement sous « prêt » publié, et seulement si le client a vu la coupure : dans un état
        // inaccessible la liste est déjà grisée par l'état, et une coupure retenue par
        // l'anti-clignotement ne doit pas faire basculer la marque (un second changement dans la
        // fenêtre). Ce que l'on DIT de la fraîcheur (`list_fresh`) ne dépend pas de ce choix.
        let awaiting = state == EngineState::Ready && inner.gap_shown && !inner.list_loaded;
        Self::list_unreliable(inner) || awaiting
    }

    /// La liste n'est pas suivie sans réserve : lecture en échec, suivi perdu, conteneur illisible.
    fn list_unreliable(inner: &Inner) -> bool {
        inner.read_failing || inner.tracking_lost || inner.degraded > 0
    }

    /// La liste est à jour maintenant : le moteur est vu prêt (dernière sonde), la liste a été relue
    /// depuis qu'il a été vu non prêt, et elle est suivie sans réserve. Fondé sur l'observé ; seul le
    /// choix de montrer la marque (`gap_shown`) suit le publié (limite connue : BR-SVC-018).
    fn list_fresh(inner: &Inner) -> bool {
        inner.observed == EngineState::Ready && inner.list_loaded && !Self::list_unreliable(inner)
    }

    /// FIX:01M4SFCRJWX8KMBC1ZK9ZWETTW : dernier moment où la liste était à jour, pas fin de la
    /// dernière relecture complète (une liste suivie par les événements est à jour sans relecture).
    fn list_current_at(inner: &Inner, now: OffsetDateTime) -> Option<OffsetDateTime> {
        // Dans les états sans liste, le client n'en a pas : pas de date, quoi que le registre contienne.
        if !inner.has_list || inner.debounce.published().keeps_no_list() {
            return None;
        }
        if Self::list_fresh(inner) {
            Some(now)
        } else {
            inner.stale_since.or(Some(now))
        }
    }

    /// Note l'instant où la liste cesse d'être à jour (premier appel depuis qu'elle l'est) ; le lève
    /// quand elle l'est de nouveau.
    fn track_staleness(inner: &mut Inner, now: OffsetDateTime) {
        if Self::list_fresh(inner) {
            inner.stale_since = None;
        } else if inner.stale_since.is_none() {
            inner.stale_since = Some(now);
        }
    }

    /// Une sonde vient d'observer `observed`, publié ou non. Tant que le moteur n'est pas vu « prêt »,
    /// la liste n'est plus suivie : elle devra être relue au retour, même si l'anti-clignotement
    /// n'a rien publié (coupure retenue, puis retour dans la fenêtre).
    fn note_observed(inner: &mut Inner, observed: EngineState) {
        // FIX:01M4SFCQ7N5TN8VC21KBQR87PP : quel que soit l'état publié (inaccessible compris), un
        // moteur observé non prêt défait la relecture : une liste relue avant une coupure ne repart
        // jamais « fraîche » au retour, quelle que soit la durée de la coupure.
        if observed != EngineState::Ready {
            inner.list_loaded = false;
        }
        Self::drop_list_if_none_kept(inner);
    }

    /// Un état du moteur vient d'être publié.
    fn note_published(inner: &mut Inner) {
        if inner.debounce.published() != EngineState::Ready {
            inner.gap_shown = true;
        } else if inner.list_loaded {
            // « Prêt » est parti avec une liste relue : le client la voit sans marque.
            inner.gap_shown = false;
        }
        Self::drop_list_if_none_kept(inner);
    }

    /// Dans les états sans liste (`BR-SVC-018`), le registre est vidé pour de bon, pas seulement
    /// masqué : à chaque sonde et à chaque publication, pour qu'une relecture faite pendant un retour
    /// retenu n'y survive pas à une seconde disparition sans publication.
    fn drop_list_if_none_kept(inner: &mut Inner) {
        if inner.debounce.published().keeps_no_list() {
            inner.list_loaded = false;
            inner.gap_shown = true;
            inner.containers.clear();
            inner.windows.clear();
            inner.list_read_at = None;
            inner.has_list = false;
            inner.stale_since = None;
        }
    }

    fn engine_key(inner: &Inner) -> EngineKey {
        (
            inner.debounce.published(),
            inner.shown.as_ref().map(|shown| shown.name.clone()),
            inner.shown.as_ref().map(|shown| shown.version.clone()),
            Self::list_degraded(inner),
            inner.external,
        )
    }

    /// « Vérifier à nouveau » : relance la détection tout de suite et rend l'état. Sans effet sur le
    /// moteur ; trop rapproché de la sonde précédente, ou pendant qu'une sonde est déjà en cours,
    /// l'état courant est rendu sans sonder. La demande est ouverte aux deux rôles (spec : « tous ») ;
    /// le résultat est publié tout de suite, hors anti-clignotement : l'état affiché ne change que si
    /// l'état réel du moteur a changé, et au plus une fois par `min_recheck`.
    pub async fn recheck(&self) -> EngineStatus {
        if let Some(flag) = self.enabled.then(|| self.claim_probe()).flatten() {
            self.probe_with(true, flag).await;
            self.wake.notify_one();
        }
        self.engine_status()
    }

    /// Réserve la sonde d'une relance, d'un seul geste : deux demandes simultanées n'en lancent pas
    /// deux.
    fn claim_probe(&self) -> Option<ProbingFlag<'_>> {
        let mut inner = self.lock();
        let now = self.mono();
        let too_soon = inner
            .last_probe
            .is_some_and(|at| now.saturating_sub(at) < self.timings.min_recheck);
        if inner.probing > 0 || too_soon {
            return None;
        }
        inner.last_probe = Some(now);
        inner.probing += 1;
        Some(ProbingFlag(self))
    }

    /// Une sonde a-t-elle eu lieu depuis moins de `within` ?
    fn probed_within(&self, within: Duration) -> bool {
        self.lock()
            .last_probe
            .is_some_and(|at| self.mono().saturating_sub(at) < within)
    }

    /// Sonde (sauf désactivé) puis applique le résultat. Rend l'état observé.
    async fn probe_and_apply(&self, force: bool) -> EngineState {
        let flag = self.enabled.then(|| {
            let mut inner = self.lock();
            inner.last_probe = Some(self.mono());
            inner.probing += 1;
            ProbingFlag(self)
        });
        match flag {
            Some(flag) => self.probe_with(force, flag).await,
            None => self.apply_probe(None, force),
        }
    }

    /// Comme `probe_and_apply` pour une sonde déjà comptée (`flag`).
    async fn probe_with(&self, force: bool, flag: ProbingFlag<'_>) -> EngineState {
        let probe = if self.enabled {
            let probe = self.engine.probe().await;
            drop(flag);
            Some(probe)
        } else {
            None
        };
        self.apply_probe(probe, force)
    }

    fn apply_probe(&self, probe: Option<EngineProbe>, force: bool) -> EngineState {
        // La sonde répond mais la liste est refusée : pour l'utilisateur, l'accès est refusé.
        let probe = match probe {
            Some(EngineProbe {
                outcome: ProbeOutcome::Answered(_),
                installed,
            }) if self.lock().list_denied => Some(EngineProbe {
                outcome: ProbeOutcome::PermissionDenied,
                installed,
            }),
            other => other,
        };
        // Le moteur a disparu (socket absent, refusé, muet) : le refus de la liste ne vaut plus, il ne
        // doit pas s'afficher à tort quand le moteur reviendra.
        if matches!(
            probe.as_ref().map(|probe| &probe.outcome),
            Some(ProbeOutcome::SocketMissing | ProbeOutcome::Refused | ProbeOutcome::NoAnswer)
        ) {
            self.lock().list_denied = false;
        }
        let observed = classify_probe(self.enabled, probe.as_ref());
        let announced = match probe.map(|probe| probe.outcome) {
            Some(ProbeOutcome::Answered(announced)) => Some(announced),
            _ => None,
        };
        let now = self.mono();
        let wall = self.clock.now();
        let mut inner = self.lock();
        inner.checked_at = wall;
        inner.observed = observed;
        Self::note_observed(&mut inner, observed);
        if observed == EngineState::Ready {
            inner.last_ready_at = Some(wall);
        }
        let published = if force {
            inner.debounce.force(now, observed)
        } else {
            inner
                .debounce
                .observe(now, observed)
                .or_else(|| inner.debounce.flush(now))
        };
        if let Some(state) = published {
            tracing::info!(?state, "état du moteur de conteneurs");
            Self::note_published(&mut inner);
        }
        // Le nom et la version suivent l'état publié : ils changent avec lui, ou quand le moteur
        // publié change de version.
        inner.last_announced = announced;
        if published.is_some() || inner.debounce.published() == observed {
            inner.shown = inner.last_announced.clone();
        }
        self.commit(&mut inner);
        observed
    }

    /// Publie l'échéance de l'anti-clignotement si elle est arrivée.
    fn flush_debounce(&self) {
        let now = self.mono();
        let mut inner = self.lock();
        if let Some(state) = inner.debounce.flush(now) {
            tracing::info!(?state, "état du moteur de conteneurs");
            Self::note_published(&mut inner);
            // L'état retenu est celui de la dernière sonde : il part avec ce qu'elle a annoncé.
            inner.shown = inner.last_announced.clone();
            self.commit(&mut inner);
        }
    }

    /// Dans combien de temps un changement retenu pourra partir.
    fn debounce_wait(&self) -> Option<Duration> {
        let deadline = self.lock().debounce.deadline()?;
        Some(deadline.saturating_sub(self.mono()))
    }

    /// Relit tous les conteneurs. Une erreur laisse le registre inchangé. Rend le nombre de
    /// conteneurs illisibles : ils gardent leur dernière observation, ou apparaissent « non
    /// reconnus » s'ils sont nouveaux (`Observation::unreadable`).
    pub async fn resync(&self) -> Result<usize, EngineError> {
        let listing = match self.engine.list().await {
            Ok(listing) => listing,
            Err(error) => {
                {
                    let mut inner = self.lock();
                    inner.read_failing = true;
                    if error != EngineError::Denied {
                        self.commit(&mut inner);
                    }
                }
                if error == EngineError::Denied {
                    self.lock().list_denied = true;
                    self.apply_probe(
                        Some(EngineProbe {
                            outcome: ProbeOutcome::PermissionDenied,
                            installed: true,
                        }),
                        false,
                    );
                }
                return Err(error);
            }
        };
        let unreadable = listing.unreadable.len();
        self.resolve_owners(listing.observations.iter().map(refer).collect())
            .await;
        let recovered = {
            let mut inner = self.lock();
            let previous = std::mem::take(&mut inner.containers);
            if !inner.has_list {
                // Première liste depuis le vidage : sa date de péremption éventuelle ne remonte pas plus
                // haut que cette lecture.
                inner.has_list = true;
                inner.stale_since = None;
            }
            // Une lecture qui finit après que le moteur a été vu partir ne vaut pas relecture.
            if matches!(
                inner.observed,
                EngineState::Ready | EngineState::AccessDenied
            ) {
                inner.list_loaded = true;
                if inner.debounce.published() == EngineState::Ready {
                    inner.gap_shown = false;
                }
            }
            for observation in listing.observations {
                let key = key_of(&inner.owners, &observation);
                note_start(&mut inner.windows, key, &observation);
                inner.containers.insert(observation.id.clone(), observation);
            }
            for missing in listing.unreadable {
                let kept = previous
                    .get(&missing.id)
                    .cloned()
                    .unwrap_or_else(|| Observation::unreadable(&missing.id, &missing.name));
                inner.containers.insert(missing.id, kept);
            }
            if unreadable > 0 && inner.degraded == 0 {
                // Une fois par épisode : la relecture suivante ne répète pas.
                tracing::warn!(
                    unreadable,
                    "conteneurs illisibles : leur dernière description est gardée"
                );
            } else if unreadable == 0 && inner.degraded > 0 {
                tracing::info!("tous les conteneurs sont de nouveau lisibles");
            }
            inner.degraded = unreadable;
            inner.read_failing = false;
            if unreadable == 0 {
                inner.list_read_at = Some(self.clock.now());
                inner.tracking_lost = false;
            }
            let recovered = std::mem::take(&mut inner.list_denied);
            self.commit(&mut inner);
            recovered
        };
        self.persist_supervised().await;
        if recovered {
            // La liste est lisible de nouveau : l'état « prêt » revient avec la prochaine sonde.
            self.probe_and_apply(false).await;
        }
        Ok(unreadable)
    }

    /// Relit les conteneurs touchés par un lot d'événements (un seul échange par conteneur, quelques
    /// uns à la fois). Rend le nombre de conteneurs dont l'inspection a échoué : ils gardent leur
    /// dernière observation. Seule la perte du moteur (ou le dépassement du délai) est une erreur.
    async fn refresh(&self, ids: Vec<String>) -> Result<usize, EngineError> {
        let inspections = stream::iter(ids)
            .map(|id| async move {
                let result = self.engine.inspect(&id).await;
                (id, result)
            })
            .buffer_unordered(REFRESH_CONCURRENCY)
            .collect::<Vec<_>>();
        let read = tokio::time::timeout(REFRESH_TIMEOUT, inspections)
            .await
            .map_err(|_| EngineError::Timeout)?;
        self.resolve_owners(
            read.iter()
                .filter_map(|(_, result)| result.as_ref().ok().and_then(Option::as_ref))
                .map(refer)
                .collect(),
        )
        .await;
        let unreadable = {
            let mut unreadable = 0;
            let mut inner = self.lock();
            for (id, result) in read {
                match result {
                    Ok(Some(observation)) => {
                        let key = key_of(&inner.owners, &observation);
                        note_start(&mut inner.windows, key, &observation);
                        inner.containers.insert(observation.id.clone(), observation);
                    }
                    Ok(None) => {
                        inner.containers.remove(&id);
                    }
                    Err(error) if error.engine_gone() => return Err(error),
                    Err(_) => unreadable += 1,
                }
            }
            if unreadable > 0 {
                inner.tracking_lost = true;
            }
            self.commit(&mut inner);
            unreadable
        };
        self.persist_supervised().await;
        Ok(unreadable)
    }

    /// Recalcule la vue de chaque service et diffuse ce qui a changé, moteur compris. Aucun lot si
    /// rien n'a changé.
    fn commit(&self, inner: &mut Inner) {
        let now = self.clock.now();
        Self::track_staleness(inner, now);
        let mut every: HashMap<ServiceKey, ServiceView> = HashMap::new();
        // Dans ces états (moteur absent, non pris en charge, désactivé), aucune liste ancienne n'est
        // gardée (`BR-SVC-018`) ; dans les états inaccessibles la dernière liste reste.
        if !inner.debounce.published().keeps_no_list() {
            Self::build_views(inner, now, &mut every);
        }
        // Le seul passage du filtre de publication : tout ce qui suit (comparaison, `rev`, lots,
        // instantané) ne voit que `target`.
        let target = Self::publish(inner, every);
        let engine_key = Self::engine_key(inner);
        let engine_changed = engine_key != inner.published_engine;
        let mut upserts: Vec<ServiceView> = target
            .values()
            .filter(|view| inner.published.get(&view.key) != Some(*view))
            .cloned()
            .collect();
        let mut removed: Vec<ServiceKey> = inner
            .published
            .keys()
            .filter(|key| !target.contains_key(*key))
            .cloned()
            .collect();
        if !engine_changed && upserts.is_empty() && removed.is_empty() {
            return;
        }
        sort_views(&mut upserts);
        removed.sort();
        inner.rev += 1;
        inner.published = target;
        inner.published_engine = engine_key;
        let change = ServicesChange {
            rev: inner.rev,
            engine: engine_changed.then(|| self.status_of(inner)),
            upserts,
            removed,
        };
        // Sans abonné, `send` rend une erreur : personne n'écoute, ce n'est pas un échec.
        let _ = self.changes.send(Arc::new(change));
    }

    /// LE filtre de publication (HRT-79, `BR-ADOPT-001`). Rend les seuls services qui sortent de
    /// l'agent (créés par Hearth, adoptés) et pose le nombre des autres. Les adoptions dont le
    /// conteneur n'existe plus tombent d'abord (`BR-ADOPT-017`), seulement quand la liste des
    /// conteneurs est celle du moteur : jamais parce qu'un moteur absent l'a vidée.
    fn publish(
        inner: &mut Inner,
        every: HashMap<ServiceKey, ServiceView>,
    ) -> HashMap<ServiceKey, ServiceView> {
        if inner.has_list && !inner.debounce.published().keeps_no_list() {
            let live: HashSet<ServiceKey> = inner
                .containers
                .values()
                .filter_map(|observation| key_of(&inner.owners, observation))
                .collect();
            for key in lapsed(&inner.adopted, &live) {
                inner.adopted.remove(&key);
                inner.lapsed.insert(key);
            }
        }
        let mut published = HashMap::with_capacity(every.len());
        let mut hidden = HashSet::new();
        for (key, mut view) in every {
            match visibility_of(&key, &inner.adopted) {
                Visibility::Hearth => {}
                Visibility::Adopted(adoption) => view.adoption = Some(adoption.clone()),
                Visibility::Hidden => {
                    hidden.insert(key);
                    continue;
                }
            }
            published.insert(key, view);
        }
        inner.hidden = hidden;
        // Les conteneurs, pas les services : un conteneur arrêté, reste d'une recréation, compte comme
        // les autres tant qu'aucun service publié ne le porte.
        inner.external = inner
            .containers
            .values()
            .filter(|observation| {
                key_of(&inner.owners, observation).is_none_or(|key| {
                    matches!(visibility_of(&key, &inner.adopted), Visibility::Hidden)
                })
            })
            .count();
        published
    }

    /// Les vues de tous les services : un conteneur par service (celui qui tourne, à défaut le plus
    /// récent), la clé `u_<unité>` pour un conteneur lancé sous une unité (HRT-63), puis les services
    /// supervisés retenus dont le conteneur n'existe pas, d'après l'état de leur unité.
    fn build_views(
        inner: &mut Inner,
        now: OffsetDateTime,
        target: &mut HashMap<ServiceKey, ServiceView>,
    ) {
        Self::release_shared_units(inner);
        let mut chosen: HashMap<ServiceKey, Observation> = HashMap::new();
        for observation in inner.containers.values() {
            let Some(key) = key_of(&inner.owners, observation) else {
                continue;
            };
            if chosen
                .get(&key)
                .is_none_or(|current| better(observation, current))
            {
                chosen.insert(key, observation.clone());
            }
        }
        // Un conteneur disparu ne laisse pas son propriétaire en mémoire.
        let containers = &inner.containers;
        inner.owners.retain(|id, _| containers.contains_key(id));
        let remembered_keys: HashSet<ServiceKey> =
            inner.remembered.keys().map(ServiceKey::for_unit).collect();
        inner
            .windows
            .retain(|key, _| chosen.contains_key(key) || remembered_keys.contains(key));
        let mut present_units: HashSet<UnitName> = HashSet::new();
        for (key, observation) in chosen {
            let restarts_10m = inner.windows.entry(key.clone()).or_default().count(now);
            let unit = match inner.owners.get(&observation.id) {
                Some(Some(unit)) => Some(unit.clone()),
                _ => None,
            };
            let (derived, supervision) = match unit {
                // Aucune action de Hearth dans cette tranche : l'arrêt n'est jamais « demandé ».
                None => (derive_state(&observation, restarts_10m, false), None),
                Some(unit) => {
                    present_units.insert(unit.clone());
                    let status = Self::status_of_unit(inner, &unit);
                    // HRT-79 : Hearth ne retient, en mémoire comme en base, que les services adoptés.
                    // Les réglages d'un conteneur caché ne sont gardés nulle part.
                    if inner.adopted.contains_key(&key) {
                        if matches!(observation.run_state, RunState::Exited | RunState::Dead)
                            && let Some(code) = observation.exit_code
                        {
                            inner.last_exit.insert(unit.clone(), code);
                        }
                        let settings = settings_of(&observation);
                        let changed = inner
                            .remembered
                            .get(&unit)
                            .is_none_or(|known| known.observation != settings);
                        inner.remembered.insert(
                            unit.clone(),
                            Remembered {
                                observation: settings,
                                last_seen: now,
                            },
                        );
                        if changed {
                            inner.dirty.insert(unit.clone());
                        }
                    }
                    let derived = derive_supervised(
                        &observation,
                        true,
                        status.as_ref(),
                        restarts_10m,
                        None,
                        None,
                        now,
                    );
                    (
                        derived,
                        Some(Supervision {
                            unit,
                            status,
                            container_present: true,
                        }),
                    )
                }
            };
            target.insert(
                key.clone(),
                ServiceView {
                    key,
                    observation,
                    derived,
                    restarts_10m,
                    supervision,
                    adoption: None,
                },
            );
        }
        // Un service supervisé dont le conteneur vient de disparaître : on retient le moment.
        for unit in inner.present_units.difference(&present_units) {
            inner.dirty.insert(unit.clone());
        }
        inner.present_units = present_units;
        if !inner.has_list {
            return;
        }
        let vanished: Vec<(UnitName, Remembered)> = inner
            .remembered
            .iter()
            .filter(|(unit, _)| !inner.present_units.contains(*unit))
            .map(|(unit, memory)| (unit.clone(), memory.clone()))
            .collect();
        for (unit, memory) in vanished {
            // Le système ne connaît plus cette unité : le service n'existe plus (oublié à la
            // prochaine relecture des unités).
            if inner.units.get(&unit) == Some(&UnitLookup::NotLoaded) {
                continue;
            }
            let key = ServiceKey::for_unit(&unit);
            let restarts_10m = inner.windows.entry(key.clone()).or_default().count(now);
            let status = Self::status_of_unit(inner, &unit);
            let derived = derive_supervised(
                &memory.observation,
                false,
                status.as_ref(),
                restarts_10m,
                inner.last_exit.get(&unit).copied(),
                Some(memory.last_seen),
                now,
            );
            target.insert(
                key.clone(),
                ServiceView {
                    key,
                    observation: memory.observation,
                    derived,
                    restarts_10m,
                    supervision: Some(Supervision {
                        unit,
                        status,
                        container_present: false,
                    }),
                    adoption: None,
                },
            );
        }
    }

    /// Une unité ne supervise qu'UN conteneur vivant. Quand deux conteneurs vivants ou plus lui sont
    /// attribués (ils auraient la même clé `u_` et l'un disparaîtrait de la liste, `BR-SVC-001`), aucun
    /// n'est supervisé : chacun reste listé sous sa clé `c_`, et le service retenu de l'unité est oublié
    /// (HRT-63). Un conteneur arrêté, reste d'une recréation, ne compte pas.
    fn release_shared_units(inner: &mut Inner) {
        let mut live_by_unit: HashMap<UnitName, Vec<String>> = HashMap::new();
        for observation in inner.containers.values() {
            let alive = matches!(
                observation.run_state,
                RunState::Running | RunState::Restarting | RunState::Paused
            );
            if alive && let Some(Some(unit)) = inner.owners.get(&observation.id) {
                live_by_unit
                    .entry(unit.clone())
                    .or_default()
                    .push(observation.id.clone());
            }
        }
        for (unit, ids) in live_by_unit {
            if ids.len() < 2 {
                continue;
            }
            for id in ids {
                inner.owners.insert(id, None);
            }
            if inner.remembered.remove(&unit).is_some() {
                inner.forgotten.insert(unit.clone());
            }
            inner.dirty.remove(&unit);
            inner.present_units.remove(&unit);
            inner.last_exit.remove(&unit);
        }
    }

    /// L'état connu d'une unité : `None` tant que le superviseur n'a rien répondu ou ne répond pas.
    fn status_of_unit(inner: &Inner, unit: &UnitName) -> Option<UnitStatus> {
        match inner.units.get(unit) {
            Some(UnitLookup::Loaded(status)) => Some(*status),
            _ => None,
        }
    }

    /// Cherche le propriétaire des conteneurs dont on ne l'a pas encore cherché, une fois pour la vie du
    /// conteneur, puis l'état des unités nouvellement vues. Le superviseur ne rend jamais d'erreur : un
    /// système muet laisse tout conteneur sans propriétaire.
    async fn resolve_owners(&self, candidates: Vec<ContainerRef>) {
        let unknown: Vec<ContainerRef> = {
            let inner = self.lock();
            candidates
                .into_iter()
                .filter(|container| !inner.owners.contains_key(&container.id))
                .collect()
        };
        if unknown.is_empty() {
            return;
        }
        let found = self.supervisor.owners(&unknown).await;
        let new_units: Vec<UnitName> = {
            let mut inner = self.lock();
            for container in &unknown {
                inner
                    .owners
                    .insert(container.id.clone(), found.get(&container.id).cloned());
            }
            let mut fresh: Vec<UnitName> = found
                .values()
                .filter(|unit| !inner.units.contains_key(*unit))
                .cloned()
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            fresh.sort();
            fresh.truncate(MAX_UNITS_PER_POLL);
            fresh
        };
        for unit in new_units {
            let lookup = self.supervisor.unit(&unit).await;
            self.lock().units.insert(unit, lookup);
        }
    }

    /// Relit l'état de chaque unité connue (supervisant un conteneur, ou retenue) et recalcule les
    /// vues. Une unité que le système ne connaît plus, dont le conteneur n'existe plus, est oubliée avec
    /// son service retenu.
    async fn poll_units(&self) {
        let units: Vec<UnitName> = {
            let mut inner = self.lock();
            inner.last_unit_poll = Some(self.mono());
            // `BR-ADOPT-027` : une confirmation qu'aucun compte n'a donnée dans le délai tombe.
            let now = self.clock.now();
            let expired: Vec<ServiceKey> = inner
                .unconfirmed
                .iter()
                .filter(|(_, since)| confirmation_expired(**since, now))
                .map(|(key, _)| key.clone())
                .collect();
            for key in expired {
                inner.unconfirmed.remove(&key);
                if let Some(unit) = key.unit() {
                    inner.gone_reads.remove(&unit);
                }
                inner.lapsed.insert(key);
            }
            // Les adoptions (confirmées ou non) d'abord : ce sont elles dont l'unité doit être relue,
            // avant celle d'un conteneur quelconque. Au-delà de la borne, l'adoption qui n'est pas
            // relue devient « à confirmer » plus bas.
            let mut adopted_units: Vec<UnitName> = inner
                .adopted
                .keys()
                .chain(inner.unconfirmed.keys())
                .filter_map(ServiceKey::unit)
                .collect();
            adopted_units.sort();
            adopted_units.dedup();
            let mut known: HashSet<UnitName> = inner.owners.values().flatten().cloned().collect();
            known.extend(inner.remembered.keys().cloned());
            known.retain(|unit| !adopted_units.contains(unit));
            let mut others: Vec<UnitName> = known.into_iter().collect();
            others.sort();
            adopted_units.truncate(MAX_ADOPTED_UNITS_PER_POLL);
            let room = MAX_UNITS_PER_POLL.saturating_sub(adopted_units.len());
            adopted_units.extend(others.into_iter().take(room));
            adopted_units
        };
        if units.is_empty() {
            return;
        }
        let read: HashSet<UnitName> = units.iter().cloned().collect();
        let mut looked = Vec::with_capacity(units.len());
        for unit in units {
            let lookup = self.supervisor.unit(&unit).await;
            looked.push((unit, lookup));
        }
        {
            let mut inner = self.lock();
            // Le compte des lectures « introuvable » ne survit pas à une unité qu'on ne relit plus.
            inner.gone_reads.retain(|unit, _| read.contains(unit));
            for (unit, lookup) in looked {
                let container_present = inner.present_units.contains(&unit);
                let key = ServiceKey::for_unit(&unit);
                // Seule une adoption fait tomber : le compte d'une unité qui n'est pas adoptée ne
                // servirait qu'à frapper une adoption future.
                let counts =
                    inner.adopted.contains_key(&key) || inner.unconfirmed.contains_key(&key);
                let gone_reads = if counts && unit_is_gone(lookup, container_present) {
                    let reads = inner.gone_reads.entry(unit.clone()).or_insert(0);
                    *reads = reads.saturating_add(1);
                    *reads
                } else {
                    inner.gone_reads.remove(&unit);
                    0
                };
                if gone_long_enough(gone_reads) {
                    // `BR-ADOPT-017` : l'adoption tient tant que l'unité existe. Le système répond
                    // clairement « introuvable » (supprimée, ou transitoire arrêtée), aucun conteneur ne
                    // la porte, et c'est vrai depuis plusieurs lectures de suite : l'adoption tombe, sans
                    // entrée au journal (aucun compte n'agit). Une unité écrite dans un fichier reste
                    // connue une fois arrêtée (`loaded/inactive`) ; une unité masquée, en erreur, ou une
                    // réponse que le système ne donne pas (`Unavailable`) ne fait rien tomber.
                    inner.gone_reads.remove(&unit);
                    let was_unconfirmed = inner.unconfirmed.remove(&key).is_some();
                    if inner.adopted.remove(&key).is_some() || was_unconfirmed {
                        inner.lapsed.insert(key);
                    }
                    if inner.remembered.remove(&unit).is_some() {
                        inner.forgotten.insert(unit.clone());
                    }
                    inner.dirty.remove(&unit);
                    inner.last_exit.remove(&unit);
                    inner.units.remove(&unit);
                } else {
                    inner.units.insert(unit, lookup);
                }
            }
            // `BR-ADOPT-027` : une adoption par unité dont l'unité n'a pas été relue ne dit plus rien
            // d'une unité qui pourrait être une homonyme : elle devient « à confirmer ».
            let now = self.clock.now();
            let unread = unread_unit_adoptions(inner.adopted.keys(), &read);
            for key in unread {
                inner.adopted.remove(&key);
                inner.unconfirmed.insert(key.clone(), now);
                if let Some(unit) = key.unit() {
                    inner.gone_reads.remove(&unit);
                    if inner.remembered.remove(&unit).is_some() {
                        inner.forgotten.insert(unit.clone());
                    }
                    inner.dirty.remove(&unit);
                    inner.last_exit.remove(&unit);
                }
            }
            self.commit(&mut inner);
        }
        self.persist_supervised().await;
    }

    /// Les unités sont-elles à relire (toutes les deux périodes de sonde) ?
    fn units_due(&self) -> bool {
        let inner = self.lock();
        inner.last_unit_poll.is_none_or(|at| {
            self.mono().saturating_sub(at) >= self.timings.ready_period * UNIT_POLL_PERIODS
        })
    }

    /// Écrit en base les services supervisés dont les réglages ont changé et oublie ceux dont l'unité
    /// a disparu. Un échec est tracé et ne gêne rien : la prochaine modification réécrit.
    async fn persist_supervised(&self) {
        let (saves, forgets) = {
            let mut inner = self.lock();
            let dirty: Vec<UnitName> = inner.dirty.drain().collect();
            let saves: Vec<SupervisedRecord> = dirty
                .into_iter()
                .filter_map(|unit| {
                    inner.remembered.get(&unit).map(|memory| SupervisedRecord {
                        unit,
                        observation: memory.observation.clone(),
                        last_seen_at: memory.last_seen,
                    })
                })
                .collect();
            let forgets: Vec<UnitName> = inner.forgotten.drain().collect();
            (saves, forgets)
        };
        let lapsed: Vec<ServiceKey> = self.lock().lapsed.drain().collect();
        for key in lapsed {
            if let Err(error) = self.repo.forget_adopted(&key).await {
                tracing::warn!(%error, "adoption tombée non oubliée");
                // Remise dans la file : la ligne reste en base tant qu'elle n'est pas effacée, un oubli
                // perdu la ferait revenir à la prochaine relecture (HRT-80, suivi 8 de la revue).
                self.lock().lapsed.insert(key);
            }
        }
        for record in saves {
            if let Err(error) = self.repo.save_supervised(&record).await {
                tracing::warn!(%error, "service supervisé non retenu");
            }
        }
        for unit in forgets {
            if let Err(error) = self.repo.forget_supervised(&unit).await {
                tracing::warn!(%error, "service supervisé non oublié");
            }
        }
    }

    /// Qui tient ce port du serveur, parmi TOUS les conteneurs que le moteur connaît, arrêtés compris
    /// (conflits de ports, `BR-ADOPT-022`). Un service publié est nommé ; un conteneur non adopté
    /// est `Holder::Unmanaged`, sans son nom ni son image. Les ports d'un service en réseau hôte ne
    /// sont pas listés et ne sont donc pas vus ici (limite de `Observation`).
    pub fn port_holder(&self, host_port: u16, protocol: PortProtocol) -> Option<Holder> {
        let inner = self.lock();
        inner
            .containers
            .values()
            .find(|observation| {
                observation
                    .ports
                    .iter()
                    .any(|port| port.host_port == host_port && port.protocol == protocol)
            })
            .map(|observation| Self::holder_of(&inner, observation))
    }

    /// Qui porte ce nom de service, sans tenir compte de la casse (`BR-ADOPT-022` : « Ce nom est déjà
    /// pris », sans autre indication quand c'est un conteneur non adopté).
    pub fn name_holder(&self, name: &str) -> Option<Holder> {
        let inner = self.lock();
        inner
            .containers
            .values()
            .find(|observation| observation.name.to_lowercase() == name.to_lowercase())
            .map(|observation| Self::holder_of(&inner, observation))
    }

    fn holder_of(inner: &Inner, observation: &Observation) -> Holder {
        match key_of(&inner.owners, observation).map(|key| visibility_of(&key, &inner.adopted)) {
            Some(Visibility::Hearth | Visibility::Adopted(_)) => {
                Holder::Service(observation.name.clone())
            }
            Some(Visibility::Hidden) | None => Holder::Unmanaged,
        }
    }

    /// Les conteneurs non gérés que le moteur connaît en ce moment, un par clé (le conteneur qui tourne,
    /// à défaut le plus récent), triés par nom sans tenir compte de la casse (`BR-ADOPT-005`,
    /// `BR-ADOPT-006`). Un service que le système lance mais dont le conteneur n'existe pas n'en fait pas
    /// partie. **La seule lecture du registre qui nomme des conteneurs cachés**, réservée à
    /// `ServiceAdoption` (droit exigé par la couche d'accès) : jamais diffusée, jamais gardée.
    pub fn candidates(&self) -> Vec<Candidate> {
        let mut inner = self.lock();
        let now = self.clock.now();
        let mut found: Vec<Candidate> = Self::hidden_candidates(&mut inner, now)
            .into_values()
            .collect();
        found.sort_by(|a, b| compare_names(&a.name, &b.name).then_with(|| a.key.cmp(&b.key)));
        found
    }

    /// Ce que cette clé est pour l'acte d'adoption, d'après le registre à cet instant (`BR-ADOPT-010`).
    pub fn adoption_lookup(&self, key: &ServiceKey) -> AdoptionLookup {
        let mut inner = self.lock();
        if let Some(adoption) = inner.adopted.get(key)
            && inner.published.contains_key(key)
        {
            return AdoptionLookup::Adopted {
                by: adoption.by.clone(),
            };
        }
        let now = self.clock.now();
        match Self::hidden_candidates(&mut inner, now).remove(key) {
            Some(candidate) => AdoptionLookup::Candidate(candidate),
            None => AdoptionLookup::Unknown,
        }
    }

    /// Le nom et l'unité d'un service PUBLIÉ (créé par Hearth ou adopté), pour le journal d'un refus de
    /// lecture des journaux : le service est déjà dans la liste du compte refusé. `None` pour toute autre
    /// clé, cachée ou inconnue.
    pub fn published_label(&self, key: &ServiceKey) -> Option<(String, Option<UnitName>)> {
        let inner = self.lock();
        let view = inner.published.get(key)?;
        Some((
            view.observation.name.clone(),
            view.supervision
                .as_ref()
                .map(|supervision| supervision.unit.clone()),
        ))
    }

    /// Les candidats par clé : tout conteneur que le filtre de publication cache, au mieux de sa clé.
    fn hidden_candidates(inner: &mut Inner, now: OffsetDateTime) -> HashMap<ServiceKey, Candidate> {
        if !inner.has_list || inner.debounce.published().keeps_no_list() {
            return HashMap::new();
        }
        let mut chosen: HashMap<ServiceKey, Observation> = HashMap::new();
        for observation in inner.containers.values() {
            let Some(key) = key_of(&inner.owners, observation) else {
                continue;
            };
            if !matches!(visibility_of(&key, &inner.adopted), Visibility::Hidden) {
                continue;
            }
            if chosen
                .get(&key)
                .is_none_or(|current| better(observation, current))
            {
                chosen.insert(key, observation.clone());
            }
        }
        let mut found = HashMap::with_capacity(chosen.len());
        for (key, observation) in chosen {
            let unit = match inner.owners.get(&observation.id) {
                Some(Some(unit)) => Some(unit.clone()),
                _ => None,
            };
            let restarts = inner
                .windows
                .get_mut(&key)
                .map_or(0, |window| window.count(now));
            let state = match &unit {
                None => derive_state(&observation, restarts, false).state,
                Some(unit) => {
                    let status = Self::status_of_unit(inner, unit);
                    derive_supervised(
                        &observation,
                        true,
                        status.as_ref(),
                        restarts,
                        None,
                        None,
                        now,
                    )
                    .state
                }
            };
            found.insert(
                key.clone(),
                Candidate {
                    key,
                    name: observation.name.clone(),
                    image: observation.image.clone(),
                    unit,
                    state,
                },
            );
        }
        found
    }

    /// Cette adoption par unité est-elle « à confirmer » (`BR-ADOPT-027`) ?
    pub fn is_unconfirmed(&self, key: &ServiceKey) -> bool {
        self.lock().unconfirmed.contains_key(key)
    }

    /// Un compte qui a le droit vient de confirmer cette adoption (la ligne est réécrite à son nom) :
    /// elle n'est plus « à confirmer » et son compte de lectures « introuvable » repart de zéro.
    pub fn confirm_adoption(&self, key: &ServiceKey) {
        let mut inner = self.lock();
        inner.unconfirmed.remove(key);
        if let Some(unit) = key.unit() {
            inner.gone_reads.remove(&unit);
        }
    }

    /// Lit les services adoptés (HRT-79) et recalcule ce qui est publié. À appeler après toute écriture
    /// de l'adoption (HRT-80) ; le registre l'appelle une fois au démarrage. Une lecture en échec laisse
    /// l'ensemble connu : jamais plus de services publiés qu'avant. Un service qui n'est plus adopté
    /// sort de la liste et son souvenir est oublié ; il est de nouveau compté.
    pub async fn reload_adopted(&self) {
        match self.repo.load_adopted().await {
            Ok(records) => {
                {
                    let mut inner = self.lock();
                    // Une adoption « à confirmer » garde sa ligne mais pas ses droits (`BR-ADOPT-027`) ;
                    // si la ligne a disparu (relâchée, tombée), elle n'est plus à confirmer.
                    let in_base: HashSet<ServiceKey> =
                        records.iter().map(|record| record.key.clone()).collect();
                    inner.unconfirmed.retain(|key, _| in_base.contains(key));
                    let unconfirmed: HashSet<ServiceKey> =
                        inner.unconfirmed.keys().cloned().collect();
                    inner.adopted = records
                        .into_iter()
                        .filter(|record| !unconfirmed.contains(&record.key))
                        .map(|AdoptedRecord { key, by, at }| (key, Adoption { by, at }))
                        .collect();
                    let tracked: HashSet<ServiceKey> = inner
                        .adopted
                        .keys()
                        .chain(inner.unconfirmed.keys())
                        .cloned()
                        .collect();
                    inner
                        .gone_reads
                        .retain(|unit, _| tracked.contains(&ServiceKey::for_unit(unit)));
                    // Ce qu'on retenait d'un service qui n'est plus adopté n'a plus de raison d'être.
                    let stale: Vec<UnitName> = inner
                        .remembered
                        .keys()
                        .filter(|unit| !inner.adopted.contains_key(&ServiceKey::for_unit(unit)))
                        .cloned()
                        .collect();
                    for unit in stale {
                        inner.remembered.remove(&unit);
                        inner.dirty.remove(&unit);
                        inner.last_exit.remove(&unit);
                        inner.forgotten.insert(unit);
                    }
                    self.commit(&mut inner);
                }
                self.persist_supervised().await;
            }
            Err(error) => tracing::warn!(%error, "services adoptés illisibles"),
        }
    }

    /// Reprend les services supervisés retenus par un démarrage précédent, s'ils sont adoptés.
    async fn load_remembered(&self) {
        match self.repo.load_supervised().await {
            Ok(records) => {
                let mut inner = self.lock();
                for record in records {
                    if !inner
                        .adopted
                        .contains_key(&ServiceKey::for_unit(&record.unit))
                    {
                        // Défense : la base ne le permet plus (migration 0011), mais ce qui n'est pas
                        // adopté ne se publie jamais depuis la mémoire.
                        inner.forgotten.insert(record.unit);
                        continue;
                    }
                    inner
                        .remembered
                        .entry(record.unit)
                        .or_insert_with(|| Remembered {
                            observation: record.observation,
                            last_seen: record.last_seen_at,
                        });
                }
                self.commit(&mut inner);
            }
            Err(error) => tracing::warn!(%error, "services supervisés retenus illisibles"),
        }
    }

    /// Les variables d'un service avec leurs VALEURS, pour un compte qui a le droit « gérer les services » (BR-SVC-009, jugé par la couche d'accès). Le
    /// service est celui que la liste montre ; sa lecture passe par le moteur, jamais par la liste.
    /// La réussite est consignée au journal AVANT de rendre les valeurs : sans nom ni valeur, la cible
    /// est le service. Rien n'est lu en arrière-plan : une valeur ne quitte le moteur que par cet appel.
    pub async fn reveal_env(
        &self,
        key: &ServiceKey,
        actor: Actor,
        sink: &dyn AuditSink,
    ) -> Result<Vec<EnvEntry>, RevealError> {
        // BR-GRANT-015 : le coupe-circuit l'emporte sur le droit. Pilotage désactivé, aucun service
        // n'est publié et le moteur n'est jamais ouvert, même pour un compte qui a le droit.
        if !self.enabled {
            return Err(RevealError::NotFound);
        }
        let (container_id, name) = {
            let inner = self.lock();
            let view = inner.published.get(key).ok_or(RevealError::NotFound)?;
            if view
                .supervision
                .as_ref()
                .is_some_and(|supervision| !supervision.container_present)
            {
                return Err(RevealError::NoContainer);
            }
            (view.observation.id.clone(), view.observation.name.clone())
        };
        let entries = match self.engine.env(&container_id).await {
            Ok(Some(entries)) => entries,
            Ok(None) => return Err(RevealError::NotFound),
            Err(_) => return Err(RevealError::EngineUnavailable),
        };
        let target = ClientName::parse(&name).map_or(Target::None, Target::Service);
        // La consignation précède les valeurs ET les conditionne : si elle échoue, `entries` est jeté
        // et rien n'est rendu.
        sink.record_required(
            actor,
            AuditAction::ServiceEnvReveal,
            target,
            Outcome::Succeeded,
        )
        .await
        .map_err(|_| RevealError::JournalUnavailable)?;
        Ok(entries)
    }

    /// Ce que cette clé désigne en ce moment pour la lecture des journaux (HRT-66) : le conteneur
    /// COURANT du service, jamais celui qu'on a lu avant (un service géré par le système change de
    /// conteneur à chaque démarrage, BR-SVC-016). Aucune décision de droit ici : elle est prise par
    /// `service_logs::LogAccess` (`may_read`), qui reçoit l'origine fournie par le filtre de publication
    /// (HRT-79) : créé par Hearth, adopté, ou « hors Hearth non adopté » (une clé que le filtre retient :
    /// `may_read` la refuse, donc la réponse est celle d'un service inconnu, `BR-ADOPT-004`). La cible
    /// d'une clé retenue ne porte RIEN du conteneur (ni identifiant, ni état).
    pub fn log_target(&self, key: &ServiceKey) -> LogTarget {
        let inner = self.lock();
        let Some(view) = inner.published.get(key) else {
            return if inner.hidden.contains(key) {
                LogTarget::NoContainer {
                    origin: LogOrigin::External,
                }
            } else {
                LogTarget::Missing
            };
        };
        let origin = if view.adoption.is_some() {
            LogOrigin::Adopted
        } else {
            LogOrigin::Hearth
        };
        if view
            .supervision
            .as_ref()
            .is_some_and(|supervision| !supervision.container_present)
        {
            return LogTarget::NoContainer { origin };
        }
        LogTarget::Container(LogContainer::from_observation(&view.observation, origin))
    }

    /// Combien attendre avant de retenter une relecture après `failures` échecs de suite : une
    /// période de sonde au premier, doublée à chaque échec suivant, plafonnée à 30 périodes.
    fn backoff(&self, failures: u32) -> Duration {
        let factor = 1u32
            .checked_shl(failures.saturating_sub(1))
            .unwrap_or(u32::MAX);
        self.timings
            .ready_period
            .saturating_mul(factor.min(MAX_BACKOFF_FACTOR))
    }

    fn list_denied(&self) -> bool {
        self.lock().list_denied
    }

    /// La tâche de fond : détection du moteur, relecture, suivi des événements. Ne rend jamais la
    /// main (on l'arrête en abandonnant la tâche qui la porte). Sans moteur, elle sonde toutes les
    /// 5 secondes et ne dit rien de plus que le changement d'état : aucune erreur bruyante.
    pub async fn run(self: Arc<Self>) {
        if !self.enabled {
            // Pilotage désactivé : le socket n'est jamais ouvert.
            std::future::pending::<()>().await;
        }
        // Les services adoptés d'abord (rien n'est publié sans eux), puis les services supervisés d'un
        // démarrage précédent, avec l'état actuel de leur unité.
        self.reload_adopted().await;
        self.load_remembered().await;
        self.poll_units().await;
        // Relectures complètes échouées de suite, et quand retenter.
        let mut failures: u32 = 0;
        let mut next_read = Instant::now();
        // Phases « prêt » finies en moins d'une période de suite (flux qui casse aussitôt).
        let mut quick_breaks: u32 = 0;
        loop {
            let started = Instant::now();
            // Une relance de l'utilisateur vient de sonder : on réutilise son résultat.
            let observed = if self.probed_within(self.timings.min_recheck) {
                self.lock().observed
            } else {
                self.probe_and_apply(false).await
            };
            let readable = observed == EngineState::Ready
                || (observed == EngineState::AccessDenied && self.list_denied());
            if !readable {
                failures = 0;
                self.wait_for(self.timings.other_period).await;
            } else if Instant::now() < next_read {
                // Relecture en recul : la sonde garde sa cadence, la liste attend.
                self.wait_for(self.timings.ready_period).await;
            } else {
                let Some(read) = self.while_probing(self.resync()).await else {
                    // Le moteur a disparu pendant la lecture : la boucle le resonde aussitôt.
                    continue;
                };
                match read {
                    Ok(unreadable) => {
                        if failures > 0 {
                            tracing::info!("relecture des services rétablie");
                        }
                        failures = 0;
                        self.ready_phase(unreadable).await;
                        // Une rupture du flux est reprise tout de suite (sonde puis relecture complète) ;
                        // si elle se répète sans que la phase dure une période, on recule : jamais de
                        // boucle serrée sur un moteur dont le flux ne tient pas.
                        if started.elapsed() < self.timings.ready_period {
                            quick_breaks = quick_breaks.saturating_add(1);
                        } else {
                            quick_breaks = 0;
                        }
                        if quick_breaks >= 2 {
                            next_read = Instant::now() + self.backoff(quick_breaks - 1);
                        }
                        // Le suivi en direct est suspendu tant que le flux casse en boucle.
                        self.set_tracking_lost(quick_breaks >= 2);
                    }
                    Err(error) => {
                        if failures == 0 {
                            // Une fois par série d'échecs ; les suivants restent au niveau debug.
                            tracing::warn!(%error, "relecture des services impossible");
                        } else {
                            tracing::debug!(%error, "relecture des services impossible");
                        }
                        failures = failures.saturating_add(1);
                        next_read = Instant::now() + self.backoff(failures);
                    }
                }
            }
        }
    }

    fn set_tracking_lost(&self, lost: bool) {
        let mut inner = self.lock();
        if inner.tracking_lost != lost {
            inner.tracking_lost = lost;
            self.commit(&mut inner);
        }
    }

    /// Mène `work` (une lecture du moteur, qui peut durer jusqu'à 30 s) SANS aveugler la tâche : la
    /// sonde garde sa cadence et l'anti-clignotement ses échéances pendant ce temps. Si le moteur
    /// n'est plus prêt, la lecture est abandonnée (`None`) : il n'y a plus rien à lire.
    async fn while_probing<T>(&self, work: impl Future<Output = T>) -> Option<T> {
        let mut ping = interval(self.timings.ready_period);
        ping.set_missed_tick_behavior(MissedTickBehavior::Delay);
        ping.tick().await;
        let probes = async {
            loop {
                ping.tick().await;
                let state = self.probe_and_apply(false).await;
                // Une liste refusée qu'on est justement en train de retenter reste « lisible ».
                let readable = state == EngineState::Ready
                    || (state == EngineState::AccessDenied && self.list_denied());
                if !readable {
                    return;
                }
            }
        };
        tokio::pin!(work, probes);
        loop {
            let debounce_wait = self.debounce_wait();
            tokio::select! {
                out = &mut work => return Some(out),
                () = &mut probes => return None,
                () = sleep(debounce_wait.unwrap_or_default()), if debounce_wait.is_some() => {
                    self.flush_debounce();
                }
            }
        }
    }

    /// Attend la prochaine sonde (`period`) ; publie au passage les changements retenus par
    /// l'anti-clignotement.
    async fn wait_for(&self, period: Duration) {
        let deadline = Instant::now() + period;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return;
            }
            let slice = self
                .debounce_wait()
                .map_or(remaining, |wait| wait.min(remaining));
            tokio::select! {
                () = sleep(slice) => {}
                () = self.wake.notified() => return,
            }
            self.flush_debounce();
        }
    }

    /// Le moteur est prêt et la liste lue : suit les événements jusqu'à une rupture. `unreadable` :
    /// conteneurs que la relecture qui vient d'avoir lieu n'a pas pu lire.
    async fn ready_phase(&self, unreadable: usize) {
        let mut events = self.engine.events();
        let mut touched: HashSet<String> = HashSet::new();
        let mut batch_at: Option<Instant> = None;
        // Le flux d'événements ne se connecte qu'à sa première lecture : une dernière relecture
        // juste après ferme l'intervalle entre la liste et cette connexion.
        let mut settle_at = Some(Instant::now() + self.timings.batch_window * 4);
        // Relectures de suite qui ont laissé des conteneurs illisibles (pour reculer).
        let mut retries: u32 = u32::from(unreadable > 0);
        let mut ping = interval(self.timings.ready_period);
        ping.set_missed_tick_behavior(MissedTickBehavior::Delay);
        ping.tick().await;
        loop {
            let debounce_wait = self.debounce_wait();
            tokio::select! {
                event = events.next() => match event {
                    Some(Ok(event)) => {
                        tracing::trace!(action = %event.action, "événement du moteur");
                        touched.insert(event.container_id);
                        batch_at.get_or_insert_with(|| Instant::now() + self.timings.batch_window);
                    }
                    // Flux rompu : le moteur est à revérifier (sonde immédiate, relecture complète).
                    Some(Err(_)) | None => return,
                },
                () = sleep_until(batch_at.unwrap_or_else(Instant::now)), if batch_at.is_some() => {
                    batch_at = None;
                    let ids: Vec<String> = touched.drain().collect();
                    let Some(refreshed) = self.while_probing(self.refresh(ids)).await else {
                        return;
                    };
                    match refreshed {
                        Ok(0) => {}
                        // Des conteneurs illisibles : une relecture complète les reprendra, en reculant.
                        Ok(_) => {
                            settle_at.get_or_insert_with(|| Instant::now() + self.backoff(retries));
                        }
                        Err(_) => return,
                    }
                }
                () = sleep_until(settle_at.unwrap_or_else(Instant::now)), if settle_at.is_some() => {
                    settle_at = None;
                    let Some(read) = self.while_probing(self.resync()).await else {
                        return;
                    };
                    match read {
                        Ok(0) => retries = 0,
                        Ok(_) => {
                            retries = retries.saturating_add(1);
                            settle_at = Some(Instant::now() + self.backoff(retries));
                        }
                        Err(_) => return,
                    }
                }
                _ = ping.tick() => {
                    if self.probe_and_apply(false).await != EngineState::Ready {
                        return;
                    }
                    // Les démarrages de plus de 10 minutes sortent de la fenêtre sans événement.
                    {
                        let mut inner = self.lock();
                        self.commit(&mut inner);
                    }
                    // L'état des unités change sans événement du moteur (échec, relance) : relu à
                    // intervalles, la sonde gardant sa cadence pendant ce temps.
                    if self.units_due() {
                        let polled = tokio::time::timeout(
                            REFRESH_TIMEOUT,
                            self.while_probing(self.poll_units()),
                        )
                        .await;
                        // Le moteur n'est plus prêt : retour à la sonde. Un délai dépassé ne dit
                        // rien du moteur, la relecture suivante reprend.
                        if matches!(polled, Ok(None)) {
                            return;
                        }
                    }
                }
                () = sleep(debounce_wait.unwrap_or_default()), if debounce_wait.is_some() => {
                    self.flush_debounce();
                }
                () = self.wake.notified() => {}
            }
        }
    }
}

/// Note le début d'exécution d'un conteneur qui tourne dans la fenêtre des démarrages.
fn note_start(
    windows: &mut HashMap<ServiceKey, StartWindow>,
    key: Option<ServiceKey>,
    observation: &Observation,
) {
    if observation.run_state != RunState::Running {
        return;
    }
    let (Some(key), Some(started_at)) = (key, observation.started_at) else {
        return;
    };
    windows.entry(key).or_default().observe(started_at);
}

/// Le conteneur d'une observation, pour chercher son propriétaire.
fn refer(observation: &Observation) -> ContainerRef {
    ContainerRef {
        id: observation.id.clone(),
        name: observation.name.clone(),
    }
}

fn sort_views(views: &mut [ServiceView]) {
    views.sort_by(|a, b| {
        compare_names(&a.observation.name, &b.observation.name).then_with(|| a.key.cmp(&b.key))
    });
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::domain::services::{CONFIRMATION_TTL, ServiceStateKind};
    use crate::infrastructure::engine::fake::{FakeEngine, container, ready_probe};

    struct Wall(Mutex<OffsetDateTime>);

    impl Wall {
        fn advance(&self, by: time::Duration) {
            *self.0.lock().unwrap_or_else(PoisonError::into_inner) += by;
        }
    }

    impl Clock for Wall {
        fn now(&self) -> OffsetDateTime {
            *self.0.lock().unwrap_or_else(PoisonError::into_inner)
        }
    }

    #[derive(Default)]
    struct Mono(AtomicU64);

    impl Mono {
        fn advance(&self, by: Duration) {
            self.0.fetch_add(by.as_millis() as u64, Ordering::SeqCst);
        }
    }

    impl MonotonicClock for Mono {
        fn elapsed(&self) -> time::Duration {
            time::Duration::milliseconds(self.0.load(Ordering::SeqCst) as i64)
        }
    }

    struct Linux;

    impl HostEnvironment for Linux {
        fn describe(&self) -> HostDescription {
            HostDescription {
                os_id: "debian".into(),
                systemd: true,
                agent_account: "root".into(),
                agent_is_admin: true,
                ..HostDescription::default()
            }
        }
    }

    fn registry(enabled: bool) -> (Arc<ServicesRegistry>, Arc<FakeEngine>, Arc<Mono>) {
        registry_at(enabled, crate::infrastructure::engine::fake::at(12))
    }

    fn registry_at(
        enabled: bool,
        wall: OffsetDateTime,
    ) -> (Arc<ServicesRegistry>, Arc<FakeEngine>, Arc<Mono>) {
        let engine = Arc::new(FakeEngine::new());
        let mono = Arc::new(Mono::default());
        let registry = ServicesRegistry::new(
            engine.clone(),
            &Linux,
            Arc::new(Wall(Mutex::new(wall))),
            mono.clone(),
            enabled,
            EngineTimings::production(),
        );
        adopt_vocabulary(&registry);
        (registry, engine, mono)
    }

    /// Les services que les tests de ce module traitent comme adoptés d'avance : le registre ne publie
    /// rien d'autre que les services créés par Hearth et les adoptés (HRT-79), et ces tests parlent de
    /// l'état du moteur, de la fraîcheur, des clés et de la supervision, pas de la publication (voir
    /// plus bas les tests de publication, qui partent d'un registre sans adoption).
    fn vocabulary() -> Vec<AdoptedRecord> {
        let at = crate::infrastructure::engine::fake::at(12);
        let mut keys: Vec<ServiceKey> = ["web", "db", "cache", "Api", "new"]
            .iter()
            .filter_map(|name| ServiceKey::for_container(&container(name, RunState::Running).id))
            .collect();
        keys.extend(
            ('a'..='f').filter_map(|id| ServiceKey::for_container(&id.to_string().repeat(64))),
        );
        keys.extend(
            ["docker-terraria.service", "docker-x.service"]
                .iter()
                .filter_map(|unit| UnitName::parse(unit))
                .map(|unit| ServiceKey::for_unit(&unit)),
        );
        keys.into_iter()
            .map(|key| AdoptedRecord {
                key,
                by: "marie".to_owned(),
                at,
            })
            .collect()
    }

    fn adopt_vocabulary(registry: &ServicesRegistry) {
        registry.lock().adopted = vocabulary()
            .into_iter()
            .map(|AdoptedRecord { key, by, at }| (key, Adoption { by, at }))
            .collect();
    }

    #[tokio::test]
    async fn the_first_state_is_checking_and_the_first_probe_publishes_at_once() {
        let (registry, _engine, _mono) = registry(true);
        assert_eq!(registry.engine_status().state, EngineState::Checking);
        registry.probe_and_apply(false).await;
        let status = registry.engine_status();
        assert_eq!(status.state, EngineState::Ready);
        assert_eq!(status.name.as_deref(), Some("Docker Engine - Community"));
        assert_eq!(status.version.as_deref(), Some("24.0.7"));
        assert!(status.last_ready_at.is_some());
    }

    #[tokio::test]
    async fn a_disabled_engine_is_never_probed() {
        let (registry, engine, _mono) = registry(false);
        assert_eq!(registry.engine_status().state, EngineState::Disabled);
        registry.recheck().await;
        assert_eq!(engine.probe_count(), 0, "le socket n'est jamais ouvert");
        assert!(registry.snapshot().services.is_empty());
    }

    #[tokio::test]
    async fn a_resync_lists_every_container_stopped_ones_included_sorted_by_name() {
        let (registry, engine, _mono) = registry(true);
        engine.put_silently(container("web", RunState::Running));
        engine.put_silently(container("Api", RunState::Exited));
        engine.put_silently(container("db", RunState::Created));
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        let snapshot = registry.snapshot();
        let names: Vec<_> = snapshot
            .services
            .iter()
            .map(|view| view.observation.name.as_str())
            .collect();
        assert_eq!(names, vec!["Api", "db", "web"]);
        assert_eq!(
            snapshot.services[1].derived.state,
            ServiceStateKind::Stopped
        );
        assert!(snapshot.services[1].derived.never_started);
        assert_eq!(snapshot.rev, 2, "l'état du moteur, puis la liste");
    }

    #[tokio::test]
    async fn a_change_is_one_delta_and_nothing_changed_is_no_delta() {
        let (registry, engine, _mono) = registry(true);
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("première lecture");
        let (mut receiver, before) = registry.subscribe();
        // Rien n'a changé : relire ne diffuse rien.
        registry.resync().await.expect("resync");
        assert!(receiver.try_recv().is_err());
        // Une adoption sans unité tombe avec son conteneur (BR-ADOPT-017) : la première lecture, vide,
        // a défait celle du vocabulaire.
        adopt_vocabulary(&registry);
        engine.put_silently(container("web", RunState::Running));
        registry
            .refresh(vec![container("web", RunState::Running).id])
            .await
            .expect("refresh");
        let change = receiver.try_recv().expect("un lot");
        assert_eq!(change.rev, before.rev + 1);
        assert_eq!(change.upserts.len(), 1);
        assert!(change.removed.is_empty() && change.engine.is_none());
        // Le conteneur disparaît : un lot `removed`.
        let id = container("web", RunState::Running).id;
        engine.remove(&id);
        registry.refresh(vec![id.clone()]).await.expect("refresh");
        let change = receiver.try_recv().expect("un lot");
        assert_eq!(change.removed.len(), 1);
        assert!(change.upserts.is_empty());
    }

    #[tokio::test]
    async fn the_engine_state_changes_at_most_once_per_window_and_never_flashes() {
        let (registry, engine, mono) = registry(true);
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::Ready);
        // Le moteur tombe à 1 s : retenu.
        mono.advance(Duration::from_secs(1));
        engine.set_outcome(ProbeOutcome::Refused, true);
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::Ready);
        // Il revient avant la fin de la fenêtre : rien n'est jamais publié.
        mono.advance(Duration::from_secs(1));
        engine.set_probe(ready_probe());
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::Ready);
        // Il retombe, et cette fois la fenêtre se termine : publié à son échéance, avec l'état réel.
        engine.set_outcome(ProbeOutcome::Refused, true);
        mono.advance(Duration::from_secs(1));
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::Ready);
        mono.advance(Duration::from_secs(3));
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::Stopped);
    }

    #[tokio::test]
    async fn an_inaccessible_engine_keeps_the_last_list_and_a_missing_one_drops_it() {
        let (registry, engine, mono) = registry(true);
        engine.put_silently(container("web", RunState::Running));
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        // Arrêté : la liste reste (grisée par le client).
        mono.advance(Duration::from_secs(6));
        engine.set_outcome(ProbeOutcome::Refused, true);
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::Stopped);
        assert_eq!(registry.snapshot().services.len(), 1);
        // Absent : aucune liste ancienne n'est gardée.
        mono.advance(Duration::from_secs(6));
        engine.set_outcome(ProbeOutcome::SocketMissing, false);
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::Absent);
        assert!(registry.snapshot().services.is_empty());
    }

    #[tokio::test]
    async fn a_list_from_before_the_engine_vanished_is_never_shown_as_fresh_when_it_returns() {
        // FIX:01M4SFCQ7N5TN8VC21KBQR87PP : N1 de la revue de la PR 77.
        let (registry, engine, mono) = registry(true);
        engine.put_silently(container("web", RunState::Running));
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        assert_eq!(registry.snapshot().services.len(), 1);
        assert!(!registry.engine_status().list_degraded);
        // Le moteur disparaît (publié après la fenêtre) : le registre est vidé pour de bon.
        mono.advance(Duration::from_secs(6));
        engine.set_outcome(ProbeOutcome::SocketMissing, false);
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::Absent);
        assert!(registry.snapshot().services.is_empty());
        // Il revient : « prêt » est publié AVANT toute relecture. L'ancienne liste ne repart pas, et
        // ce qui est publié n'est pas présenté comme frais.
        mono.advance(Duration::from_secs(6));
        engine.set_ready();
        registry.probe_and_apply(false).await;
        let back = registry.snapshot();
        assert_eq!(back.engine.state, EngineState::Ready);
        assert!(
            back.services.is_empty(),
            "aucun service d'avant la disparition"
        );
        assert!(
            back.engine.list_degraded,
            "liste pas encore relue : marquée"
        );
        assert_eq!(back.engine.list_read_at, None);
        // La relecture propre rétablit la liste et lève la marque.
        registry.resync().await.expect("resync");
        let read = registry.snapshot();
        assert_eq!(read.services.len(), 1);
        assert!(!read.engine.list_degraded);
    }

    #[tokio::test]
    async fn a_list_read_during_a_held_back_return_is_not_kept_across_a_second_loss() {
        // FIX:01M4SFCQ7N5TN8VC21KBQR87PP : N1, enchaînement R1 de la revue de la PR 78.
        let (registry, engine, mono) = registry(true);
        // (1) prêt, liste lue.
        engine.put_silently(container("web", RunState::Running));
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        // (2) le moteur disparaît : « absent » est publié, le registre est vidé.
        mono.advance(Duration::from_secs(6));
        engine.set_outcome(ProbeOutcome::SocketMissing, false);
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::Absent);
        // (3) il revient dans la fenêtre : « prêt » est retenu, mais la liste est relue.
        mono.advance(Duration::from_secs(1));
        engine.set_ready();
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::Absent);
        registry.resync().await.expect("resync");
        // (4) il redisparaît avant l'échéance : rien n'est publié.
        mono.advance(Duration::from_secs(1));
        engine.set_outcome(ProbeOutcome::SocketMissing, false);
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::Absent);
        // Le moteur a changé de contenu entre-temps.
        engine.remove(&container("web", RunState::Running).id);
        engine.put_silently(container("db", RunState::Running));
        // (5) il revient une fois la fenêtre finie : « prêt » est publié tout de suite.
        mono.advance(Duration::from_secs(6));
        engine.set_ready();
        registry.probe_and_apply(false).await;
        let back = registry.snapshot();
        assert_eq!(back.engine.state, EngineState::Ready);
        assert!(
            back.services.is_empty(),
            "la liste lue en (3) date d'avant la seconde disparition : {:?}",
            back.services
        );
        assert!(back.engine.list_degraded, "liste pas relue : marquée");
        assert_eq!(
            back.engine.list_current_at, None,
            "aucune liste, aucune date"
        );
        // La relecture rétablit la liste actuelle (les adoptions tombées avec leurs conteneurs sont
        // reposées : ce test parle de fraîcheur, pas d'adoption).
        adopt_vocabulary(&registry);
        registry.resync().await.expect("resync");
        let read = registry.snapshot();
        assert_eq!(read.services.len(), 1);
        assert!(!read.engine.list_degraded);
        assert_eq!(read.engine.list_current_at, Some(read.engine.at));
    }

    #[tokio::test]
    async fn a_list_not_read_since_a_held_back_loss_is_never_dated_now() {
        // FIX:01M4SFCQ7N5TN8VC21KBQR87PP : variante de R1 (« Vérifier à nouveau » après une coupure retenue).
        let (registry, engine, mono, wall) = registry_with_wall();
        engine.put_silently(container("web", RunState::Running));
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        // Coupure dans la fenêtre : rien n'est publié.
        mono.advance(Duration::from_secs(1));
        wall.advance(time::Duration::seconds(1));
        let lost = registry.clock.now();
        engine.set_outcome(ProbeOutcome::Refused, true);
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::Ready);
        // Retour, relance de l'utilisateur : toujours rien de publié, mais la liste n'est pas relue.
        mono.advance(Duration::from_secs(1));
        wall.advance(time::Duration::seconds(1));
        engine.set_ready();
        registry.probe_and_apply(true).await;
        let status = registry.engine_status();
        assert_eq!(status.state, EngineState::Ready);
        assert_eq!(
            status.list_current_at,
            Some(lost),
            "liste non relue : datée de la perte"
        );
        registry.resync().await.expect("resync");
        let status = registry.engine_status();
        assert_eq!(status.list_current_at, Some(status.at));
    }

    /// Les trois états inaccessibles par lesquels une liste relue pendant un retour retenu peut
    /// traverser une seconde coupure.
    #[derive(Clone, Copy)]
    enum Entry {
        Stopped,
        Unresponsive,
        Denied,
    }

    /// FIX:01M4SFCQ7N5TN8VC21KBQR87PP : suite de N1 (HRT-73), enchaînement en cinq étapes sous un
    /// état INACCESSIBLE publié. La liste relue pendant le retour retenu ne vaut plus rien après une
    /// seconde coupure : au retour, elle est marquée et datée de la coupure, jamais de maintenant.
    async fn assert_second_loss_under_inaccessible_state(entry: Entry) {
        let (registry, engine, mono, wall) = registry_with_wall();
        let loss = |engine: &FakeEngine| match entry {
            Entry::Stopped | Entry::Denied => engine.set_outcome(ProbeOutcome::Refused, true),
            Entry::Unresponsive => engine.set_outcome(ProbeOutcome::NoAnswer, true),
        };
        // (1) prêt, liste lue.
        engine.put_silently(container("web", RunState::Running));
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        // (2) l'état inaccessible est publié, la liste reste (grisée, datée).
        mono.advance(Duration::from_secs(6));
        wall.advance(time::Duration::seconds(6));
        let expected = match entry {
            Entry::Stopped => EngineState::Stopped,
            Entry::Unresponsive => EngineState::Unresponsive,
            Entry::Denied => EngineState::AccessDenied,
        };
        if let Entry::Denied = entry {
            engine.fail_list(Some(EngineError::Denied));
            assert_eq!(registry.resync().await, Err(EngineError::Denied));
        } else {
            loss(&engine);
        }
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, expected);
        assert_eq!(registry.snapshot().services.len(), 1);
        // (3) retour dans la fenêtre : « prêt » est retenu, mais la liste est relue.
        mono.advance(Duration::from_secs(1));
        wall.advance(time::Duration::seconds(1));
        engine.fail_list(None);
        engine.set_ready();
        if !matches!(entry, Entry::Denied) {
            registry.probe_and_apply(false).await;
        }
        registry.resync().await.expect("resync");
        assert_eq!(registry.engine_status().state, expected);
        // (4) seconde coupure avant l'échéance : rien n'est publié.
        mono.advance(Duration::from_secs(1));
        wall.advance(time::Duration::seconds(1));
        let lost = registry.clock.now();
        loss(&engine);
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, expected);
        // (5) coupure de durée quelconque, puis retour une fois la fenêtre finie : « prêt » est publié.
        mono.advance(Duration::from_secs(6));
        wall.advance(time::Duration::hours(3));
        engine.set_ready();
        registry.probe_and_apply(false).await;
        let back = registry.snapshot();
        assert_eq!(back.engine.state, EngineState::Ready);
        assert!(
            back.engine.list_degraded,
            "liste relue avant la seconde coupure : marquée"
        );
        assert_eq!(
            back.engine.list_current_at,
            Some(lost),
            "datée de la coupure, jamais de maintenant"
        );
        // La relecture rétablit la liste actuelle.
        registry.resync().await.expect("resync");
        let read = registry.engine_status();
        assert!(!read.list_degraded);
        assert_eq!(read.list_current_at, Some(read.at));
    }

    #[tokio::test]
    async fn a_list_read_under_stopped_is_not_fresh_after_a_second_loss() {
        assert_second_loss_under_inaccessible_state(Entry::Stopped).await;
    }

    #[tokio::test]
    async fn a_list_read_under_unresponsive_is_not_fresh_after_a_second_loss() {
        assert_second_loss_under_inaccessible_state(Entry::Unresponsive).await;
    }

    #[tokio::test]
    async fn a_list_read_under_access_denied_is_not_fresh_after_a_second_loss() {
        assert_second_loss_under_inaccessible_state(Entry::Denied).await;
    }

    #[tokio::test]
    async fn a_list_read_under_access_denied_is_not_fresh_after_moving_to_stopped() {
        // FIX:01M4SFCQ7N5TN8VC21KBQR87PP : passage d'un état inaccessible à un autre.
        let (registry, engine, mono, wall) = registry_with_wall();
        engine.put_silently(container("web", RunState::Running));
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        // Accès refusé publié après un refus de liste.
        mono.advance(Duration::from_secs(6));
        wall.advance(time::Duration::seconds(6));
        engine.fail_list(Some(EngineError::Denied));
        assert_eq!(registry.resync().await, Err(EngineError::Denied));
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::AccessDenied);
        // La liste est de nouveau permise et relue, toujours sous « accès refusé » publié.
        mono.advance(Duration::from_secs(1));
        wall.advance(time::Duration::seconds(1));
        engine.fail_list(None);
        registry.resync().await.expect("resync");
        // Le moteur s'arrête : « arrêté » est publié à l'échéance.
        let lost = registry.clock.now();
        mono.advance(Duration::from_secs(6));
        wall.advance(time::Duration::seconds(6));
        engine.set_outcome(ProbeOutcome::Refused, true);
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::Stopped);
        // Il revient bien plus tard.
        mono.advance(Duration::from_secs(6));
        wall.advance(time::Duration::hours(3));
        engine.set_ready();
        registry.probe_and_apply(false).await;
        let back = registry.snapshot();
        assert_eq!(back.engine.state, EngineState::Ready);
        assert!(
            back.engine.list_degraded,
            "liste pas relue depuis : marquée"
        );
        assert!(
            back.engine.list_current_at < Some(back.engine.at),
            "jamais datée de maintenant : {:?} (coupure vers {lost})",
            back.engine.list_current_at
        );
    }

    /// Un registre dont on pilote l'horloge murale.
    fn registry_with_wall() -> (Arc<ServicesRegistry>, Arc<FakeEngine>, Arc<Mono>, Arc<Wall>) {
        let engine = Arc::new(FakeEngine::new());
        let mono = Arc::new(Mono::default());
        let wall = Arc::new(Wall(Mutex::new(crate::infrastructure::engine::fake::at(
            12,
        ))));
        let registry = ServicesRegistry::new(
            engine.clone(),
            &Linux,
            wall.clone(),
            mono.clone(),
            true,
            EngineTimings::production(),
        );
        adopt_vocabulary(&registry);
        (registry, engine, mono, wall)
    }

    #[tokio::test]
    async fn the_date_on_the_wire_is_the_last_moment_the_list_was_up_to_date() {
        // FIX:01M4SFCRJWX8KMBC1ZK9ZWETTW : N2 de la revue de la PR 77.
        let (registry, engine, mono, wall) = registry_with_wall();
        engine.put_silently(container("web", RunState::Running));
        // Aucune liste lue : `at` est là (l'agent connaît le champ), pas de date de liste.
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().list_current_at, None);
        let read = registry.clock.now();
        registry.resync().await.expect("resync");
        assert_eq!(registry.engine_status().list_read_at, Some(read));
        // Trois jours plus tard, la liste suivie en direct est toujours à jour : son âge est zéro,
        // alors que la dernière relecture complète date de trois jours.
        wall.advance(time::Duration::days(3));
        mono.advance(Duration::from_secs(1));
        registry.probe_and_apply(false).await;
        let status = registry.engine_status();
        assert_eq!(status.list_read_at, Some(read));
        assert_eq!(status.list_current_at, Some(status.at));
        assert_eq!(status.at, read + time::Duration::days(3));
        // Le moteur cesse de répondre : la liste cesse d'être à jour À CET INSTANT, pas à la lecture.
        let lost = registry.clock.now();
        engine.set_outcome(ProbeOutcome::Refused, true);
        mono.advance(Duration::from_secs(1));
        registry.probe_and_apply(false).await;
        wall.advance(time::Duration::minutes(10));
        mono.advance(Duration::from_secs(6));
        registry.probe_and_apply(false).await;
        let status = registry.engine_status();
        assert_eq!(status.state, EngineState::Stopped);
        assert_eq!(status.list_current_at, Some(lost));
        assert_eq!(status.at - lost, time::Duration::minutes(10));
        assert_eq!(registry.snapshot().services.len(), 1, "grisée, datée");
        // Une sonde de plus ne rajeunit pas la liste.
        wall.advance(time::Duration::minutes(1));
        mono.advance(Duration::from_secs(6));
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().list_current_at, Some(lost));
        // Le moteur revient, la liste est relue : de nouveau à jour.
        engine.set_ready();
        mono.advance(Duration::from_secs(6));
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        let status = registry.engine_status();
        assert_eq!(status.list_current_at, Some(status.at));
        assert!(!status.list_degraded);
    }

    #[tokio::test]
    async fn a_list_that_stops_being_followed_is_dated_from_the_moment_it_was_noticed() {
        let (registry, engine, _mono, wall) = registry_with_wall();
        let web = container("web", RunState::Running);
        engine.put_silently(web.clone());
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        wall.advance(time::Duration::hours(2));
        let noticed = registry.clock.now();
        engine.fail_inspect(&web.id, Some(EngineError::Timeout));
        assert_eq!(registry.refresh(vec![web.id.clone()]).await, Ok(1));
        wall.advance(time::Duration::minutes(5));
        let status = registry.engine_status();
        assert!(status.list_degraded);
        assert_eq!(status.list_current_at, Some(noticed));
        assert_eq!(status.at - noticed, time::Duration::minutes(5));
    }

    #[tokio::test]
    async fn a_missing_engine_has_no_list_to_date() {
        let (registry, engine, _mono, _wall) = registry_with_wall();
        engine.set_outcome(ProbeOutcome::SocketMissing, false);
        registry.probe_and_apply(false).await;
        let status = registry.engine_status();
        assert_eq!(status.state, EngineState::Absent);
        assert_eq!(status.list_current_at, None);
        assert_eq!(status.list_read_at, None);
    }

    #[tokio::test]
    async fn a_recheck_reflects_the_result_at_once_and_is_rate_limited() {
        let (registry, engine, mono) = registry(true);
        registry.probe_and_apply(false).await;
        mono.advance(Duration::from_secs(2));
        engine.set_outcome(ProbeOutcome::NoAnswer, true);
        let status = registry.recheck().await;
        assert_eq!(status.state, EngineState::Unresponsive);
        // Une seconde demande tout de suite : l'état courant, sans nouvelle sonde.
        let probes = engine.probe_count();
        registry.recheck().await;
        assert_eq!(engine.probe_count(), probes);
        mono.advance(Duration::from_secs(2));
        registry.recheck().await;
        assert_eq!(engine.probe_count(), probes + 1);
    }

    #[tokio::test]
    async fn three_starts_in_ten_minutes_make_a_restart_loop_and_old_ones_do_not_count() {
        let start = crate::infrastructure::engine::fake::at(10);
        let (registry, engine, _mono) = registry_at(true, start + time::Duration::minutes(5));
        registry.probe_and_apply(false).await;
        let mut web = container("web", RunState::Running);
        for minute in [0i64, 2, 4] {
            web.started_at = Some(start + time::Duration::minutes(minute));
            engine.put_silently(web.clone());
            registry
                .refresh(vec![web.id.clone()])
                .await
                .expect("refresh");
        }
        let view = registry.snapshot().services.remove(0);
        assert_eq!(view.restarts_10m, 3);
        assert_eq!(view.derived.state, ServiceStateKind::RestartLoop);

        // Les mêmes démarrages, vus une heure plus tard : hors fenêtre, le service tourne.
        let (late, engine, _mono) = registry_at(true, start + time::Duration::hours(1));
        late.probe_and_apply(false).await;
        engine.put_silently(web.clone());
        late.refresh(vec![web.id.clone()]).await.expect("refresh");
        let view = late.snapshot().services.remove(0);
        assert_eq!(view.restarts_10m, 0);
        assert_eq!(view.derived.state, ServiceStateKind::Running);
    }

    fn fast(ready_ms: u64) -> EngineTimings {
        EngineTimings {
            ready_period: Duration::from_millis(ready_ms),
            other_period: Duration::from_millis(ready_ms),
            debounce_window: Duration::from_millis(10),
            batch_window: Duration::from_millis(5),
            min_recheck: Duration::ZERO,
        }
    }

    fn registry_timed(timings: EngineTimings) -> (Arc<ServicesRegistry>, Arc<FakeEngine>) {
        let engine = Arc::new(FakeEngine::new());
        let registry = ServicesRegistry::new(
            engine.clone(),
            &Linux,
            Arc::new(Wall(Mutex::new(crate::infrastructure::engine::fake::at(
                12,
            )))),
            Arc::new(Mono::default()),
            true,
            timings,
        );
        adopt_vocabulary(&registry);
        (registry, engine)
    }

    #[tokio::test]
    async fn a_ready_engine_without_a_container_gives_an_empty_list() {
        let (registry, _engine, _mono) = registry(true);
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        let snapshot = registry.snapshot();
        assert_eq!(snapshot.engine.state, EngineState::Ready);
        assert!(snapshot.services.is_empty());
    }

    #[tokio::test]
    async fn one_unreadable_container_does_not_freeze_the_list() {
        let (registry, engine, _mono) = registry(true);
        let web = container("web", RunState::Running);
        engine.put_silently(web.clone());
        engine.put_silently(container("db", RunState::Running));
        registry.probe_and_apply(false).await;
        assert_eq!(registry.resync().await.expect("resync"), 0);
        // `db` devient illisible (délai, réponse illisible) : sa ligne garde sa dernière description,
        // le reste de la liste continue de suivre le moteur.
        let db_id = container("db", RunState::Running).id;
        engine.fail_inspect(&db_id, Some(EngineError::Timeout));
        engine.update(&web.id, |observation| {
            observation.run_state = RunState::Exited
        });
        assert_eq!(registry.resync().await.expect("resync"), 1);
        let snapshot = registry.snapshot();
        let state_of = |name: &str| {
            snapshot
                .services
                .iter()
                .find(|view| view.observation.name == name)
                .map(|view| view.derived.state)
        };
        assert_eq!(state_of("db"), Some(ServiceStateKind::Running), "gardée");
        assert_eq!(state_of("web"), Some(ServiceStateKind::Stopped), "suivie");
        // Un conteneur NOUVEAU et illisible apparaît « non reconnu », sous son nom.
        adopt_vocabulary(&registry);
        engine.put_silently(container("new", RunState::Running));
        engine.fail_inspect(
            &container("new", RunState::Running).id,
            Some(EngineError::Refused),
        );
        assert_eq!(registry.resync().await.expect("resync"), 2);
        let view = registry
            .snapshot()
            .services
            .into_iter()
            .find(|view| view.observation.name == "new")
            .expect("la ligne existe");
        assert_eq!(view.derived.state, ServiceStateKind::Unknown);
        // Rétabli : tout est relu.
        engine.fail_inspect(&db_id, None);
        engine.fail_inspect(&container("new", RunState::Running).id, None);
        assert_eq!(registry.resync().await.expect("resync"), 0);
        assert!(
            registry
                .snapshot()
                .services
                .iter()
                .all(|view| view.derived.state != ServiceStateKind::Unknown)
        );
    }

    #[tokio::test]
    async fn an_event_for_an_unreadable_container_keeps_it_and_says_so() {
        let (registry, engine, _mono) = registry(true);
        let web = container("web", RunState::Running);
        engine.put_silently(web.clone());
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        engine.update(&web.id, |observation| {
            observation.run_state = RunState::Exited
        });
        engine.fail_inspect(&web.id, Some(EngineError::Timeout));
        assert_eq!(registry.refresh(vec![web.id.clone()]).await, Ok(1));
        assert_eq!(
            registry.snapshot().services[0].derived.state,
            ServiceStateKind::Running
        );
        // Le moteur lui-même est perdu : c'est une erreur, pas un conteneur illisible.
        engine.fail_inspect(&web.id, Some(EngineError::Unreachable));
        assert_eq!(
            registry.refresh(vec![web.id.clone()]).await,
            Err(EngineError::Unreachable)
        );
    }

    #[tokio::test]
    async fn a_refused_list_while_the_probe_answers_is_access_denied_not_ready() {
        let (registry, engine, mono) = registry(true);
        engine.put_silently(container("web", RunState::Running));
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::Ready);
        engine.fail_list(Some(EngineError::Denied));
        assert_eq!(registry.resync().await, Err(EngineError::Denied));
        // La fenêtre de l'anti-clignotement passée, l'état publié est « accès refusé » et le reste
        // tant que la liste est refusée, même si la sonde répond.
        mono.advance(Duration::from_secs(6));
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::AccessDenied);
        mono.advance(Duration::from_secs(6));
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::AccessDenied);
        // La liste est de nouveau permise : l'état « prêt » revient.
        engine.fail_list(None);
        mono.advance(Duration::from_secs(6));
        registry.resync().await.expect("resync");
        assert_eq!(registry.engine_status().state, EngineState::Ready);
    }

    #[tokio::test]
    async fn a_refusal_does_not_outlive_the_engine_that_vanished() {
        let (registry, engine, mono) = registry(true);
        engine.put_silently(container("web", RunState::Running));
        registry.probe_and_apply(false).await;
        engine.fail_list(Some(EngineError::Denied));
        assert_eq!(registry.resync().await, Err(EngineError::Denied));
        assert!(registry.list_denied());
        // Le moteur disparaît, puis revient avec une liste permise : plus aucun « accès refusé ».
        engine.set_outcome(ProbeOutcome::SocketMissing, true);
        registry.probe_and_apply(false).await;
        assert!(!registry.list_denied());
        engine.set_ready();
        engine.fail_list(None);
        mono.advance(Duration::from_secs(6));
        registry.probe_and_apply(false).await;
        mono.advance(Duration::from_secs(6));
        registry.probe_and_apply(false).await;
        assert_eq!(registry.engine_status().state, EngineState::Ready);
    }

    #[tokio::test]
    async fn a_listing_that_keeps_failing_backs_off_instead_of_looping() {
        let (registry, engine) = registry_timed(fast(50));
        engine.fail_list(Some(EngineError::Refused));
        let task = tokio::spawn(registry.clone().run());
        tokio::time::sleep(Duration::from_secs(1)).await;
        task.abort();
        let reads = engine.list_count();
        assert!((2..=8).contains(&reads), "{reads} relectures en 1 s");
        // Et la sonde, elle, garde sa cadence : l'état reste suivi.
        assert!(engine.probe_count() >= 8, "{} sondes", engine.probe_count());
    }

    #[tokio::test]
    async fn a_slow_listing_does_not_blind_the_probe_nor_the_debounce() {
        let (registry, engine) = registry_timed(fast(50));
        engine.put_silently(container("web", RunState::Running));
        engine.set_list_delay(Duration::from_secs(3));
        let task = tokio::spawn(registry.clone().run());
        // La première lecture dure 3 s ; pendant ce temps, la sonde garde sa cadence (50 ms)...
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let probes = engine.probe_count();
        assert!(probes >= 8, "{probes} sondes pendant une lecture lente");
        // ... et un moteur qui tombe est vu SANS attendre la fin de la lecture (l'horloge de ce test est
        // figée : l'état publié attend l'anti-clignotement, l'état observé non).
        engine.set_outcome(ProbeOutcome::SocketMissing, false);
        let started = Instant::now();
        while registry.lock().observed == EngineState::Ready {
            assert!(
                started.elapsed() < Duration::from_secs(1),
                "perte du moteur non vue pendant la lecture lente"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        task.abort();
    }

    #[tokio::test]
    async fn a_failed_or_partial_read_is_told_with_the_date_of_the_last_good_one() {
        let (registry, engine, mono) = registry(true);
        let web = container("web", RunState::Running);
        engine.put_silently(web.clone());
        registry.probe_and_apply(false).await;
        // Avant toute lecture : ni date, et la liste (vide) est marquée : elle n'a pas été lue.
        assert_eq!(registry.engine_status().list_read_at, None);
        assert_eq!(registry.engine_status().list_current_at, None);
        assert!(registry.engine_status().list_degraded);
        registry.resync().await.expect("resync");
        let first = registry.engine_status();
        assert_eq!(first.list_read_at, Some(registry.clock.now()));
        assert!(!first.list_degraded);
        // Lecture en échec alors que la sonde répond : la liste d'avant reste, datée, marquée.
        engine.fail_list(Some(EngineError::Refused));
        assert_eq!(registry.resync().await, Err(EngineError::Refused));
        mono.advance(Duration::from_secs(6));
        let failed = registry.engine_status();
        assert!(failed.list_degraded);
        assert_eq!(
            failed.list_read_at, first.list_read_at,
            "date de la dernière bonne lecture"
        );
        assert_eq!(
            registry.snapshot().services.len(),
            1,
            "la liste d'avant reste"
        );
        // Rétablie : le drapeau tombe.
        engine.fail_list(None);
        registry.resync().await.expect("resync");
        assert!(!registry.engine_status().list_degraded);
        // Lecture partielle : un conteneur illisible marque la liste, sans changer la date.
        engine.fail_inspect(&web.id, Some(EngineError::Timeout));
        let before = registry.engine_status().list_read_at;
        assert_eq!(registry.resync().await, Ok(1));
        assert!(registry.engine_status().list_degraded);
        assert_eq!(registry.engine_status().list_read_at, before);
        // Un conteneur touché par un événement et illisible marque aussi la liste.
        engine.fail_inspect(&web.id, None);
        registry.resync().await.expect("resync");
        assert!(!registry.engine_status().list_degraded);
        engine.fail_inspect(&web.id, Some(EngineError::Timeout));
        assert_eq!(registry.refresh(vec![web.id.clone()]).await, Ok(1));
        assert!(registry.engine_status().list_degraded);
    }

    #[tokio::test]
    async fn a_change_of_list_freshness_is_published_to_subscribers() {
        let (registry, engine, _mono) = registry(true);
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        let (mut changes, _) = registry.subscribe();
        engine.fail_list(Some(EngineError::Refused));
        let _ = registry.resync().await;
        let change = changes.try_recv().expect("un lot");
        assert!(change.engine.as_ref().expect("moteur").list_degraded);
    }

    #[tokio::test]
    async fn an_event_stream_that_breaks_at_once_is_not_retried_in_a_tight_loop() {
        let (registry, engine) = registry_timed(fast(50));
        engine.set_events_broken(true);
        let task = tokio::spawn(registry.clone().run());
        tokio::time::sleep(Duration::from_secs(1)).await;
        task.abort();
        let reads = engine.list_count();
        // La première rupture est reprise tout de suite ; les suivantes reculent.
        assert!((2..=14).contains(&reads), "{reads} relectures en 1 s");
    }

    // -----------------------------------------------------------------------------------------
    // Services supervisés (HRT-63) : clé `u_`, service gardé d'après son unité, mémoire, révélation
    // -----------------------------------------------------------------------------------------

    use crate::domain::services::{Hidden, UNIT_GONE_READS, UnitActivity};
    use crate::infrastructure::supervisor::fake::{FakeSupervisor, running_unit, unit_named};

    /// Un dépôt en mémoire : ce qu'il a reçu, pour vérifier ce que l'agent retient.
    struct MemoryRepo {
        saved: Mutex<HashMap<UnitName, SupervisedRecord>>,
        forgotten: Mutex<Vec<UnitName>>,
        saves: AtomicU64,
        /// Les services adoptés que la base « contient » (HRT-79).
        adopted: Mutex<Vec<AdoptedRecord>>,
        /// La base refuse d'effacer une adoption (indisponible).
        forget_fails: std::sync::atomic::AtomicBool,
    }

    #[async_trait]
    impl ServiceRepo for MemoryRepo {
        async fn load_supervised(&self) -> Result<Vec<SupervisedRecord>, StoreError> {
            let saved = self.saved.lock().unwrap_or_else(PoisonError::into_inner);
            Ok(saved.values().cloned().collect())
        }

        async fn save_supervised(&self, record: &SupervisedRecord) -> Result<(), StoreError> {
            self.saves.fetch_add(1, Ordering::SeqCst);
            self.saved
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(record.unit.clone(), record.clone());
            Ok(())
        }

        async fn forget_supervised(&self, unit: &UnitName) -> Result<(), StoreError> {
            self.saved
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(unit);
            self.forgotten
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(unit.clone());
            Ok(())
        }

        async fn load_adopted(&self) -> Result<Vec<AdoptedRecord>, StoreError> {
            Ok(self
                .adopted
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone())
        }

        async fn forget_adopted(&self, key: &ServiceKey) -> Result<(), StoreError> {
            if self.forget_fails.load(Ordering::SeqCst) {
                return Err(StoreError::Unavailable {
                    resource: "adopted_services",
                    message: "test".to_owned(),
                });
            }
            self.adopted
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .retain(|record| &record.key != key);
            Ok(())
        }
    }

    impl Default for MemoryRepo {
        fn default() -> Self {
            Self {
                saved: Mutex::default(),
                forgotten: Mutex::default(),
                saves: AtomicU64::new(0),
                adopted: Mutex::new(vocabulary()),
                forget_fails: std::sync::atomic::AtomicBool::new(false),
            }
        }
    }

    type Supervised = (
        Arc<ServicesRegistry>,
        Arc<FakeEngine>,
        Arc<FakeSupervisor>,
        Arc<MemoryRepo>,
    );

    fn supervised_registry() -> Supervised {
        let engine = Arc::new(FakeEngine::new());
        let supervisor = Arc::new(FakeSupervisor::new());
        let repo = Arc::new(MemoryRepo::default());
        let registry = supervised_over(&engine, &supervisor, &repo);
        (registry, engine, supervisor, repo)
    }

    fn supervised_over(
        engine: &Arc<FakeEngine>,
        supervisor: &Arc<FakeSupervisor>,
        repo: &Arc<MemoryRepo>,
    ) -> Arc<ServicesRegistry> {
        let wall = Arc::new(Wall(Mutex::new(crate::infrastructure::engine::fake::at(
            12,
        ))));
        supervised_with_clock(engine, supervisor, repo, wall)
    }

    fn supervised_with_clock(
        engine: &Arc<FakeEngine>,
        supervisor: &Arc<FakeSupervisor>,
        repo: &Arc<MemoryRepo>,
        wall: Arc<Wall>,
    ) -> Arc<ServicesRegistry> {
        let registry = ServicesRegistry::with_supervision(
            engine.clone(),
            supervisor.clone(),
            repo.clone(),
            &Linux,
            wall,
            Arc::new(Mono::default()),
            true,
            EngineTimings::production(),
        );
        adopt_vocabulary(&registry);
        registry
    }

    /// Le conteneur qu'un système recrée à chaque démarrage : même nom, autre identifiant.
    fn recreated(name: &str, state: RunState, id_char: char) -> Observation {
        let mut observation = container(name, state);
        observation.id = id_char.to_string().repeat(64);
        observation
    }

    fn only_service(registry: &ServicesRegistry) -> ServiceView {
        let mut services = registry.snapshot().services;
        assert_eq!(services.len(), 1, "{services:?}");
        services.remove(0)
    }

    #[tokio::test]
    async fn a_container_under_a_unit_is_the_service_u_unit_whatever_container_the_system_creates()
    {
        let (registry, engine, supervisor, _repo) = supervised_registry();
        let unit = unit_named("docker-terraria.service");
        let first = recreated("terraria", RunState::Running, 'a');
        supervisor.set_owner(&first.id, &unit);
        supervisor.set_status(&unit, running_unit());
        engine.put_silently(first.clone());
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        let view = only_service(&registry);
        assert_eq!(view.key.as_str(), "u_docker-terraria.service");
        assert_eq!(view.derived.state, ServiceStateKind::Running);
        let supervision = view.supervision.expect("supervisé");
        assert!(supervision.container_present);
        assert_eq!(supervision.unit, unit);
        assert_eq!(
            supervision.status.map(|s| s.timeout_stop_s),
            Some(Some(120))
        );

        // Le système arrête puis relance : un autre conteneur, le même service.
        engine.remove(&first.id);
        let second = recreated("terraria", RunState::Running, 'b');
        supervisor.set_owner(&second.id, &unit);
        engine.put_silently(second.clone());
        registry.resync().await.expect("resync");
        let view = only_service(&registry);
        assert_eq!(view.key.as_str(), "u_docker-terraria.service");
        assert_eq!(view.observation.id, second.id);
    }

    #[tokio::test]
    async fn a_container_without_an_owner_keeps_its_container_key() {
        let (registry, engine, _supervisor, _repo) = supervised_registry();
        let web = container("web", RunState::Running);
        engine.put_silently(web.clone());
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        let view = only_service(&registry);
        assert_eq!(view.key.as_str(), format!("c_{}", web.id));
        assert!(view.supervision.is_none());
    }

    #[tokio::test]
    async fn the_owner_of_a_container_is_searched_once_for_its_whole_life() {
        let (registry, engine, supervisor, _repo) = supervised_registry();
        engine.put_silently(container("web", RunState::Running));
        engine.put_silently(container("db", RunState::Running));
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        assert_eq!(supervisor.owner_containers_asked(), 2);
        // Relire n'y change rien : ni /proc relu, ni conteneur redemandé.
        registry.resync().await.expect("resync");
        registry
            .refresh(vec![container("web", RunState::Running).id])
            .await
            .expect("refresh");
        assert_eq!(supervisor.owner_containers_asked(), 2);
        // Un conteneur neuf est cherché, lui seul.
        engine.put_silently(container("cache", RunState::Running));
        registry.resync().await.expect("resync");
        assert_eq!(supervisor.owner_containers_asked(), 3);
    }

    #[tokio::test]
    async fn a_supervised_service_whose_container_vanished_stays_listed_from_its_unit() {
        let (registry, engine, supervisor, repo) = supervised_registry();
        let unit = unit_named("docker-terraria.service");
        let mut first = recreated("terraria", RunState::Running, 'a');
        first.auto_remove = true;
        first.stop_timeout_s = Some(120);
        supervisor.set_owner(&first.id, &unit);
        supervisor.set_status(&unit, running_unit());
        engine.put_silently(first.clone());
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");

        // Le conteneur est supprimé à l'arrêt (`--rm`) : le service reste listé.
        engine.remove(&first.id);
        let set = |activity, auto_restart| {
            supervisor.set_status(
                &unit,
                UnitStatus {
                    activity,
                    auto_restart,
                    restarts: true,
                    timeout_stop_s: Some(120),
                },
            );
        };
        for (activity, auto_restart, expected) in [
            (UnitActivity::Inactive, false, ServiceStateKind::Stopped),
            (UnitActivity::Failed, false, ServiceStateKind::Error),
            // UNE attente de relance est un démarrage, pas une boucle (`BR-SVC-006`).
            (UnitActivity::Activating, true, ServiceStateKind::Starting),
            (UnitActivity::Activating, false, ServiceStateKind::Starting),
        ] {
            set(activity, auto_restart);
            registry
                .refresh(vec![first.id.clone()])
                .await
                .expect("refresh");
            registry.poll_units().await;
            let view = only_service(&registry);
            assert_eq!(view.key.as_str(), "u_docker-terraria.service");
            assert_eq!(view.derived.state, expected, "{activity:?}");
            let supervision = view.supervision.as_ref().expect("supervisé");
            assert!(!supervision.container_present);
            // Les derniers réglages connus : nom, image, suppression à l'arrêt.
            assert_eq!(view.observation.name, "terraria");
            assert!(view.observation.auto_remove);
        }
        // L'unité est connue de la base : elle l'a retenue dès le premier passage.
        let saved = repo.saved.lock().unwrap_or_else(PoisonError::into_inner);
        assert!(saved.contains_key(&unit));
    }

    fn waiting_for_relaunch() -> UnitStatus {
        UnitStatus {
            activity: UnitActivity::Activating,
            auto_restart: true,
            restarts: true,
            timeout_stop_s: Some(120),
        }
    }

    #[tokio::test]
    async fn three_relaunches_by_the_system_are_a_loop_one_is_not() {
        let (registry, engine, supervisor, _repo) = supervised_registry();
        let unit = unit_named("docker-x.service");
        let base = crate::infrastructure::engine::fake::at(12);
        supervisor.set_status(&unit, running_unit());
        let mut last = recreated("x", RunState::Running, 'a');
        for (index, id_char) in ['a', 'b', 'c'].into_iter().enumerate() {
            engine.remove(&last.id);
            last = recreated("x", RunState::Running, id_char);
            last.started_at = Some(base - time::Duration::minutes(4 - 2 * index as i64));
            supervisor.set_owner(&last.id, &unit);
            engine.put_silently(last.clone());
            registry.probe_and_apply(false).await;
            registry.resync().await.expect("resync");
            if index == 0 {
                // Un seul démarrage, puis l'unité attend de relancer le conteneur disparu.
                engine.remove(&last.id);
                supervisor.set_status(&unit, waiting_for_relaunch());
                registry.resync().await.expect("resync");
                registry.poll_units().await;
                let view = only_service(&registry);
                assert_eq!(view.restarts_10m, 1);
                assert_eq!(
                    view.derived.state,
                    ServiceStateKind::Starting,
                    "une seule relance n'est pas une boucle"
                );
                supervisor.set_status(&unit, running_unit());
            }
        }
        engine.remove(&last.id);
        supervisor.set_status(&unit, waiting_for_relaunch());
        registry.resync().await.expect("resync");
        registry.poll_units().await;
        let view = only_service(&registry);
        assert_eq!(view.restarts_10m, 3);
        assert_eq!(view.derived.state, ServiceStateKind::RestartLoop);
    }

    #[tokio::test]
    async fn a_unit_active_without_a_container_stops_being_starting_after_ten_minutes() {
        let engine = Arc::new(FakeEngine::new());
        let supervisor = Arc::new(FakeSupervisor::new());
        let repo = Arc::new(MemoryRepo::default());
        let wall = Arc::new(Wall(Mutex::new(crate::infrastructure::engine::fake::at(
            12,
        ))));
        let registry = supervised_with_clock(&engine, &supervisor, &repo, wall.clone());
        let unit = unit_named("docker-x.service");
        let first = recreated("x", RunState::Running, 'a');
        supervisor.set_owner(&first.id, &unit);
        supervisor.set_status(&unit, running_unit());
        engine.put_silently(first.clone());
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        // Le conteneur finit, l'unité reste « active » : le moteur n'a rien.
        engine.remove(&first.id);
        registry.resync().await.expect("resync");
        registry.poll_units().await;
        assert_eq!(
            only_service(&registry).derived.state,
            ServiceStateKind::Starting
        );
        wall.advance(time::Duration::minutes(11));
        registry.poll_units().await;
        let view = only_service(&registry);
        assert_eq!(view.derived.state, ServiceStateKind::Unknown);
        assert!(!view.supervision.expect("supervisé").container_present);
    }

    #[tokio::test]
    async fn two_live_containers_under_one_unit_are_both_listed_and_neither_is_supervised() {
        let (registry, engine, supervisor, _repo) = supervised_registry();
        let unit = unit_named("docker-x.service");
        let first = recreated("x", RunState::Running, 'a');
        supervisor.set_owner(&first.id, &unit);
        supervisor.set_status(&unit, running_unit());
        engine.put_silently(first.clone());
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        assert_eq!(
            only_service(&registry).key.as_str(),
            "u_docker-x.service",
            "seul sous son unité, il est supervisé"
        );
        // Un second conteneur vivant revendique la même unité : plus aucun n'est supervisé, aucun ne
        // disparaît de la liste (`BR-SVC-001`).
        let second = recreated("y", RunState::Running, 'b');
        supervisor.set_owner(&second.id, &unit);
        engine.put_silently(second.clone());
        adopt_vocabulary(&registry);
        registry.resync().await.expect("resync");
        let services = registry.snapshot().services;
        let mut keys: Vec<String> = services
            .iter()
            .map(|view| view.key.as_str().to_owned())
            .collect();
        keys.sort();
        assert_eq!(
            keys,
            vec![format!("c_{}", first.id), format!("c_{}", second.id)]
        );
        assert!(services.iter().all(|view| view.supervision.is_none()));
    }

    #[tokio::test]
    async fn a_stopped_leftover_of_a_recreation_does_not_unsupervise_the_live_container() {
        let (registry, engine, supervisor, _repo) = supervised_registry();
        let unit = unit_named("docker-x.service");
        let live = recreated("x", RunState::Running, 'a');
        let stale = recreated("x-old", RunState::Exited, 'b');
        supervisor.set_owner(&live.id, &unit);
        supervisor.set_owner(&stale.id, &unit);
        supervisor.set_status(&unit, running_unit());
        engine.put_silently(live.clone());
        engine.put_silently(stale);
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        let view = only_service(&registry);
        assert_eq!(view.key.as_str(), "u_docker-x.service");
        assert_eq!(view.observation.id, live.id);
    }

    #[tokio::test]
    async fn an_unknown_unit_state_is_not_a_stopped_service() {
        let (registry, engine, supervisor, _repo) = supervised_registry();
        let unit = unit_named("docker-x.service");
        let first = recreated("x", RunState::Running, 'a');
        supervisor.set_owner(&first.id, &unit);
        supervisor.set_status(&unit, running_unit());
        engine.put_silently(first.clone());
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        engine.remove(&first.id);
        // Le superviseur se tait (pas de systemctl) : « état non reconnu », jamais « arrêté ».
        supervisor.set_unit(&unit, UnitLookup::Unavailable);
        registry.poll_units().await;
        registry
            .refresh(vec![first.id.clone()])
            .await
            .expect("refresh");
        assert_eq!(
            only_service(&registry).derived.state,
            ServiceStateKind::Unknown
        );
    }

    #[tokio::test]
    async fn a_unit_the_system_forgets_is_dropped_with_its_memory() {
        let (registry, engine, supervisor, repo) = supervised_registry();
        let unit = unit_named("docker-x.service");
        let first = recreated("x", RunState::Running, 'a');
        supervisor.set_owner(&first.id, &unit);
        supervisor.set_status(&unit, running_unit());
        engine.put_silently(first.clone());
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        engine.remove(&first.id);
        registry
            .refresh(vec![first.id.clone()])
            .await
            .expect("refresh");
        assert_eq!(
            registry.snapshot().services.len(),
            1,
            "gardé tant que l'unité existe"
        );
        supervisor.set_unit(&unit, UnitLookup::NotLoaded);
        for _ in 0..UNIT_GONE_READS {
            registry.poll_units().await;
        }
        assert!(registry.snapshot().services.is_empty());
        let forgotten = repo
            .forgotten
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        assert_eq!(*forgotten, vec![unit]);
    }

    #[tokio::test]
    async fn only_the_settings_are_retained_never_a_variable_name_or_the_command() {
        let (registry, engine, supervisor, repo) = supervised_registry();
        let unit = unit_named("docker-x.service");
        let mut first = recreated("x", RunState::Running, 'a');
        first.env_names = vec![Hidden::new("SENTINEL_NAME")];
        first.command = Some(vec![Hidden::new("--token=SENTINEL-CMD")]);
        supervisor.set_owner(&first.id, &unit);
        supervisor.set_status(&unit, running_unit());
        engine.put_silently(first.clone());
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        // Le détail complet est servi tant que le conteneur existe...
        let view = only_service(&registry);
        assert_eq!(view.observation.env_names.len(), 1);
        // ... mais rien de cela n'est retenu.
        let saved = repo.saved.lock().unwrap_or_else(PoisonError::into_inner);
        let record = saved.get(&unit).expect("retenu");
        assert!(record.observation.env_names.is_empty());
        assert!(record.observation.command.is_none());
        assert_eq!(record.observation.run_state, RunState::Exited);
        assert!(!format!("{record:?}").contains("SENTINEL"));
    }

    #[tokio::test]
    async fn a_remembered_service_is_listed_after_a_restart_of_the_agent_without_its_container() {
        let engine = Arc::new(FakeEngine::new());
        let supervisor = Arc::new(FakeSupervisor::new());
        let repo = Arc::new(MemoryRepo::default());
        let unit = unit_named("docker-terraria.service");
        {
            let registry = supervised_over(&engine, &supervisor, &repo);
            let first = recreated("terraria", RunState::Running, 'a');
            supervisor.set_owner(&first.id, &unit);
            supervisor.set_status(&unit, running_unit());
            engine.put_silently(first.clone());
            registry.probe_and_apply(false).await;
            registry.resync().await.expect("resync");
            engine.remove(&first.id);
        }
        // L'agent redémarre : le conteneur n'existe plus, l'unité est arrêtée.
        supervisor.set_status(
            &unit,
            UnitStatus {
                activity: UnitActivity::Inactive,
                auto_restart: false,
                restarts: true,
                timeout_stop_s: Some(120),
            },
        );
        let registry = supervised_over(&engine, &supervisor, &repo);
        registry.load_remembered().await;
        registry.poll_units().await;
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        let view = only_service(&registry);
        assert_eq!(view.key.as_str(), "u_docker-terraria.service");
        assert_eq!(view.derived.state, ServiceStateKind::Stopped);
        assert_eq!(
            view.derived.exit_code, None,
            "pas de code après un redémarrage de l'agent"
        );
    }

    /// Un journal qui se souvient de ce qu'on lui écrit.
    #[derive(Default)]
    struct RecordingSink {
        entries: Mutex<Vec<(AuditAction, Target, Outcome)>>,
        /// Le journal refuse les écritures obligatoires (base pleine ou verrouillée).
        failing: std::sync::atomic::AtomicBool,
    }

    #[async_trait]
    impl AuditSink for RecordingSink {
        async fn record(
            &self,
            _actor: Actor,
            action: AuditAction,
            target: Target,
            outcome: Outcome,
        ) {
            self.entries
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((action, target, outcome));
        }

        async fn record_required(
            &self,
            actor: Actor,
            action: AuditAction,
            target: Target,
            outcome: Outcome,
        ) -> Result<(), crate::application::ports::AuditWriteError> {
            if self.failing.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(crate::application::ports::AuditWriteError);
            }
            self.record(actor, action, target, outcome).await;
            Ok(())
        }

        async fn record_grouped_required(
            &self,
            actor: Actor,
            action: AuditAction,
            target: Target,
            outcome: Outcome,
        ) -> Result<(), crate::application::ports::AuditWriteError> {
            self.record_required(actor, action, target, outcome).await
        }
    }

    #[tokio::test]
    async fn no_value_is_returned_when_the_journal_write_fails() {
        let (registry, engine, _supervisor, _repo) = supervised_registry();
        let web = container("web", RunState::Running);
        engine.put_silently(web.clone());
        engine.set_env(&web.id, &[("DB_PASSWORD", "SENTINEL-VALUE")]);
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        let key = only_service(&registry).key;
        let sink = RecordingSink::default();
        sink.failing
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let outcome = registry.reveal_env(&key, Actor::system(), &sink).await;
        assert_eq!(outcome.err(), Some(RevealError::JournalUnavailable));
        assert!(
            sink.entries
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_empty(),
            "rien n'est consigné, rien n'est rendu"
        );
        sink.failing
            .store(false, std::sync::atomic::Ordering::SeqCst);
        assert!(
            registry
                .reveal_env(&key, Actor::system(), &sink)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn the_values_are_read_only_on_request_and_the_success_is_journaled_without_them() {
        let (registry, engine, _supervisor, _repo) = supervised_registry();
        let web = container("web", RunState::Running);
        engine.put_silently(web.clone());
        engine.set_env(&web.id, &[("DB_PASSWORD", "SENTINEL-VALUE")]);
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        registry
            .refresh(vec![web.id.clone()])
            .await
            .expect("refresh");
        assert_eq!(
            engine.env_read_count(),
            0,
            "aucune valeur n'est lue en arrière-plan"
        );
        let key = only_service(&registry).key;
        let sink = RecordingSink::default();
        let entries = registry
            .reveal_env(&key, Actor::system(), &sink)
            .await
            .expect("révélées");
        assert_eq!(engine.env_read_count(), 1);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name.expose(), "DB_PASSWORD");
        assert_eq!(entries[0].value.expose(), "SENTINEL-VALUE");
        let journaled = sink.entries.lock().unwrap_or_else(PoisonError::into_inner);
        assert_eq!(journaled.len(), 1);
        assert_eq!(journaled[0].0, AuditAction::ServiceEnvReveal);
        assert_eq!(journaled[0].2, Outcome::Succeeded);
        let written = format!("{:?} {:?}", journaled[0].1, journaled[0].1.text());
        assert!(written.contains("web"));
        assert!(!written.contains("SENTINEL") && !written.contains("DB_PASSWORD"));
    }

    #[tokio::test]
    async fn a_reveal_that_fails_reads_nothing_and_journals_no_success() {
        let (registry, engine, supervisor, _repo) = supervised_registry();
        let sink = RecordingSink::default();
        let unknown = ServiceKey::parse("c_0123abcd").expect("clé");
        assert_eq!(
            registry
                .reveal_env(&unknown, Actor::system(), &sink)
                .await
                .err(),
            Some(RevealError::NotFound)
        );
        // Un service supervisé dont le conteneur n'existe plus : rien à lire.
        let unit = unit_named("docker-x.service");
        let first = recreated("x", RunState::Running, 'a');
        supervisor.set_owner(&first.id, &unit);
        supervisor.set_status(&unit, running_unit());
        engine.put_silently(first.clone());
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        let key = only_service(&registry).key;
        // Le moteur refuse la lecture.
        engine.fail_inspect(&first.id, Some(EngineError::Timeout));
        assert_eq!(
            registry
                .reveal_env(&key, Actor::system(), &sink)
                .await
                .err(),
            Some(RevealError::EngineUnavailable)
        );
        engine.fail_inspect(&first.id, None);
        engine.remove(&first.id);
        registry
            .refresh(vec![first.id.clone()])
            .await
            .expect("refresh");
        supervisor.set_unit(&unit, UnitLookup::Unavailable);
        assert_eq!(
            registry
                .reveal_env(&key, Actor::system(), &sink)
                .await
                .err(),
            Some(RevealError::NoContainer)
        );
        assert!(
            sink.entries
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_empty(),
            "aucune réussite consignée"
        );
    }

    // -----------------------------------------------------------------------------------------
    // Publication (HRT-79, ADR-0043) : seuls les services créés par Hearth et les services adoptés
    // sortent ; les autres conteneurs sont comptés. Ces tests partent d'un registre SANS adoption.
    // -----------------------------------------------------------------------------------------

    /// Un registre sans aucune adoption, sur un dépôt en mémoire vide.
    fn unadopted() -> Supervised {
        let engine = Arc::new(FakeEngine::new());
        let supervisor = Arc::new(FakeSupervisor::new());
        let repo = Arc::new(MemoryRepo::default());
        repo.adopted
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        let registry = registry_over(&engine, &supervisor, &repo);
        (registry, engine, supervisor, repo)
    }

    fn registry_over(
        engine: &Arc<FakeEngine>,
        supervisor: &Arc<FakeSupervisor>,
        repo: &Arc<MemoryRepo>,
    ) -> Arc<ServicesRegistry> {
        ServicesRegistry::with_supervision(
            engine.clone(),
            supervisor.clone(),
            repo.clone(),
            &Linux,
            Arc::new(Wall(Mutex::new(crate::infrastructure::engine::fake::at(
                12,
            )))),
            Arc::new(Mono::default()),
            true,
            EngineTimings::production(),
        )
    }

    fn adopt_in_base(repo: &MemoryRepo, key: &str) {
        repo.adopted
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(AdoptedRecord {
                key: ServiceKey::parse(key).expect("clé"),
                by: "marie".to_owned(),
                at: crate::infrastructure::engine::fake::at(12),
            });
    }

    fn key_of_name(name: &str) -> String {
        format!("c_{}", container(name, RunState::Running).id)
    }

    #[tokio::test]
    async fn without_adoption_nothing_is_published_and_every_container_is_counted_stopped_ones_included()
     {
        let (registry, engine, _supervisor, _repo) = unadopted();
        engine.put_silently(container("web", RunState::Running));
        engine.put_silently(container("db", RunState::Exited));
        engine.put_silently(container("cache", RunState::Created));
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        let snapshot = registry.snapshot();
        assert!(snapshot.services.is_empty(), "{:?}", snapshot.services);
        assert_eq!(snapshot.engine.external_count, 3);
        // Le registre complet reste en mémoire, mais rien n'en sort : ni par la clé.
        assert_eq!(registry.lock().containers.len(), 3);
        let sink = RecordingSink::default();
        let hidden = ServiceKey::parse(&key_of_name("web")).expect("clé");
        assert_eq!(
            registry
                .reveal_env(&hidden, Actor::system(), &sink)
                .await
                .err(),
            Some(RevealError::NotFound),
            "un conteneur non adopté est un service inconnu"
        );
    }

    #[tokio::test]
    async fn a_change_to_a_hidden_container_moves_neither_the_revision_nor_the_deltas() {
        let (registry, engine, _supervisor, _repo) = unadopted();
        let web = container("web", RunState::Running);
        engine.put_silently(web.clone());
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        let (mut receiver, before) = registry.subscribe();
        // Il s'arrête, redémarre : rien ne part, `rev` ne bouge pas.
        engine.put_silently(container("web", RunState::Exited));
        registry
            .refresh(vec![web.id.clone()])
            .await
            .expect("refresh");
        engine.put_silently(web.clone());
        registry
            .refresh(vec![web.id.clone()])
            .await
            .expect("refresh");
        assert!(receiver.try_recv().is_err(), "aucun lot");
        assert_eq!(registry.snapshot().rev, before.rev);
        // Un conteneur de plus : un seul lot, qui ne porte que le nombre.
        let db = container("db", RunState::Running);
        engine.put_silently(db.clone());
        registry
            .refresh(vec![db.id.clone()])
            .await
            .expect("refresh");
        let change = receiver.try_recv().expect("le compte a changé");
        assert!(change.upserts.is_empty() && change.removed.is_empty());
        assert_eq!(change.engine.as_ref().map(|e| e.external_count), Some(2));
        assert_eq!(change.rev, before.rev + 1);
    }

    #[tokio::test]
    async fn an_adopted_container_is_published_by_its_key_and_the_others_stay_counted() {
        let (registry, engine, _supervisor, repo) = unadopted();
        engine.put_silently(container("web", RunState::Running));
        engine.put_silently(container("db", RunState::Running));
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        let (mut receiver, _) = registry.subscribe();
        adopt_in_base(&repo, &key_of_name("web"));
        registry.reload_adopted().await;
        let snapshot = registry.snapshot();
        assert_eq!(snapshot.services.len(), 1);
        let view = &snapshot.services[0];
        assert_eq!(view.observation.name, "web");
        assert_eq!(view.adoption.as_ref().map(|a| a.by.as_str()), Some("marie"));
        assert_eq!(snapshot.engine.external_count, 1);
        // Un lot : le service apparaît et le compte baisse, en un seul message.
        let change = receiver.try_recv().expect("un lot");
        assert_eq!(change.upserts.len(), 1);
        assert_eq!(change.engine.as_ref().map(|e| e.external_count), Some(1));
        // La révélation ne trouve que le service adopté.
        let sink = RecordingSink::default();
        let other = ServiceKey::parse(&key_of_name("db")).expect("clé");
        assert_eq!(
            registry
                .reveal_env(&other, Actor::system(), &sink)
                .await
                .err(),
            Some(RevealError::NotFound)
        );
    }

    #[tokio::test]
    async fn releasing_an_adoption_hides_the_service_again_and_forgets_what_was_kept() {
        let (registry, engine, supervisor, repo) = unadopted();
        let unit = unit_named("docker-terraria.service");
        let first = recreated("terraria", RunState::Running, 'a');
        supervisor.set_owner(&first.id, &unit);
        supervisor.set_status(&unit, running_unit());
        engine.put_silently(first);
        adopt_in_base(&repo, "u_docker-terraria.service");
        registry.probe_and_apply(false).await;
        registry.reload_adopted().await;
        registry.resync().await.expect("resync");
        assert_eq!(registry.snapshot().services.len(), 1);
        assert!(
            repo.saved
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .contains_key(&unit),
            "un service adopté est retenu"
        );
        // L'adoption est retirée de la base : le service sort et son souvenir est oublié.
        repo.adopted
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        registry.reload_adopted().await;
        let snapshot = registry.snapshot();
        assert!(snapshot.services.is_empty());
        assert_eq!(snapshot.engine.external_count, 1);
        assert!(registry.lock().remembered.is_empty());
        assert_eq!(
            *repo
                .forgotten
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
            vec![unit]
        );
    }

    #[tokio::test]
    async fn a_hidden_container_run_by_a_unit_is_never_remembered_in_memory_or_in_the_base() {
        let (registry, engine, supervisor, repo) = unadopted();
        let unit = unit_named("docker-terraria.service");
        let first = recreated("terraria", RunState::Running, 'a');
        supervisor.set_owner(&first.id, &unit);
        supervisor.set_status(&unit, running_unit());
        engine.put_silently(first.clone());
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        assert!(registry.snapshot().services.is_empty());
        assert_eq!(repo.saves.load(Ordering::SeqCst), 0);
        assert!(registry.lock().remembered.is_empty());
        // Son conteneur disparaît : rien ne reste listé « d'après son unité ».
        engine.remove(&first.id);
        registry.refresh(vec![first.id]).await.expect("refresh");
        registry.poll_units().await;
        assert!(registry.snapshot().services.is_empty());
        assert_eq!(registry.snapshot().engine.external_count, 0);
    }

    #[tokio::test]
    async fn an_adoption_by_unit_survives_the_recreation_of_the_container_and_a_restart_of_the_agent()
     {
        let (registry, engine, supervisor, repo) = unadopted();
        let unit = unit_named("docker-terraria.service");
        let first = recreated("terraria", RunState::Running, 'a');
        supervisor.set_owner(&first.id, &unit);
        supervisor.set_status(&unit, running_unit());
        engine.put_silently(first.clone());
        adopt_in_base(&repo, "u_docker-terraria.service");
        registry.probe_and_apply(false).await;
        registry.reload_adopted().await;
        registry.resync().await.expect("resync");
        assert_eq!(
            only_service(&registry).key.as_str(),
            "u_docker-terraria.service"
        );
        // Le système recrée le conteneur sous l'unité : même service, toujours adopté.
        engine.remove(&first.id);
        let second = recreated("terraria", RunState::Running, 'b');
        supervisor.set_owner(&second.id, &unit);
        engine.put_silently(second.clone());
        registry.resync().await.expect("resync");
        let view = only_service(&registry);
        assert_eq!(view.key.as_str(), "u_docker-terraria.service");
        assert!(view.adoption.is_some());
        assert_eq!(registry.snapshot().engine.external_count, 0);
        // Le redémarrage de l'agent : un nouveau registre, la même base, le conteneur absent.
        engine.remove(&second.id);
        let restarted = registry_over(&engine, &supervisor, &repo);
        restarted.reload_adopted().await;
        restarted.load_remembered().await;
        restarted.probe_and_apply(false).await;
        restarted.resync().await.expect("resync");
        let kept = only_service(&restarted);
        assert_eq!(kept.key.as_str(), "u_docker-terraria.service");
        assert!(!kept.supervision.expect("supervisé").container_present);
    }

    #[tokio::test]
    async fn an_adoption_without_a_unit_falls_with_its_container_and_a_namesake_does_not_inherit_it()
     {
        let (registry, engine, _supervisor, repo) = unadopted();
        let web = container("web", RunState::Running);
        engine.put_silently(web.clone());
        adopt_in_base(&repo, &format!("c_{}", web.id));
        registry.probe_and_apply(false).await;
        registry.reload_adopted().await;
        registry.resync().await.expect("resync");
        assert_eq!(only_service(&registry).observation.name, "web");
        // Un tiers supprime puis recrée « web » : autre identifiant, même nom.
        engine.remove(&web.id);
        let mut namesake = container("web", RunState::Running);
        namesake.id = "9".repeat(64);
        engine.put_silently(namesake);
        registry.resync().await.expect("resync");
        let snapshot = registry.snapshot();
        assert!(snapshot.services.is_empty(), "{:?}", snapshot.services);
        assert_eq!(snapshot.engine.external_count, 1, "compté comme non géré");
        assert!(
            repo.adopted
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_empty(),
            "l'ancienne adoption est oubliée en base"
        );
    }

    #[tokio::test]
    async fn a_vanished_engine_does_not_end_the_adoption_of_a_container_it_still_holds() {
        let (registry, engine, _supervisor, repo) = unadopted();
        let web = container("web", RunState::Running);
        engine.put_silently(web.clone());
        adopt_in_base(&repo, &format!("c_{}", web.id));
        registry.probe_and_apply(false).await;
        registry.reload_adopted().await;
        registry.resync().await.expect("resync");
        // Le moteur disparaît (liste vidée), puis revient avec le même conteneur.
        engine.set_outcome(ProbeOutcome::SocketMissing, false);
        registry.probe_and_apply(true).await;
        assert!(registry.snapshot().services.is_empty());
        engine.set_ready();
        registry.probe_and_apply(true).await;
        registry.resync().await.expect("resync");
        assert_eq!(only_service(&registry).observation.name, "web");
    }

    #[tokio::test]
    async fn a_port_or_a_name_held_by_a_container_that_is_not_adopted_is_told_without_naming_it() {
        use crate::domain::services::{PortBinding, PortScope};
        let (registry, engine, _supervisor, repo) = unadopted();
        let port = |host_port| PortBinding {
            host_port,
            container_port: 80,
            protocol: PortProtocol::Tcp,
            scope: PortScope::All,
        };
        let mut hidden = container("sentinel-hidden", RunState::Exited);
        hidden.ports = vec![port(8080)];
        let mut web = container("web", RunState::Running);
        web.ports = vec![port(9090)];
        engine.put_silently(hidden);
        engine.put_silently(web.clone());
        adopt_in_base(&repo, &format!("c_{}", web.id));
        registry.probe_and_apply(false).await;
        registry.reload_adopted().await;
        registry.resync().await.expect("resync");
        // Le conteneur non adopté tient le port, même arrêté : dit sans son nom ni son image.
        let held = registry.port_holder(8080, PortProtocol::Tcp);
        assert_eq!(held, Some(Holder::Unmanaged));
        assert!(!format!("{held:?}").contains("sentinel"));
        assert_eq!(
            registry.name_holder("SENTINEL-hidden"),
            Some(Holder::Unmanaged),
            "le nom est pris, sans la casse"
        );
        // Le service adopté, lui, est nommé : il est dans la liste.
        assert_eq!(
            registry.port_holder(9090, PortProtocol::Tcp),
            Some(Holder::Service("web".to_owned()))
        );
        assert_eq!(registry.port_holder(9090, PortProtocol::Udp), None);
        assert_eq!(registry.port_holder(7070, PortProtocol::Tcp), None);
        assert_eq!(registry.name_holder("libre"), None);
    }

    // -----------------------------------------------------------------------------------------
    // Adopter (HRT-80) : les candidats, la recherche d'une clé, la fin d'une adoption par unité
    // -----------------------------------------------------------------------------------------

    fn adopted_in_base(repo: &MemoryRepo) -> Vec<String> {
        let mut keys: Vec<String> = repo
            .adopted
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|record| record.key.as_str().to_owned())
            .collect();
        keys.sort();
        keys
    }

    /// Un registre sans aucune adoption ni unité connue, avec un superviseur.
    fn unadopted_supervised() -> Supervised {
        unadopted()
    }

    #[tokio::test]
    async fn the_candidates_are_the_hidden_containers_sorted_by_name_with_their_unit_and_state() {
        let (registry, engine, supervisor, _repo) = unadopted_supervised();
        let unit = unit_named("docker-terraria.service");
        let terraria = recreated("Terraria", RunState::Running, 'a');
        supervisor.set_owner(&terraria.id, &unit);
        supervisor.set_status(&unit, running_unit());
        engine.put_silently(terraria.clone());
        engine.put_silently(container("alpha", RunState::Exited));
        engine.put_silently(container("beta", RunState::Running));
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        let found = registry.candidates();
        let names: Vec<&str> = found.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["alpha", "beta", "Terraria"], "tri sans la casse");
        let by_name = |name: &str| found.iter().find(|c| c.name == name).expect("candidat");
        assert_eq!(
            by_name("Terraria").key.as_str(),
            "u_docker-terraria.service"
        );
        assert_eq!(
            by_name("Terraria").unit.as_ref().map(UnitName::as_str),
            Some("docker-terraria.service")
        );
        assert_eq!(by_name("Terraria").state, ServiceStateKind::Running);
        assert_eq!(by_name("alpha").unit, None);
        assert_eq!(by_name("alpha").state, ServiceStateKind::Stopped);
        assert_eq!(
            by_name("alpha").key.as_str(),
            format!("c_{}", container("alpha", RunState::Exited).id)
        );
        // Les candidats sont ce que le filtre cache : le compte des non gérés, lui, est le même.
        assert_eq!(
            registry.snapshot().engine.external_count as usize,
            found.len()
        );
    }

    #[tokio::test]
    async fn an_adopted_container_is_no_longer_a_candidate_and_a_key_is_found_as_what_it_is() {
        let (registry, engine, _supervisor, repo) = unadopted_supervised();
        let web = container("web", RunState::Running);
        let db = container("db", RunState::Running);
        engine.put_silently(web.clone());
        engine.put_silently(db.clone());
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        let web_key = ServiceKey::parse(&format!("c_{}", web.id)).expect("clé");
        let db_key = ServiceKey::parse(&format!("c_{}", db.id)).expect("clé");
        let invented = ServiceKey::parse("c_0000").expect("clé");
        assert!(matches!(
            registry.adoption_lookup(&web_key),
            AdoptionLookup::Candidate(_)
        ));
        assert_eq!(registry.adoption_lookup(&invented), AdoptionLookup::Unknown);
        adopt_in_base(&repo, web_key.as_str());
        registry.reload_adopted().await;
        assert_eq!(
            registry.adoption_lookup(&web_key),
            AdoptionLookup::Adopted {
                by: "marie".to_owned()
            }
        );
        let names: Vec<String> = registry.candidates().into_iter().map(|c| c.name).collect();
        assert_eq!(names, ["db"], "l'adopté n'est plus proposé");
        assert!(matches!(
            registry.adoption_lookup(&db_key),
            AdoptionLookup::Candidate(_)
        ));
    }

    #[tokio::test]
    async fn the_container_of_a_unit_is_adopted_by_its_unit_never_by_its_container_key() {
        let (registry, engine, supervisor, _repo) = unadopted_supervised();
        let unit = unit_named("docker-7dtd.service");
        let first = recreated("7dtd", RunState::Running, 'a');
        supervisor.set_owner(&first.id, &unit);
        supervisor.set_status(&unit, running_unit());
        engine.put_silently(first.clone());
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        let by_container = ServiceKey::parse(&format!("c_{}", first.id)).expect("clé");
        let by_unit = ServiceKey::for_unit(&unit);
        // La clé du conteneur d'une unité n'est pas une identité de ce service : elle ne désigne rien.
        assert_eq!(
            registry.adoption_lookup(&by_container),
            AdoptionLookup::Unknown
        );
        assert!(matches!(
            registry.adoption_lookup(&by_unit),
            AdoptionLookup::Candidate(_)
        ));
    }

    #[tokio::test]
    async fn nothing_is_a_candidate_before_the_list_is_read() {
        let (registry, engine, _supervisor, _repo) = unadopted_supervised();
        engine.put_silently(container("web", RunState::Running));
        assert!(registry.candidates().is_empty(), "liste pas encore lue");
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        assert_eq!(registry.candidates().len(), 1);
    }

    #[tokio::test]
    async fn an_adoption_by_unit_survives_a_stop_of_a_unit_the_system_still_knows() {
        let (registry, engine, supervisor, repo) = supervised_registry();
        let unit = unit_named("docker-x.service");
        let first = recreated("x", RunState::Running, 'a');
        supervisor.set_owner(&first.id, &unit);
        supervisor.set_status(&unit, running_unit());
        engine.put_silently(first.clone());
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        // Une unité écrite dans un fichier, arrêtée : `loaded/inactive`, son conteneur est supprimé.
        engine.remove(&first.id);
        registry
            .refresh(vec![first.id.clone()])
            .await
            .expect("refresh");
        supervisor.set_status(
            &unit,
            UnitStatus {
                activity: UnitActivity::Inactive,
                auto_restart: false,
                restarts: false,
                timeout_stop_s: None,
            },
        );
        registry.poll_units().await;
        assert!(
            adopted_in_base(&repo).contains(&"u_docker-x.service".to_owned()),
            "un simple arrêt ne fait pas tomber l'adoption"
        );
        let view = only_service(&registry);
        assert_eq!(view.key.as_str(), "u_docker-x.service");
        assert_eq!(view.derived.state, ServiceStateKind::Stopped);
    }

    #[tokio::test]
    async fn an_adoption_by_unit_falls_when_the_system_no_longer_knows_the_unit_and_not_on_a_silent_answer()
     {
        let (registry, engine, supervisor, repo) = supervised_registry();
        let unit = unit_named("docker-x.service");
        let first = recreated("x", RunState::Running, 'a');
        supervisor.set_owner(&first.id, &unit);
        supervisor.set_status(&unit, running_unit());
        engine.put_silently(first.clone());
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        engine.remove(&first.id);
        registry
            .refresh(vec![first.id.clone()])
            .await
            .expect("refresh");
        // Le système ne répond pas : on ne sait rien, rien ne tombe.
        supervisor.set_unit(&unit, UnitLookup::Unavailable);
        registry.poll_units().await;
        assert!(adopted_in_base(&repo).contains(&"u_docker-x.service".to_owned()));
        assert_eq!(registry.snapshot().services.len(), 1);
        // Unité supprimée, masquée, ou transitoire déchargée à l'arrêt : le système ne la connaît plus.
        supervisor.set_unit(&unit, UnitLookup::NotLoaded);
        for _ in 0..UNIT_GONE_READS - 1 {
            registry.poll_units().await;
            assert!(
                adopted_in_base(&repo).contains(&"u_docker-x.service".to_owned()),
                "une ou deux lectures « introuvable » ne suffisent pas"
            );
        }
        registry.poll_units().await;
        assert!(
            !adopted_in_base(&repo).contains(&"u_docker-x.service".to_owned()),
            "l'adoption d'une unité disparue est oubliée de la base"
        );
        assert!(registry.snapshot().services.is_empty());
        // Elle ne revient pas toute seule : la même unité recréée est un conteneur non géré.
        supervisor.set_status(&unit, running_unit());
        let again = recreated("x", RunState::Running, 'b');
        supervisor.set_owner(&again.id, &unit);
        engine.put_silently(again);
        registry.resync().await.expect("resync");
        assert!(registry.snapshot().services.is_empty());
        assert_eq!(registry.snapshot().engine.external_count, 1);
    }

    /// `BR-ADOPT-017` : chaque état que le système peut rendre pour une unité sans conteneur, lu
    /// bien plus de fois qu'il n'en faut : seul « introuvable » fait tomber, et seulement de suite.
    #[tokio::test]
    async fn only_a_clear_not_found_repeated_ends_an_adoption_by_unit() {
        let stopped = UnitLookup::Loaded(UnitStatus {
            activity: UnitActivity::Inactive,
            auto_restart: false,
            restarts: false,
            timeout_stop_s: None,
        });
        // Unavailable couvre : absence de réponse, erreur, masquée, fichier invalide (voir `parse_show`).
        for (lookup, falls) in [
            (stopped, false),
            (UnitLookup::Unavailable, false),
            (UnitLookup::NotLoaded, true),
        ] {
            let (registry, engine, supervisor, repo) = supervised_registry();
            let unit = unit_named("docker-x.service");
            let first = recreated("x", RunState::Running, 'a');
            supervisor.set_owner(&first.id, &unit);
            supervisor.set_status(&unit, running_unit());
            engine.put_silently(first.clone());
            registry.probe_and_apply(false).await;
            registry.resync().await.expect("resync");
            engine.remove(&first.id);
            registry
                .refresh(vec![first.id.clone()])
                .await
                .expect("refresh");
            supervisor.set_unit(&unit, lookup);
            for _ in 0..UNIT_GONE_READS * 3 {
                registry.poll_units().await;
            }
            assert_eq!(
                !adopted_in_base(&repo).contains(&"u_docker-x.service".to_owned()),
                falls,
                "{lookup:?}"
            );
        }
    }

    #[tokio::test]
    async fn a_read_that_says_anything_else_restarts_the_count_of_missing_reads() {
        let (registry, engine, supervisor, repo) = supervised_registry();
        let unit = unit_named("docker-x.service");
        let first = recreated("x", RunState::Running, 'a');
        supervisor.set_owner(&first.id, &unit);
        supervisor.set_status(&unit, running_unit());
        engine.put_silently(first.clone());
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        engine.remove(&first.id);
        registry
            .refresh(vec![first.id.clone()])
            .await
            .expect("refresh");
        for _ in 0..UNIT_GONE_READS - 1 {
            supervisor.set_unit(&unit, UnitLookup::NotLoaded);
            registry.poll_units().await;
        }
        // Une réponse muette entre deux : le compte repart de zéro.
        supervisor.set_unit(&unit, UnitLookup::Unavailable);
        registry.poll_units().await;
        for _ in 0..UNIT_GONE_READS - 1 {
            supervisor.set_unit(&unit, UnitLookup::NotLoaded);
            registry.poll_units().await;
        }
        assert!(adopted_in_base(&repo).contains(&"u_docker-x.service".to_owned()));
        supervisor.set_unit(&unit, UnitLookup::NotLoaded);
        registry.poll_units().await;
        assert!(!adopted_in_base(&repo).contains(&"u_docker-x.service".to_owned()));
    }

    /// `BR-ADOPT-027` : une unité partagée par deux conteneurs vivants n'est plus attribuée à aucun
    /// d'eux, mais son adoption est relue : elle tombe quand le système ne connaît plus l'unité.
    #[tokio::test]
    async fn the_adoption_of_a_unit_shared_by_two_live_containers_is_still_read_and_falls_with_the_unit()
     {
        let (registry, engine, supervisor, repo) = supervised_registry();
        let unit = unit_named("docker-x.service");
        let first = recreated("x1", RunState::Running, 'g');
        let second = recreated("x2", RunState::Running, 'h');
        for each in [&first, &second] {
            supervisor.set_owner(&each.id, &unit);
            engine.put_silently(each.clone());
        }
        supervisor.set_status(&unit, running_unit());
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        assert!(
            registry.snapshot().services.is_empty(),
            "partagée : aucune clé u_"
        );
        let before = supervisor.unit_queries();
        registry.poll_units().await;
        assert!(
            supervisor.unit_queries() > before,
            "l'unité adoptée est relue"
        );
        assert!(adopted_in_base(&repo).contains(&"u_docker-x.service".to_owned()));
        supervisor.set_unit(&unit, UnitLookup::NotLoaded);
        for _ in 0..UNIT_GONE_READS {
            registry.poll_units().await;
        }
        assert!(!adopted_in_base(&repo).contains(&"u_docker-x.service".to_owned()));
        assert!(!registry.is_unconfirmed(&ServiceKey::for_unit(&unit)));
    }

    fn many_units(count: usize) -> Vec<UnitName> {
        (0..count)
            .map(|index| unit_named(&format!("docker-z{index:03}.service")))
            .collect()
    }

    fn adopt_units(registry: &ServicesRegistry, repo: &MemoryRepo, units: &[UnitName]) {
        let at = crate::infrastructure::engine::fake::at(12);
        for unit in units {
            let key = ServiceKey::for_unit(unit);
            registry.lock().adopted.insert(
                key.clone(),
                Adoption {
                    by: "marie".to_owned(),
                    at,
                },
            );
            repo.adopted
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(AdoptedRecord {
                    key,
                    by: "marie".to_owned(),
                    at,
                });
        }
    }

    /// `BR-ADOPT-027` : au-delà de la borne de relecture des unités adoptées, l'adoption dont l'unité n'est pas relue est
    /// « à confirmer » : elle n'ouvre plus aucun droit, une unité du même nom qui reparaît ne l'hérite
    /// pas, et elle tombe au bout du délai si personne ne la confirme.
    #[tokio::test]
    async fn an_adoption_by_unit_that_is_no_longer_read_is_to_confirm_and_a_namesake_does_not_inherit_it()
     {
        let engine = Arc::new(FakeEngine::new());
        let supervisor = Arc::new(FakeSupervisor::new());
        let repo = Arc::new(MemoryRepo::default());
        let wall = Arc::new(Wall(Mutex::new(crate::infrastructure::engine::fake::at(
            12,
        ))));
        let registry = supervised_with_clock(&engine, &supervisor, &repo, wall.clone());
        let units = many_units(MAX_ADOPTED_UNITS_PER_POLL + 3);
        adopt_units(&registry, &repo, &units);
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        let late = units.last().expect("des unités").clone();
        let late_key = ServiceKey::for_unit(&late);
        assert!(!registry.is_unconfirmed(&late_key));
        registry.poll_units().await;
        assert!(
            registry.is_unconfirmed(&late_key),
            "plus relue : à confirmer"
        );
        assert!(
            !registry.is_unconfirmed(&ServiceKey::for_unit(&units[0])),
            "relue : adoption intacte"
        );
        // Une unité du même nom reparaît avec un conteneur : cachée, simple candidat.
        let namesake = recreated("namesake", RunState::Running, 'g');
        supervisor.set_owner(&namesake.id, &late);
        supervisor.set_status(&late, running_unit());
        engine.put_silently(namesake);
        registry.resync().await.expect("resync");
        assert!(
            registry
                .snapshot()
                .services
                .iter()
                .all(|view| view.key != late_key),
            "n'hérite d'aucun droit : pas publiée"
        );
        assert!(matches!(
            registry.adoption_lookup(&late_key),
            AdoptionLookup::Candidate(_)
        ));
        assert!(
            adopted_in_base(&repo).contains(&late_key.as_str().to_owned()),
            "la ligne reste jusqu'à la confirmation ou l'échéance"
        );
        // Une relecture de la base ne la rend pas : il faut un compte qui a le droit.
        registry.reload_adopted().await;
        assert!(registry.is_unconfirmed(&late_key));
        assert!(matches!(
            registry.adoption_lookup(&late_key),
            AdoptionLookup::Candidate(_)
        ));
        // Le délai passe sans confirmation : elle tombe, ligne comprise.
        wall.advance(CONFIRMATION_TTL - time::Duration::minutes(1));
        registry.poll_units().await;
        assert!(registry.is_unconfirmed(&late_key));
        wall.advance(time::Duration::minutes(2));
        registry.poll_units().await;
        assert!(!registry.is_unconfirmed(&late_key));
        assert!(!adopted_in_base(&repo).contains(&late_key.as_str().to_owned()));
    }

    /// `BR-ADOPT-027` : une fois confirmée, l'adoption reprend ses droits.
    #[tokio::test]
    async fn confirming_an_adoption_to_confirm_gives_it_back_its_rights() {
        let engine = Arc::new(FakeEngine::new());
        let supervisor = Arc::new(FakeSupervisor::new());
        let repo = Arc::new(MemoryRepo::default());
        let registry = supervised_over(&engine, &supervisor, &repo);
        let units = many_units(MAX_ADOPTED_UNITS_PER_POLL + 1);
        adopt_units(&registry, &repo, &units);
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        registry.poll_units().await;
        let late_key = ServiceKey::for_unit(units.last().expect("des unités"));
        assert!(registry.is_unconfirmed(&late_key));
        registry.confirm_adoption(&late_key);
        registry.reload_adopted().await;
        assert!(!registry.is_unconfirmed(&late_key));
        assert!(registry.lock().adopted.contains_key(&late_key));
    }

    /// `BR-ADOPT-027` : le compte des lectures « introuvable » ne survit pas à une unité qu'on ne
    /// relit plus, sinon une adoption ultérieure tombe sur une seule lecture.
    #[tokio::test]
    async fn the_count_of_missing_reads_is_erased_when_the_unit_leaves_the_rereading() {
        let (registry, engine, supervisor, repo) = supervised_registry();
        let unit = unit_named("docker-x.service");
        let first = recreated("x", RunState::Running, 'a');
        supervisor.set_owner(&first.id, &unit);
        supervisor.set_status(&unit, running_unit());
        engine.put_silently(first.clone());
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        engine.remove(&first.id);
        registry
            .refresh(vec![first.id.clone()])
            .await
            .expect("refresh");
        supervisor.set_unit(&unit, UnitLookup::NotLoaded);
        for _ in 0..UNIT_GONE_READS - 1 {
            registry.poll_units().await;
        }
        assert_eq!(
            registry.lock().gone_reads.get(&unit),
            Some(&(UNIT_GONE_READS - 1))
        );
        // D'autres adoptions passent devant (ordre alphabétique) : l'unité sort de la relecture.
        let others: Vec<UnitName> = (0..MAX_ADOPTED_UNITS_PER_POLL)
            .map(|index| unit_named(&format!("docker-a{index:03}.service")))
            .collect();
        adopt_units(&registry, &repo, &others);
        registry.poll_units().await;
        assert!(registry.lock().gone_reads.is_empty(), "compte effacé");
        assert!(registry.is_unconfirmed(&ServiceKey::for_unit(&unit)));
    }

    /// Une adoption tombée dont l'effacement échoue reste dans la file : elle est effacée à la première
    /// écriture qui réussit, au lieu de revenir à la prochaine relecture de la base.
    #[tokio::test]
    async fn a_fallen_adoption_whose_erasure_fails_is_erased_at_the_next_success() {
        let (registry, engine, supervisor, repo) = supervised_registry();
        let unit = unit_named("docker-x.service");
        let first = recreated("x", RunState::Running, 'a');
        supervisor.set_owner(&first.id, &unit);
        supervisor.set_status(&unit, running_unit());
        engine.put_silently(first.clone());
        registry.probe_and_apply(false).await;
        registry.resync().await.expect("resync");
        engine.remove(&first.id);
        registry
            .refresh(vec![first.id.clone()])
            .await
            .expect("refresh");
        supervisor.set_unit(&unit, UnitLookup::NotLoaded);
        repo.forget_fails
            .store(true, std::sync::atomic::Ordering::SeqCst);
        for _ in 0..UNIT_GONE_READS {
            registry.poll_units().await;
        }
        assert!(
            adopted_in_base(&repo).contains(&"u_docker-x.service".to_owned()),
            "la base n'a pas pu effacer"
        );
        repo.forget_fails
            .store(false, std::sync::atomic::Ordering::SeqCst);
        registry.poll_units().await;
        registry.persist_supervised().await;
        assert!(!adopted_in_base(&repo).contains(&"u_docker-x.service".to_owned()));
    }
}
