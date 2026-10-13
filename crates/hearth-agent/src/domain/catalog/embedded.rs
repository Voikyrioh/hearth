//! Les fiches embarquées dans le binaire par `build.rs` (HRT-113). La table et les empreintes sont calculées à la
//! compilation par le chargeur RÉEL : une fiche qu'il refuse n'arrive jamais jusqu'ici, la compilation échoue.

use super::loading::error::FicheRefusee;
use super::loading::loader::{Origin, load};
use super::loading::model::{Fiche, fiche_digest};
use super::loading::schema::fiche::Status;

/// Une fiche telle qu'embarquée : ses deux fichiers et l'empreinte calculée à la compilation.
pub struct EmbeddedFiche {
    /// Nom du dossier, égal à l'identifiant de la fiche.
    pub dir: &'static str,
    pub fiche_toml: &'static [u8],
    pub settings_toml: &'static [u8],
    pub digest: [u8; 32],
}

impl EmbeddedFiche {
    /// Charge la fiche avec le chargeur courant (l'agent le refait à chaque démarrage).
    pub fn load(&self) -> Result<Fiche, FicheRefusee> {
        load(Origin::Embedded, self.fiche_toml, self.settings_toml)
    }

    /// Empreinte recalculée à l'exécution, pour la comparer à celle de la compilation.
    pub fn recomputed_digest(&self) -> [u8; 32] {
        fiche_digest(self.fiche_toml, self.settings_toml)
    }
}

include!(concat!(env!("OUT_DIR"), "/embedded_fiches.rs"));

/// Toutes les fiches embarquées, chargées. Une fiche refusée ne peut pas exister (la compilation échoue) ; un
/// chargeur durci plus tard peut en refuser une : elle est alors listée parmi les refus, jamais perdue en silence.
pub fn load_all() -> (Vec<Fiche>, Vec<(&'static str, FicheRefusee)>) {
    let mut accepted = Vec::new();
    let mut refused = Vec::new();
    for embedded in EMBEDDED {
        match embedded.load() {
            Ok(fiche) => accepted.push(fiche),
            Err(refusal) => refused.push((embedded.dir, refusal)),
        }
    }
    (accepted, refused)
}

/// Les fiches que l'application a le droit de PROPOSER : les fiches `published` seulement. Toute fiche est
/// embarquée dans le binaire, `preview` comprise (la publication n'est pas bloquée par une fiche en cours de
/// preuve), mais une fiche `preview` n'est jamais proposée ni installable : c'est la seule porte du catalogue
/// vers l'application, une fiche `preview` n'en sort pas.
pub fn offered() -> Vec<Fiche> {
    load_all()
        .0
        .into_iter()
        .filter(|fiche| fiche.file.status == Status::Published)
        .collect()
}
