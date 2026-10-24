//! Lire le catalogue des fiches de serveurs de jeu (HRT-102, ADR-0050) : `GET /catalog` et `GET /catalog/{id}`.
//! Deux lectures, ouvertes à tout compte authentifié, sans acte ni journal. Chemins construits ici : l'identifiant
//! d'une fiche ne peut pas devenir un autre chemin. Installer depuis une fiche est le déploiement ordinaire
//! (`deploy_service`, membre `source`).

use hearth_proto::api::catalog::{CatalogView, FicheDetail};
use serde::de::DeserializeOwned;

use super::LinkManager;
use crate::domain::server::ServerId;
use crate::error::LinkError;

const CATALOG_PATH: &str = "/catalog";

/// Longueur maximale de l'identifiant d'une fiche (comme `MAX_FICHE_TOKEN_CHARS` du fil).
const FICHE_ID_MAX_LEN: usize = 64;

/// Un identifiant (de fiche, de question, de contrat) est un mot court : minuscules, chiffres, tiret et souligné ;
/// tout autre texte n'est pas un identifiant, ne part pas et n'est jamais répété. UNE seule définition, pour la
/// liaison et pour la coquille.
pub fn is_fiche_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= FICHE_ID_MAX_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// Le chemin du détail d'une fiche, ou `None` si l'identifiant n'en est pas un.
pub fn fiche_path(id: &str) -> Option<String> {
    is_fiche_id(id).then(|| format!("{CATALOG_PATH}/{id}"))
}

fn parse<T: DeserializeOwned>(body: serde_json::Value, what: &str) -> Result<T, LinkError> {
    serde_json::from_value(body).map_err(|_| LinkError::Protocol(format!("{what} illisible")))
}

impl LinkManager {
    /// Les fiches que CET agent propose (`GET /catalog`), plus le nombre de celles qu'il a écartées. Hors
    /// « Connecté » : `NotConnected`.
    pub async fn read_catalog(&self, id: &ServerId) -> Result<CatalogView, LinkError> {
        let (target, token) = self.credentials(id)?;
        let body = self
            .typed_get(&target, &token, CATALOG_PATH.to_owned())
            .await?;
        parse(body, "catalogue")
    }

    /// Le détail d'une fiche (`GET /catalog/{id}`) avec son empreinte. Fiche inconnue de cet agent :
    /// `Rejected(Some(FicheUnknown))`. Un identifiant qui n'en est pas un : `InvalidInput`, rien ne part.
    pub async fn read_fiche(&self, id: &ServerId, fiche: &str) -> Result<FicheDetail, LinkError> {
        let path =
            fiche_path(fiche).ok_or(LinkError::InvalidInput(crate::error::InputField::Fiche))?;
        let (target, token) = self.credentials(id)?;
        let body = self.typed_get(&target, &token, path).await?;
        parse(body, "fiche")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fiche_identifier_never_becomes_another_path() {
        assert_eq!(fiche_path("terraria").as_deref(), Some("/catalog/terraria"));
        assert_eq!(
            fiche_path("7-days_to_die").as_deref(),
            Some("/catalog/7-days_to_die")
        );
        for bad in ["", "../x", "a/b", "A", "a b", "a?x=1", &"a".repeat(65)] {
            assert!(fiche_path(bad).is_none(), "{bad}");
        }
    }
}
