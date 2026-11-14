//! La racine des sauvegardes ne chevauche aucun dossier de données (BR-BACKUP-004 (b), ADR-0047 point 5).
//!
//! Si elle était, contenait ou se trouvait dans un dossier de données d'un service, une sauvegarde pourrait
//! s'archiver elle-même et une restauration pourrait remplacer des sauvegardes. Le contrôle se fait sur les
//! chemins canoniques ET sur les identités (périphérique, inode) : un lien ou un montage « bind » ne le
//! contourne pas. Refait à chaque démarrage et avant chaque opération.

use crate::domain::services::delete::{FileId, normalize};

/// Ce que l'on sait d'un chemin : sa forme canonique et sa lignée (son identité d'abord, puis celle de chacun de
/// ses parents). Un chemin qui n'existe pas (`exists` faux) a la lignée de son plus proche parent qui existe :
/// la première identité n'est alors pas la sienne, elle ne sert jamais à le comparer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathFacts {
    pub canonical: String,
    pub lineage: Vec<FileId>,
    pub exists: bool,
}

fn components(path: &str) -> Vec<String> {
    normalize(path)
        .split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

fn is_prefix(shorter: &[String], longer: &[String]) -> bool {
    shorter.len() <= longer.len() && shorter.iter().zip(longer).all(|(a, b)| a == b)
}

/// Les deux chemins se chevauchent-ils : égaux, l'un dans l'autre, par leur forme ou par leur identité ?
pub fn overlaps(backups: &PathFacts, data: &PathFacts) -> bool {
    let (b, d) = (components(&backups.canonical), components(&data.canonical));
    if is_prefix(&b, &d) || is_prefix(&d, &b) {
        return true;
    }
    // Par l'identité : le dossier de données est la racine des sauvegardes ou l'un de ses parents, ou
    // l'inverse. On ne compare QUE l'identité propre d'un chemin QUI EXISTE à la lignée de l'autre (la racine du
    // système de fichiers est parent de tout ; et deux chemins absents partagent le même parent existant, sans
    // se chevaucher pour autant).
    let in_lineage = |path: &PathFacts, other: &[FileId]| {
        path.exists && path.lineage.first().is_some_and(|id| other.contains(id))
    };
    in_lineage(data, &backups.lineage) || in_lineage(backups, &data.lineage)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(canonical: &str, lineage: &[FileId]) -> PathFacts {
        PathFacts {
            canonical: canonical.into(),
            lineage: lineage.to_vec(),
            exists: !lineage.is_empty(),
        }
    }

    #[test]
    fn distinct_siblings_do_not_overlap_even_with_a_common_prefix_in_the_name() {
        let backups = facts("/var/lib/hearth-backups", &[(1, 30), (1, 20), (1, 2)]);
        let data = facts(
            "/var/lib/hearth/services",
            &[(1, 40), (1, 21), (1, 20), (1, 2)],
        );
        assert!(!overlaps(&backups, &data));
    }

    #[test]
    fn a_root_inside_or_around_or_equal_to_the_data_folder_overlaps() {
        let data = facts("/var/lib/hearth/services", &[(1, 40), (1, 21), (1, 2)]);
        for canonical in [
            "/var/lib/hearth/services",
            "/var/lib/hearth/services/backups",
            "/var/lib/hearth",
            "/var",
        ] {
            assert!(overlaps(&facts(canonical, &[]), &data), "{canonical}");
        }
    }

    #[test]
    fn a_link_or_a_bind_mount_is_caught_by_identity() {
        // Un autre nom, la même identité : le dossier de données est la racine des sauvegardes.
        let data = facts("/srv/data/services", &[(1, 40), (1, 21), (1, 2)]);
        let backups = facts("/mnt/elsewhere", &[(1, 40), (1, 90), (1, 2)]);
        assert!(overlaps(&backups, &data));
        // La racine des sauvegardes sous un dossier de données par un autre nom.
        let inside = facts("/mnt/x/b", &[(1, 55), (1, 40), (1, 2)]);
        assert!(overlaps(&inside, &data));
    }

    #[test]
    fn the_filesystem_root_shared_by_every_lineage_is_not_an_overlap() {
        let data = facts("/a/data", &[(1, 40), (1, 2)]);
        let backups = facts("/b/backups", &[(1, 50), (1, 2)]);
        assert!(!overlaps(&backups, &data));
    }

    #[test]
    fn two_paths_that_do_not_exist_yet_never_overlap_through_their_shared_parent() {
        // Les deux n'existent pas : leur lignée est celle du même parent, `/var/lib`.
        let missing = |canonical: &str| PathFacts {
            canonical: canonical.into(),
            lineage: vec![(1, 20), (1, 2)],
            exists: false,
        };
        assert!(!overlaps(
            &missing("/var/lib/hearth-backups"),
            &missing("/var/lib/hearth/services")
        ));
        // Mais l'un dans l'autre, par leur forme, reste un chevauchement.
        assert!(overlaps(
            &missing("/var/lib/hearth/services/b"),
            &missing("/var/lib/hearth/services")
        ));
        // Une racine absente DANS un dossier de données qui existe : le parent existant est le dossier ou dessous.
        let data = facts("/data/services", &[(1, 40), (1, 10), (1, 2)]);
        let inside = PathFacts {
            canonical: "/mnt/lien/b".into(),
            lineage: vec![(1, 40), (1, 10), (1, 2)],
            exists: false,
        };
        assert!(overlaps(&inside, &data));
    }

    #[test]
    fn dots_and_double_slashes_do_not_hide_an_overlap() {
        let data = facts("/var/lib/hearth/services", &[]);
        assert!(overlaps(
            &facts("/var//lib/./hearth/services/../services/x", &[]),
            &data
        ));
    }
}
