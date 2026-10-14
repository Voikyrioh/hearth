//! Lecture indulgente de ce que le CLIENT lit de l'agent (BR-COMPAT-001 point 10). Une seule règle :
//!
//! - un élément illisible d'une LISTE est écarté et COMPTÉ ([`Items::unreadable`]) : jamais avalé en silence,
//!   jamais fatal pour la liste ;
//! - un nombre absent, nul, du mauvais type ou hors des bornes de son type devient « inconnu » (`None` pour
//!   une valeur facultative, `NaN` pour une mesure, comme partout dans ce dépôt), JAMAIS borné en silence à une
//!   valeur plausible.
//!
//! Ne sert qu'à ce qu'un client lit d'un agent. Ce que l'AGENT lit du client reste strict (point 5).

use std::ops::{Deref, DerefMut};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

/// Une liste lue de l'autre côté : les éléments lisibles, et combien ne l'étaient pas (un élément d'une
/// version plus récente dont la forme a changé, ou un champ obligatoire absent). Sur le fil, c'est un tableau
/// ordinaire (rien ne change) ; le compte n'est jamais écrit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Items<T> {
    items: Vec<T>,
    unreadable: u32,
}

impl<T> Items<T> {
    /// Combien d'éléments de la liste reçue n'ont pas pu être lus par cette version.
    pub fn unreadable(&self) -> u32 {
        self.unreadable
    }

    /// Une liste dont on sait déjà combien d'éléments n'ont pas été lus (fusion de plusieurs pages).
    pub fn with_unreadable(items: Vec<T>, unreadable: u32) -> Self {
        Self { items, unreadable }
    }

    pub fn into_vec(self) -> Vec<T> {
        self.items
    }
}

impl<T> Default for Items<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            unreadable: 0,
        }
    }
}

impl<T> From<Vec<T>> for Items<T> {
    fn from(items: Vec<T>) -> Self {
        Self {
            items,
            unreadable: 0,
        }
    }
}

impl<T> FromIterator<T> for Items<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self::from(iter.into_iter().collect::<Vec<_>>())
    }
}

impl<T> Deref for Items<T> {
    type Target = Vec<T>;
    fn deref(&self) -> &Vec<T> {
        &self.items
    }
}

impl<T> DerefMut for Items<T> {
    fn deref_mut(&mut self) -> &mut Vec<T> {
        &mut self.items
    }
}

impl<T> IntoIterator for Items<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a Items<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

impl<T> PartialEq<Vec<T>> for Items<T>
where
    T: PartialEq,
{
    fn eq(&self, other: &Vec<T>) -> bool {
        self.items == *other
    }
}

impl<T: Serialize> Serialize for Items<T> {
    fn serialize<S: Serializer>(&self, out: S) -> Result<S::Ok, S::Error> {
        self.items.serialize(out)
    }
}

impl<'de, T: DeserializeOwned> Deserialize<'de> for Items<T> {
    fn deserialize<D: Deserializer<'de>>(input: D) -> Result<Self, D::Error> {
        // Le champ lui-même doit être un tableau : changer la FORME d'un champ n'est pas une valeur ajoutée,
        // c'est un autre protocole (BR-COMPAT-001 point 3).
        let raw = Vec::<Value>::deserialize(input)?;
        let mut items = Vec::with_capacity(raw.len());
        let mut unreadable = 0_u32;
        for value in raw {
            match serde_json::from_value::<T>(value) {
                Ok(item) => items.push(item),
                Err(_) => unreadable = unreadable.saturating_add(1),
            }
        }
        Ok(Self { items, unreadable })
    }
}

/// Une valeur facultative de 0 à 100 (`percent`) : absente, nulle, non entière, négative ou au-dessus de 100
/// elle est INCONNUE (`None`), jamais ramenée à une borne.
pub fn percent<'de, D: Deserializer<'de>>(input: D) -> Result<Option<u8>, D::Error> {
    let value = Option::<Value>::deserialize(input)?;
    Ok(value
        .and_then(|value| value.as_u64())
        .filter(|percent| *percent <= 100)
        .and_then(|percent| u8::try_from(percent).ok()))
}

/// Une mesure `f32` obligatoire : `null` (c'est ainsi que `serde_json` écrit `NaN`), un texte ou une valeur
/// hors de l'intervalle d'un `f32` sont une mesure INCONNUE (`NaN`), jamais zéro ni une borne.
pub fn measure<'de, D: Deserializer<'de>>(input: D) -> Result<f32, D::Error> {
    Ok(Option::<Value>::deserialize(input)?
        .and_then(|value| value.as_f64())
        .filter(|number| number.is_finite() && number.abs() <= f64::from(f32::MAX))
        .map_or(f32::NAN, |number| number as f32))
}

/// Une liste de mesures `f32` : chaque élément suit [`measure`].
pub fn measures<'de, D: Deserializer<'de>>(input: D) -> Result<Vec<f32>, D::Error> {
    Ok(Vec::<Option<Value>>::deserialize(input)?
        .into_iter()
        .map(|value| {
            value
                .and_then(|value| value.as_f64())
                .filter(|number| number.is_finite() && number.abs() <= f64::from(f32::MAX))
                .map_or(f32::NAN, |number| number as f32)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    use super::*;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Row {
        name: String,
    }

    #[derive(Deserialize)]
    struct Holder {
        rows: Items<Row>,
        #[serde(default, deserialize_with = "percent")]
        percent: Option<u8>,
        #[serde(deserialize_with = "measure")]
        cpu: f32,
        #[serde(deserialize_with = "measures")]
        cores: Vec<f32>,
    }

    fn read(value: serde_json::Value) -> Holder {
        serde_json::from_value(value).expect("lisible")
    }

    #[test]
    fn an_unreadable_row_is_dropped_and_counted_never_fatal_for_the_list() {
        let holder = read(json!({
            "rows": [{ "name": "a" }, { "nom": "b" }, 3, { "name": "c", "plus": 1 }],
            "percent": 5, "cpu": 1.5, "cores": [1.0]
        }));
        assert_eq!(holder.rows.len(), 2);
        assert_eq!(holder.rows.unreadable(), 2);
        assert_eq!(holder.rows[1].name, "c");
        // Sur le fil, rien d'autre qu'un tableau.
        assert_eq!(
            serde_json::to_value(&holder.rows).unwrap(),
            json!([{ "name": "a" }, { "name": "c" }])
        );
    }

    #[test]
    fn a_field_that_is_no_longer_a_list_is_still_refused() {
        let result = serde_json::from_value::<Items<Row>>(json!({ "rows": [] }));
        assert!(result.is_err());
    }

    #[test]
    fn a_percent_out_of_bounds_is_unknown_never_clamped() {
        for (raw, expected) in [
            (json!(42), Some(42)),
            (json!(100), Some(100)),
            (json!(101), None),
            (json!(300), None),
            (json!(-1), None),
            (json!(12.5), None),
            (json!("12"), None),
            (json!(null), None),
        ] {
            let holder = read(json!({
                "rows": [], "percent": raw, "cpu": 0.0, "cores": []
            }));
            assert_eq!(holder.percent, expected, "{raw}");
        }
        let holder = read(json!({ "rows": [], "cpu": 0.0, "cores": [] }));
        assert_eq!(holder.percent, None);
    }

    #[test]
    fn a_null_measure_is_not_a_number_never_zero() {
        let holder = read(json!({
            "rows": [], "cpu": null, "cores": [1.0, null, "x", 2.5]
        }));
        assert!(holder.cpu.is_nan());
        assert_eq!(holder.cores.len(), 4);
        assert!(holder.cores[1].is_nan() && holder.cores[2].is_nan());
        assert!((holder.cores[3] - 2.5).abs() < f32::EPSILON);
    }
}
