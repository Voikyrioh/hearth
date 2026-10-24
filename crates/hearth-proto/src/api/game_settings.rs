//! Les réglages d'un serveur de jeu (HRT-108, ADR-0051, BR-FICHE-037 à 053) : `GET /services/{key}/settings`,
//! `POST /services/{key}/settings/analyze` (lecture d'un texte collé, rien n'est écrit) et
//! `PUT /services/{key}/settings` (acte `GameSettingsSave` `0x1E`). Annoncés par la capacité
//! [`CAPABILITY_GAME_SETTINGS`].
//!
//! Les libellés, aides, types et bornes d'un réglage ne passent PAS ici : le client les lit dans la fiche
//! (`GET /catalog/{id}`). Ces types ne portent que les VALEURS voulues et l'état du fichier. **Un secret n'y
//! figure jamais** (ni sa valeur ni sa longueur) : seulement « défini » ou non. Un réglage inconnu de la fiche
//! n'a sa valeur montrée que si son nom ne ressemble pas à un secret.
//!
//! Évolution : toute énumération que le client lit tolère une valeur inconnue (`Unknown`), tout champ ajouté
//! plus tard est facultatif (`serde(default)`).

use serde::{Deserialize, Serialize};

/// Capacité annoncée par `GET /hello` quand l'agent sert les réglages d'un serveur de jeu.
pub const CAPABILITY_GAME_SETTINGS: &str = "game-settings";

/// Taille maximale d'un texte collé, en octets (celle d'un fichier de réglages que le format accepte).
pub const MAX_IMPORT_BYTES: usize = 2 * 1024 * 1024;
/// Nombre maximal de changements d'un enregistrement.
pub const MAX_CHANGES: usize = 400;

/// Ce qu'un changement demande au service (déduit du canal, la fiche ne le déclare pas).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingEffect {
    /// Rien à appliquer (aucun changement).
    None,
    /// Prend effet au prochain démarrage (le jeu relit son fichier).
    Restart,
    /// Prend effet quand Hearth recrée le service.
    Recreate,
    #[serde(other)]
    Unknown,
}

impl SettingEffect {
    /// Le texte signé dans l'acte.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Restart => "restart",
            Self::Recreate => "recreate",
            Self::Unknown => "unknown",
        }
    }
}

/// D'où vient un enregistrement (signé dans l'acte).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SaveOrigin {
    /// Le formulaire de l'onglet « Réglages ».
    Form,
    /// Un texte collé, analysé puis appliqué.
    Import,
    /// Un retour aux valeurs proposées.
    Reset,
    /// Le choix « Garder la valeur du fichier » ou « Remettre la mienne » après un écart.
    Drift,
    #[serde(other)]
    Unknown,
}

impl SaveOrigin {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Form => "form",
            Self::Import => "import",
            Self::Reset => "reset",
            Self::Drift => "drift",
            Self::Unknown => "unknown",
        }
    }
}

/// Ce que l'état d'un réglage dit de lui (les trois états de BR-FICHE-037).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueState {
    /// Une valeur est voulue (`value`).
    Written,
    /// Non écrit : le jeu prend son défaut.
    Unwritten,
    /// Un secret est défini ; sa valeur n'est jamais rendue.
    SecretSet,
    #[serde(other)]
    Unknown,
}

/// La valeur voulue d'un réglage de la fiche.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingValue {
    pub id: String,
    pub state: ValueState,
    /// La valeur voulue ; absente pour un secret et pour « non écrit ».
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Figé après la création du monde : affiché, jamais modifiable.
    #[serde(default)]
    pub frozen: bool,
    /// Imposé par la fiche : affiché, jamais modifiable.
    #[serde(default)]
    pub imposed: bool,
    /// N'existe plus dans la version du jeu : gardé et grisé, jamais supprimé (BR-FICHE-049).
    #[serde(default)]
    pub gone: bool,
    /// Ce qui se passe si on le change.
    pub effect: SettingEffect,
}

/// L'état du fichier de réglages du jeu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileState {
    Ok,
    /// Le fichier n'existe pas (le jeu l'a effacé, ou n'a jamais démarré).
    Missing,
    /// Le format ne sait pas le découper : rien n'y est retouché (`SETTINGS_FILE_UNREADABLE`).
    Unreadable,
    #[serde(other)]
    Unknown,
}

/// Un réglage de la fiche dont le fichier ne dit plus ce que Hearth croit (BR-FICHE-048). Jamais un secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriftItem {
    pub id: String,
    /// La valeur voulue par Hearth ; absente : « non écrit ».
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hearth: Option<String>,
    /// La valeur lue dans le fichier ; absente : la ligne n'y est pas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

/// Un réglage que la fiche ne connaît pas (section « Autres réglages », BR-FICHE-047).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OtherSetting {
    pub key: String,
    /// Absente quand le nom ressemble à un secret (PASSWORD, PASS, SECRET, TOKEN, KEY).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default)]
    pub masked: bool,
}

/// La fiche d'après laquelle ce service a été installé.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FicheRef {
    pub id: String,
    pub version: u32,
    pub digest: String,
}

/// Réponse de `GET /services/{key}/settings`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsView {
    /// La révision à renvoyer avec un enregistrement (`base_rev`) : `SETTINGS_CONFLICT` si elle a changé.
    pub rev: u32,
    pub fiche: FicheRef,
    pub file_state: FileState,
    /// Empreinte (SHA-256, hexadécimal) du fichier tel que lu ; à renvoyer dans `file_stamp`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_stamp: Option<String>,
    /// Le fichier n'est plus tel que Hearth l'a écrit en dernier (BR-FICHE-048).
    #[serde(default)]
    pub file_changed: bool,
    /// Le fichier brut peut être édité (faux : le jeu le réécrit à chaque démarrage).
    #[serde(default)]
    pub raw_edit: bool,
    pub settings: Vec<SettingValue>,
    #[serde(default)]
    pub drifts: Vec<DriftItem>,
    #[serde(default)]
    pub others: Vec<OtherSetting>,
    /// Réglages imposés que le fichier a défaits : signalés, jamais réécrits.
    #[serde(default)]
    pub imposed_broken: Vec<String>,
}

/// Corps de `POST /services/{key}/settings/analyze`. Le texte peut porter un mot de passe : il n'est ni gardé
/// ni journalisé, il est lu puis oublié.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalyzeRequest {
    pub text: String,
}

impl std::fmt::Debug for AnalyzeRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnalyzeRequest")
            .field("text_len", &self.text.len())
            .finish()
    }
}

/// L'état d'une ligne du texte collé (BR-FICHE-052).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportState {
    /// Reconnu : sera repris.
    Take,
    /// Déjà la valeur voulue.
    Same,
    /// Valeur que ce jeu ne connaît pas (hors choix, hors bornes, trop longue...) : `reason`.
    Rejected,
    /// Imposé par la fiche : ignoré.
    Imposed,
    /// Figé après la création du monde : ignoré.
    Frozen,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportLine {
    pub id: String,
    pub state: ImportState,
    /// La raison d'un refus (code court : `bounds`, `choice`, `charset`, `length`, `version`...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// La valeur collée ; absente pour un secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pasted: Option<String>,
    #[serde(default)]
    pub secret: bool,
}

/// Réponse de `POST /services/{key}/settings/analyze` : rien n'a été écrit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportView {
    pub lines: Vec<ImportLine>,
    /// Lignes que la fiche ne connaît pas : gardées dans le fichier, jamais reprises du texte.
    pub unknown: u32,
    pub duplicates: u32,
    /// Réglages imposés que le texte retire ou change (avertissement).
    #[serde(default)]
    pub imposed_undone: Vec<String>,
}

/// Le geste sur un réglage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeAction {
    /// Poser `value`.
    Set,
    /// « Non écrit » : retirer la ligne.
    Unset,
    /// Remettre la valeur proposée (ou retirer la ligne si la fiche n'en propose pas).
    Reset,
    /// Reprendre la valeur du texte collé (`import_text`) ; pour un secret collé, c'est le seul moyen de le poser
    /// sans qu'il revienne au client.
    FromImport,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingChange {
    pub id: String,
    pub action: ChangeAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

impl std::fmt::Debug for SettingChange {
    /// La valeur peut être un mot de passe : jamais dans un `Debug`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SettingChange")
            .field("id", &self.id)
            .field("action", &self.action)
            .field("has_value", &self.value.is_some())
            .finish()
    }
}

/// Un réglage inconnu de la fiche : changer sa valeur, ou le retirer du fichier (BR-FICHE-047).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OtherChange {
    pub key: String,
    /// `true` : retirer la ligne (après confirmation côté client).
    #[serde(default)]
    pub remove: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

impl std::fmt::Debug for OtherChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OtherChange")
            .field("key", &self.key)
            .field("remove", &self.remove)
            .field("has_value", &self.value.is_some())
            .finish()
    }
}

/// Le choix du compte pour un réglage en écart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftResolution {
    /// « Garder la valeur du fichier » (choix par défaut).
    KeepFile,
    /// « Remettre la mienne ».
    KeepMine,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriftChoice {
    pub id: String,
    pub choice: DriftResolution,
}

/// Corps de `PUT /services/{key}/settings` (acte `GameSettingsSave` `0x1E`). Le membre `reauth` de la
/// confirmation voyage à côté, lu par la couche de confirmation.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveSettingsRequest {
    /// La révision lue avec la vue.
    pub base_rev: u32,
    /// L'empreinte du fichier lue avec la vue : si le fichier a changé depuis, `SETTINGS_FILE_CHANGED`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_stamp: Option<String>,
    pub origin: SaveOrigin,
    /// Ce que le compte s'attend à devoir faire ensuite (signé) ; l'agent le recalcule et refuse s'il diffère.
    pub effect: SettingEffect,
    #[serde(default)]
    pub changes: Vec<SettingChange>,
    #[serde(default)]
    pub others: Vec<OtherChange>,
    #[serde(default)]
    pub drift_choices: Vec<DriftChoice>,
    /// Pour l'origine `import` : le texte collé, relu par l'agent pour les changements `from_import`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub import_text: Option<String>,
}

impl std::fmt::Debug for SaveSettingsRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SaveSettingsRequest")
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

/// Réponse de `PUT /services/{key}/settings`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveSettingsResponse {
    /// La nouvelle révision.
    pub rev: u32,
    /// Ce qu'il faut faire pour appliquer : la sorte la plus lourde des changements.
    pub effect: SettingEffect,
    /// Le fichier a été écrit (faux : rien n'a changé dans ses octets).
    pub file_written: bool,
    /// Nombre de réglages dont la valeur voulue a changé.
    pub changed: u32,
    /// Réglages dont on a repris la valeur du fichier (choix « Garder la valeur du fichier »).
    #[serde(default)]
    pub adopted: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_never_shows_a_value_in_its_debug_text() {
        let request = SaveSettingsRequest {
            base_rev: 3,
            file_stamp: None,
            origin: SaveOrigin::Form,
            effect: SettingEffect::Restart,
            changes: vec![SettingChange {
                id: "password".into(),
                action: ChangeAction::Set,
                value: Some("SENTINELLE-9f3a".into()),
            }],
            others: vec![OtherChange {
                key: "mod_secret".into(),
                remove: false,
                value: Some("SENTINELLE-9f3a".into()),
            }],
            drift_choices: Vec::new(),
            import_text: Some("password=SENTINELLE-9f3a".into()),
        };
        assert!(!format!("{request:?}").contains("SENTINELLE"));
        assert!(
            !format!(
                "{:?}",
                AnalyzeRequest {
                    text: "password=SENTINELLE-9f3a".into()
                }
            )
            .contains("SENTINELLE")
        );
    }

    #[test]
    fn unknown_words_from_a_newer_peer_are_tolerated() {
        let origin: SaveOrigin = serde_json::from_str("\"somewhere\"").expect("toléré");
        assert_eq!(origin, SaveOrigin::Unknown);
        let effect: SettingEffect = serde_json::from_str("\"reboot\"").expect("toléré");
        assert_eq!(effect, SettingEffect::Unknown);
        let state: ValueState = serde_json::from_str("\"mystery\"").expect("toléré");
        assert_eq!(state, ValueState::Unknown);
    }

    #[test]
    fn a_secret_setting_value_has_no_value_field_on_the_wire() {
        let secret = SettingValue {
            id: "password".into(),
            state: ValueState::SecretSet,
            value: None,
            frozen: false,
            imposed: false,
            gone: false,
            effect: SettingEffect::Restart,
        };
        let json = serde_json::to_string(&secret).expect("sérialisé");
        assert!(!json.contains("\"value\""), "{json}");
    }
}
