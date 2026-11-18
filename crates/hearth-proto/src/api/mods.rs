//! Les mods d'un serveur de jeu installé (HRT-256, ADR-0062, BR-MOD-020 à 033) : `GET /services/{key}/mods`.
//!
//! La liste est lue dans le volume du service, écrit par le jeu et par ses mods : ce que l'agent rend a déjà passé
//! une forme (identifiants, versions, noms) ou n'est pas rendu. Aucun chemin, aucun nom d'entrée d'archive.
//!
//! Évolution : tout champ ajouté plus tard est facultatif (`serde(default)`), toute valeur d'énumération inconnue
//! se lit `Unknown`.

use serde::{Deserialize, Serialize};

/// Ce que le serveur fait des mods (BR-MOD-030).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Applicable {
    /// Le moteur charge des mods `.jar` : la liste suit.
    Mods,
    /// Vanilla : aucun mod.
    NoMods,
    /// Paper, Purpur : des greffons, pas des mods.
    Plugins,
    /// Le jeu n'offre pas cette voie.
    NoWay,
    #[default]
    #[serde(other)]
    Unknown,
}

/// D'où vient la version de Minecraft qui tourne (BR-MOD-021).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MinecraftFrom {
    /// Le réglage de la description.
    Setting,
    /// La ligne de démarrage de `logs/latest.log`.
    Log,
    /// Le manifeste du pack Modrinth.
    ModrinthManifest,
    /// Rien de lisible : la version n'est jamais devinée.
    #[default]
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PackKind {
    None,
    ServerPack,
    ModrinthPack,
    #[default]
    #[serde(other)]
    Unknown,
}

/// Le jeu tel que l'agent le comprend pour juger les mods.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ModsGame {
    /// `fabric`, `quilt`, `forge`, `neoforge` ; absent si le moteur n'est pas connu.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loader: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minecraft: Option<String>,
    #[serde(default)]
    pub minecraft_from: MinecraftFrom,
    /// Vrai quand le serveur prend la dernière version à chaque démarrage (`LATEST`).
    #[serde(default)]
    pub updates_every_start: bool,
    #[serde(default)]
    pub pack: PackKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModPlace {
    /// Dans le dossier que le jeu lit.
    Active,
    /// Dans « Mods retirés », jamais lu par le jeu.
    Retired,
    #[default]
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModOrigin {
    ServerPack,
    Hearth,
    ModrinthPack,
    Manual,
    #[default]
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentView {
    Both,
    Server,
    Client,
    #[default]
    #[serde(other)]
    Unknown,
}

/// Ce que le mod dit de lui-même, chaque valeur ayant passé une forme.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModIdentityView {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub loader: String,
    /// Vrai quand le mod déclare une plage de Minecraft que l'agent comprend.
    #[serde(default)]
    pub minecraft_declared: bool,
    #[serde(default)]
    pub environment: EnvironmentView,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VerdictCode {
    Compatible,
    Unverified,
    ToComplete,
    IncompatibleLoader,
    IncompatibleMinecraft,
    Duplicate,
    Unrecognized,
    #[default]
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModVerdictView {
    pub code: VerdictCode,
    /// Pour `to_complete` : les identifiants de mods manquants.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
    /// Pour un mod qu'un autre mod demande : les identifiants de ces mods.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needed_by: Vec<String>,
}

/// Un fichier du dossier des mods ou de « Mods retirés ».
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModItem {
    /// Le nom du fichier (alphabet sûr) ; absent quand `readable_name` est faux : le fichier se montre par son rang.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// L'ordre du fichier dans son dossier.
    pub rank: u32,
    pub place: ModPlace,
    /// Faux pour un fichier `.jar.disabled`.
    pub enabled: bool,
    pub origin: ModOrigin,
    /// Faux : nom hors de l'alphabet sûr, jamais rendu ni manipulable.
    pub readable_name: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<ModIdentityView>,
    pub verdict: ModVerdictView,
}

/// La réponse de `GET /services/{key}/mods`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModsView {
    pub applicable: Applicable,
    #[serde(default)]
    pub game: ModsGame,
    #[serde(default)]
    pub mods: Vec<ModItem>,
    /// Jars non lus faute de temps : le client relit.
    #[serde(default)]
    pub unread: u32,
    /// Le dossier compte plus de fichiers que la borne : le reste n'est pas listé.
    #[serde(default)]
    pub truncated: bool,
}

// ---------------------------------------------------------------------------------------------------------------
// Contrôler des dépôts (HRT-257) : `POST /services/{key}/mods/check`
// ---------------------------------------------------------------------------------------------------------------

/// Le corps de `POST /services/{key}/mods/check` : 1 à 8 identifiants de dépôts de la zone d'attente du compte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModsCheckRequest {
    pub deposits: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CheckOutcome {
    /// Le mod s'ajoute.
    Add,
    /// Le mod remplace un mod installé (l'ancien part dans « Mods retirés »).
    Replace,
    /// Le dépôt est refusé et retiré de la zone d'attente.
    Refused,
    #[default]
    #[serde(other)]
    Unknown,
}

/// Le fichier qu'un dépôt remplace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModReplaces {
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Les deux valeurs d'un refus de moteur ou de version : celle du mod et celle du serveur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RefusalValues {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loader: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_loader: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_minecraft: Option<String>,
}

/// Le jugement d'un dépôt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModCheckFile {
    pub deposit: String,
    pub outcome: CheckOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<ModIdentityView>,
    /// Le nom sous lequel Hearth poserait le mod, choisi par Hearth (jamais le nom envoyé).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaces: Option<ModReplaces>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<ModVerdictView>,
    /// Pour un dépôt refusé : `MOD_UNRECOGNIZED`, `MOD_ARCHIVE_REFUSED`, `MOD_TOO_LARGE`, `MOD_INCOMPATIBLE_LOADER`,
    /// `MOD_INCOMPATIBLE_MINECRAFT`, `MOD_DUPLICATE_IN_BATCH`, `MOD_NAME_TAKEN`, `MOD_IN_MODRINTH_PACK`,
    /// `MOD_SAME_VERSION`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refusal_values: Option<RefusalValues>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModsCheckView {
    pub files: Vec<ModCheckFile>,
}

// ---------------------------------------------------------------------------------------------------------------
// La liste des mods d'une sauvegarde (HRT-293) : `GET /services/{key}/backups/{id}/mods`
// ---------------------------------------------------------------------------------------------------------------

/// L'état de la liste des mods d'une sauvegarde (BR-MOD-034).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModsListState {
    /// Liste complète.
    Recorded,
    /// Des jars n'ont pas pu être lus à temps : leurs entrées n'ont pas d'identité.
    Partial,
    /// La lecture des mods a échoué : la sauvegarde est bonne, sa liste est vide.
    Unreadable,
    /// Le dossier des mods est un élément ignoré : il n'est pas dans la sauvegarde.
    NotInBackup,
    /// Sauvegarde d'avant la 0.2.2 : aucune liste, jamais reconstituée.
    NotRecorded,
    #[default]
    #[serde(other)]
    Unknown,
}

/// Où est un fichier dans une sauvegarde.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BackupModPlace {
    Active,
    Disabled,
    Retired,
    #[default]
    #[serde(other)]
    Unknown,
}

/// Le moteur et la version de Minecraft du serveur au moment de la sauvegarde.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BackupModsGame {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loader: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minecraft: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BackupModsCounts {
    pub active: u32,
    pub disabled: u32,
    pub retired: u32,
    pub not_listed: u32,
}

/// Un fichier de la sauvegarde.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupModItem {
    /// Absent : nom hors de l'alphabet sûr, montré par son rang.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    pub rank: u32,
    pub place: BackupModPlace,
    pub origin: ModOrigin,
    /// Absente : « Non reconnu dans cette sauvegarde ».
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<ModIdentityView>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKindView {
    Added,
    Removed,
    VersionChanged,
    Enabled,
    Disabled,
    #[default]
    #[serde(other)]
    Unknown,
}

/// Un écart entre la sauvegarde et les mods actuels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModChange {
    pub kind: ChangeKindView,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_version: Option<String>,
}

/// La comparaison avec les mods actuels (`compare=current`), calculée par l'agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupModsCompare {
    /// Faux tant que la liste actuelle a des jars non lus : le client relit.
    pub complete: bool,
    pub unread: u32,
    /// Rien n'a changé.
    pub same: bool,
    pub changes: Vec<ModChange>,
}

/// La réponse de `GET /services/{key}/backups/{id}/mods`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupModsView {
    pub state: ModsListState,
    /// Date de l'enregistrement, RFC 3339 en UTC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recorded_at: Option<String>,
    #[serde(default)]
    pub game: BackupModsGame,
    #[serde(default)]
    pub counts: BackupModsCounts,
    #[serde(default)]
    pub mods: Vec<BackupModItem>,
    /// Absente sans `compare=current`, ou quand la lecture actuelle est bloquée.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compare: Option<BackupModsCompare>,
}

/// Le résumé joint à chaque sauvegarde de `GET /services/{key}/backups` (sans les entrées).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupModsSummary {
    pub state: ModsListState,
    pub active: u32,
    pub disabled: u32,
    pub retired: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_view_travels_as_json_and_tolerates_later_values() {
        let view = ModsView {
            applicable: Applicable::Mods,
            game: ModsGame {
                loader: Some("fabric".into()),
                minecraft: Some("1.20.1".into()),
                minecraft_from: MinecraftFrom::Setting,
                updates_every_start: false,
                pack: PackKind::None,
            },
            mods: vec![ModItem {
                file: Some("a.jar".into()),
                rank: 0,
                place: ModPlace::Active,
                enabled: true,
                origin: ModOrigin::Manual,
                readable_name: true,
                identity: None,
                verdict: ModVerdictView {
                    code: VerdictCode::Unrecognized,
                    missing: vec![],
                    needed_by: vec![],
                },
            }],
            unread: 0,
            truncated: false,
        };
        let json = serde_json::to_value(&view).expect("json");
        assert_eq!(json["applicable"], "mods");
        assert_eq!(json["mods"][0]["verdict"]["code"], "unrecognized");
        let back: ModsView = serde_json::from_value(json.clone()).expect("lu");
        assert_eq!(back, view);
        let mut future = json;
        future["applicable"] = serde_json::json!("something_new");
        future["mods"][0]["origin"] = serde_json::json!("elsewhere");
        let read: ModsView = serde_json::from_value(future).expect("tolérant");
        assert_eq!(read.applicable, Applicable::Unknown);
        assert_eq!(read.mods[0].origin, ModOrigin::Unknown);
    }
}
