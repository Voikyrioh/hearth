//! Ce que la coquille échange avec l'interface pour supprimer un service (HRT-86, `BR-DEL-*`).
//!
//! - **Entrée** (`DeleteRequestDto`) : les deux cases et le nom retapé. Rien de secret : le nom retapé est celui
//!   du service que l'utilisateur a sous les yeux, il n'est pas signé (l'agent le compare à celui qu'il connaît).
//! - **Sorties** : copies fidèles de `hearth_proto::api::service_delete`, chaque énumération porte `Unknown`
//!   (un sort plus récent que ce client se lit « inconnu », jamais comme une erreur). Aucun texte de l'agent
//!   n'est repris : l'interface choisit ses phrases d'après les codes. Les chemins des dossiers sont ceux du
//!   serveur : l'agent ne les envoie qu'aux comptes qui ont le droit de gérer les services, l'interface ne les
//!   met jamais dans un journal ni dans une trace. Les nombres sont bornés pour `specta` (`f64`).

use hearth_proto::api::service_delete::{DeletePreview, DeleteRequest, FolderFate, FolderPreview};
use serde::{Deserialize, Serialize};
use specta::Type;

use super::dto::OperationKindDto;
use crate::services::dto::{EngineStateDto, ServiceStateDto};

/// La demande de suppression : les deux cases (décochées par défaut) et le nom retapé.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeleteRequestDto {
    pub erase_data: bool,
    pub erase_backups: bool,
    pub confirm_name: Option<String>,
}

impl From<DeleteRequestDto> for DeleteRequest {
    fn from(request: DeleteRequestDto) -> Self {
        Self {
            erase_data: request.erase_data,
            erase_backups: request.erase_backups,
            confirm_name: request.confirm_name,
        }
    }
}

/// Ce que la suppression fera d'un dossier géré par Hearth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FolderFateDto {
    Erasable,
    KeptShared,
    KeptUnverifiable,
    KeptOutsideRoot,
    Unknown,
}

impl From<FolderFate> for FolderFateDto {
    fn from(fate: FolderFate) -> Self {
        match fate {
            FolderFate::Erasable => Self::Erasable,
            FolderFate::KeptShared => Self::KeptShared,
            FolderFate::KeptUnverifiable => Self::KeptUnverifiable,
            FolderFate::KeptOutsideRoot => Self::KeptOutsideRoot,
            FolderFate::Unknown => Self::Unknown,
        }
    }
}

/// Un dossier géré par Hearth, tel que la fenêtre le montre.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FolderPreviewDto {
    pub path: String,
    pub bytes: f64,
    pub items: f64,
    /// La mesure a été coupée : « plus de {taille} ».
    pub partial: bool,
    /// Le dossier n'a pas pu être mesuré du tout.
    pub unmeasured: bool,
    pub fate: FolderFateDto,
    pub shared_with: Vec<String>,
    pub shared_with_unmanaged: bool,
}

impl From<&FolderPreview> for FolderPreviewDto {
    fn from(folder: &FolderPreview) -> Self {
        Self {
            path: folder.path.clone(),
            bytes: folder.bytes as f64,
            items: folder.items as f64,
            partial: folder.partial,
            unmeasured: folder.unmeasured,
            fate: folder.fate.into(),
            shared_with: folder.shared_with.clone(),
            shared_with_unmanaged: folder.shared_with_unmanaged,
        }
    }
}

/// De quoi bâtir la fenêtre « Supprimer {nom} ? ».
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeletePreviewDto {
    pub name: String,
    pub image: String,
    pub running: bool,
    pub stop_grace_s: u32,
    pub container_missing: bool,
    pub folders: Vec<FolderPreviewDto>,
    pub server_folders: Vec<String>,
    pub volumes: u32,
    pub measured_in_time: bool,
    pub backups: u32,
}

impl From<&DeletePreview> for DeletePreviewDto {
    fn from(preview: &DeletePreview) -> Self {
        Self {
            name: preview.name.clone(),
            image: preview.image.clone(),
            running: preview.running,
            stop_grace_s: preview.stop_grace_s,
            container_missing: preview.container_missing,
            folders: preview.folders.iter().map(Into::into).collect(),
            server_folders: preview.server_folders.clone(),
            volumes: preview.volumes,
            measured_in_time: preview.measured_in_time,
            backups: preview.backups,
        }
    }
}

/// Pourquoi l'agent n'a pas donné la prévisualisation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeletePreviewRefusal {
    /// Le service n'existe plus (BR-DEL-018).
    ServiceNotFound,
    /// Un service adopté ou créé hors de Hearth : Hearth ne le supprime jamais (BR-DEL-002).
    ServiceAdopted,
    /// Le moteur n'est pas prêt (BR-DEL-022) ; son état, si l'agent l'a dit.
    EngineUnavailable {
        engine: Option<EngineStateDto>,
    },
    Other,
}

/// La réponse de la consultation : prête, ou refusée par son code.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeletePreviewOutcome {
    Ready { preview: Box<DeletePreviewDto> },
    Refused { refusal: DeletePreviewRefusal },
}

/// Pourquoi l'agent a refusé la suppression. Sans texte : l'interface choisit le message d'après `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeleteRefusal {
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
    /// Un service adopté : jamais supprimé (BR-DEL-002).
    ServiceAdopted,
    /// Une action tient déjà le service (BR-DEL-021) ; qui, et laquelle.
    ServiceBusy {
        by: Option<String>,
        action: Option<OperationKindDto>,
    },
    /// L'état du service ne permet pas la suppression (BR-DEL-003).
    NotDrivable {
        state: Option<ServiceStateDto>,
    },
    /// Le nom retapé ne correspond pas (BR-DEL-011).
    NameMismatch,
    /// La case des sauvegardes est refusée : l'agent n'en a pas encore.
    BackupsUnavailable,
    Other,
}

/// L'issue de l'envoi : accepté (`202`, l'opération court à l'arrière-plan), refusé, ou résultat inconnu (le
/// lien est tombé avant la réponse : jamais rejoué, BR-DEL-023).
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeleteOutcome {
    Accepted {
        #[serde(rename = "operationId")]
        operation_id: String,
    },
    Refused {
        refusal: DeleteRefusal,
    },
    Unknown {
        #[serde(rename = "opId")]
        op_id: String,
    },
}
