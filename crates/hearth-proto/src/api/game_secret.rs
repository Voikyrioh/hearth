//! Le mot de passe d'un serveur de jeu (HRT-103, BR-JEU-025 à BR-JEU-027) : `GET /services/{key}/game-secret`.
//!
//! Réservé aux comptes qui ont le droit « gérer les services » (BR-GRANT-004), lu dans la description chiffrée du
//! service (jamais dans le moteur ni dans le fichier du jeu), rendu sans cache, et chaque révélation est inscrite au
//! journal d'activité SANS la valeur (BR-JEU-026). Jamais un secret généré par Hearth (BR-JEU-039, BR-JEU-040).
//!
//! Évolution : tout champ ajouté plus tard est facultatif (`serde(default)`).

use serde::{Deserialize, Serialize};

use super::services::Concealed;

/// La réponse de `GET /services/{key}/game-secret`. Le `Debug` ne montre pas la valeur (`Concealed`) ; la réponse ne
/// se stocke pas (`Cache-Control: no-store`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameSecret {
    /// Le mot de passe que les joueurs tapent ; absent quand le serveur n'en a pas (BR-JEU-027 : toute personne qui
    /// peut joindre le port peut rejoindre la partie).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<Concealed>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_password_travels_as_json_and_never_shows_in_debug() {
        let reveal = GameSecret {
            password: Some(Concealed::new("SENTINEL-PW")),
        };
        assert!(!format!("{reveal:?}").contains("SENTINEL"));
        let json = serde_json::to_value(&reveal).expect("json");
        assert_eq!(json["password"], "SENTINEL-PW");
        let none = serde_json::to_value(GameSecret { password: None }).expect("json");
        assert!(none.get("password").is_none());
        let back: GameSecret = serde_json::from_value(json).expect("lu");
        assert_eq!(back, reveal);
        // Un champ d'une version plus récente est ignoré, un champ absent aussi.
        let future: GameSecret =
            serde_json::from_value(serde_json::json!({ "other": 1 })).expect("lu");
        assert_eq!(future.password, None);
    }
}
