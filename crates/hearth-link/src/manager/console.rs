//! La console d'un serveur de jeu et les joueurs connectés depuis le client (HRT-291, ADR-0067, `BR-CONSOLE-001` à
//! `011`) : décrire ce que la console permet, envoyer UNE commande, lire les joueurs connectés. Chemins construits ici :
//! la clé du service est vérifiée puis encodée ; jamais un chemin fourni par l'appelant.
//!
//! - **Décrire** (`GET …/console`) et **lire les joueurs** (`GET …/players`) sont des LECTURES : rien n'est envoyé au
//!   jeu par la première, la seconde passe par le cache de l'agent.
//! - **Envoyer** (`POST …/console`) est l'ACTE `ConsoleCommand` (`0x30`) : l'acte signé est reconstruit depuis le corps
//!   par `domain/act.rs` (la clé, l'empreinte de la ligne normalisée, la classe), jamais le texte. Jamais rejoué : un
//!   lien tombé avant la réponse donne `ResultUnknown` (la commande a peut-être été exécutée). Le texte d'une
//!   commande et la réponse du jeu ne sont ni gardés ni tracés.

use hearth_proto::admin_act::is_service_key;
use hearth_proto::api::console::{ConsoleRequest, ConsoleView};
use hearth_proto::api::players::PlayersView;

use super::parse::parse;
use super::services::key_segment;
use super::{ActionOutcome, ActionRequest, LinkManager};
use crate::domain::secret::Secret;
use crate::domain::server::ServerId;
use crate::error::{InputField, LinkError};
use crate::ports::transport::Method;

/// Le chemin de la console d'un service, ou `None` si `key` n'a pas la forme d'une clé de service.
pub fn console_path(key: &str) -> Option<String> {
    is_service_key(key).then(|| format!("/services/{}/console", key_segment(key)))
}

/// Le chemin des joueurs connectés.
pub fn players_path(key: &str) -> Option<String> {
    is_service_key(key).then(|| format!("/services/{}/players", key_segment(key)))
}

impl LinkManager {
    /// Ce que la console d'un service permet maintenant (`GET /services/{key}/console`) : rien n'est envoyé au jeu.
    pub async fn describe_console(
        &self,
        id: &ServerId,
        key: &str,
    ) -> Result<ConsoleView, LinkError> {
        let path = console_path(key).ok_or(LinkError::InvalidInput(InputField::Filter))?;
        let (target, token) = self.credentials(id)?;
        let body = self.typed_get(&target, &token, path).await?;
        parse(body, "console du service")
    }

    /// Les joueurs connectés d'un service (`GET /services/{key}/players`) : ouvert à tout compte, sans journal.
    pub async fn read_players(&self, id: &ServerId, key: &str) -> Result<PlayersView, LinkError> {
        let path = players_path(key).ok_or(LinkError::InvalidInput(InputField::Filter))?;
        let (target, token) = self.credentials(id)?;
        let body = self.typed_get(&target, &token, path).await?;
        parse(body, "joueurs connectés")
    }

    /// Envoie une commande (`POST …/console`, acte `ConsoleCommand`). `password` : le mot de passe de confirmation si
    /// l'agent l'exige (toujours pour une commande sensible, jamais gardé). La réponse de l'agent (`200` ou refus typé)
    /// est rendue telle quelle. Le corps porte le texte de la commande : il ne se journalise jamais.
    pub async fn send_console(
        &self,
        id: &ServerId,
        key: &str,
        request: &ConsoleRequest,
        password: Option<&Secret>,
    ) -> Result<ActionOutcome, LinkError> {
        let path = console_path(key).ok_or(LinkError::UnreadableAct)?;
        let body = serde_json::to_value(request)
            .map_err(|_| LinkError::Protocol("commande de console illisible".into()))?;
        let action = ActionRequest {
            method: Method::Post,
            path,
            body: Some(body),
        };
        self.execute_act(id, action, password).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::act::{Route, classify};
    use hearth_proto::api::console::CLASS_SENSITIVE;

    #[test]
    fn a_service_key_becomes_a_fixed_path_and_anything_else_never_leaves() {
        assert_eq!(
            console_path("c_0123abcd").as_deref(),
            Some("/services/c_0123abcd/console")
        );
        assert_eq!(
            players_path("c_0123abcd").as_deref(),
            Some("/services/c_0123abcd/players")
        );
        for bad in ["", "../x", "a/b", "c_ab/../x", "c_ab?x=1"] {
            assert!(console_path(bad).is_none(), "{bad}");
            assert!(players_path(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn the_signed_act_is_the_one_of_the_agent_and_never_carries_the_text() {
        let key = "h_01J9ZY0G3Q8M2K6W4T7V5N1B9D";
        let request = ConsoleRequest {
            command: "op Bram".into(),
            class: CLASS_SENSITIVE,
        };
        let body = serde_json::to_value(&request).expect("json");
        let path = console_path(key).expect("chemin");
        let Ok(Route::Act(act)) = classify("POST", &path, Some(&body)) else {
            panic!("acte attendu");
        };
        assert_eq!(act.target(), key);
        let expected = hearth_proto::admin_act::AdminAct::console_command(key.into(), &request);
        assert_eq!(act.params(), expected.params());
        assert!(
            act.params()
                .iter()
                .all(|part| !String::from_utf8_lossy(part).contains("Bram"))
        );
    }
}
