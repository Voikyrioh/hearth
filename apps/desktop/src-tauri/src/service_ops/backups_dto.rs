//! Ce que la coquille échange avec l'interface pour les sauvegardes d'un service (HRT-98, HRT-97, `BR-BACKUP-*`).
//!
//! - **Entrées** : la demande de création (mode, note, éléments ignorés pour cette fois) et les éléments ignorés du
//!   service. La note n'est jamais signée ni journalisée ; un chemin est celui du SERVICE (cible de montage), jamais
//!   celui de la machine.
//! - **Sorties** : copies fidèles de `hearth_proto::api::backups`, chaque énumération porte `Unknown` (une valeur
//!   plus récente que ce client se lit « inconnue », jamais comme une erreur). Aucun texte de l'agent n'est repris :
//!   l'interface choisit ses phrases d'après les codes. La note d'un compte est du texte de l'agent : l'interface la
//!   montre comme du texte, jamais comme du HTML. Les nombres sont bornés pour `specta` (`f64`).

use hearth_proto::api::backups::{
    BackupFlag, BackupIntegrity, BackupMode, BackupPreview, BackupRequest, BackupSettings,
    BackupView, BackupsView, BigFile, IgnoredItem, IgnoredView, IgnoredWhy, VerifyResult,
};
use serde::{Deserialize, Serialize};
use specta::Type;

use super::dto::OperationKindDto;
use crate::services::dto::{EngineStateDto, ServiceStateDto};

/// Un dossier du service et un chemin dans ce dossier (`/` pour séparateur ; vide : le dossier entier).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IgnoredItemDto {
    pub root: String,
    pub path: String,
}

impl From<IgnoredItemDto> for IgnoredItem {
    fn from(item: IgnoredItemDto) -> Self {
        Self {
            root: item.root,
            path: item.path,
        }
    }
}

impl From<&IgnoredItem> for IgnoredItemDto {
    fn from(item: &IgnoredItem) -> Self {
        Self {
            root: item.root.clone(),
            path: item.path.clone(),
        }
    }
}

/// Comment la copie est faite (BR-BACKUP-006).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum BackupModeDto {
    Stopped,
    Hot,
    StopCopyStart,
    Unknown,
}

impl From<BackupMode> for BackupModeDto {
    fn from(mode: BackupMode) -> Self {
        match mode {
            BackupMode::Stopped => Self::Stopped,
            BackupMode::Hot => Self::Hot,
            BackupMode::StopCopyStart => Self::StopCopyStart,
            BackupMode::Unknown => Self::Unknown,
        }
    }
}

impl BackupModeDto {
    /// Le mode envoyé : un mode que ce client ne connaît pas n'est jamais demandé.
    fn wire(self) -> Option<BackupMode> {
        match self {
            Self::Stopped => Some(BackupMode::Stopped),
            Self::Hot => Some(BackupMode::Hot),
            Self::StopCopyStart => Some(BackupMode::StopCopyStart),
            Self::Unknown => None,
        }
    }
}

/// Un repère d'une sauvegarde (BR-BACKUP-012).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum BackupFlagDto {
    Hot,
    MaybeInconsistent,
    ForcedStop,
    BeforeRestore,
    Unknown,
}

impl From<BackupFlag> for BackupFlagDto {
    fn from(flag: BackupFlag) -> Self {
        match flag {
            BackupFlag::Hot => Self::Hot,
            BackupFlag::MaybeInconsistent => Self::MaybeInconsistent,
            BackupFlag::ForcedStop => Self::ForcedStop,
            BackupFlag::BeforeRestore => Self::BeforeRestore,
            BackupFlag::Unknown => Self::Unknown,
        }
    }
}

/// L'état d'intégrité d'une sauvegarde (BR-BACKUP-013).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum BackupIntegrityDto {
    Verified,
    Corrupt,
    Unchecked,
    Missing,
    Unknown,
}

impl From<BackupIntegrity> for BackupIntegrityDto {
    fn from(integrity: BackupIntegrity) -> Self {
        match integrity {
            BackupIntegrity::Verified => Self::Verified,
            BackupIntegrity::Corrupt => Self::Corrupt,
            BackupIntegrity::Unchecked => Self::Unchecked,
            BackupIntegrity::Missing => Self::Missing,
            BackupIntegrity::Unknown => Self::Unknown,
        }
    }
}

/// Pourquoi un élément est ignoré (BR-BACKUP-003).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum IgnoredWhyDto {
    LooksLikeBackups,
    Chosen,
    ThisTime,
    Unknown,
}

impl From<IgnoredWhy> for IgnoredWhyDto {
    fn from(why: IgnoredWhy) -> Self {
        match why {
            IgnoredWhy::LooksLikeBackups => Self::LooksLikeBackups,
            IgnoredWhy::Chosen => Self::Chosen,
            IgnoredWhy::ThisTime => Self::ThisTime,
            IgnoredWhy::Unknown => Self::Unknown,
        }
    }
}

/// Un élément ignoré et sa raison.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IgnoredViewDto {
    pub root: String,
    pub path: String,
    pub why: IgnoredWhyDto,
}

impl From<&IgnoredView> for IgnoredViewDto {
    fn from(view: &IgnoredView) -> Self {
        Self {
            root: view.item.root.clone(),
            path: view.item.path.clone(),
            why: view.why.into(),
        }
    }
}

/// Les éléments ignorés d'un service : ce que le compte ajoute, et ce qu'il retire des ignorés par défaut.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupSettingsDto {
    pub ignored: Vec<IgnoredItemDto>,
    pub included: Vec<IgnoredItemDto>,
}

impl From<BackupSettingsDto> for BackupSettings {
    fn from(settings: BackupSettingsDto) -> Self {
        Self {
            ignored: settings.ignored.into_iter().map(Into::into).collect(),
            included: settings.included.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<&BackupSettings> for BackupSettingsDto {
    fn from(settings: &BackupSettings) -> Self {
        Self {
            ignored: settings.ignored.iter().map(Into::into).collect(),
            included: settings.included.iter().map(Into::into).collect(),
        }
    }
}

/// Une sauvegarde de la liste.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupViewDto {
    pub id: String,
    pub created_at: String,
    pub archive_bytes: f64,
    pub data_bytes: f64,
    pub files: f64,
    pub mode: BackupModeDto,
    pub flags: Vec<BackupFlagDto>,
    pub changed_files: u32,
    pub skipped: u32,
    pub note: Option<String>,
    pub account: String,
    pub image: String,
    pub integrity: BackupIntegrityDto,
    pub verified_at: Option<String>,
}

impl From<&BackupView> for BackupViewDto {
    fn from(view: &BackupView) -> Self {
        Self {
            id: view.id.clone(),
            created_at: view.created_at.clone(),
            archive_bytes: view.archive_bytes as f64,
            data_bytes: view.data_bytes as f64,
            files: view.files as f64,
            mode: view.mode.into(),
            flags: view.flags.iter().copied().map(Into::into).collect(),
            changed_files: view.changed_files,
            skipped: view.skipped,
            note: view.note.clone(),
            account: view.account.clone(),
            image: view.image.clone(),
            integrity: view.integrity.into(),
            verified_at: view.verified_at.clone(),
        }
    }
}

/// La liste des sauvegardes d'un service, l'espace pris et les éléments ignorés.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupsViewDto {
    pub backups: Vec<BackupViewDto>,
    pub total_bytes: f64,
    pub free_bytes: Option<f64>,
    pub same_disk: bool,
    pub settings: BackupSettingsDto,
    pub nothing_to_save: bool,
}

impl From<&BackupsView> for BackupsViewDto {
    fn from(view: &BackupsView) -> Self {
        Self {
            backups: view.backups.iter().map(Into::into).collect(),
            total_bytes: view.total_bytes as f64,
            free_bytes: view.free_bytes.map(|bytes| bytes as f64),
            same_disk: view.same_disk,
            settings: (&view.settings).into(),
            nothing_to_save: view.nothing_to_save,
        }
    }
}

/// Un gros fichier signalé à la fenêtre de création (BR-BACKUP-033).
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BigFileDto {
    pub root: String,
    pub path: String,
    pub bytes: f64,
}

impl From<&BigFile> for BigFileDto {
    fn from(file: &BigFile) -> Self {
        Self {
            root: file.item.root.clone(),
            path: file.item.path.clone(),
            bytes: file.bytes as f64,
        }
    }
}

/// De quoi bâtir la fenêtre de création.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupPreviewDto {
    pub name: String,
    pub running: bool,
    pub stop_grace_s: u32,
    pub roots: Vec<String>,
    pub not_saved: Vec<String>,
    pub bytes: f64,
    pub files: f64,
    pub partial: bool,
    pub big_files: Vec<BigFileDto>,
    pub ignored: Vec<IgnoredViewDto>,
    pub ignored_more: u32,
    pub free_bytes: Option<f64>,
    pub reserve_bytes: f64,
    pub enough_space: bool,
    pub same_disk: bool,
    pub nothing_to_save: bool,
}

impl From<&BackupPreview> for BackupPreviewDto {
    fn from(preview: &BackupPreview) -> Self {
        Self {
            name: preview.name.clone(),
            running: preview.running,
            stop_grace_s: preview.stop_grace_s,
            roots: preview.roots.clone(),
            not_saved: preview.not_saved.clone(),
            bytes: preview.bytes as f64,
            files: preview.files as f64,
            partial: preview.partial,
            big_files: preview.big_files.iter().map(Into::into).collect(),
            ignored: preview.ignored.iter().map(Into::into).collect(),
            ignored_more: preview.ignored_more,
            free_bytes: preview.free_bytes.map(|bytes| bytes as f64),
            reserve_bytes: preview.reserve_bytes as f64,
            enough_space: preview.enough_space,
            same_disk: preview.same_disk,
            nothing_to_save: preview.nothing_to_save,
        }
    }
}

/// La demande de création : le mode, la note facultative et les éléments ignorés pour cette sauvegarde seulement.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupRequestDto {
    pub mode: BackupModeDto,
    pub note: Option<String>,
    pub ignore_once: Vec<IgnoredItemDto>,
}

impl BackupRequestDto {
    /// La requête de l'agent ; `None` si le mode n'est pas un mode que ce client sait demander.
    pub fn wire(self) -> Option<BackupRequest> {
        Some(BackupRequest {
            mode: self.mode.wire()?,
            note: self.note.filter(|note| !note.is_empty()),
            ignore_once: self.ignore_once.into_iter().map(Into::into).collect(),
        })
    }
}

/// Le résultat d'une vérification.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VerifyResultDto {
    pub integrity: BackupIntegrityDto,
    pub verified_at: Option<String>,
    pub files_checked: f64,
}

impl From<&VerifyResult> for VerifyResultDto {
    fn from(result: &VerifyResult) -> Self {
        Self {
            integrity: result.integrity.into(),
            verified_at: result.verified_at.clone(),
            files_checked: result.files_checked as f64,
        }
    }
}

/// Pourquoi l'agent n'a pas donné une lecture (liste, estimation, vérification). Sans texte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BackupReadRefusal {
    ServiceNotFound,
    /// Un service adopté : ses sauvegardes ne sont pas prises en charge (BR-BACKUP-028).
    ServiceAdopted,
    EngineUnavailable {
        engine: Option<EngineStateDto>,
    },
    /// La racine des sauvegardes chevauche un dossier de données ou n'est pas utilisable (BR-BACKUP-005).
    PathRefused {
        reason: Option<String>,
    },
    BackupNotFound,
    /// Une opération utilise la sauvegarde (BR-BACKUP-027).
    BackupInUse,
    Other,
}

/// La liste des sauvegardes : prête, ou refusée par son code.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BackupsOutcome {
    Ready { view: Box<BackupsViewDto> },
    Refused { refusal: BackupReadRefusal },
}

/// L'estimation de la fenêtre de création : prête, ou refusée par son code.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BackupPreviewOutcome {
    Ready { preview: Box<BackupPreviewDto> },
    Refused { refusal: BackupReadRefusal },
}

/// Le résultat d'une vérification : rendu, ou refusé par son code.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BackupVerifyOutcome {
    Done { result: VerifyResultDto },
    Refused { refusal: BackupReadRefusal },
}

/// Pourquoi l'agent a refusé un acte de sauvegarde. Sans texte : l'interface choisit le message d'après `kind`.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BackupRefusal {
    WrongPassword,
    PasswordRequired,
    TooManyAttempts {
        retry_after_s: u32,
    },
    Busy,
    EngineUnavailable {
        engine: Option<EngineStateDto>,
    },
    ServiceNotFound,
    ServiceAdopted,
    /// Une action tient déjà le service ; qui, et laquelle.
    ServiceBusy {
        by: Option<String>,
        action: Option<OperationKindDto>,
    },
    /// Une autre sauvegarde ou restauration tient la place du serveur (BR-BACKUP-010).
    HeavyIoBusy,
    NotDrivable {
        state: Option<ServiceStateDto>,
    },
    /// Pas assez de place sur le disque des sauvegardes, aucune case pour passer outre (BR-BACKUP-009).
    NoSpace {
        needed_bytes: Option<f64>,
        free_bytes: Option<f64>,
        reserve_bytes: Option<f64>,
    },
    /// Le service n'a aucun dossier de données à sauvegarder (BR-BACKUP-002).
    NothingToSave,
    /// La note dépasse 80 caractères ou la liste d'éléments est trop longue.
    InvalidInput {
        field: Option<String>,
    },
    PathRefused {
        reason: Option<String>,
    },
    BackupNotFound,
    BackupInUse,
    /// Le mode demandé n'est pas permis dans l'état du service.
    ModeRefused,
    Other,
}

/// L'issue de la création : acceptée (`202`, l'opération court à l'arrière-plan), refusée, ou résultat inconnu (le
/// lien est tombé avant la réponse : jamais rejoué).
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BackupCreateOutcome {
    Accepted {
        #[serde(rename = "operationId")]
        operation_id: String,
        #[serde(rename = "backupId")]
        backup_id: String,
    },
    Refused {
        refusal: BackupRefusal,
    },
    Unknown {
        #[serde(rename = "opId")]
        op_id: String,
    },
}

/// L'issue de la suppression d'une sauvegarde.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BackupDeleteOutcome {
    Done {
        #[serde(rename = "removedFiles")]
        removed_files: bool,
    },
    Refused {
        refusal: BackupRefusal,
    },
    Unknown {
        #[serde(rename = "opId")]
        op_id: String,
    },
}

/// L'issue de l'enregistrement des éléments ignorés.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BackupSettingsOutcome {
    Done {
        settings: BackupSettingsDto,
    },
    Refused {
        refusal: BackupRefusal,
    },
    Unknown {
        #[serde(rename = "opId")]
        op_id: String,
    },
}
