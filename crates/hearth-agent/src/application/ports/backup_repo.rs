//! Le registre des sauvegardes (`backups`, `backup_settings`, HRT-97, ADR-0047, BR-BACKUP-004).
//!
//! **Le disque est la vérité de l'existence, la base est la vérité de la propriété** : une archive sans ligne
//! n'est pas à Hearth ; une ligne sans archive est « disparue ». Ce dépôt ne connaît ni la clé ni une note en
//! clair : il range la note scellée que le cas d'usage lui donne.

use async_trait::async_trait;
use hearth_proto::api::backups::{BackupFlag, BackupIntegrity, BackupMode, BackupSettings};
use time::OffsetDateTime;

use super::StoreError;
use crate::domain::backups::BackupId;
use crate::domain::mods::backup_list::{RecordedSet, SetState};
use crate::domain::sealed::SealedBlob;
use crate::domain::services::key::ServiceKey;

/// Une ligne du registre, relue.
#[derive(Debug, Clone)]
pub struct BackupRow {
    pub id: BackupId,
    pub service_key: ServiceKey,
    pub service_name: String,
    /// `true` : l'archive est publiée (`ready`) ; `false` : en cours d'écriture.
    pub ready: bool,
    pub created_at: OffsetDateTime,
    pub created_by: String,
    pub dev: u64,
    pub ino: u64,
    /// La date de modification de l'archive publiée (nanosecondes) : avec l'inode et la taille (`archive_bytes`),
    /// ce qui prouve que le fichier est encore celui que Hearth a écrit. `0` tant que la ligne n'est pas `ready`.
    pub archive_mtime_ns: i64,
    pub archive_bytes: u64,
    pub data_bytes: u64,
    pub files: u64,
    pub mode: BackupMode,
    pub flags: Vec<BackupFlag>,
    pub changed_files: u32,
    /// Éléments que la copie n'a pas archivés.
    pub skipped: u32,
    pub image: String,
    pub integrity: BackupIntegrity,
    pub verified_at: Option<OffsetDateTime>,
    pub manifest_sha256: String,
    pub archive_sha256: String,
    pub note: Option<SealedBlob>,
}

/// Une archive qui commence : inscrite `writing`, avec l'inode de son fichier partiel.
#[derive(Debug, Clone)]
pub struct NewBackup {
    pub id: BackupId,
    pub service_key: ServiceKey,
    pub service_name: String,
    pub created_at: OffsetDateTime,
    pub created_by: String,
    pub dev: u64,
    pub ino: u64,
    pub mode: BackupMode,
    pub image: String,
    pub note: Option<SealedBlob>,
}

/// Ce qu'une archive publiée sait d'elle-même (le manifeste vient d'être écrit à côté).
#[derive(Debug, Clone)]
pub struct Published {
    pub archive_bytes: u64,
    /// La date de modification de l'archive publiée, relevée sur le fichier.
    pub archive_mtime_ns: i64,
    pub data_bytes: u64,
    pub files: u64,
    pub flags: Vec<BackupFlag>,
    pub changed_files: u32,
    pub skipped: u32,
    pub manifest_sha256: String,
    pub archive_sha256: String,
    /// La liste des mods de cette sauvegarde (HRT-293, BR-MOD-034), écrite dans la MÊME transaction que le passage à
    /// `ready` : une sauvegarde prête a sa liste, une sauvegarde interrompue n'en a pas. `None` : service sans mods.
    pub mods: Option<ModsRecord>,
}

/// La liste des mods d'une sauvegarde, avec sa date d'enregistrement.
#[derive(Debug, Clone)]
pub struct ModsRecord {
    pub set: RecordedSet,
    pub at: OffsetDateTime,
}

/// Le résumé d'une liste, sans ses entrées (la liste des sauvegardes le joint).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModsSummaryRow {
    pub state: SetState,
    pub active: u32,
    pub disabled: u32,
    pub retired: u32,
}

/// Ce que l'on sait des fichiers d'une ligne AVANT de les écrire : de quoi les retrouver (et eux seuls) si
/// l'agent s'arrête entre l'écriture et la publication.
#[derive(Debug, Clone)]
pub struct Publishing {
    pub archive_bytes: u64,
    pub manifest_sha256: String,
    pub archive_sha256: String,
}

#[async_trait]
pub trait BackupRepo: Send + Sync {
    async fn insert_writing(&self, new: &NewBackup) -> Result<(), StoreError>;

    /// Avant d'écrire le manifeste et de publier l'archive : la ligne `writing` apprend la taille de l'archive et les
    /// empreintes. Un agent qui s'arrête ensuite sait retirer SES fichiers (l'archive par inode et taille, le
    /// manifeste par empreinte). `false` : la ligne n'existe plus ou n'est pas `writing`.
    async fn note_publishing(
        &self,
        id: &BackupId,
        publishing: &Publishing,
    ) -> Result<bool, StoreError>;

    /// `writing` devient `ready` (intégrité non vérifiée). `false` : la ligne n'existe plus ou n'est pas `writing`.
    async fn mark_ready(&self, id: &BackupId, published: &Published) -> Result<bool, StoreError>;

    /// Écrit le résultat d'une vérification. `verified_at` : la date, seulement pour `Verified`.
    async fn set_integrity(
        &self,
        id: &BackupId,
        integrity: BackupIntegrity,
        verified_at: Option<OffsetDateTime>,
    ) -> Result<(), StoreError>;

    async fn get(&self, id: &BackupId) -> Result<Option<BackupRow>, StoreError>;

    /// Les sauvegardes publiées d'un service, les plus récentes d'abord. Jamais une ligne `writing`.
    async fn list(&self, key: &ServiceKey) -> Result<Vec<BackupRow>, StoreError>;

    /// Retire une ligne, et sa liste de mods dans la même transaction. `true` : elle existait.
    async fn delete(&self, id: &BackupId) -> Result<bool, StoreError>;

    /// La liste des mods d'une sauvegarde, avec ses entrées. `None` : aucune liste (sauvegarde d'avant la 0.2.2).
    async fn mods_of(&self, id: &BackupId) -> Result<Option<ModsRecord>, StoreError>;

    /// Les résumés des listes de mods des sauvegardes d'un service, par identifiant de sauvegarde (sans les entrées).
    async fn mods_summaries(
        &self,
        key: &ServiceKey,
    ) -> Result<std::collections::HashMap<String, ModsSummaryRow>, StoreError>;

    /// Les lignes `writing` : des archives que l'agent n'a pas fini d'écrire (démarrage après un arrêt).
    async fn writing(&self) -> Result<Vec<BackupRow>, StoreError>;

    /// Les éléments ignorés d'un service (vides si le service n'a pas de ligne).
    async fn settings(&self, key: &ServiceKey) -> Result<BackupSettings, StoreError>;

    async fn save_settings(
        &self,
        key: &ServiceKey,
        settings: &BackupSettings,
        by: &str,
        at: OffsetDateTime,
    ) -> Result<(), StoreError>;
}
