use async_trait::async_trait;
use time::OffsetDateTime;

use super::StoreError;
use crate::domain::services::ServiceKey;

/// Les conteneurs que l'agent a créés pour un service de Hearth (`service_containers`, HRT-68, ADR-0037).
///
/// C'est la SEULE preuve qu'un conteneur est « créé par Hearth » : son identifiant, rendu par le moteur à la
/// création et consigné ici, pour une clé `h_` connue. L'étiquette `io.hearth.service-id` d'un conteneur est une
/// affirmation que n'importe qui peut poser, pas une preuve.
#[async_trait]
pub trait ServiceContainers: Send + Sync {
    /// Consigne un conteneur créé pour ce service. Rejouer le même couple est sans effet.
    async fn record(
        &self,
        key: &ServiceKey,
        container_id: &str,
        at: OffsetDateTime,
    ) -> Result<(), StoreError>;

    /// Tous les conteneurs consignés : (identifiant du conteneur, clé de son service). Une ligne illisible est
    /// omise (tracée), pas fatale.
    async fn all(&self) -> Result<Vec<(String, ServiceKey)>, StoreError>;

    /// Les identifiants consignés pour un service.
    async fn of(&self, key: &ServiceKey) -> Result<Vec<String>, StoreError>;

    /// Oublie un conteneur (supprimé).
    async fn forget_container(&self, container_id: &str) -> Result<(), StoreError>;

    /// Oublie tous les conteneurs d'un service (service supprimé).
    async fn forget_service(&self, key: &ServiceKey) -> Result<(), StoreError>;
}
