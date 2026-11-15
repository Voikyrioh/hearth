//! La lecture typée d'une réponse de l'agent, commune à tous les modules de lecture du gestionnaire.

use crate::error::LinkError;

/// Lit une réponse typée. L'erreur de lecture ne recopie JAMAIS celle de `serde` : une erreur de type cite la valeur
/// fautive (le nom d'un conteneur, une valeur de réglage, un mot de passe).
pub(super) fn parse<T: serde::de::DeserializeOwned>(
    body: serde_json::Value,
    what: &str,
) -> Result<T, LinkError> {
    serde_json::from_value(body).map_err(|_| LinkError::Protocol(format!("{what} illisible")))
}
