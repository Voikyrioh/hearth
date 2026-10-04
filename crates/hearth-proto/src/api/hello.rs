//! Réponse de `GET /api/v1/hello` : identité publique de l'agent, lisible sans authentification.

use serde::{Deserialize, Serialize};

/// Plage de versions d'interface acceptées par l'agent (bornes incluses).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiRange {
    pub min: u32,
    pub max: u32,
}

/// Identité de l'agent telle que présentée avant toute connexion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloResponse {
    /// Toujours `"hearth"` : permet au client de reconnaître un agent Hearth.
    pub product: String,
    pub agent_version: String,
    pub api: ApiRange,
    pub machine_name: String,
    /// Identifiant stable de l'installation, généré une seule fois.
    pub install_id: String,
    /// `true` si l'installation est gérée de l'extérieur (pas de mise à jour automatique).
    pub managed: bool,
    pub mac_addresses: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> HelloResponse {
        HelloResponse {
            product: "hearth".into(),
            agent_version: "0.1.0".into(),
            api: ApiRange { min: 1, max: 1 },
            machine_name: "forge".into(),
            install_id: "0123456789abcdef0123456789abcdef".into(),
            managed: false,
            mac_addresses: vec!["AA:BB:CC:DD:EE:FF".into()],
        }
    }

    #[test]
    fn serializes_with_the_documented_field_names() {
        let json = serde_json::to_value(sample()).expect("serialization");
        assert_eq!(
            json,
            serde_json::json!({
                "product": "hearth",
                "agent_version": "0.1.0",
                "api": { "min": 1, "max": 1 },
                "machine_name": "forge",
                "install_id": "0123456789abcdef0123456789abcdef",
                "managed": false,
                "mac_addresses": ["AA:BB:CC:DD:EE:FF"],
            })
        );
    }

    #[test]
    fn round_trips_through_json() {
        let text = serde_json::to_string(&sample()).expect("serialization");
        let back: HelloResponse = serde_json::from_str(&text).expect("deserialization");
        assert_eq!(back, sample());
    }
}
