//! Ports de la mise à jour du client : tout ce qui sort du processus (horloge, disque, flux de
//! versions, fenêtre) est derrière un trait, injecté par la racine de composition (`lib.rs`).

use async_trait::async_trait;

use super::domain::{Candidate, UpdateRecord};
use super::dto::UpdateStateDto;

/// Horloge murale en millisecondes depuis l'époque (injectée : les règles de fréquence et de
/// report se testent sans attendre).
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> i64;
}

/// Mémoire durable de la mise à jour (fichier du dossier de données de l'application).
pub trait UpdateStore: Send + Sync {
    /// Absent ou illisible : l'état par défaut, jamais une erreur (le client doit démarrer).
    fn load(&self) -> UpdateRecord;
    fn save(&self, record: &UpdateRecord) -> Result<(), String>;
}

/// Le flux de versions et l'installateur. Rien ici n'est joignable par l'interface : seule la
/// coquille appelle ce port, et seulement pour une vérification prévue ou un clic.
#[async_trait]
pub trait Feed: Send + Sync {
    /// Interroge le flux. `Ok(None)` : le flux ne propose rien de plus récent.
    async fn check(&self) -> Result<Option<Candidate>, FeedError>;

    /// Télécharge l'installateur de `version` (celui de la dernière annonce rendue par `check`) et
    /// vérifie sa signature contre la clé embarquée. Rien n'est écrit sur le disque avant que la
    /// signature soit bonne. `progress(reçus, total)`.
    async fn download(
        &self,
        version: &str,
        progress: &mut (dyn FnMut(u64, Option<u64>) + Send),
    ) -> Result<VerifiedInstaller, DownloadError>;

    /// Lance l'installateur vérifié ; sous Windows, le client se ferme et l'installateur le
    /// relance. Ne rend la main qu'en cas d'échec.
    fn install(&self, version: &str, installer: VerifiedInstaller) -> Result<(), DownloadError>;
}

/// Pourquoi une vérification n'a rien donné. Jamais montré à l'interface (BR-UPDATE-007, 008) :
/// seul le journal en garde la raison.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct FeedError {
    /// Aucune requête n'a pu partir (pas de réseau : connexion ou résolution du nom impossible).
    /// Une telle tentative ne consomme pas le quota de 24 h (BR-UPDATE-001) ; toute autre
    /// (réponse en erreur, service muet après connexion, réponse invalide) le consomme.
    pub no_request_sent: bool,
    pub message: String,
}

impl FeedError {
    pub fn offline(message: impl Into<String>) -> Self {
        Self {
            no_request_sent: true,
            message: message.into(),
        }
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            no_request_sent: false,
            message: message.into(),
        }
    }
}

/// Les octets d'un installateur dont la signature a été vérifiée. Seul un `Feed` qui vient de
/// vérifier une signature en fabrique un (`VerifiedInstaller::new`, invisible hors de la crate) : on
/// ne peut pas passer d'autres octets à `Feed::install`.
pub struct VerifiedInstaller(Vec<u8>);

impl VerifiedInstaller {
    pub(crate) fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.0
    }

    /// Pour les faux ports des tests uniquement : n'existe pas dans un binaire de publication.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn unverified_for_tests(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Les octets, pour les assertions des tests uniquement.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn into_bytes_for_tests(self) -> Vec<u8> {
        self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Pourquoi un téléchargement ou une installation a échoué.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DownloadError {
    /// Coupure ou réponse incomplète : relançable (BR-UPDATE-009).
    #[error("téléchargement interrompu : {0}")]
    Interrupted(String),
    /// Signature fausse, d'une autre clé, fichier altéré ou qui n'est pas un installateur
    /// (BR-UPDATE-010) : refusé.
    #[error("mise à jour corrompue : {0}")]
    Corrupted(String),
    /// Autre échec (disque, installateur impossible à lancer).
    #[error("mise à jour impossible : {0}")]
    Failed(String),
    /// Le port n'a plus l'annonce de cette version en main (relance de l'application, annonce
    /// remplacée) : le service refait une vérification, sur clic, puis réessaie.
    #[error("annonce absente")]
    NotStaged,
}

/// Où l'état est publié pour la fenêtre (événement Tauri `update://state`).
pub trait StateSink: Send + Sync {
    fn publish(&self, state: &UpdateStateDto);
}
