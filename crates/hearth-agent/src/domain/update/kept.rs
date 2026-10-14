//! Une valeur du fil que l'agent relit de SON PROPRE disque, gardée telle qu'elle a été écrite
//! quand il ne la comprend pas (BR-COMPAT-001, FIX:01M4VR6Y4Q4AVQVT9FCME7Z2SB).
//!
//! `UpdateStep` et `UpdateOutcome` sont des types du fil, tolérants (`Unknown`) ; mais l'agent les écrit
//! aussi dans `update/last.json` et `update/state.json`, relus par l'ancien binaire après un retour
//! arrière. Relire une issue écrite par un agent plus récent en `Unknown` puis la réécrire détruirait
//! l'information d'origine : ici, la chaîne lue est conservée et réécrite à l'identique.

use std::collections::BTreeMap;
use std::fmt;

use serde::de::DeserializeOwned;
use serde::de::value::StrDeserializer;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Une énumération du fil qui a une variante de repli pour ce qu'elle ne connaît pas.
pub trait Tolerant: Copy + PartialEq + Serialize + DeserializeOwned {
    /// La variante de repli (`#[serde(other)] Unknown`).
    const UNKNOWN: Self;
}

impl Tolerant for hearth_proto::api::update::UpdateStep {
    const UNKNOWN: Self = Self::Unknown;
}

impl Tolerant for hearth_proto::api::update::UpdateOutcome {
    const UNKNOWN: Self = Self::Unknown;
}

impl Tolerant for hearth_proto::api::update::UpdateReason {
    const UNKNOWN: Self = Self::Unknown;
}

/// Les champs d'un objet du disque que cette version ne connaît pas (écrits par une version plus
/// récente), gardés tels quels et réécrits avec l'objet (BR-COMPAT-001 point 7 : rien de perdu).
///
/// À poser en `#[serde(flatten)]` sur chaque objet que l'agent relit PUIS réécrit (`last.json`,
/// `state.json` et son demandeur, `phase.json`). Les valeurs sont gardées en JSON brut : objets
/// imbriqués, listes, nombres, `null` repartent à l'identique (l'ORDRE des clés d'un objet n'est pas
/// gardé, les clés sont écrites triées ; la mise en forme du fichier change, jamais son contenu).
/// Un champ inconnu n'est jamais interprété, jamais tracé (seul leur nombre l'est : `Debug`).
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Extras(BTreeMap<String, serde_json::Value>);

impl Extras {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Un champ inconnu, par son nom (tests, diagnostic).
    pub fn get(&self, name: &str) -> Option<&serde_json::Value> {
        self.0.get(name)
    }
}

/// Le contenu d'un champ inconnu peut être n'importe quoi : seul leur nombre se montre.
impl fmt::Debug for Extras {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "Extras({} champs)", self.0.len())
    }
}

/// Une valeur lue du disque : connue, ou étrangère (texte d'origine gardé, jamais interprété).
#[derive(Clone, PartialEq, Eq)]
pub enum Kept<T> {
    Known(T),
    /// Écrite par une autre version : le texte exact, réécrit tel quel.
    Foreign(String),
}

impl<T: Tolerant> Kept<T> {
    /// Ce que le fil dit : la valeur, ou `Unknown` quand l'agent ne la comprend pas. Jamais une valeur
    /// connue à la place d'une valeur étrangère.
    pub fn wire(&self) -> T {
        match self {
            Self::Known(value) => *value,
            Self::Foreign(_) => T::UNKNOWN,
        }
    }

    pub fn is_foreign(&self) -> bool {
        matches!(self, Self::Foreign(_))
    }

    /// La valeur, seulement si elle est connue.
    pub fn known(&self) -> Option<T> {
        match self {
            Self::Known(value) => Some(*value),
            Self::Foreign(_) => None,
        }
    }
}

impl<T> From<T> for Kept<T> {
    fn from(value: T) -> Self {
        Self::Known(value)
    }
}

/// `record.outcome == UpdateOutcome::Succeeded` : une valeur étrangère n'est égale à aucune valeur connue.
impl<T: Tolerant> PartialEq<T> for Kept<T> {
    fn eq(&self, other: &T) -> bool {
        matches!(self, Self::Known(value) if value == other)
    }
}

impl<T: fmt::Debug> fmt::Debug for Kept<T> {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Known(value) => value.fmt(out),
            Self::Foreign(raw) => write!(out, "Foreign({raw:?})"),
        }
    }
}

impl<T: Tolerant> Serialize for Kept<T> {
    fn serialize<S: Serializer>(&self, out: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Known(value) => value.serialize(out),
            Self::Foreign(raw) => out.serialize_str(raw),
        }
    }
}

impl<'de, T: Tolerant> Deserialize<'de> for Kept<T> {
    fn deserialize<D: Deserializer<'de>>(input: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(input)?;
        let parsed = T::deserialize(StrDeserializer::<D::Error>::new(&raw))?;
        Ok(if parsed == T::UNKNOWN {
            Self::Foreign(raw)
        } else {
            Self::Known(parsed)
        })
    }
}

#[cfg(test)]
mod tests {
    use hearth_proto::api::update::{UpdateOutcome, UpdateStep};

    use super::*;

    #[test]
    fn a_known_value_round_trips_as_itself() {
        let kept: Kept<UpdateOutcome> = serde_json::from_str("\"rolled_back\"").unwrap();
        assert_eq!(kept, UpdateOutcome::RolledBack);
        assert_eq!(serde_json::to_string(&kept).unwrap(), "\"rolled_back\"");
    }

    #[test]
    fn a_value_from_the_future_is_kept_and_rewritten_verbatim() {
        let kept: Kept<UpdateOutcome> = serde_json::from_str("\"partly_applied\"").unwrap();
        assert!(kept.is_foreign());
        assert_eq!(kept.wire(), UpdateOutcome::Unknown);
        assert_ne!(kept, UpdateOutcome::Succeeded);
        assert_ne!(kept, UpdateOutcome::Failed);
        assert_eq!(serde_json::to_string(&kept).unwrap(), "\"partly_applied\"");
        let step: Kept<UpdateStep> = serde_json::from_str("\"etape_du_futur\"").unwrap();
        assert_eq!(step.known(), None);
        assert_eq!(serde_json::to_string(&step).unwrap(), "\"etape_du_futur\"");
    }

    #[test]
    fn the_word_unknown_itself_is_foreign_never_a_known_value() {
        let kept: Kept<UpdateOutcome> = serde_json::from_str("\"unknown\"").unwrap();
        assert!(kept.is_foreign());
        assert_eq!(serde_json::to_string(&kept).unwrap(), "\"unknown\"");
    }
}
