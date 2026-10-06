//! Ce que la coquille dit à l'interface sur la mise à jour du client (types sérialisés, typés par
//! tauri-specta). Pur. Dates en millisecondes depuis l'époque (`f64` : un `i64` n'existe pas côté
//! web), compteurs en `u32`. L'interface ne décide rien : `banner_visible` est calculé ici.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Nom de l'événement Tauri qui porte l'état complet à chaque changement.
pub const STATE_EVENT: &str = "update://state";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePhase {
    Idle,
    /// Interrogation du flux de versions.
    Checking,
    /// Téléchargement de l'installateur (signature vérifiée à la fin, avant toute écriture).
    Downloading,
    /// Installateur lancé : le client va se fermer et se relancer.
    Installing,
}

/// Pourquoi la dernière mise à jour demandée n'a pas abouti. La version en cours reste utilisable
/// dans tous les cas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum UpdateFailure {
    /// BR-UPDATE-009 : coupure ; relançable.
    Interrupted,
    /// BR-UPDATE-010 : signature ou contenu refusés.
    Corrupted,
    /// Autre échec.
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AvailableDto {
    pub version: String,
    /// Texte brut (jamais du HTML) ; vide si la release n'en porte pas.
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStateDto {
    /// Croît à chaque publication : l'interface écarte tout état plus ancien que le dernier vu.
    pub seq: u32,
    pub current_version: String,
    pub phase: UpdatePhase,
    /// Avancement du téléchargement, 0 à 100 ; absent hors téléchargement.
    pub progress: Option<u8>,
    pub available: Option<AvailableDto>,
    /// Le bandeau « Nouvelle version disponible » doit être affiché (BR-UPDATE-003, 006).
    pub banner_visible: bool,
    /// Le bandeau reparaît à cette date (« Plus tard »).
    pub postponed_until: Option<f64>,
    /// Dernière vérification qui a obtenu une réponse (BR-UPDATE-007).
    pub last_checked_at: Option<f64>,
    /// La dernière vérification a réussi et rien de plus récent n'existe : « Tu es à jour ».
    pub up_to_date: bool,
    pub failure: Option<UpdateFailure>,
}
