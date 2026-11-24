//! Ce que la coquille échange avec l'interface pour le Catalogue et l'installation d'un serveur de jeu (HRT-102,
//! ADR-0050, `BR-FICHE-*`).
//!
//! - **Sorties** : copies fidèles de `hearth_proto::api::catalog`, rien de plus. Une fiche ne porte aucun secret ;
//!   le client n'en reçoit aucun. Une énumération porte `Unknown` (une valeur plus récente que ce client se lit
//!   « inconnue »). Les nombres d'octets sont des `f64` (`specta`), bornés par ce que l'agent peut écrire.
//! - **Entrée** (`FicheInstallRequestDto`) : la valeur d'une réponse peut être un secret (mot de passe du serveur,
//!   jeton). `Debug` n'en montre que le nombre ; elle traverse la coquille sans être gardée ni tracée.

use hearth_proto::api::catalog::{
    AgreementView, CatalogView, CharsetView, CpuNeed, DiskNeed, DownloadView, EvidenceView,
    FicheCard, FicheDetail, FicheStatus, FirstStartView, FolderView, ImageView, ImposedView,
    ItemCharsetView, LevelView, ListView, MemoryNeed, ModsView, PortMappingView, PortView,
    QuestionKind, QuestionView, ResourcesView, SecretKindView, SettingView, StopView, ThemeView,
    VariantView, WorldSavedView,
};
use hearth_proto::api::deploy::{
    DeployAccept, DeployPack, DeployResources, DeploySource, DeploySpec, FicheAnswer, FichePort,
};
use serde::{Deserialize, Serialize};
use specta::Type;

use super::deploy_dto::{DeployAcceptDto, DeployResourcesDto};
use crate::link_dto::{InvalidField, LinkFailure};

/// Le plus grand entier qu'un nombre de l'interface représente sans perte.
const MAX_SAFE: f64 = 9_007_199_254_740_991.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FicheStatusDto {
    Published,
    Preview,
    Unknown,
}

impl From<FicheStatus> for FicheStatusDto {
    fn from(status: FicheStatus) -> Self {
        match status {
            FicheStatus::Published => Self::Published,
            FicheStatus::Preview => Self::Preview,
            FicheStatus::Unknown => Self::Unknown,
        }
    }
}

/// Une fiche dans la liste.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FicheCardDto {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub version: u32,
    pub status: FicheStatusDto,
    pub installable: bool,
    /// Ce qui retient une fiche en cours de preuve, dit au joueur (BR-FICHE-061) ; `null` pour une fiche publiée.
    pub preview_reason: Option<String>,
    /// La mémoire recommandée et sa preuve (BR-FICHE-064) ; `null` quand la fiche ne recommande aucun chiffre.
    pub memory: Option<FicheMemoryHintDto>,
    /// L'identifiant du JEU dans le magasin Steam (BR-FICHE-062) ; `null` pour un jeu hors Steam.
    pub steam_app_id: Option<u32>,
    /// L'illustration embarquée de l'agent, pour un jeu absent de Steam (BR-FICHE-062) ; `null` sinon.
    pub illustration: Option<FicheIllustrationDto>,
}

/// Ce qu'il faut pour demander l'illustration d'une fiche et réserver sa place dans la page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FicheIllustrationDto {
    pub digest: String,
    pub bytes: u32,
    pub width: u16,
    pub height: u16,
    pub media_type: String,
}

/// La mémoire conseillée d'une carte : le chiffre et sa preuve.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FicheMemoryHintDto {
    pub recommended_bytes: f64,
    pub evidence: FicheEvidenceDto,
}

impl From<&FicheCard> for FicheCardDto {
    fn from(card: &FicheCard) -> Self {
        let FicheCard {
            id,
            name,
            summary,
            version,
            status,
            installable,
            preview_reason,
            memory,
            steam_app_id,
            illustration,
        } = card;
        Self {
            id: id.clone(),
            name: name.clone(),
            summary: summary.clone(),
            version: *version,
            status: (*status).into(),
            installable: *installable,
            preview_reason: preview_reason.clone(),
            steam_app_id: *steam_app_id,
            illustration: illustration.as_ref().map(|view| FicheIllustrationDto {
                digest: view.digest.clone(),
                bytes: view.bytes,
                width: view.width,
                height: view.height,
                media_type: view.media_type.clone(),
            }),
            memory: memory.as_ref().map(|hint| FicheMemoryHintDto {
                recommended_bytes: hint.recommended_bytes as f64,
                evidence: (&hint.evidence).into(),
            }),
        }
    }
}

/// Une fiche que l'agent a écartée : son dossier et l'invariant violé, jamais un contenu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RefusedFicheDto {
    pub dir: String,
    pub invariant: String,
}

/// La réponse de `GET /catalog`.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CatalogDto {
    pub fiches: Vec<FicheCardDto>,
    pub refused: Vec<RefusedFicheDto>,
    /// Les fiches d'une version plus récente que ce client, illisibles ici : comptées, jamais montrées.
    pub unreadable: u32,
}

impl From<&CatalogView> for CatalogDto {
    fn from(view: &CatalogView) -> Self {
        Self {
            fiches: view.fiches.iter().map(FicheCardDto::from).collect(),
            refused: view
                .refused
                .iter()
                .map(|refused| RefusedFicheDto {
                    dir: refused.dir.clone(),
                    invariant: refused.invariant.clone(),
                })
                .collect(),
            unreadable: view
                .fiches
                .unreadable()
                .saturating_add(view.refused.unreadable()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FicheImageDto {
    pub repository: String,
    pub tag: String,
    pub digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FicheVariantDto {
    pub id: String,
    pub label: String,
    pub help: String,
    pub default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FichePortDto {
    pub id: String,
    pub container: u16,
    /// `tcp` ou `udp`.
    pub protocol: String,
    /// `game`, `query`, `voice`, `web_admin`, `remote_console` ou `other`.
    pub role: String,
    /// `always`, `optional` ou `never`.
    pub publish: String,
    pub default_host: Option<u16>,
    pub mapping: FichePortMappingDto,
    pub only_variants: Vec<String>,
}

/// Comment le port du serveur se relie au port du service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FichePortMappingDto {
    Nat,
    SameNumber,
    Unknown,
}

impl From<PortMappingView> for FichePortMappingDto {
    fn from(mapping: PortMappingView) -> Self {
        match mapping {
            PortMappingView::Nat => Self::Nat,
            PortMappingView::SameNumber => Self::SameNumber,
            PortMappingView::Unknown => Self::Unknown,
        }
    }
}

/// Le jeu de caractères d'un texte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FicheCharsetDto {
    Line,
    FileStem,
    ArgSafe,
    Unknown,
}

impl From<CharsetView> for FicheCharsetDto {
    fn from(charset: CharsetView) -> Self {
        match charset {
            CharsetView::Line => Self::Line,
            CharsetView::FileStem => Self::FileStem,
            CharsetView::ArgSafe => Self::ArgSafe,
            CharsetView::Unknown => Self::Unknown,
        }
    }
}

/// Le genre d'un secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FicheSecretKindDto {
    GamePassword,
    AdminPassword,
    ServerToken,
    Unknown,
}

impl From<SecretKindView> for FicheSecretKindDto {
    fn from(kind: SecretKindView) -> Self {
        match kind {
            SecretKindView::GamePassword => Self::GamePassword,
            SecretKindView::AdminPassword => Self::AdminPassword,
            SecretKindView::ServerToken => Self::ServerToken,
            SecretKindView::Unknown => Self::Unknown,
        }
    }
}

/// Quelle adresse de la fiche s'ouvre (`open_fiche_link`) : jamais l'adresse elle-même.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FicheLinkKind {
    /// Le contrat de licence, par l'identifiant du contrat.
    Agreement,
    /// Le site où créer un jeton de serveur, par l'identifiant de la question.
    Token,
    /// La source d'un chiffre de l'éditeur : `cpu`, `memory` ou `disk`.
    Evidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FicheFolderDto {
    pub id: String,
    pub role: String,
    pub target: String,
    pub backup: bool,
    pub only_variants: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FicheAgreementDto {
    pub id: String,
    pub title: String,
    pub url: String,
    pub sentence: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FicheQuestionKindDto {
    Text,
    Secret,
    Int,
    Bool,
    Choice,
    IdList,
    Opaque,
    Unknown,
}

impl From<QuestionKind> for FicheQuestionKindDto {
    fn from(kind: QuestionKind) -> Self {
        match kind {
            QuestionKind::Text => Self::Text,
            QuestionKind::Secret => Self::Secret,
            QuestionKind::Int => Self::Int,
            QuestionKind::Bool => Self::Bool,
            QuestionKind::Choice => Self::Choice,
            QuestionKind::IdList => Self::IdList,
            QuestionKind::Opaque => Self::Opaque,
            QuestionKind::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FicheChoiceDto {
    pub value: String,
    pub label: String,
}

/// Une question d'installation, avec ce qu'il faut pour la poser sans rien inventer.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FicheQuestionDto {
    pub id: String,
    pub label: String,
    pub help: String,
    pub secret: bool,
    pub proposed: Option<String>,
    pub kind: FicheQuestionKindDto,
    pub choices: Vec<FicheChoiceDto>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub min_len: Option<u32>,
    pub max_len: Option<u32>,
    pub charset: Option<FicheCharsetDto>,
    pub frozen_after_creation: bool,
    pub variants: Vec<String>,
    /// `game_password`, `admin_password` ou `server_token` pour un secret.
    pub secret_kind: Option<FicheSecretKindDto>,
    pub obtain_url: Option<String>,
}

impl From<&QuestionView> for FicheQuestionDto {
    fn from(question: &QuestionView) -> Self {
        let QuestionView {
            id,
            label,
            help,
            secret,
            proposed,
            kind,
            choices,
            min,
            max,
            min_len,
            max_len,
            charset,
            frozen_after_creation,
            variants,
            secret_kind,
            obtain_url,
        } = question;
        Self {
            id: id.clone(),
            label: label.clone(),
            help: help.clone(),
            secret: *secret,
            // Un secret ne porte jamais de valeur proposée, même si un agent en envoyait une.
            proposed: if *secret { None } else { proposed.clone() },
            kind: (*kind).into(),
            choices: choices
                .iter()
                .map(|choice| FicheChoiceDto {
                    value: choice.value.clone(),
                    label: choice.label.clone(),
                })
                .collect(),
            min: min.map(|value| value as f64),
            max: max.map(|value| value as f64),
            min_len: *min_len,
            max_len: *max_len,
            charset: charset.map(Into::into),
            frozen_after_creation: *frozen_after_creation,
            variants: variants.clone(),
            secret_kind: secret_kind.map(Into::into),
            obtain_url: obtain_url.clone(),
        }
    }
}

/// D'où vient un chiffre de ressource (BR-FICHE-008).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FicheEvidenceDto {
    Measured,
    MeasuredIdle,
    Publisher { url: String },
    Unmeasured,
    Unknown,
}

impl From<&EvidenceView> for FicheEvidenceDto {
    fn from(evidence: &EvidenceView) -> Self {
        match evidence {
            EvidenceView::Measured => Self::Measured,
            EvidenceView::MeasuredIdle => Self::MeasuredIdle,
            EvidenceView::Publisher { url } => Self::Publisher { url: url.clone() },
            EvidenceView::Unmeasured => Self::Unmeasured,
            EvidenceView::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FicheCpuNeedDto {
    pub min_millis: Option<u32>,
    pub recommended_millis: Option<u32>,
    pub evidence: FicheEvidenceDto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FicheMemoryNeedDto {
    pub min_bytes: Option<f64>,
    pub recommended_bytes: Option<f64>,
    pub evidence: FicheEvidenceDto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FicheDiskNeedDto {
    pub install_bytes: Option<f64>,
    pub evidence: FicheEvidenceDto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FicheResourcesDto {
    pub cpu: FicheCpuNeedDto,
    pub memory: FicheMemoryNeedDto,
    pub disk: FicheDiskNeedDto,
}

fn bytes(value: Option<u64>) -> Option<f64> {
    value.map(|bytes| bytes as f64)
}

impl From<&ResourcesView> for FicheResourcesDto {
    fn from(resources: &ResourcesView) -> Self {
        let ResourcesView { cpu, memory, disk } = resources;
        let CpuNeed {
            min_millis,
            recommended_millis,
            evidence: cpu_evidence,
        } = cpu;
        let MemoryNeed {
            min_bytes,
            recommended_bytes,
            evidence: memory_evidence,
        } = memory;
        let DiskNeed {
            install_bytes,
            evidence: disk_evidence,
        } = disk;
        Self {
            cpu: FicheCpuNeedDto {
                min_millis: *min_millis,
                recommended_millis: *recommended_millis,
                evidence: cpu_evidence.into(),
            },
            memory: FicheMemoryNeedDto {
                min_bytes: bytes(*min_bytes),
                recommended_bytes: bytes(*recommended_bytes),
                evidence: memory_evidence.into(),
            },
            disk: FicheDiskNeedDto {
                install_bytes: bytes(*install_bytes),
                evidence: disk_evidence.into(),
            },
        }
    }
}

/// Un téléchargement que le programme du jeu fait lui-même au premier démarrage (BR-FICHE-010).
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FicheDownloadDto {
    pub what: String,
    pub source: String,
    pub bytes: Option<f64>,
    pub evidence: FicheEvidenceDto,
}

/// Le premier démarrage d'une fiche (BR-FICHE-056).
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FicheFirstStartDto {
    #[serde(rename = "readyTimeoutSeconds")]
    pub ready_timeout_s: u32,
    pub world_generation: bool,
    pub downloads: Vec<FicheDownloadDto>,
    pub stages: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FicheWorldSavedDto {
    ByImageOnSignal,
    BySequence,
    NotSaved,
    Unknown,
}

/// La façon d'arrêter le jeu (BR-JEU-002).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FicheStopDto {
    #[serde(rename = "graceSeconds")]
    pub grace_s: u32,
    pub world_saved: FicheWorldSavedDto,
    pub sequence_tested: bool,
    /// Hearth enregistre le monde par la console avant l'arrêt (HRT-292).
    pub console_save: bool,
}

/// Un réglage posé par la fiche, avec sa raison (BR-JEU-039).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FicheImposedDto {
    pub label: String,
    pub value: Option<String>,
    pub why: String,
}

/// Un thème en mots simples (BR-FICHE-041).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FicheThemeDto {
    pub id: String,
    pub label: String,
    pub order: u32,
}

impl From<&ThemeView> for FicheThemeDto {
    fn from(theme: &ThemeView) -> Self {
        Self {
            id: theme.id.clone(),
            label: theme.label.clone(),
            order: theme.order,
        }
    }
}

/// Le niveau d'un réglage ; une valeur plus récente se lit `Unknown` (traitée comme avancée par l'interface).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FicheLevelDto {
    Essential,
    Common,
    Advanced,
    Unknown,
}

impl From<LevelView> for FicheLevelDto {
    fn from(level: LevelView) -> Self {
        match level {
            LevelView::Essential => Self::Essential,
            LevelView::Common => Self::Common,
            LevelView::Advanced => Self::Advanced,
            LevelView::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FicheItemCharsetDto {
    Digits,
    Slug,
    Unknown,
}

impl From<ItemCharsetView> for FicheItemCharsetDto {
    fn from(charset: ItemCharsetView) -> Self {
        match charset {
            ItemCharsetView::Digits => Self::Digits,
            ItemCharsetView::Slug => Self::Slug,
            ItemCharsetView::Unknown => Self::Unknown,
        }
    }
}

/// Ce que la fiche dit d'une liste d'identifiants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FicheListDto {
    pub item_charset: FicheItemCharsetDto,
    pub separator: String,
    pub max_items: u32,
}

impl From<&ListView> for FicheListDto {
    fn from(list: &ListView) -> Self {
        Self {
            item_charset: list.item_charset.into(),
            separator: list.separator.clone(),
            max_items: list.max_items,
        }
    }
}

/// Un réglage du catalogue de la fiche (BR-FICHE-037) : son contrôle et tout ce qu'il faut pour le montrer.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FicheSettingDto {
    pub question: FicheQuestionDto,
    pub key: String,
    pub theme: String,
    pub level: FicheLevelDto,
    pub game_default: Option<String>,
    pub imposed_why: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub replaced_by: Option<String>,
    pub list: Option<FicheListDto>,
}

impl From<&SettingView> for FicheSettingDto {
    fn from(setting: &SettingView) -> Self {
        let question = FicheQuestionDto::from(&setting.question);
        Self {
            // Un secret ne porte jamais de défaut, même si un agent en envoyait un.
            game_default: if question.secret {
                None
            } else {
                setting.game_default.clone()
            },
            question,
            key: setting.key.clone(),
            theme: setting.theme.clone(),
            level: setting.level.into(),
            imposed_why: setting.imposed_why.clone(),
            since: setting.since.clone(),
            until: setting.until.clone(),
            replaced_by: setting.replaced_by.clone(),
            list: setting.list.as_ref().map(Into::into),
        }
    }
}

/// Les réglages que la reprise d'un monde remplit ou ne touche pas (HRT-106, BR-MONDE-014).
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FicheWorldImportDto {
    pub names_setting: String,
    pub carried_by_world: Vec<String>,
}

/// Les voies de mods d'une fiche (HRT-124), dans l'ordre où l'écran les montre.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FicheModsDto {
    /// `drop_files`, `drop_pack` ou `drop_jars` ; une voie que ce client ne connaît pas n'est pas montrée.
    pub ways: Vec<String>,
    pub pack: Option<FichePackWayDto>,
    /// L'indication de la fiche pour le dossier « Tes mods ».
    pub files_hint: Option<String>,
}

/// Les genres de pack que la fiche prend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FichePackWayDto {
    pub genres: Vec<FichePackGenreDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FichePackGenreDto {
    /// `server_pack` ou `modrinth_pack`.
    pub kind: String,
    /// Les variantes de la fiche (moteurs de serveur) permises avec ce genre.
    pub variants: Vec<String>,
}

fn mods_dto(view: &ModsView) -> FicheModsDto {
    FicheModsDto {
        ways: view
            .ways
            .iter()
            .filter(|way| matches!(way.as_str(), "drop_files" | "drop_pack" | "drop_jars"))
            .cloned()
            .collect(),
        pack: view.pack.as_ref().map(|pack| FichePackWayDto {
            genres: pack
                .genres
                .iter()
                .filter(|genre| matches!(genre.kind.as_str(), "server_pack" | "modrinth_pack"))
                .map(|genre| FichePackGenreDto {
                    kind: genre.kind.clone(),
                    variants: genre
                        .variants
                        .iter()
                        .filter(|variant| hearth_link::is_stable_id(variant))
                        .cloned()
                        .collect(),
                })
                .collect(),
        }),
        files_hint: view.files_hint.clone(),
    }
}

/// Le détail d'une fiche (`GET /catalog/{id}`).
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FicheDetailDto {
    pub card: FicheCardDto,
    pub digest: String,
    pub image: FicheImageDto,
    pub owner_uid: u32,
    pub owner_gid: u32,
    pub variants: Vec<FicheVariantDto>,
    pub ports: Vec<FichePortDto>,
    pub folders: Vec<FicheFolderDto>,
    pub agreements: Vec<FicheAgreementDto>,
    pub questions: Vec<FicheQuestionDto>,
    pub resources: FicheResourcesDto,
    pub first_start: Option<FicheFirstStartDto>,
    pub stop: Option<FicheStopDto>,
    pub imposed: Vec<FicheImposedDto>,
    /// Les thèmes en mots simples (HRT-109) ; vide d'un agent plus ancien.
    pub themes: Vec<FicheThemeDto>,
    /// Le catalogue complet de réglages (HRT-109) ; vide d'un agent plus ancien.
    pub settings: Vec<FicheSettingDto>,
    /// Ce que la fiche dit d'un monde repris (HRT-106) ; `None` : la fiche ne reprend pas de monde.
    pub world_import: Option<FicheWorldImportDto>,
    /// Les voies de mods que la fiche offre (HRT-124, BR-MOD-001) ; absent : le jeu n'en offre aucune (BR-MOD-014).
    pub mods: Option<FicheModsDto>,
    /// Des réglages ou thèmes d'une version plus récente que ce client, illisibles ici : l'onglet le dit, mais la
    /// fiche reste installable (ils ne changent pas ce que l'installation écrit).
    pub settings_unreadable: u32,
    /// Des éléments d'une version plus récente que ce client, illisibles ici : une fiche incomplète pour ce
    /// client ne s'installe pas.
    pub unreadable: u32,
}

impl From<&FicheDetail> for FicheDetailDto {
    fn from(detail: &FicheDetail) -> Self {
        let FicheDetail {
            card,
            digest,
            image,
            owner_uid,
            owner_gid,
            variants,
            ports,
            folders,
            agreements,
            questions,
            resources,
            first_start,
            stop,
            imposed,
            themes,
            settings,
            // Les voies de mods : lues par le client avec HRT-125, pas encore par cette projection.
            world_import,
            mods,
        } = detail;
        let ImageView {
            repository,
            tag,
            digest: image_digest,
        } = image;
        let unreadable = [
            variants.unreadable(),
            ports.unreadable(),
            folders.unreadable(),
            agreements.unreadable(),
            questions.unreadable(),
            imposed.unreadable(),
        ]
        .into_iter()
        .fold(0u32, u32::saturating_add);
        Self {
            card: card.into(),
            digest: digest.clone(),
            image: FicheImageDto {
                repository: repository.clone(),
                tag: tag.clone(),
                digest: image_digest.clone(),
            },
            owner_uid: *owner_uid,
            owner_gid: *owner_gid,
            variants: variants.iter().map(variant).collect(),
            ports: ports.iter().map(port).collect(),
            folders: folders.iter().map(folder).collect(),
            agreements: agreements.iter().map(agreement).collect(),
            questions: questions.iter().map(FicheQuestionDto::from).collect(),
            resources: resources.into(),
            first_start: first_start.as_ref().map(first_start_dto),
            stop: stop.as_ref().map(stop_dto),
            imposed: imposed.iter().map(imposed_dto).collect(),
            themes: themes.iter().map(FicheThemeDto::from).collect(),
            settings: settings.iter().map(FicheSettingDto::from).collect(),
            world_import: world_import.as_ref().map(|view| FicheWorldImportDto {
                names_setting: view.names_setting.clone(),
                carried_by_world: view.carried_by_world.clone(),
            }),
            mods: mods.as_ref().map(mods_dto),
            settings_unreadable: themes.unreadable().saturating_add(settings.unreadable()),
            unreadable,
        }
    }
}

fn first_start_dto(view: &FirstStartView) -> FicheFirstStartDto {
    let FirstStartView {
        ready_timeout_s,
        world_generation,
        downloads,
        stages,
    } = view;
    FicheFirstStartDto {
        ready_timeout_s: *ready_timeout_s,
        world_generation: *world_generation,
        downloads: downloads
            .iter()
            .map(
                |DownloadView {
                     what,
                     source,
                     bytes,
                     evidence,
                 }| FicheDownloadDto {
                    what: what.clone(),
                    source: source.clone(),
                    bytes: bytes.map(|bytes| bytes as f64),
                    evidence: evidence.into(),
                },
            )
            .collect(),
        stages: stages.clone(),
    }
}

pub(super) fn stop_dto(view: &StopView) -> FicheStopDto {
    let StopView {
        grace_s,
        world_saved,
        sequence_tested,
        console_save,
    } = view;
    FicheStopDto {
        grace_s: *grace_s,
        world_saved: match world_saved {
            WorldSavedView::ByImageOnSignal => FicheWorldSavedDto::ByImageOnSignal,
            WorldSavedView::BySequence => FicheWorldSavedDto::BySequence,
            WorldSavedView::NotSaved => FicheWorldSavedDto::NotSaved,
            WorldSavedView::Unknown => FicheWorldSavedDto::Unknown,
        },
        sequence_tested: *sequence_tested,
        console_save: *console_save,
    }
}

fn imposed_dto(view: &ImposedView) -> FicheImposedDto {
    let ImposedView { label, value, why } = view;
    FicheImposedDto {
        label: label.clone(),
        value: value.clone(),
        why: why.clone(),
    }
}

fn variant(view: &VariantView) -> FicheVariantDto {
    let VariantView {
        id,
        label,
        help,
        default,
    } = view;
    FicheVariantDto {
        id: id.clone(),
        label: label.clone(),
        help: help.clone(),
        default: *default,
    }
}

fn port(view: &PortView) -> FichePortDto {
    let PortView {
        id,
        container,
        protocol,
        role,
        publish,
        default_host,
        mapping,
        only_variants,
    } = view;
    FichePortDto {
        id: id.clone(),
        container: *container,
        protocol: protocol.clone(),
        role: role.clone(),
        publish: publish.clone(),
        default_host: *default_host,
        mapping: (*mapping).into(),
        only_variants: only_variants.clone(),
    }
}

fn folder(view: &FolderView) -> FicheFolderDto {
    let FolderView {
        id,
        role,
        target,
        backup,
        only_variants,
    } = view;
    FicheFolderDto {
        id: id.clone(),
        role: role.clone(),
        target: target.clone(),
        backup: *backup,
        only_variants: only_variants.clone(),
    }
}

fn agreement(view: &AgreementView) -> FicheAgreementDto {
    let AgreementView {
        id,
        title,
        url,
        sentence,
    } = view;
    FicheAgreementDto {
        id: id.clone(),
        title: title.clone(),
        url: url.clone(),
        sentence: sentence.clone(),
    }
}

/// Le résultat de la lecture d'une fiche : trouvée, ou inconnue de cet agent (`FICHE_UNKNOWN`, y compris une fiche
/// d'essai sur un agent publié).
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FicheReadDto {
    Found { fiche: Box<FicheDetailDto> },
    Unknown,
}

/// La réponse à une question de la fiche. La valeur peut être un secret.
#[derive(Clone, Deserialize, Type)]
pub struct FicheAnswerDto {
    pub id: String,
    pub value: String,
}

/// Un port de la fiche et le port du serveur choisi.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FichePortChoiceDto {
    pub id: String,
    pub host_port: u16,
}

/// La demande d'installation depuis une fiche (membre `source` de `POST /deployments`, BR-FICHE-013).
#[derive(Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FicheInstallRequestDto {
    pub name: String,
    pub fiche: String,
    pub version: u32,
    pub digest: String,
    pub variant: Option<String>,
    pub answers: Vec<FicheAnswerDto>,
    /// Les contrats que l'utilisateur a acceptés par un geste (identifiants).
    pub agreements: Vec<String>,
    pub ports: Vec<FichePortChoiceDto>,
    pub optional_ports: Vec<String>,
    /// Reprendre un monde existant (HRT-106) : le monde choisi et les deux gestes ; `None` : un monde neuf.
    pub world: Option<super::worlds_dto::WorldPickDto>,
    pub resources: DeployResourcesDto,
    pub accept: DeployAcceptDto,
    /// L'identifiant du pack de mods déjà déposé dans la zone d'attente de l'agent (HRT-124), jamais un chemin.
    pub pack: Option<String>,
}

impl std::fmt::Debug for FicheInstallRequestDto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Aucune valeur de réponse : seulement leur nombre.
        f.debug_struct("FicheInstallRequestDto")
            .field("name", &self.name)
            .field("fiche", &self.fiche)
            .field("version", &self.version)
            .field("variant", &self.variant)
            .field("answers", &self.answers.len())
            .field("agreements", &self.agreements)
            .field("ports", &self.ports)
            .field("pack", &self.pack.is_some())
            .field("resources", &self.resources)
            .field("accept", &self.accept)
            .finish_non_exhaustive()
    }
}

/// Un nombre d'octets de l'interface : entier, fini, positif, sans perte. Sinon la demande ne part pas.
fn exact_bytes(value: f64) -> Option<u64> {
    let valid = value.is_finite() && (0.0..=MAX_SAFE).contains(&value) && value.fract() == 0.0;
    valid.then_some(value as u64)
}

impl FicheInstallRequestDto {
    /// Le corps envoyé à l'agent : la fiche décide l'image, les variables, les ports, les dossiers et la
    /// commande, la demande n'en dit aucun. La reprise d'un monde n'existe pas dans cette version.
    pub fn into_spec(self) -> Result<DeploySpec, LinkFailure> {
        let memory_bytes =
            exact_bytes(self.resources.memory_bytes).ok_or(LinkFailure::InvalidInput {
                field: InvalidField::Other,
            })?;
        // Le pack est désigné par l'identifiant que l'agent a donné au dépôt, jamais par un chemin ni un nom de fichier.
        let pack = match self.pack {
            None => None,
            Some(deposit) if hearth_link::is_deposit_id(&deposit) => Some(DeployPack { deposit }),
            Some(_) => {
                return Err(LinkFailure::InvalidInput {
                    field: InvalidField::Other,
                });
            }
        };
        Ok(DeploySpec {
            name: self.name,
            image: String::new(),
            tag: None,
            env: Vec::new(),
            ports: Vec::new(),
            mounts: Vec::new(),
            command: None,
            resources: DeployResources {
                cpu_millis: Some(self.resources.cpu_millis),
                memory_bytes: Some(memory_bytes),
            },
            accept: DeployAccept {
                reserved_ports: self.accept.reserved_ports,
                low_disk: self.accept.low_disk,
            },
            source: Some(DeploySource::Fiche {
                id: self.fiche,
                version: self.version,
                digest: self.digest,
                variant: self.variant,
                answers: self
                    .answers
                    .into_iter()
                    .map(|answer| FicheAnswer {
                        id: answer.id,
                        value: answer.value,
                    })
                    .collect(),
                agreements: self.agreements,
                ports: self
                    .ports
                    .into_iter()
                    .map(|port| FichePort {
                        id: port.id,
                        host_port: port.host_port,
                    })
                    .collect(),
                optional_ports: self.optional_ports,
                world_imported: self.world.is_some(),
                world: self.world.map(Into::into),
                pack,
            }),
            extra: std::collections::BTreeMap::new(),
        })
    }
}
