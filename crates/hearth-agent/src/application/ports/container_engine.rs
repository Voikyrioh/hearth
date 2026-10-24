use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::Stream;
use thiserror::Error;

use crate::domain::catalog::stop_plan::FicheLiteral;
use crate::domain::secret::Secret;
use crate::domain::services::logs::LogLine;
use crate::domain::services::plan::ContainerPlan;
use crate::domain::services::{EngineProbe, Hidden, Observation};

/// Erreur d'un échange avec le moteur. Le texte du moteur n'en fait JAMAIS partie : il est traduit
/// dans l'adaptateur (`infrastructure/engine/errors.rs`) et un texte inconnu devient `Refused`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum EngineError {
    /// Le conteneur demandé n'existe pas (ou plus).
    #[error("le moteur ne connaît pas ce conteneur")]
    NotFound,
    /// Le moteur est injoignable (socket absent, connexion refusée ou coupée).
    #[error("le moteur est injoignable")]
    Unreachable,
    /// Le moteur n'a pas répondu à temps.
    #[error("le moteur n'a pas répondu à temps")]
    Timeout,
    /// Le moteur a répondu par un refus ou une réponse inexploitable.
    #[error("le moteur a refusé la demande")]
    Refused,
    /// Le moteur (ou un proxy de socket) refuse l'accès à l'agent (401, 403).
    #[error("le moteur refuse l'accès")]
    Denied,
    /// Le moteur ne sait pas faire cela pour ce conteneur : ses journaux ne se relisent pas (pilote
    /// de journaux `none`, 501), BR-SLOG-017.
    #[error("le moteur ne permet pas de relire les journaux de ce conteneur")]
    Unsupported,
    /// Un port que le conteneur publie est déjà pris sur le serveur : il ne démarre pas (HRT-64,
    /// BR-LIFE-006). Reconnu dans le texte du moteur par l'adaptateur, qui ne le fait jamais sortir.
    #[error("un port demandé par le conteneur est déjà pris")]
    PortInUse,
    /// Récupérer une image (HRT-68, BR-DEPLOY-017) : l'image n'existe pas.
    #[error("image introuvable")]
    ImageNotFound,
    /// L'image existe, pas cette version.
    #[error("version introuvable")]
    TagNotFound,
    /// Le registre répond la même chose pour une image privée et pour une image inconnue.
    #[error("image privée ou introuvable")]
    ImagePrivateOrMissing,
    /// Le registre est injoignable depuis le serveur.
    #[error("registre injoignable")]
    RegistryUnreachable,
    /// La limite de téléchargements du registre est atteinte.
    #[error("limite du registre atteinte")]
    RegistryRateLimited,
    /// L'image n'existe pas pour l'architecture du serveur.
    #[error("architecture incompatible")]
    PlatformMismatch,
    /// Le disque du moteur est plein.
    #[error("disque plein")]
    DiskFull,
    /// Un conteneur porte déjà ce nom (créer).
    #[error("nom déjà pris")]
    NameTaken,
}

impl EngineError {
    /// Le moteur n'est plus joignable ou ferme sa porte : inutile de continuer à lire d'autres
    /// conteneurs. Les autres erreurs (délai, réponse illisible) ne disent rien des conteneurs voisins.
    pub fn engine_gone(self) -> bool {
        matches!(self, Self::Unreachable | Self::Denied)
    }
}

/// Tous les conteneurs lus. Un conteneur dont l'inspection échoue (délai, réponse illisible) ne fait
/// pas échouer la liste : il est rendu à part, que le registre garde en état dégradé.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Listing {
    pub observations: Vec<Observation>,
    pub unreadable: Vec<Unreadable>,
}

/// Un conteneur vu dans la liste mais dont l'inspection a échoué.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unreadable {
    pub id: String,
    /// Nom donné par la liste (sans le « / » initial), vide si elle n'en donne pas.
    pub name: String,
}

/// Un événement du moteur qui touche un conteneur (création, démarrage, arrêt, suppression,
/// renommage, pause…). Seul l'identifiant sert : le registre relit le conteneur touché.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineEvent {
    pub container_id: String,
    /// Le verbe du moteur (`start`, `die`, `destroy`…), pour la trace seulement.
    pub action: String,
}

/// Une variable d'environnement d'un conteneur, lue pour la révéler à un compte qui a le droit de gérer les services
/// (`GET /services/{key}/env`, BR-SVC-009). Le nom est [`Hidden`], la valeur un [`Secret`] : ni l'un ni
/// l'autre ne s'affiche (`Debug` masqué), et la valeur est effacée de la mémoire à la libération.
#[derive(Debug)]
pub struct EnvEntry {
    pub name: Hidden,
    pub value: Secret,
}

/// Le flux des événements. Sa FIN (ou une erreur) veut dire que le moteur est à revérifier : sonde
/// immédiate puis relecture complète.
pub type EngineEvents = Pin<Box<dyn Stream<Item = Result<EngineEvent, EngineError>> + Send>>;

/// La progression d'une récupération d'image, agrégée sur toutes les couches (HRT-68).
pub use crate::domain::services::pull::PullProgress;

/// La récupération d'une image : une progression à chaque événement du moteur ; la FIN du flux veut dire que
/// l'image est là ; une erreur la termine. L'ABANDONNER ferme la connexion au moteur : le téléchargement
/// s'arrête et rien n'est créé (annulation, BR-DEPLOY-016).
pub type PullStream = Pin<Box<dyn Stream<Item = Result<PullProgress, EngineError>> + Send>>;

/// Ce que le moteur dit de la machine (BR-DEPLOY-038, BR-DEPLOY-018). Lu sur la machine, pas dans un conteneur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineInfo {
    /// Processeurs logiques.
    pub cpus: u32,
    /// Mémoire totale, en octets.
    pub memory_bytes: u64,
    /// Le dossier où le moteur range ses images (`DockerRootDir`), pour lire l'espace libre.
    pub storage_dir: Option<String>,
}

/// Ce que la lecture des journaux d'un conteneur demande au moteur (HRT-66).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogQuery {
    /// Garder la connexion ouverte et envoyer les lignes à mesure qu'elles s'écrivent. Le flux finit
    /// quand le conteneur s'arrête.
    pub follow: bool,
    /// Les N dernières lignes ; `None` : toutes celles que le moteur a gardées (à partir de `since_ms`).
    pub tail: Option<u32>,
    /// Les lignes écrites à partir de la seconde qui contient cette heure (millisecondes depuis
    /// l'époque) : le moteur ne filtre qu'à la seconde, l'appelant écarte le surplus (curseur de reprise).
    pub since_ms: Option<i64>,
    /// Le conteneur a un terminal : le moteur envoie un flux brut, pas des trames.
    pub tty: bool,
}

/// Le flux des lignes d'un conteneur, déjà assemblées, nettoyées et bornées (jamais plus de
/// `MAX_LINE_CHARS` caractères par ligne). Une erreur termine le flux ; sa FIN veut dire que le
/// conteneur s'est arrêté (ou que le moteur a coupé). L'abandonner ferme la connexion au moteur.
pub type LogStream = Pin<Box<dyn Stream<Item = Result<LogLine, EngineError>> + Send>>;

/// Le moteur de conteneurs vu par l'agent (HRT-61, ADR-0036).
///
/// Cinq méthodes de lecture (HRT-63 ajoute `env`, HRT-66 ajoute `logs`), puis les trois premières qui
/// ÉCRIVENT (HRT-64) : `start`, `stop`, `restart`. Rien d'autre n'écrit encore : ni créer, ni supprimer, ni
/// télécharger. Une méthode d'écriture « pour plus tard » n'a pas sa place ici : chaque tranche ajoute les
/// siennes avec son faux et son cas dans la suite de contrat (`tests/support/engine_contract.rs`).
///
/// Aucun type de `bollard` ne sort de l'adaptateur. Une [`Observation`] ne porte ni valeur ni nom de
/// variable d'environnement, ni commande.
#[async_trait]
pub trait ContainerEngine: Send + Sync {
    /// Sonde le moteur : joignable ? droits ? réponse sous 5 s ? nom, version, version d'API.
    /// Ne rend jamais d'erreur : tout échec est un résultat de sonde (`ProbeOutcome`).
    async fn probe(&self) -> EngineProbe;

    /// Tous les conteneurs, arrêtés compris, avec leur description complète. Un conteneur disparu
    /// entre la liste et son inspection est omis ; un conteneur illisible est rendu dans
    /// `unreadable`. Une erreur n'est rendue que si le moteur lui-même est perdu (la liste échoue,
    /// ou il n'est plus joignable) ou si la relecture entière dépasse son délai.
    async fn list(&self) -> Result<Listing, EngineError>;

    /// Un conteneur ; `None` s'il n'existe pas (ou plus : un conteneur supprimé entre la liste et
    /// l'inspection n'est pas une erreur).
    async fn inspect(&self, id: &str) -> Result<Option<Observation>, EngineError>;

    /// Les événements qui touchent les conteneurs, à partir de maintenant.
    fn events(&self) -> EngineEvents;

    /// Les variables d'environnement d'un conteneur, avec leurs VALEURS (HRT-63). Appelée à la seule
    /// demande d'un compte qui a le droit, jamais en arrière-plan ; au plus `MAX_ENV_NAMES` variables, comme
    /// la liste des noms. `None` si le conteneur n'existe plus.
    async fn env(&self, id: &str) -> Result<Option<Vec<EnvEntry>>, EngineError>;

    /// Les journaux d'un conteneur (HRT-66, BR-SLOG-*). Lecture seule, comme le reste. Le texte d'une
    /// ligne est du contenu sensible : ni l'implémentation ni l'appelant ne le tracent. Les erreurs
    /// arrivent dans le flux : `NotFound` (conteneur disparu), `Unsupported` (journaux non relisibles),
    /// `Unreachable` / `Timeout` / `Denied` / `Refused`.
    fn logs(&self, id: &str, query: LogQuery) -> LogStream;

    /// Démarre un conteneur (HRT-64, BR-LIFE-004). Déjà en marche : réussit sans rien changer (le moteur
    /// répond « non modifié »). `NotFound` s'il n'existe plus ; `PortInUse` si un port qu'il publie est pris
    /// (le conteneur reste arrêté). Rend la main quand le moteur a lancé le processus : « en marche » ne veut
    /// pas dire « prêt » (BR-LIFE-005).
    async fn start(&self, id: &str) -> Result<(), EngineError>;

    /// Arrête un conteneur : l'ordre d'arrêt propre, puis, si le conteneur est encore là au bout de `grace`,
    /// `SIGKILL` (BR-LIFE-009). Déjà arrêté : réussit sans rien changer. Ne rend la main qu'une fois le
    /// conteneur arrêté ; l'appelant borne l'attente (`BR-LIFE-021`), l'adaptateur aussi (`Timeout`).
    async fn stop(&self, id: &str, grace: Duration) -> Result<(), EngineError>;

    /// Arrête (avec le même délai de grâce) puis démarre, d'un seul ordre du moteur. Un démarrage qui
    /// échoue après l'arrêt laisse le conteneur arrêté et rend l'erreur du démarrage (`PortInUse`…).
    async fn restart(&self, id: &str, grace: Duration) -> Result<(), EngineError>;

    /// Écrit UNE ligne sur l'entrée standard d'un conteneur en marche (HRT-103, ADR-0052 point 4, BR-JEU-002,
    /// BR-JEU-016) : attache l'entrée, écrit la ligne et sa fin de ligne, se détache. **Le texte est un
    /// [`FicheLiteral`]**, type qui ne se construit que dans `domain/catalog`. L'adaptateur INSPECTE d'abord le
    /// conteneur et refuse (`Refused`) s'il ne tourne pas ou si son entrée n'est pas ouverte : le démon accepterait
    /// l'attache et l'ordre partirait dans le vide. Aucune route ne porte cette fonction et aucun texte du client n'y
    /// entre ; seule la suite d'arrêt ou d'enregistrement d'un service de fiche l'appelle (`application::game_stop`).
    /// Elle n'exécute aucune commande dans le conteneur. `NotFound` si le conteneur n'existe plus ; `Refused` si
    /// le moteur refuse l'attache (conteneur arrêté, entrée non ouverte à la création) ; jamais de texte du moteur.
    /// Le texte de la ligne n'est ni tracé ni gardé.
    async fn send_stdin_line(&self, id: &str, line: &FicheLiteral) -> Result<(), EngineError>;

    /// L'image `dépôt:version` est-elle déjà sur le serveur (HRT-68, BR-DEPLOY-018) ? Si oui, elle est
    /// utilisée telle quelle, sans interroger le registre.
    async fn image_present(&self, image: &str) -> Result<bool, EngineError>;

    /// Récupère l'image (HRT-68). Voir [`PullStream`]. Les échecs sont typés (`ImageNotFound`,
    /// `TagNotFound`, `ImagePrivateOrMissing`, `RegistryUnreachable`, `RegistryRateLimited`,
    /// `PlatformMismatch`, `DiskFull`) et ne portent jamais le texte du moteur. Aucun identifiant de registre
    /// n'est transmis : les registres privés sont hors de ce lot (BR-DEPLOY-017).
    fn pull(&self, image: &str) -> PullStream;

    /// Crée le conteneur d'un service depuis le plan fermé, SANS le démarrer (HRT-68). Rend son identifiant.
    /// `NameTaken` si un conteneur porte déjà ce nom ; `NotFound` si l'image a disparu entre-temps.
    async fn create(&self, plan: &ContainerPlan) -> Result<String, EngineError>;

    /// Supprime un conteneur arrêté ou jamais démarré, volumes anonymes compris, sans toucher aux dossiers
    /// montés. Absent : réussit (le but est atteint). Sert à ne laisser aucun conteneur orphelin après un
    /// échec au démarrage (BR-DEPLOY-019).
    async fn remove(&self, id: &str) -> Result<(), EngineError>;

    /// Les bornes de la machine et le dossier des images (HRT-68).
    async fn info(&self) -> Result<EngineInfo, EngineError>;
}
