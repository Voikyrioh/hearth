//! Lecture des comptes d'un serveur (HRT-13) : UNE méthode typée, chemins fixes construits ici,
//! méthode toujours `GET`, aucune requête quelconque. Même garanties que le journal (cible épinglée,
//! jeton du coffre, rien hors « Connecté », erreurs typées dont le refus de rôle) : le contrôle
//! d'accès est celui de l'agent, jamais celui de la bibliothèque.
//!
//! Une lecture n'est pas une action : pas de clé d'opération, pas de suivi, jamais d'issue
//! « résultat inconnu » ; si le lien tombe elle échoue et l'interface la refait.

use hearth_proto::api::accounts::{AccountItem, AccountsResponse};
use hearth_proto::api::sessions::MeResponse;
use hearth_proto::lenient::Items;

use super::LinkManager;
use crate::domain::server::ServerId;
use crate::domain::services::agent_knows_service_rights;
use crate::error::LinkError;

const LIST_PATH: &str = "/accounts";
const ME_PATH: &str = "/me";

/// La liste des comptes et le compte de la session courante.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountsRead {
    /// Un compte illisible par cette version est écarté et compté (`Items::unreadable`).
    pub accounts: Items<AccountItem>,
    /// Identifiant de l'AGENT du compte de la session : « qui est moi » ne se décide jamais par une
    /// comparaison de texte avec la saisie de connexion.
    pub me: String,
    /// L'agent annonce `service-rights` (BR-GRANT-016) : sans cela, `rights` est absent de chaque compte et
    /// « Non autorisée » ne dirait rien de vrai.
    pub rights_known: bool,
}

impl LinkManager {
    /// Les comptes du serveur (administrateurs : l'agent répond `FORBIDDEN_ROLE` à un compte Lecture
    /// seule, rendu `LinkError::Rejected(Some(ForbiddenRole))`), puis le compte de la session. Le
    /// compte de la session n'est lu que si la liste l'a été : un refus n'en produit pas un second.
    pub async fn accounts_list(&self, id: &ServerId) -> Result<AccountsRead, LinkError> {
        let (target, token) = self.credentials(id)?;
        let list = self
            .typed_get(&target, &token, LIST_PATH.to_owned())
            .await?;
        let list: AccountsResponse = serde_json::from_value(list)
            .map_err(|e| LinkError::Protocol(format!("liste des comptes illisible : {e}")))?;
        let me = self.typed_get(&target, &token, ME_PATH.to_owned()).await?;
        let capabilities = self.agent_capabilities(&target).await?;
        let me: MeResponse = serde_json::from_value(me)
            .map_err(|e| LinkError::Protocol(format!("compte courant illisible : {e}")))?;
        if list.accounts.unreadable() > 0 {
            tracing::warn!(
                unreadable = list.accounts.unreadable(),
                "comptes illisibles par cette version, écartés"
            );
        }
        Ok(AccountsRead {
            accounts: list.accounts,
            me: me.account.id,
            rights_known: agent_knows_service_rights(&capabilities),
        })
    }
}
