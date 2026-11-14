//! Le quotidien d'un serveur de jeu depuis le client (HRT-104, `BR-JEU-017` à `BR-JEU-021`, `BR-JEU-025` à
//! `BR-JEU-027`) : `GET /services/{key}/game` (la vue : fiche, façon d'arrêter, ports publiés, mot de passe oui ou
//! non) et `GET /services/{key}/game-secret` (le mot de passe du serveur de jeu, réservé au droit « gérer les
//! services »). Deux LECTURES : aucun acte, aucune confirmation. Chemins construits ici : la clé du service est
//! vérifiée puis encodée par le code, jamais fournie comme chemin. Rien n'est gardé : la valeur du mot de passe est
//! rendue à l'appelant et à lui seul (`Concealed`, jamais dans un `Debug` ni une trace).

use hearth_proto::admin_act::is_service_key;
use hearth_proto::api::game_secret::GameSecret;
use hearth_proto::api::game_view::GameView;
use serde::de::DeserializeOwned;

use super::LinkManager;
use super::services::key_segment;
use crate::domain::server::ServerId;
use crate::error::{InputField, LinkError};

/// Le chemin de la vue d'un service de jeu, ou `None` si `key` n'a pas la forme d'une clé de service.
pub fn game_view_path(key: &str) -> Option<String> {
    is_service_key(key).then(|| format!("/services/{}/game", key_segment(key)))
}

/// Le chemin du mot de passe d'un service de jeu, ou `None` si `key` n'a pas la forme d'une clé de service.
pub fn game_secret_path(key: &str) -> Option<String> {
    is_service_key(key).then(|| format!("/services/{}/game-secret", key_segment(key)))
}

/// Lit une réponse typée. L'erreur de lecture ne recopie JAMAIS celle de `serde` : elle citerait la valeur fautive.
fn parse<T: DeserializeOwned>(body: serde_json::Value, what: &str) -> Result<T, LinkError> {
    serde_json::from_value(body).map_err(|_| LinkError::Protocol(format!("{what} illisible")))
}

impl LinkManager {
    /// La vue d'un service de jeu (`GET /services/{key}/game`). Un service qui n'est pas un service de fiche (ou un
    /// agent sans cette route) : `Rejected` (404). Hors « Connecté » : `NotConnected`. Une clé qui n'en est pas
    /// une : `InvalidInput`, rien ne part.
    pub async fn read_game_view(&self, id: &ServerId, key: &str) -> Result<GameView, LinkError> {
        let path = game_view_path(key).ok_or(LinkError::InvalidInput(InputField::Filter))?;
        let (target, token) = self.credentials(id)?;
        let body = self.typed_get(&target, &token, path).await?;
        parse(body, "vue du serveur de jeu")
    }

    /// Le mot de passe du serveur de jeu (`GET /services/{key}/game-secret`) : une LECTURE que l'agent réserve aux
    /// comptes qui ont le droit « gérer les services » et qu'il consigne au journal, sans la valeur. Rien n'est
    /// rejoué ni gardé : si le lien tombe, l'appel échoue et l'interface le refait. La réponse n'entre dans aucune
    /// trace, et une réponse illisible ne cite jamais son contenu.
    pub async fn reveal_game_password(
        &self,
        id: &ServerId,
        key: &str,
    ) -> Result<GameSecret, LinkError> {
        let path = game_secret_path(key).ok_or(LinkError::InvalidInput(InputField::Filter))?;
        let (target, token) = self.credentials(id)?;
        let body = self.typed_get(&target, &token, path).await?;
        parse(body, "mot de passe du serveur de jeu")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_service_key_becomes_one_encoded_segment_and_anything_else_never_leaves() {
        assert_eq!(
            game_view_path("h_0123abcd").as_deref(),
            Some("/services/h_0123abcd/game")
        );
        assert_eq!(
            game_secret_path("h_0123abcd").as_deref(),
            Some("/services/h_0123abcd/game-secret")
        );
        for bad in ["", "../x", "a/b", "h_ab/../game", "h_ab?x=1"] {
            assert!(game_view_path(bad).is_none(), "{bad}");
            assert!(game_secret_path(bad).is_none(), "{bad}");
        }
    }
}
