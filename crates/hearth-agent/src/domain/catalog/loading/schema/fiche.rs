//! Schéma de `fiche.toml` (format version 2) : la surface de sécurité d'une fiche, le conteneur et ce qu'il
//! fait. Chaque structure refuse tout champ inconnu (`deny_unknown_fields`) : le format est fermé par
//! l'analyse elle-même. La section 5.2 de la conception est une notation ; les formes exactes (tables en
//! ligne ou sous-tables) sont celles de ces structures.

use std::collections::BTreeMap;

use serde::Deserialize;

use super::super::value::FileFormat;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FicheFile {
    pub format: u32,
    pub id: String,
    pub version: u32,
    pub family: String,
    pub name: String,
    pub summary: String,
    pub status: Status,
    pub platform: Platform,
    pub image: Image,
    pub owner: Owner,
    #[serde(default)]
    pub variant: Vec<Variant>,
    #[serde(default)]
    pub folder: Vec<Folder>,
    #[serde(default)]
    pub port: Vec<Port>,
    #[serde(default)]
    pub channel: Vec<Channel>,
    #[serde(default)]
    pub env: Vec<EnvVar>,
    #[serde(default)]
    pub secret: Vec<GeneratedSecret>,
    pub remote_console: Option<RemoteConsole>,
    #[serde(default)]
    pub agreement: Vec<Agreement>,
    pub first_start: FirstStart,
    pub ready: Option<Ready>,
    pub game_version: Option<GameVersion>,
    pub stop: Stop,
    pub save: Option<Save>,
    pub resources: Resources,
    pub worlds: Option<Worlds>,
    #[serde(default)]
    pub not_exposed: Vec<NotExposed>,
    #[serde(default)]
    pub check: Vec<Check>,
    #[serde(default)]
    pub migration: Vec<Migration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Published,
    Preview,
}

/// `"linux"` ou `{ unavailable = "<raison>" }` (ARK Ascended : listé, non installable).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Platform {
    Name(PlatformName),
    Unavailable(PlatformUnavailable),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformName {
    Linux,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformUnavailable {
    pub unavailable: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameUpdates {
    Never,
    AtInstall,
    EveryStart,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Image {
    pub repository: String,
    pub tag: String,
    pub digest: Option<String>,
    pub game_updates: GameUpdates,
}

/// Propriétaire numérique des dossiers gérés créés.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Owner {
    pub uid: u32,
    pub gid: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Variant {
    pub id: String,
    pub label: String,
    pub help: String,
    pub default: bool,
    pub image_tag: Option<String>,
    pub image_digest: Option<String>,
    #[serde(default)]
    pub env: Vec<VariantEnv>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariantEnv {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FolderRole {
    World,
    GameFiles,
    Config,
    Logs,
    Mods,
    Data,
}

/// Un dossier géré du service. Aucun champ pour un chemin du serveur : la source est toujours
/// `<racine de données>/<service>/<id>` (I2).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Folder {
    pub id: String,
    pub role: FolderRole,
    pub target: String,
    pub read_only: bool,
    pub backup: bool,
    pub drop_hint: Option<String>,
    pub only_variants: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Tcp,
    Udp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortRole {
    Game,
    Query,
    Voice,
    WebAdmin,
    RemoteConsole,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Publish {
    Always,
    Optional,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortMapping {
    /// Port du conteneur fixe, seul le port du serveur change.
    Nat,
    /// Le jeu annonce son port : conteneur, serveur et réglage du jeu valent le même nombre.
    SameNumber,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Port {
    pub id: String,
    pub container: u16,
    pub protocol: Protocol,
    pub role: PortRole,
    pub publish: Publish,
    pub default_host: Option<u16>,
    pub group: Option<String>,
    pub mapping: PortMapping,
    /// Réglage (identifiant) qui porte le numéro de port pour le jeu, obligatoire en `same_number`.
    pub game_setting: Option<String>,
    pub only_variants: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelKind {
    Env,
    Args,
    File,
    /// Tout autre genre : toujours refusé (le message dit qu'aucun genre n'exécute de commande).
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum FileMode {
    #[serde(rename = "0600")]
    M0600,
    #[serde(rename = "0640")]
    M0640,
    #[serde(rename = "0644")]
    M0644,
}

impl FileMode {
    pub fn octal(self) -> u32 {
        match self {
            Self::M0600 => 0o600,
            Self::M0640 => 0o640,
            Self::M0644 => 0o644,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WrittenByGame {
    Never,
    AtStop,
    Anytime,
    Unverified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quote {
    None,
    Double,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KvOptions {
    pub separator: String,
    pub comment: String,
    pub quote: Quote,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct XmlOptions {
    pub root: String,
    pub element: String,
    pub key_attr: String,
    pub value_attr: String,
}

/// Un canal par lequel l'image reçoit un réglage. À plat : les champs d'un canal `file` sont refusés
/// pour un canal `env` ou `args` par le chargeur (`ChampIncoherent`).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Channel {
    pub id: String,
    pub kind: ChannelKind,
    pub format: Option<FileFormat>,
    pub folder: Option<String>,
    pub path: Option<String>,
    pub mode: Option<FileMode>,
    pub written_by_game: Option<WrittenByGame>,
    pub raw_edit: Option<bool>,
    pub kv: Option<KvOptions>,
    pub xml: Option<XmlOptions>,
    pub only_variants: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerKind {
    Uid,
    Gid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum When {
    Always,
    WorldImported,
    WorldNew,
}

/// Valeur construite d'un préfixe littéral, de la valeur d'un réglage et d'un suffixe littéral : rien d'autre.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Derive {
    pub prefix: Option<String>,
    pub from: String,
    pub suffix: Option<String>,
}

/// Variable d'environnement fixée par la fiche. Exactement une des quatre origines.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvVar {
    pub name: String,
    pub value: Option<String>,
    pub derive: Option<Derive>,
    /// Identifiant d'un `[[secret]]` généré.
    pub secret: Option<String>,
    pub owner: Option<OwnerKind>,
    pub when: Option<When>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretOrigin {
    Generated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretAlphabet {
    Alnum,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedSecret {
    pub id: String,
    pub origin: SecretOrigin,
    pub length: u32,
    pub alphabet: SecretAlphabet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteConsoleProtocol {
    SourceRcon,
    TelnetLine,
}

/// Console distante déclarée. Hearth ne s'y connecte pas dans ce format (ADR-0052, point 8).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteConsole {
    pub protocol: RemoteConsoleProtocol,
    pub port: String,
    pub password: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgreementBinds {
    pub channel: String,
    pub key: String,
    pub accepted_value: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Agreement {
    pub id: String,
    pub title: String,
    pub url: String,
    pub sentence: String,
    pub binds: AgreementBinds,
}

/// Preuve d'une ressource : `measured`, `measured_idle`, `unmeasured`, ou `{ publisher = "<adresse>" }`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Evidence {
    Plain(PlainEvidence),
    Publisher(PublisherEvidence),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlainEvidence {
    Measured,
    MeasuredIdle,
    Unmeasured,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublisherEvidence {
    pub publisher: String,
}

impl Evidence {
    pub fn is_unmeasured(&self) -> bool {
        matches!(self, Self::Plain(PlainEvidence::Unmeasured))
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Download {
    pub what: String,
    pub source_label: String,
    pub bytes: Option<u64>,
    pub evidence: Evidence,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage {
    pub label: String,
    pub log_contains: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FirstStart {
    pub ready_timeout_s: u32,
    pub world_generation: bool,
    #[serde(default)]
    pub download: Vec<Download>,
    #[serde(default)]
    pub stage: Vec<Stage>,
}

/// Statut de preuve d'un élément écrit avant d'avoir été essayé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Proof {
    Tested,
    Untested,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ready {
    pub log_contains: String,
    pub status: Proof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionScheme {
    DottedNumeric,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameVersion {
    pub after: String,
    pub until_any: Vec<String>,
    pub scheme: VersionScheme,
    pub status: Proof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum StopSignal {
    #[serde(rename = "image_default")]
    ImageDefault,
    #[serde(rename = "SIGTERM")]
    Sigterm,
    #[serde(rename = "SIGINT")]
    Sigint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldSaved {
    ByImageOnSignal,
    BySequence,
    NotSaved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    StdinLine,
    WaitLog,
    Wait,
    /// Tout autre genre : toujours refusé (I4, aucune exécution).
    #[serde(other)]
    Unknown,
}

/// Étape d'une séquence. À plat comme un canal : les champs d'un autre genre sont refusés par le chargeur.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub kind: StepKind,
    pub line: Option<String>,
    pub quits: Option<bool>,
    pub contains: Option<String>,
    pub timeout_s: Option<u32>,
    pub required: Option<bool>,
    pub seconds: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sequence {
    pub status: Proof,
    #[serde(default)]
    pub step: Vec<Step>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stop {
    pub grace_s: u32,
    pub signal: StopSignal,
    pub world_saved: WorldSaved,
    pub sequence: Option<Sequence>,
}

/// Enregistrer sans arrêter, avant une copie à chaud. Jamais d'étape `quits`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Save {
    pub status: Proof,
    #[serde(default)]
    pub step: Vec<Step>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CpuResource {
    pub min_millis: Option<u32>,
    pub recommended_millis: Option<u32>,
    pub evidence: Evidence,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryResource {
    pub min_bytes: Option<u64>,
    pub recommended_bytes: Option<u64>,
    pub evidence: Evidence,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiskResource {
    pub install_bytes: Option<u64>,
    pub evidence: Evidence,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resources {
    pub cpu: CpuResource,
    pub memory: MemoryResource,
    pub disk: DiskResource,
    /// Crans déclarés « Fluide », « Ultra », « Communautaire » (zéro à trois ; HRT-122).
    #[serde(default)]
    pub memory_tier: Vec<MemoryTier>,
    pub tiers: Option<Tiers>,
}

/// Un cran de mémoire déclaré. L'identifiant reste un texte pour que le refus soit typé (`CranInconnu`).
/// Aucun champ de texte libre : les libellés et les textes sont ceux de Hearth.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryTier {
    pub id: String,
    pub from_bytes: u64,
    pub evidence: Evidence,
}

/// Crans qui n'ont pas de sens pour ce jeu : jamais servis, même avec un chiffre.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tiers {
    #[serde(default)]
    pub hidden: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldUnit {
    File,
    Dir,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldBackups {
    pub dated_copies: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Worlds {
    pub folder: String,
    pub unit: WorldUnit,
    pub suffix: Option<String>,
    #[serde(default)]
    pub companions: Vec<String>,
    pub depth: Option<u32>,
    #[serde(default)]
    pub also: Vec<String>,
    pub names_setting: String,
    #[serde(default)]
    pub carried_by_world: Vec<String>,
    pub loaded_contains: Option<String>,
    pub backups: Option<WorldBackups>,
}

/// Réglage du jeu que la fiche ne règle pas, et pourquoi.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NotExposed {
    pub key: String,
    pub channel: String,
    pub why: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckIf {
    pub setting: String,
    pub equals: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckThen {
    pub setting: String,
    pub must_equal: Option<String>,
    pub max: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    #[serde(rename = "if")]
    pub condition: CheckIf,
    pub then: CheckThen,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapOp {
    pub id: String,
    pub values: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationOp {
    Rename([String; 2]),
    Retire(String),
    Map(MapOp),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Migration {
    pub from_version: u32,
    pub ops: Vec<MigrationOp>,
}
