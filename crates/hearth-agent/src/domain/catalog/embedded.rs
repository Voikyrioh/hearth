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
    /// L'illustration originale d'un jeu absent de Steam, jugée et embarquée par `build.rs` (BR-FICHE-062).
    pub illustration: Option<Illustration>,
}

/// Une illustration embarquée : les octets du JPEG et leur empreinte SHA-256, calculée à la compilation. Ni l'un ni
/// l'autre n'entre dans l'empreinte de la fiche (`fiche_digest`).
pub struct Illustration {
    pub bytes: &'static [u8],
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

/// Le catalogue d'un agent : les fiches LISTÉES (publiées et en cours de preuve) et les fiches PROPOSÉES (détail,
/// installation). Un agent de publication liste ses fiches `preview` avec leur raison sans les proposer ; une
/// construction d'essai (`include_preview`, HRT-101, BR-FICHE-002) les propose aussi.
pub struct Partition {
    /// Les fiches que l'application a le droit de PROPOSER (détail, installation, mondes).
    pub offered: Vec<Fiche>,
    /// Les fiches que la liste montre : toutes les fiches embarquées chargées.
    pub listed: Vec<Fiche>,
}

/// Sépare les fiches listées des fiches proposées. Fonction pure : la porte est un paramètre, jamais une lecture
/// d'environnement. C'est le SEUL endroit qui applique cette porte (`CatalogService` l'appelle, les tests aussi).
pub fn partition(fiches: Vec<Fiche>, include_preview: bool) -> Partition {
    let offered = fiches
        .iter()
        .filter(|fiche| include_preview || fiche.file.status == Status::Published)
        .cloned()
        .collect();
    Partition {
        offered,
        listed: fiches,
    }
}

/// Les fiches embarquées proposées par une construction qui a (ou n'a pas) la porte d'essai (tests seulement :
/// l'application passe par `CatalogService`).
#[cfg(test)]
pub fn offered_with(include_preview: bool) -> Vec<Fiche> {
    partition(load_all().0, include_preview).offered
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(fiches: &[Fiche]) -> Vec<String> {
        fiches.iter().map(|f| f.id().to_owned()).collect()
    }

    fn previews(fiches: &[Fiche]) -> Vec<String> {
        fiches
            .iter()
            .filter(|f| f.file.status == Status::Preview)
            .map(|f| f.id().to_owned())
            .collect()
    }

    #[test]
    fn a_publication_build_lists_previews_but_offers_only_published_fiches() {
        let split = partition(load_all().0, false);
        let previews_listed = previews(&split.listed);
        assert!(
            !previews_listed.is_empty(),
            "le catalogue embarque au moins une fiche preview"
        );
        assert!(
            previews(&split.offered).is_empty(),
            "une publication ne propose aucune fiche preview"
        );
        let offered = ids(&split.offered);
        for fiche in split
            .listed
            .iter()
            .filter(|f| f.file.status == Status::Published)
        {
            assert!(
                offered.contains(&fiche.id().to_owned()),
                "toute fiche publiée est proposée"
            );
        }
    }

    #[test]
    fn a_trial_build_offers_previews_too() {
        let split = partition(load_all().0, true);
        assert_eq!(ids(&split.offered), ids(&split.listed));
        assert!(!previews(&split.offered).is_empty());
    }
}
