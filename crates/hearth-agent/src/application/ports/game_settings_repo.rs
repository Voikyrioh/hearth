//! Le dépôt des réglages voulus d'un serveur de jeu (`game_settings`, `game_setting_secrets`, HRT-108, ADR-0051).
//!
//! Il garde ce qu'on lui confie, déjà scellé : il ne connaît pas la clé et ne voit jamais un secret en clair. Une
//! ligne par service ; l'écriture est numérotée (`expected_rev`) pour que deux enregistrements concurrents ne
//! s'écrasent pas.

use async_trait::async_trait;
use thiserror::Error;
use time::OffsetDateTime;

use super::StoreError;
use crate::domain::catalog::settings_state::SettingsState;
use crate::domain::sealed::SealedBlob;
use crate::domain::services::key::ServiceKey;

/// Les réglages voulus d'un service, relus.
#[derive(Debug)]
pub struct StoredSettings {
    pub rev: u32,
    pub state: SettingsState,
    /// SHA-256 hexadécimal du fichier tel que Hearth l'a écrit en dernier.
    pub file_fingerprint: Option<String>,
    /// Les secrets changés depuis l'installation, scellés : (identifiant du réglage, scellé).
    pub sealed: Vec<(String, SealedBlob)>,
    pub updated_by: String,
    pub updated_at: OffsetDateTime,
}

/// Une écriture. Les scellés listés remplacent ceux du même réglage ; les autres sont gardés.
#[derive(Debug)]
pub struct NewSettings {
    pub key: ServiceKey,
    /// La révision courante que l'appelant a lue (`None` : aucune ligne encore).
    pub expected_rev: Option<u32>,
    pub state: SettingsState,
    pub file_fingerprint: Option<String>,
    pub sealed: Vec<(String, SealedBlob)>,
    pub by: String,
    pub at: OffsetDateTime,
}

#[derive(Debug, Error)]
pub enum SettingsStoreError {
    /// La ligne n'est plus à la révision que l'appelant avait lue.
    #[error("les réglages ont changé depuis leur lecture")]
    Conflict {
        current: Option<u32>,
        by: Option<String>,
        at: Option<OffsetDateTime>,
    },
    #[error(transparent)]
    Store(#[from] StoreError),
}

#[async_trait]
pub trait GameSettingsRepo: Send + Sync {
    /// Les réglages voulus, ou `None` (service sans ligne : ses réponses d'installation font foi).
    async fn load(&self, key: &ServiceKey) -> Result<Option<StoredSettings>, StoreError>;

    /// Écrit la ligne dans UNE transaction, avec ses scellés. Rend la nouvelle révision.
    async fn save(&self, new: NewSettings) -> Result<u32, SettingsStoreError>;

    /// Efface les réglages d'un service supprimé, scellés compris.
    async fn forget(&self, key: &ServiceKey) -> Result<(), StoreError>;
}
