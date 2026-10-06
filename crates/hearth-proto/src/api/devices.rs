//! Postes de confiance de l'utilisateur : `GET /me/devices`, `DELETE /me/devices/{id}`
//! (HRT-22, BR-TRUST-004, 022).
//!
//! Aucune clé, aucune empreinte de clé, aucun défi : la liste ne montre que des noms, des dates et
//! une adresse.

use serde::{Deserialize, Serialize};

/// Postes reconnus au plus par compte (BR-TRUST-022) : le 9e n'est pas inscrit, sans éviction.
pub const MAX_DEVICES_PER_ACCOUNT: usize = 8;

/// Un poste de confiance du compte de l'appelant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceItem {
    /// Identifiant du poste (ULID), celui de `DELETE /me/devices/{id}`.
    pub id: String,
    /// Nom annoncé par le client à l'inscription (`X-Hearth-Client`, nettoyé).
    pub name: String,
    pub created_at: String,
    /// Dernière preuve de la clé (connexion ou ouverture du flux).
    pub last_proved_at: String,
    /// Adresse d'où le poste a prouvé sa clé pour la dernière fois.
    pub last_addr: String,
    /// C'est le poste de la session qui fait la requête (il ne se retire pas depuis lui-même).
    pub current: bool,
}

/// Réponse de `GET /me/devices`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevicesResponse {
    pub devices: Vec<DeviceItem>,
    pub max: usize,
}
