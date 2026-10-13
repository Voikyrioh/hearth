use std::pin::Pin;

use async_trait::async_trait;
use futures_util::Stream;
use thiserror::Error;

use crate::domain::secret::Secret;
use crate::domain::services::logs::LogLine;
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

/// Le moteur de conteneurs vu par l'agent, EN LECTURE SEULE (HRT-61, ADR-0036).
///
/// Cinq méthodes (HRT-63 ajoute `env`, HRT-66 ajoute `logs`), aucune n'écrit : ni démarrer, ni arrêter, ni créer, ni supprimer, ni télécharger.
/// Une méthode d'écriture « pour plus tard » n'a pas sa place ici : chaque tranche ajoute les siennes
/// avec son faux et son cas dans la suite de contrat (`tests/support/engine_contract.rs`).
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
}
