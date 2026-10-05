//! Un serveur du carnet et la dernière vue qu'on en a.

use std::fmt;

use hearth_proto::api::accounts::RoleName;
use hearth_proto::api::machine::MachineResponse;
use hearth_proto::api::metrics::Sample;
use hearth_proto::fingerprint::Fingerprint;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::time::WallTime;

/// Identifiant d'un serveur dans le carnet : 1 à 64 caractères, lettres, chiffres, tiret,
/// souligné. Sert de nom de fichier et de clé du coffre : aucun caractère de chemin.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ServerId(String);

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("identifiant de serveur invalide")]
pub struct InvalidServerId;

impl ServerId {
    pub fn parse(text: &str) -> Result<Self, InvalidServerId> {
        let valid = !text.is_empty()
            && text.len() <= 64
            && text
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if valid {
            Ok(Self(text.to_owned()))
        } else {
            Err(InvalidServerId)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ServerId {
    type Error = InvalidServerId;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<ServerId> for String {
    fn from(value: ServerId) -> Self {
        value.0
    }
}

impl fmt::Display for ServerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Une entrée du carnet de serveurs. Aucun secret ici : mot de passe et jeton sont au coffre.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerRecord {
    pub id: ServerId,
    pub name: String,
    pub color: String,
    pub host: String,
    pub port: u16,
    /// Empreinte confirmée par l'utilisateur (BR-CONN-002).
    #[serde(with = "fingerprint_hex")]
    pub fingerprint: Fingerprint,
    pub username: String,
    /// Le mot de passe est-il mémorisé au coffre (reconnexion silencieuse) ?
    pub remember: bool,
    /// Adresses MAC annoncées par l'agent (réveil à distance, plus tard).
    pub mac_addresses: Vec<String>,
    pub last_contact_at: Option<WallTime>,
    /// L'utilisateur s'est déconnecté : pas de reconnexion automatique au démarrage
    /// (BR-CONN-016). Le mot de passe mémorisé, lui, est conservé.
    #[serde(default)]
    pub signed_out: bool,
    /// Rôle du compte à la dernière connexion de l'utilisateur : l'interface ferme les écrans
    /// d'administration au rôle lecture seule. `None` tant qu'on ne s'est jamais connecté.
    #[serde(default)]
    pub role: Option<RoleName>,
}

mod fingerprint_hex {
    use hearth_proto::fingerprint::Fingerprint;
    use serde::{Deserialize, Deserializer, Serializer, de};

    pub fn serialize<S: Serializer>(value: &Fingerprint, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&value.to_hex())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Fingerprint, D::Error> {
        let text = String::deserialize(deserializer)?;
        Fingerprint::from_hex(&text).map_err(de::Error::custom)
    }
}

/// Les échantillons gardés pour la dernière vue (5 minutes à 1 Hz).
pub const HISTORY_CAP: usize = 300;

/// Dernière vue connue d'un serveur : de quoi afficher quelque chose, périmé, hors ligne
/// (BR-RESIL-007).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LastKnown {
    /// Quand le dernier message reçu a été reçu.
    pub at: WallTime,
    pub machine: Option<MachineResponse>,
    /// Plus ancien d'abord, au plus [`HISTORY_CAP`].
    pub history: Vec<Sample>,
}

impl LastKnown {
    pub fn latest(&self) -> Option<&Sample> {
        self.history.last()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_cannot_carry_path_characters() {
        assert!(ServerId::parse("01J9ZY0G3Q8M2K6W4T7V5N1B9D").is_ok());
        for bad in ["", "a/b", "..", "a\\b", "a b", "a.json", &"x".repeat(65)] {
            assert!(ServerId::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_record_round_trips_through_json_with_the_fingerprint_in_hex() {
        let record = ServerRecord {
            id: ServerId::parse("srv1").unwrap(),
            name: "Forge".into(),
            color: "#7aa2f7".into(),
            host: "forge.lan".into(),
            port: 7341,
            fingerprint: Fingerprint::from_bytes([0xAB; 32]),
            username: "marie".into(),
            remember: true,
            mac_addresses: vec!["AA:BB:CC:DD:EE:FF".into()],
            last_contact_at: Some(WallTime::from_millis(1_790_000_000_000)),
            signed_out: true,
            role: Some(RoleName::Readonly),
        };
        let text = serde_json::to_string(&record).unwrap();
        assert!(text.contains(&"ab".repeat(32)), "{text}");
        let back: ServerRecord = serde_json::from_str(&text).unwrap();
        assert_eq!(back, record);
    }

    #[test]
    fn a_record_with_a_bad_fingerprint_or_id_is_refused() {
        let text = r##"{"id":"a/b","name":"x","color":"#fff","host":"h","port":1,
            "fingerprint":"00","username":"u","remember":false,"mac_addresses":[],
            "last_contact_at":null}"##;
        assert!(serde_json::from_str::<ServerRecord>(text).is_err());
    }
}
