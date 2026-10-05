//! Reconnaître un agent Hearth (BR-CONN-012) : aucun identifiant n'est envoyé à un serveur qui
//! n'en est pas un.

use hearth_proto::product::PRODUCT_NAME;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("ce serveur n'est pas un agent Hearth")]
pub struct NotAnAgent;

/// `product` est le champ du même nom de la réponse de `/hello`.
pub fn check_product(product: &str) -> Result<(), NotAnAgent> {
    if product == PRODUCT_NAME {
        Ok(())
    } else {
        Err(NotAnAgent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_hearth_product_name_is_an_agent() {
        assert_eq!(check_product("hearth"), Ok(()));
        for other in ["", "Hearth", "hearth ", "nginx", "hearth-agent"] {
            assert_eq!(check_product(other), Err(NotAnAgent), "{other:?}");
        }
    }
}
