//! Schéma de `settings.toml` : thèmes et catalogue de réglages. Du contenu, pas de la sécurité : tout ce
//! qui touche au conteneur est dans `fiche.toml`. Format fermé comme lui.

use std::collections::BTreeMap;

use serde::Deserialize;

use super::super::value::Charset;
use super::fiche::Derive;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsFile {
    #[serde(default)]
    pub theme: Vec<Theme>,
    #[serde(default)]
    pub choice_set: BTreeMap<String, ChoiceSet>,
    #[serde(default)]
    pub preset: Vec<RawSetting>,
    #[serde(default)]
    pub setting: Vec<RawSetting>,
    #[serde(default)]
    pub retired: Vec<Retired>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Theme {
    pub id: String,
    pub label: String,
    pub order: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceSet {
    pub choices: Vec<Choice>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Essential,
    Common,
    Advanced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueType {
    Text,
    Secret,
    Int,
    Bool,
    Choice,
    IdList,
    Opaque,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretKind {
    GamePassword,
    AdminPassword,
    ServerToken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemCharset {
    Digits,
    Slug,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeWhen {
    Anytime,
    WorldCreation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Verified,
    Partial,
    NameOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArgForm {
    FlagValue,
    FlagIfTrue,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArgSpec {
    pub flag: String,
    pub form: ArgForm,
}

/// Valeur imposée : exactement une de `value`, `derive`, `secret`, `port`, avec la raison.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Imposed {
    pub value: Option<String>,
    pub derive: Option<Derive>,
    /// Identifiant d'un `[[secret]]` généré.
    pub secret: Option<String>,
    /// Identifiant d'un `[[port]]` : la valeur est le port du conteneur.
    pub port: Option<String>,
    pub why: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Versions {
    pub since: Option<String>,
    pub until: Option<String>,
    pub replaced_by: Option<String>,
}

/// Un réglage tel qu'écrit, ou un préréglage (mêmes champs, `key`, `label` et `help` absents). À plat :
/// les champs qui ne vont pas avec le type sont refusés par le chargeur.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawSetting {
    pub id: String,
    pub key: Option<String>,
    pub channel: Option<String>,
    pub arg: Option<ArgSpec>,
    pub theme: Option<String>,
    pub level: Option<Level>,
    pub label: Option<String>,
    pub help: Option<String>,
    #[serde(rename = "type")]
    pub value_type: Option<ValueType>,
    pub charset: Option<Charset>,
    pub max_len: Option<usize>,
    pub min_len: Option<usize>,
    pub secret_kind: Option<SecretKind>,
    pub obtain_url: Option<String>,
    pub min: Option<i64>,
    pub max: Option<i64>,
    pub on: Option<String>,
    pub off: Option<String>,
    pub choices: Option<Vec<Choice>>,
    pub choice_set: Option<String>,
    pub item_charset: Option<ItemCharset>,
    pub separator: Option<String>,
    pub max_items: Option<usize>,
    pub game_default: Option<String>,
    pub proposed: Option<String>,
    pub change: Option<ChangeWhen>,
    pub imposed: Option<Imposed>,
    pub versions: Option<Versions>,
    pub source: Option<Source>,
    pub only_variants: Option<Vec<String>>,
    pub preset: Option<String>,
}

/// Réglage que le jeu n'a plus : gardé avec sa raison.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Retired {
    pub id: String,
    pub key: String,
    pub channel: String,
    pub label: String,
    pub until: String,
    pub replaced_by: Option<String>,
    pub why: String,
}
