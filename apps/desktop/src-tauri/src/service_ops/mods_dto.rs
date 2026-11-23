//! Ce que la coquille échange avec l'interface pour les mods d'un serveur Minecraft (HRT-260, ADR-0062,
//! `BR-MOD-020` à `036`).
//!
//! - **Entrées** : les deux listes d'identifiants d'un contrôle (dépôts) et le lot à appliquer (ajouts contrôlés,
//!   retraits, remises, désactivations, réactivations). Aucun nom n'est choisi par le client pour un fichier posé.
//! - **Sorties** : copies fidèles de `hearth_proto::api::mods`. Une énumération du fil devient son mot du fil (texte
//!   `snake_case`) : l'interface la lit contre une liste fermée et traite toute valeur inconnue comme « inconnue »,
//!   jamais comme une permission (BR-COMPAT-001). Aucun texte de l'agent n'est repris, sauf des noms de fichiers et de
//!   mods déjà passés par une forme (alphabet sûr) ; l'interface les montre comme du texte, jamais comme du HTML.

use hearth_proto::api::mods::{
    BackupModItem, BackupModsCompare, BackupModsSummary, BackupModsView, ModAdd, ModChange,
    ModCheckFile, ModIdentityView, ModItem, ModVerdictView, ModsApplyRequest, ModsCheckView,
    ModsGame, ModsView,
};
use serde::{Deserialize, Serialize};
use specta::Type;

use super::backups_dto::BackupRefusal;

/// Le mot du fil d'une énumération (`snake_case`), ou `unknown` si elle n'en a pas.
pub fn wire<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|json| json.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_owned())
}

/// Ce que le mod dit de lui-même.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModIdentityDto {
    pub id: String,
    pub name: Option<String>,
    pub version: Option<String>,
    pub loader: String,
    pub minecraft_declared: bool,
    /// `both`, `server`, `client` ou `unknown`.
    pub environment: String,
}

impl From<&ModIdentityView> for ModIdentityDto {
    fn from(view: &ModIdentityView) -> Self {
        Self {
            id: view.id.clone(),
            name: view.name.clone(),
            version: view.version.clone(),
            loader: view.loader.clone(),
            minecraft_declared: view.minecraft_declared,
            environment: wire(&view.environment),
        }
    }
}

/// Le verdict de compatibilité d'un mod.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModVerdictDto {
    /// `compatible`, `unverified`, `to_complete`, `incompatible_loader`, `incompatible_minecraft`, `duplicate`,
    /// `unrecognized` ou `unknown`.
    pub code: String,
    pub missing: Vec<String>,
    pub needed_by: Vec<String>,
}

impl From<&ModVerdictView> for ModVerdictDto {
    fn from(view: &ModVerdictView) -> Self {
        Self {
            code: wire(&view.code),
            missing: view.missing.clone(),
            needed_by: view.needed_by.clone(),
        }
    }
}

/// Un fichier du dossier des mods ou de « Mods retirés ».
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModItemDto {
    pub file: Option<String>,
    pub rank: u32,
    /// `active`, `retired` ou `unknown`.
    pub place: String,
    pub enabled: bool,
    /// `server_pack`, `hearth`, `modrinth_pack`, `manual` ou `unknown`.
    pub origin: String,
    pub readable_name: bool,
    pub identity: Option<ModIdentityDto>,
    pub verdict: ModVerdictDto,
}

impl From<&ModItem> for ModItemDto {
    fn from(item: &ModItem) -> Self {
        Self {
            file: item.file.clone(),
            rank: item.rank,
            place: wire(&item.place),
            enabled: item.enabled,
            origin: wire(&item.origin),
            readable_name: item.readable_name,
            identity: item.identity.as_ref().map(Into::into),
            verdict: (&item.verdict).into(),
        }
    }
}

/// Le jeu tel que l'agent le comprend pour juger les mods.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModsGameDto {
    pub loader: Option<String>,
    pub minecraft: Option<String>,
    /// `setting`, `log`, `modrinth_manifest` ou `unknown`.
    pub minecraft_from: String,
    pub updates_every_start: bool,
    /// `none`, `server_pack`, `modrinth_pack` ou `unknown`.
    pub pack: String,
}

impl From<&ModsGame> for ModsGameDto {
    fn from(game: &ModsGame) -> Self {
        Self {
            loader: game.loader.clone(),
            minecraft: game.minecraft.clone(),
            minecraft_from: wire(&game.minecraft_from),
            updates_every_start: game.updates_every_start,
            pack: wire(&game.pack),
        }
    }
}

/// La liste des mods d'un service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModsViewDto {
    /// `mods`, `no_mods`, `plugins`, `no_way` ou `unknown`.
    pub applicable: String,
    pub game: ModsGameDto,
    pub mods: Vec<ModItemDto>,
    pub unread: u32,
    pub truncated: bool,
}

impl From<&ModsView> for ModsViewDto {
    fn from(view: &ModsView) -> Self {
        Self {
            applicable: wire(&view.applicable),
            game: (&view.game).into(),
            mods: view.mods.iter().map(Into::into).collect(),
            unread: view.unread,
            truncated: view.truncated,
        }
    }
}

/// Pourquoi l'agent n'a pas donné une lecture des mods (liste, contrôle, liste d'une sauvegarde). Sans texte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModsReadRefusal {
    /// Clé inconnue, ou service qui n'est pas un serveur de jeu créé par Hearth.
    ServiceNotFound,
    ServiceAdopted,
    /// Le moteur du serveur ne charge pas de mods (BR-MOD-030).
    NotApplicable,
    /// Un dépôt du contrôle n'existe pas ou n'est plus dans la zone d'attente.
    DepositUnknown,
    /// Le dossier des mods est un lien ou n'est pas un dossier.
    FolderBlocked,
    /// La sauvegarde n'existe pas pour ce service.
    BackupNotFound,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModsOutcome {
    Ready { view: Box<ModsViewDto> },
    Refused { refusal: ModsReadRefusal },
}

/// Le fichier qu'un dépôt remplace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModReplacesDto {
    pub file: String,
    pub version: Option<String>,
}

/// Les valeurs d'un refus de moteur ou de version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RefusalValuesDto {
    pub loader: Option<String>,
    pub server_loader: Option<String>,
    pub server_minecraft: Option<String>,
}

/// Le jugement d'un dépôt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModCheckFileDto {
    pub deposit: String,
    /// `add`, `replace`, `refused` ou `unknown`.
    pub outcome: String,
    pub identity: Option<ModIdentityDto>,
    pub target_file: Option<String>,
    pub replaces: Option<ModReplacesDto>,
    pub verdict: Option<ModVerdictDto>,
    /// Pour un refus : `MOD_UNRECOGNIZED`, `MOD_TOO_LARGE`, `MOD_INCOMPATIBLE_LOADER`, etc.
    pub refusal: Option<String>,
    pub refusal_values: Option<RefusalValuesDto>,
}

impl From<&ModCheckFile> for ModCheckFileDto {
    fn from(file: &ModCheckFile) -> Self {
        Self {
            deposit: file.deposit.clone(),
            outcome: wire(&file.outcome),
            identity: file.identity.as_ref().map(Into::into),
            target_file: file.target_file.clone(),
            replaces: file.replaces.as_ref().map(|replaces| ModReplacesDto {
                file: replaces.file.clone(),
                version: replaces.version.clone(),
            }),
            verdict: file.verdict.as_ref().map(Into::into),
            refusal: file.refusal.clone(),
            refusal_values: file.refusal_values.as_ref().map(|values| RefusalValuesDto {
                loader: values.loader.clone(),
                server_loader: values.server_loader.clone(),
                server_minecraft: values.server_minecraft.clone(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModsCheckDto {
    pub files: Vec<ModCheckFileDto>,
}

impl From<&ModsCheckView> for ModsCheckDto {
    fn from(view: &ModsCheckView) -> Self {
        Self {
            files: view.files.iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModsCheckOutcome {
    Ready { view: Box<ModsCheckDto> },
    Refused { refusal: ModsReadRefusal },
}

/// Un ajout du lot : le dépôt contrôlé, son empreinte et le fichier qu'il remplace.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModAddDto {
    pub deposit: String,
    pub sha256: String,
    pub replace: Option<String>,
}

/// Le lot à appliquer.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModsApplyDto {
    pub add: Vec<ModAddDto>,
    pub retire: Vec<String>,
    pub restore: Vec<String>,
    pub disable: Vec<String>,
    pub enable: Vec<String>,
}

impl From<ModsApplyDto> for ModsApplyRequest {
    fn from(dto: ModsApplyDto) -> Self {
        Self {
            add: dto
                .add
                .into_iter()
                .map(|add| ModAdd {
                    deposit: add.deposit,
                    sha256: add.sha256,
                    replace: add.replace,
                })
                .collect(),
            retire: dto.retire,
            restore: dto.restore,
            disable: dto.disable,
            enable: dto.enable,
        }
    }
}

/// Pourquoi l'agent a refusé l'application d'un lot. Sans texte.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModsApplyRefusal {
    /// Un refus commun aux actes de service : mot de passe, verrou, espace, moteur (comme une sauvegarde).
    Common { refusal: BackupRefusal },
    /// Un refus propre aux mods : `MODS_BATCH_INVALID`, `MOD_REFUSED`, `MOD_FILE_MISSING`, `MOD_IN_MODRINTH_PACK`,
    /// `MOD_ALREADY_ACTIVE`, `MOD_NAME_TAKEN`, `MODS_FOLDER_BLOCKED`, `DEPOSIT_UNKNOWN`, `MODS_NOT_APPLICABLE`,
    /// `NOT_A_GAME_SERVICE`. `deposit` et `reason` : le dépôt et le code d'un `MOD_REFUSED`.
    Mod {
        code: String,
        deposit: Option<String>,
        reason: Option<String>,
    },
}

/// L'issue de l'application : acceptée (l'opération court à l'arrière-plan), refusée, ou résultat inconnu (le lien
/// est tombé avant la réponse : jamais rejoué).
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModsApplyOutcome {
    Accepted {
        #[serde(rename = "operationId")]
        operation_id: String,
        #[serde(rename = "backupId")]
        backup_id: String,
    },
    Refused {
        refusal: ModsApplyRefusal,
    },
    Unknown {
        #[serde(rename = "opId")]
        op_id: String,
    },
}

/// Les dernières lignes du journal d'une opération et, si l'agent la reconnaît, la cause et sa phrase en clair. Les
/// lignes sont du texte du jeu : jamais interprétées, jamais tracées (`Debug` ne les montre pas).
#[derive(Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OperationTailDto {
    pub lines: Vec<String>,
    pub reason: Option<String>,
    pub sentence: Option<String>,
}

impl std::fmt::Debug for OperationTailDto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OperationTailDto({} lignes)", self.lines.len())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OperationTailOutcome {
    Ready { tail: OperationTailDto },
    Refused { refusal: ModsReadRefusal },
}

// ── Les mods d'une sauvegarde (HRT-294) ──────────────────────────────────────────────────────────────────────────────

/// Le moteur et la version de Minecraft du serveur au moment de la sauvegarde.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupModsGameDto {
    pub loader: Option<String>,
    pub minecraft: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupModsCountsDto {
    pub active: u32,
    pub disabled: u32,
    pub retired: u32,
    pub not_listed: u32,
}

/// Un fichier de la sauvegarde.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupModItemDto {
    pub file: Option<String>,
    pub rank: u32,
    /// `active`, `disabled`, `retired` ou `unknown`.
    pub place: String,
    pub origin: String,
    pub identity: Option<ModIdentityDto>,
}

impl From<&BackupModItem> for BackupModItemDto {
    fn from(item: &BackupModItem) -> Self {
        Self {
            file: item.file.clone(),
            rank: item.rank,
            place: wire(&item.place),
            origin: wire(&item.origin),
            identity: item.identity.as_ref().map(Into::into),
        }
    }
}

/// Un écart entre la sauvegarde et les mods actuels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModChangeDto {
    /// `added`, `removed`, `version_changed`, `enabled`, `disabled` ou `unknown`.
    pub kind: String,
    pub id: Option<String>,
    pub name: Option<String>,
    pub file: Option<String>,
    pub backup_version: Option<String>,
    pub current_version: Option<String>,
}

impl From<&ModChange> for ModChangeDto {
    fn from(change: &ModChange) -> Self {
        Self {
            kind: wire(&change.kind),
            id: change.id.clone(),
            name: change.name.clone(),
            file: change.file.clone(),
            backup_version: change.backup_version.clone(),
            current_version: change.current_version.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupModsCompareDto {
    pub complete: bool,
    pub unread: u32,
    pub same: bool,
    pub changes: Vec<ModChangeDto>,
}

impl From<&BackupModsCompare> for BackupModsCompareDto {
    fn from(compare: &BackupModsCompare) -> Self {
        Self {
            complete: compare.complete,
            unread: compare.unread,
            same: compare.same,
            changes: compare.changes.iter().map(Into::into).collect(),
        }
    }
}

/// La liste des mods d'une sauvegarde.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupModsDto {
    /// `recorded`, `partial`, `unreadable`, `not_in_backup`, `not_recorded` ou `unknown`.
    pub state: String,
    pub recorded_at: Option<String>,
    pub game: BackupModsGameDto,
    pub counts: BackupModsCountsDto,
    pub mods: Vec<BackupModItemDto>,
    pub compare: Option<BackupModsCompareDto>,
}

impl From<&BackupModsView> for BackupModsDto {
    fn from(view: &BackupModsView) -> Self {
        Self {
            state: wire(&view.state),
            recorded_at: view.recorded_at.clone(),
            game: BackupModsGameDto {
                loader: view.game.loader.clone(),
                minecraft: view.game.minecraft.clone(),
            },
            counts: BackupModsCountsDto {
                active: view.counts.active,
                disabled: view.counts.disabled,
                retired: view.counts.retired,
                not_listed: view.counts.not_listed,
            },
            mods: view.mods.iter().map(Into::into).collect(),
            compare: view.compare.as_ref().map(Into::into),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BackupModsOutcome {
    Ready { view: Box<BackupModsDto> },
    Refused { refusal: ModsReadRefusal },
}

/// Le résumé joint à chaque sauvegarde de la liste.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupModsSummaryDto {
    pub state: String,
    pub active: u32,
    pub disabled: u32,
    pub retired: u32,
}

impl From<&BackupModsSummary> for BackupModsSummaryDto {
    fn from(summary: &BackupModsSummary) -> Self {
        Self {
            state: wire(&summary.state),
            active: summary.active,
            disabled: summary.disabled,
            retired: summary.retired,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hearth_proto::api::mods::{Applicable, ModOrigin, VerdictCode};

    #[test]
    fn an_enum_travels_as_its_wire_word_and_an_unknown_one_as_unknown() {
        assert_eq!(wire(&Applicable::NoMods), "no_mods");
        assert_eq!(wire(&ModOrigin::ModrinthPack), "modrinth_pack");
        assert_eq!(
            wire(&VerdictCode::IncompatibleMinecraft),
            "incompatible_minecraft"
        );
        assert_eq!(wire(&VerdictCode::Unknown), "unknown");
    }

    #[test]
    fn a_batch_becomes_the_request_the_agent_signs() {
        let dto = ModsApplyDto {
            add: vec![ModAddDto {
                deposit: "01J".into(),
                sha256: "ab".into(),
                replace: Some("old.jar".into()),
            }],
            retire: vec!["a.jar".into()],
            restore: vec![],
            disable: vec!["b.jar".into()],
            enable: vec![],
        };
        let request: ModsApplyRequest = dto.into();
        assert_eq!(request.total(), 3);
        assert_eq!(request.add[0].replace.as_deref(), Some("old.jar"));
    }
}
