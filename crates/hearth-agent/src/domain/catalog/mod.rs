//! Fiches de serveur de jeu, format version 2 (HRT-113 ; ADR-0050, ADR-0051, ADR-0052).
//!
//! Une fiche décrit, en TOML fermé, comment faire tourner un serveur de jeu : l'image, les ports, les dossiers,
//! les canaux de réglages, l'arrêt, les mondes. Ce dossier est du DOMAINE PUR : aucune E/S, aucun moteur, aucune
//! base, aucune route. Les octets des fiches viennent de l'appelant (`build.rs` à la compilation, un jour un
//! catalogue distant signé).
//!
//! - `loading/` : schéma, chargeur, invariants I1 à I9, `ids.lock`. AUTONOME (`super::` seulement) : `build.rs` et
//!   `xtask` l'incluent par `#[path]` pour appliquer le chargeur réel ;
//! - `formats/` : `kv_lines` et `xml_properties`, lecture et retouche sans perte d'un fichier de réglages ;
//! - `settings_state` : état des réglages, validation d'un changement, effet déduit du canal, écart ;
//! - `readiness` : le signe « prêt » (sous-chaîne littérale) et les phases d'un serveur de jeu ;
//! - `stop_plan` : séquence d'arrêt bornée, un élément `untested` n'est jamais exécuté ;
//! - `translate` : fiche, variante, réponses, secrets, contrats donnent un `ServiceSpecDraft` ;
//! - `embedded` : les fiches embarquées par `build.rs`.

pub mod embedded;
pub mod formats;
pub mod generate;
pub mod loading;
pub mod readiness;
pub mod settings_state;
pub mod stop_plan;
pub mod translate;

pub use loading::error::{FicheRefusee, Fichier, Motif};
pub use loading::loader::{Origin, load};
pub use loading::model::Fiche;

#[cfg(test)]
mod architecture_guard;
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests_fiches;
#[cfg(test)]
mod tests_memory;
#[cfg(test)]
mod tests_state;
#[cfg(test)]
mod tests_translate;
