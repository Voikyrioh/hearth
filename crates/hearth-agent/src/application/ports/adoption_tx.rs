use async_trait::async_trait;
use time::OffsetDateTime;

use super::StoreError;
use crate::domain::services::ServiceKey;

/// Ce qu'on écrit pour adopter un service (`adopted_services`, HRT-80, ADR-0043).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAdoption {
    /// L'identité stable (`u_<unité>` ou `c_<identifiant>`), jamais un nom.
    pub key: ServiceKey,
    /// Le nom du conteneur à ce moment (journal d'activité de l'acte seulement).
    pub name: String,
    /// L'unité du système qui le lance : renseignée exactement quand la clé est `u_<unité>`.
    pub unit: Option<String>,
    /// Le nom du compte qui adopte, figé : le compte peut disparaître ensuite.
    pub by: String,
    pub at: OffsetDateTime,
}

/// Le résultat d'une écriture d'adoption : une seule des deux personnes qui adoptent en même temps gagne.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdoptionWrite {
    Adopted,
    /// Déjà adopté (par ce compte) : rien n'est écrit.
    AlreadyAdopted {
        by: String,
    },
}

/// Ce qu'une adoption relâchée laisse au journal : le nom retenu et l'unité.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleasedAdoption {
    pub name: String,
    pub unit: Option<String>,
}

/// L'ÉCRITURE de l'adoption, dans l'unité de travail : la ligne et l'entrée du journal d'activité de
/// l'acte réussissent ensemble ou pas du tout. Un seul cas d'usage l'appelle (`ServiceAdoption`), et un
/// seul fichier SQL écrit `adopted_services` (`infrastructure/sqlite/adoption_repo.rs`). Une ligne écrite
/// ne publie rien tant que `ServicesRegistry::reload_adopted` ne l'a pas lue.
#[async_trait]
pub trait AdoptionTx: Send {
    /// Adopte, sauf si la clé l'est déjà. Les écritures étant sérialisées (`BEGIN IMMEDIATE`), deux
    /// comptes qui adoptent en même temps n'adoptent qu'une fois.
    async fn adopt(&mut self, adoption: &NewAdoption) -> Result<AdoptionWrite, StoreError>;

    /// Relâche, et rend ce que la ligne retenait ; `None` si la clé n'était pas adoptée. La base oublie
    /// aussi ce qu'elle retenait du service (déclencheur de la migration `0011`).
    async fn release(&mut self, key: &ServiceKey) -> Result<Option<ReleasedAdoption>, StoreError>;
}
