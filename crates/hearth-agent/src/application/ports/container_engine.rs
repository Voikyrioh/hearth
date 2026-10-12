use std::pin::Pin;

use async_trait::async_trait;
use futures_util::Stream;
use thiserror::Error;

use crate::domain::services::{EngineProbe, Observation};

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
}

/// Un événement du moteur qui touche un conteneur (création, démarrage, arrêt, suppression,
/// renommage, pause…). Seul l'identifiant sert : le registre relit le conteneur touché.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineEvent {
    pub container_id: String,
    /// Le verbe du moteur (`start`, `die`, `destroy`…), pour la trace seulement.
    pub action: String,
}

/// Le flux des événements. Sa FIN (ou une erreur) veut dire que le moteur est à revérifier : sonde
/// immédiate puis relecture complète.
pub type EngineEvents = Pin<Box<dyn Stream<Item = Result<EngineEvent, EngineError>> + Send>>;

/// Le moteur de conteneurs vu par l'agent, EN LECTURE SEULE (HRT-61, ADR-0036).
///
/// Quatre méthodes, aucune n'écrit : ni démarrer, ni arrêter, ni créer, ni supprimer, ni télécharger.
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

    /// Tous les conteneurs, arrêtés compris, avec leur description complète.
    async fn list(&self) -> Result<Vec<Observation>, EngineError>;

    /// Un conteneur ; `None` s'il n'existe pas (ou plus : un conteneur supprimé entre la liste et
    /// l'inspection n'est pas une erreur).
    async fn inspect(&self, id: &str) -> Result<Option<Observation>, EngineError>;

    /// Les événements qui touchent les conteneurs, à partir de maintenant.
    fn events(&self) -> EngineEvents;
}
