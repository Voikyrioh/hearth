//! Ce que la coquille échange avec l'interface pour les réglages d'un serveur de jeu (HRT-109, ADR-0051,
//! `BR-FICHE-037`, `040` à `049`, `051` à `054`).
//!
//! - **Sorties** : copies fidèles de `hearth_proto::api::game_settings`, rien de plus. Un secret n'y figure jamais
//!   (l'agent ne le rend pas) ; une valeur de réglage inconnu au nom suspect est déjà masquée par l'agent. Une
//!   énumération porte `Unknown` (une valeur plus récente que ce client se lit « inconnue »).
//! - **Entrées** (`SaveSettingsRequestDto`, le texte collé) : une valeur peut être un mot de passe. `Debug` n'en montre
//!   que le nombre ; elle traverse la coquille sans être gardée ni tracée.
//! - **Issue d'un enregistrement** : un refus se lit par son CODE (`details.reason`, `details.ids`), jamais par son
//!   texte ; un réglage n'est nommé que si son identifiant a la forme d'un identifiant de fiche.

use hearth_proto::api::game_settings::{
    ChangeAction, DriftChoice, DriftItem, DriftResolution, FicheRef, FileState, ImportLine,
    ImportState, ImportView, OtherChange, OtherSetting, SaveOrigin, SaveSettingsRequest,
    SaveSettingsResponse, SettingChange, SettingEffect, SettingValue, SettingsView, ValueState,
};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::link_dto::{InvalidField, LinkFailure};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SettingEffectDto {
    None,
    Restart,
    Recreate,
    Unknown,
}

impl From<SettingEffect> for SettingEffectDto {
    fn from(effect: SettingEffect) -> Self {
        match effect {
            SettingEffect::None => Self::None,
            SettingEffect::Restart => Self::Restart,
            SettingEffect::Recreate => Self::Recreate,
            SettingEffect::Unknown => Self::Unknown,
        }
    }
}

impl From<SettingEffectDto> for SettingEffect {
    fn from(effect: SettingEffectDto) -> Self {
        match effect {
            SettingEffectDto::None => Self::None,
            SettingEffectDto::Restart => Self::Restart,
            SettingEffectDto::Recreate => Self::Recreate,
            SettingEffectDto::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ValueStateDto {
    Written,
    Unwritten,
    SecretSet,
    Unknown,
}

impl From<ValueState> for ValueStateDto {
    fn from(state: ValueState) -> Self {
        match state {
            ValueState::Written => Self::Written,
            ValueState::Unwritten => Self::Unwritten,
            ValueState::SecretSet => Self::SecretSet,
            ValueState::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FileStateDto {
    Ok,
    Missing,
    Unreadable,
    Unknown,
}

impl From<FileState> for FileStateDto {
    fn from(state: FileState) -> Self {
        match state {
            FileState::Ok => Self::Ok,
            FileState::Missing => Self::Missing,
            FileState::Unreadable => Self::Unreadable,
            FileState::Unknown => Self::Unknown,
        }
    }
}

/// La valeur voulue d'un réglage de la fiche.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SettingValueDto {
    pub id: String,
    pub state: ValueStateDto,
    pub value: Option<String>,
    pub frozen: bool,
    pub imposed: bool,
    pub gone: bool,
    pub effect: SettingEffectDto,
}

impl From<&SettingValue> for SettingValueDto {
    fn from(setting: &SettingValue) -> Self {
        Self {
            id: setting.id.clone(),
            state: setting.state.into(),
            // Un secret n'a jamais de valeur côté client, même si un agent en envoyait une.
            value: if setting.state == ValueState::SecretSet {
                None
            } else {
                setting.value.clone()
            },
            frozen: setting.frozen,
            imposed: setting.imposed,
            gone: setting.gone,
            effect: setting.effect.into(),
        }
    }
}

/// Un réglage dont le fichier ne dit plus ce que Hearth veut.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DriftDto {
    pub id: String,
    pub hearth: Option<String>,
    pub file: Option<String>,
    pub file_valid: bool,
}

impl From<&DriftItem> for DriftDto {
    fn from(drift: &DriftItem) -> Self {
        Self {
            id: drift.id.clone(),
            hearth: drift.hearth.clone(),
            file: drift.file.clone(),
            file_valid: drift.file_valid,
        }
    }
}

/// Un réglage que la fiche ne connaît pas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OtherSettingDto {
    pub key: String,
    pub value: Option<String>,
    pub masked: bool,
}

impl From<&OtherSetting> for OtherSettingDto {
    fn from(other: &OtherSetting) -> Self {
        Self {
            key: other.key.clone(),
            // Un nom qui évoque un secret n'a pas de valeur côté client, même si un agent en envoyait une.
            value: if other.masked {
                None
            } else {
                other.value.clone()
            },
            masked: other.masked,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FicheRefDto {
    pub id: String,
    pub version: u32,
    pub digest: String,
}

impl From<&FicheRef> for FicheRefDto {
    fn from(fiche: &FicheRef) -> Self {
        Self {
            id: fiche.id.clone(),
            version: fiche.version,
            digest: fiche.digest.clone(),
        }
    }
}

/// La vue des réglages d'un service (`GET /services/{key}/settings`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SettingsViewDto {
    pub rev: u32,
    pub fiche: FicheRefDto,
    pub file_state: FileStateDto,
    pub file_stamp: Option<String>,
    pub file_changed: bool,
    pub raw_edit: bool,
    pub settings: Vec<SettingValueDto>,
    pub drifts: Vec<DriftDto>,
    pub others: Vec<OtherSettingDto>,
    pub imposed_broken: Vec<String>,
}

impl From<&SettingsView> for SettingsViewDto {
    fn from(view: &SettingsView) -> Self {
        Self {
            rev: view.rev,
            fiche: (&view.fiche).into(),
            file_state: view.file_state.into(),
            file_stamp: view.file_stamp.clone(),
            file_changed: view.file_changed,
            raw_edit: view.raw_edit,
            settings: view.settings.iter().map(Into::into).collect(),
            drifts: view.drifts.iter().map(Into::into).collect(),
            others: view.others.iter().map(Into::into).collect(),
            imposed_broken: view.imposed_broken.clone(),
        }
    }
}

/// La lecture des réglages d'un service : trouvés, ou « ce service n'a pas de réglages de jeu » (un service qui n'est
/// pas né d'une fiche, ou un agent plus ancien : jamais une panne).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SettingsReadDto {
    Found { view: Box<SettingsViewDto> },
    Absent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ImportStateDto {
    Take,
    Same,
    Rejected,
    Imposed,
    Frozen,
    Unknown,
}

impl From<ImportState> for ImportStateDto {
    fn from(state: ImportState) -> Self {
        match state {
            ImportState::Take => Self::Take,
            ImportState::Same => Self::Same,
            ImportState::Rejected => Self::Rejected,
            ImportState::Imposed => Self::Imposed,
            ImportState::Frozen => Self::Frozen,
            ImportState::Unknown => Self::Unknown,
        }
    }
}

/// Une ligne du texte collé, lue par l'agent. La valeur d'un secret n'y figure pas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportLineDto {
    pub id: String,
    pub state: ImportStateDto,
    pub reason: Option<String>,
    pub pasted: Option<String>,
    pub secret: bool,
}

impl From<&ImportLine> for ImportLineDto {
    fn from(line: &ImportLine) -> Self {
        Self {
            id: line.id.clone(),
            state: line.state.into(),
            reason: line.reason.clone(),
            pasted: if line.secret {
                None
            } else {
                line.pasted.clone()
            },
            secret: line.secret,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportViewDto {
    pub lines: Vec<ImportLineDto>,
    pub unknown: u32,
    pub duplicates: u32,
    pub imposed_undone: Vec<String>,
}

impl From<&ImportView> for ImportViewDto {
    fn from(view: &ImportView) -> Self {
        Self {
            lines: view.lines.iter().map(Into::into).collect(),
            unknown: view.unknown,
            duplicates: view.duplicates,
            imposed_undone: view.imposed_undone.clone(),
        }
    }
}

/// La lecture d'un texte collé : lue, ou illisible par le format du jeu (rien n'est écrit dans les deux cas).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnalyzeOutcomeDto {
    Read {
        view: Box<ImportViewDto>,
    },
    Unreadable {
        reason: Option<String>,
        line: Option<u32>,
    },
}

// ── L'enregistrement ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SaveOriginDto {
    Form,
    Import,
    Reset,
    Drift,
}

impl From<SaveOriginDto> for SaveOrigin {
    fn from(origin: SaveOriginDto) -> Self {
        match origin {
            SaveOriginDto::Form => Self::Form,
            SaveOriginDto::Import => Self::Import,
            SaveOriginDto::Reset => Self::Reset,
            SaveOriginDto::Drift => Self::Drift,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ChangeActionDto {
    Set,
    Unset,
    Reset,
    FromImport,
}

impl From<ChangeActionDto> for ChangeAction {
    fn from(action: ChangeActionDto) -> Self {
        match action {
            ChangeActionDto::Set => Self::Set,
            ChangeActionDto::Unset => Self::Unset,
            ChangeActionDto::Reset => Self::Reset,
            ChangeActionDto::FromImport => Self::FromImport,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DriftResolutionDto {
    KeepFile,
    KeepMine,
}

impl From<DriftResolutionDto> for DriftResolution {
    fn from(choice: DriftResolutionDto) -> Self {
        match choice {
            DriftResolutionDto::KeepFile => Self::KeepFile,
            DriftResolutionDto::KeepMine => Self::KeepMine,
        }
    }
}

/// Le geste sur un réglage. La valeur peut être un mot de passe : `Debug` n'en montre que la présence.
#[derive(Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SettingChangeDto {
    pub id: String,
    pub action: ChangeActionDto,
    pub value: Option<String>,
}

impl std::fmt::Debug for SettingChangeDto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SettingChangeDto")
            .field("id", &self.id)
            .field("action", &self.action)
            .field("has_value", &self.value.is_some())
            .finish()
    }
}

/// Un réglage inconnu de la fiche : changer sa valeur, ou le retirer.
#[derive(Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OtherChangeDto {
    pub key: String,
    pub remove: bool,
    pub value: Option<String>,
}

impl std::fmt::Debug for OtherChangeDto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OtherChangeDto")
            .field("remove", &self.remove)
            .field("has_value", &self.value.is_some())
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DriftChoiceDto {
    pub id: String,
    pub choice: DriftResolutionDto,
}

/// La demande d'enregistrement. Le texte collé et les valeurs peuvent porter un mot de passe : jamais dans un `Debug`.
#[derive(Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveSettingsRequestDto {
    pub base_rev: u32,
    pub file_stamp: String,
    pub origin: SaveOriginDto,
    pub effect: SettingEffectDto,
    pub changes: Vec<SettingChangeDto>,
    pub others: Vec<OtherChangeDto>,
    pub drift_choices: Vec<DriftChoiceDto>,
    pub import_text: Option<String>,
}

impl std::fmt::Debug for SaveSettingsRequestDto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SaveSettingsRequestDto")
            .field("base_rev", &self.base_rev)
            .field("origin", &self.origin)
            .field("effect", &self.effect)
            .field("changes", &self.changes.len())
            .field("others", &self.others.len())
            .field("drift_choices", &self.drift_choices.len())
            .field("has_import_text", &self.import_text.is_some())
            .finish()
    }
}

impl SaveSettingsRequestDto {
    /// La demande telle que l'agent la lit. Une empreinte de fichier vide n'est pas une empreinte : rien ne part
    /// (l'agent refuse un corps sans `file_stamp`).
    pub fn into_request(self) -> Result<SaveSettingsRequest, LinkFailure> {
        if self.file_stamp.is_empty() {
            return Err(LinkFailure::InvalidInput {
                field: InvalidField::Other,
            });
        }
        Ok(SaveSettingsRequest {
            base_rev: self.base_rev,
            file_stamp: self.file_stamp,
            origin: self.origin.into(),
            effect: self.effect.into(),
            changes: self
                .changes
                .into_iter()
                .map(|change| SettingChange {
                    id: change.id,
                    action: change.action.into(),
                    value: change.value,
                })
                .collect(),
            others: self
                .others
                .into_iter()
                .map(|other| OtherChange {
                    key: other.key,
                    remove: other.remove,
                    value: other.value,
                })
                .collect(),
            drift_choices: self
                .drift_choices
                .into_iter()
                .map(|choice| DriftChoice {
                    id: choice.id,
                    choice: choice.choice.into(),
                })
                .collect(),
            import_text: self.import_text,
        })
    }
}

/// Pourquoi l'agent a refusé l'enregistrement. Sans texte : l'interface choisit le message d'après `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SaveRefusal {
    WrongPassword,
    /// L'élévation s'est fermée côté agent : la fenêtre redemande le mot de passe sans perdre la saisie.
    PasswordRequired,
    TooManyAttempts {
        retry_after_s: u32,
    },
    Busy,
    /// Les réglages ont changé depuis la lecture (autre révision), ou l'effet annoncé n'est pas celui des changements.
    Conflict {
        by: Option<String>,
        #[serde(rename = "effectMismatch")]
        effect_mismatch: bool,
    },
    /// Le fichier a bougé depuis la lecture, ou des réglages en écart attendent un choix.
    FileChanged {
        ids: Vec<String>,
    },
    FileUnreadable {
        reason: Option<String>,
        line: Option<u32>,
    },
    /// Un changement est refusé : le réglage (s'il est de la fiche), la raison, au plus UN caractère. Jamais la valeur.
    SettingRefused {
        id: Option<String>,
        reason: String,
        character: Option<String>,
    },
    /// Un réglage reçu par variable ou argument demande de recréer le service (pas encore livré).
    RecreateNotAvailable {
        ids: Vec<String>,
    },
    /// Une action tient le service.
    OperationInProgress,
    ServiceNotFound,
    Invalid,
    Other,
}

/// L'issue d'un enregistrement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SaveOutcome {
    Saved {
        rev: u32,
        effect: SettingEffectDto,
        #[serde(rename = "fileWritten")]
        file_written: bool,
        changed: u32,
        adopted: Vec<String>,
    },
    Refused {
        refusal: SaveRefusal,
    },
    /// Le lien est tombé avant la réponse : on ne sait pas. Jamais rejoué ; l'interface relit la vue.
    Unknown {
        #[serde(rename = "opId")]
        op_id: String,
    },
}

impl From<&SaveSettingsResponse> for SaveOutcome {
    fn from(response: &SaveSettingsResponse) -> Self {
        Self::Saved {
            rev: response.rev,
            effect: response.effect.into(),
            file_written: response.file_written,
            changed: response.changed,
            adopted: response.adopted.clone(),
        }
    }
}
