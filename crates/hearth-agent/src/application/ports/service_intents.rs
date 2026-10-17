use async_trait::async_trait;
use time::OffsetDateTime;

use super::StoreError;
use crate::domain::services::ServiceKey;

/// L'arrêt voulu depuis Hearth (`service_intents`, HRT-64, BR-LIFE-010, BR-SVC-005) : écrit AVANT l'ordre
/// d'arrêt, pour distinguer « Arrêté » de « En erreur » même après un redémarrage de l'agent. Une sortie non
/// nulle d'un service dont l'arrêt a été demandé n'est pas une erreur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopIntent {
    pub key: ServiceKey,
    /// Le conteneur arrêté ; vide pour un service supervisé (son conteneur est recréé par le système à chaque
    /// démarrage).
    pub container_id: String,
    /// Identifiant du compte, figé : le compte peut disparaître ensuite.
    pub account: String,
    pub at: OffsetDateTime,
}

/// Les arrêts voulus que l'agent retient d'un démarrage à l'autre.
#[async_trait]
pub trait ServiceIntents: Send + Sync {
    /// Tous les arrêts voulus. Une ligne illisible est omise (tracée), pas fatale : elle ferait seulement
    /// afficher « En erreur » au lieu d'« Arrêté ».
    async fn load(&self) -> Result<Vec<StopIntent>, StoreError>;

    /// Retient l'arrêt voulu d'un service (remplace le précédent). Rend la main une fois la ligne écrite :
    /// l'ordre d'arrêt ne part qu'après (BR-LIFE-010).
    async fn save(&self, intent: &StopIntent) -> Result<(), StoreError>;

    /// Oublie l'arrêt voulu d'un service : Hearth l'a démarré, ou il est revu en marche, ou il n'existe plus.
    async fn clear(&self, key: &ServiceKey) -> Result<(), StoreError>;
}
