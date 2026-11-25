//! Les joueurs connectés à un serveur de jeu (HRT-268, ADR-0067, BR-CONSOLE-011) : `GET /services/{key}/players`.
//!
//! Une consultation ouverte à tout compte authentifié, sans journal. Les nombres et les pseudos viennent de la réponse
//! d'un jeu hostile : l'agent les borne et ne rend jamais un chiffre deviné (`unavailable`).

use serde::{Deserialize, Serialize};

/// Ce que la lecture a donné.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayersState {
    /// Lu : `online`, `max` et `names` sont présents.
    Ok,
    /// Le service est arrêté.
    Stopped,
    /// Le jeu démarre.
    Starting,
    /// Illisible, injoignable, ou aucune lecture possible maintenant.
    Unavailable,
    /// Pas de console pour ce service (adopté, ou fiche sans console ouverte à Hearth).
    NoConsole,
    #[serde(other)]
    Unknown,
}

/// La réponse de `GET /services/{key}/players`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayersView {
    pub state: PlayersState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub online: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub names: Option<Vec<String>>,
    /// Des noms comptés mais pas rendus (hors de la forme d'un pseudo).
    #[serde(default)]
    pub hidden_names: u32,
    /// Des noms au-delà des cent gardés.
    #[serde(default)]
    pub more_names: u32,
    /// L'heure de la lecture (RFC 3339, UTC) ; absente hors `ok`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_at: Option<String>,
}

impl PlayersView {
    /// Une réponse sans chiffre.
    pub fn without_count(state: PlayersState) -> Self {
        Self {
            state,
            online: None,
            max: None,
            names: None,
            hidden_names: 0,
            more_names: 0,
            read_at: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_state_without_count_carries_no_figure() {
        let json = serde_json::to_value(PlayersView::without_count(PlayersState::Stopped)).unwrap();
        assert_eq!(json["state"], "stopped");
        for key in ["online", "max", "names", "read_at"] {
            assert!(json.get(key).is_none(), "{key}");
        }
    }

    #[test]
    fn an_unknown_state_is_read_unknown() {
        let state: PlayersState = serde_json::from_str("\"asleep\"").unwrap();
        assert_eq!(state, PlayersState::Unknown);
    }
}
