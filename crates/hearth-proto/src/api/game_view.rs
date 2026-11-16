//! Ce que le détail d'un serveur de jeu montre au quotidien (HRT-104, `BR-JEU-017` à `BR-JEU-021`) :
//! `GET /services/{key}/game`.
//!
//! Une consultation ouverte à tout compte authentifié qui peut voir le service (un service créé par Hearth se lit de
//! tous, `BR-ADOPT-016`). Aucun secret : le mot de passe n'est ici qu'un « oui » ou un « non » (`has_password`) ; sa
//! valeur ne se lit que par [`super::game_secret`]. Un service qui n'est pas un serveur de jeu créé depuis une fiche
//! (adopté, ordinaire, fiche inconnue de cette construction) répond `404 NOT_A_GAME_SERVICE` (`BR-JEU-001`).
//!
//! Évolution : tout champ ajouté plus tard est facultatif (`serde(default)`).

use serde::{Deserialize, Serialize};

use super::catalog::StopView;

/// Un port publié par le serveur de jeu, tel que le service le tient (et non tel que la fiche le propose).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GamePort {
    /// Le numéro à ouvrir sur la box : celui du serveur.
    pub host_port: u16,
    /// `tcp`, `udp` ou `sctp`.
    pub protocol: String,
    /// `game`, `query`, `voice`, `web_admin`, `remote_console` ou `other` ; vide si la fiche ne reconnaît plus ce port.
    #[serde(default)]
    pub role: String,
    /// Le numéro diffère de celui que propose la fiche (`BR-JEU-019`).
    #[serde(default)]
    pub changed: bool,
}

/// La réponse de `GET /services/{key}/game`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameView {
    /// L'identifiant de la fiche dont le service est né.
    pub fiche: String,
    /// Le nom du jeu, tel que la fiche l'écrit (« Terraria »).
    pub name: String,
    /// La version de la fiche au moment de la création du service.
    pub fiche_version: u32,
    /// La façon dont ce jeu s'arrête (`BR-JEU-002`), lue dans la fiche de CETTE construction.
    pub stop: StopView,
    /// Les ports publiés, dans l'ordre de la description du service.
    #[serde(default)]
    pub ports: Vec<GamePort>,
    /// Le serveur a un mot de passe de jeu (`BR-JEU-027`) ; jamais sa valeur.
    #[serde(default)]
    pub has_password: bool,
    /// L'adresse que le propriétaire donne à ses joueurs (domaine ou IP publique, sans protocole ni port) ; absente
    /// tant qu'il n'en a pas renseigné (`BR-JEU-022`). Un changement passe par `PUT /services/{key}/spec`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_address: Option<String>,
    /// L'adresse de la machine sur le réseau maison (HRT-223) : pour les joueurs chez vous. Absente si l'agent ne la
    /// connaît pas ; ce n'est jamais une adresse publique.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lan_address: Option<String>,
    /// L'adresse d'internet que la box du foyer a dite à l'agent (UPnP, HRT-223), sans service tiers : absente si la
    /// box ne répond pas. L'adresse renseignée par le propriétaire (`public_address`) passe devant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detected_address: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::catalog::WorldSavedView;

    #[test]
    fn a_view_travels_as_json_and_tolerates_later_fields() {
        let view = GameView {
            fiche: "terraria".into(),
            name: "Terraria".into(),
            fiche_version: 2,
            stop: StopView {
                grace_s: 30,
                world_saved: WorldSavedView::BySequence,
                sequence_tested: false,
            },
            ports: vec![GamePort {
                host_port: 7777,
                protocol: "tcp".into(),
                role: "game".into(),
                changed: false,
            }],
            has_password: true,
            public_address: Some("jeu.example.org".into()),
            lan_address: Some("192.168.1.20".into()),
            detected_address: Some("203.0.113.9".into()),
        };
        let json = serde_json::to_value(&view).expect("json");
        assert_eq!(json["stop"]["world_saved"], "by_sequence");
        let back: GameView = serde_json::from_value(json.clone()).expect("lu");
        assert_eq!(back, view);
        let mut future = json;
        future["something_new"] = serde_json::json!(1);
        future["ports"][0]["also_new"] = serde_json::json!("x");
        let read: GameView = serde_json::from_value(future).expect("tolérant");
        assert_eq!(read, view);
    }

    #[test]
    fn missing_optional_fields_read_as_empty() {
        let read: GameView = serde_json::from_value(serde_json::json!({
            "fiche": "terraria", "name": "Terraria", "fiche_version": 1,
            "stop": { "grace_s": 10 }
        }))
        .expect("lu");
        assert!(read.ports.is_empty());
        assert!(!read.has_password);
    }
}
