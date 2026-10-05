use thiserror::Error;

use crate::domain::machine::MachineIdentity;
use crate::domain::metrics::SystemSample;

/// La sonde système n'a pas pu mesurer.
#[derive(Debug, Error)]
pub enum ProbeError {
    #[error("mesure du système impossible : {0}")]
    Unreadable(String),
}

/// Ce que la machine dit d'elle-même : processeur, mémoire, disques, réseau, sondes de
/// température. Les appels sont synchrones et peuvent lire le système : le cas d'usage les
/// exécute sur un fil dédié aux appels bloquants.
///
/// Une mesure illisible est absente de l'échantillon (liste vide, `None`) ; seule une panne
/// d'ensemble est une erreur.
pub trait SystemProbe: Send + Sync {
    /// Identité de la machine, sans les cartes graphiques (voir [`super::GpuProbe`]). Relue à
    /// chaque appel : les disques changent.
    fn identity(&self) -> MachineIdentity;

    /// Un échantillon, hors carte graphique. Le débit réseau est calculé entre deux appels.
    fn sample(&self) -> Result<SystemSample, ProbeError>;
}
