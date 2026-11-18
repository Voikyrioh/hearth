//! Le registre d'origine des fichiers du dossier des mods (`service_mod_origins`, HRT-256, ADR-0062, BR-MOD-033).
//!
//! Jamais un fichier dans le volume du service (écrit par le jeu et ses mods : hostile) : la base dit ce que Hearth a
//! vu poser, le fichier garde ses octets. Une ligne n'est crue que si le nom, la taille ET une empreinte
//! correspondent (`domain::mods::game_info::resolve_origin`).

use async_trait::async_trait;
use time::OffsetDateTime;

use super::StoreError;
use crate::domain::mods::game_info::OriginRow;
use crate::domain::services::ServiceKey;

/// Au plus ce nombre de lignes par service ; au-delà, les plus anciennes lignes `hearth` sont oubliées.
pub const MAX_ORIGIN_ROWS: usize = 4_096;

#[async_trait]
pub trait ModOrigins: Send + Sync {
    /// Les lignes d'un service (au plus `MAX_ORIGIN_ROWS`).
    async fn rows(&self, key: &ServiceKey) -> Result<Vec<OriginRow>, StoreError>;

    /// Inscrit les `.jar` d'un pack serveur (nom sans `.disabled`, taille, CRC-32). Une ligne déjà là est gardée.
    async fn record_server_pack(
        &self,
        key: &ServiceKey,
        rows: &[OriginRow],
        at: OffsetDateTime,
    ) -> Result<(), StoreError>;
}
