//! Ce que Hearth reconnaît comme SON fichier de sauvegarde (BR-BACKUP-004 (a) et (e), BR-BACKUP-013,
//! ADR-0047 point 2).
//!
//! **La propriété est prouvée, jamais devinée.** Hearth ne lit comme sauvegarde, ne renomme et ne supprime
//! qu'un fichier qui est inscrit au registre avec son chemin, son périphérique et son numéro d'inode, **et**
//! dont le manifeste voisin a l'empreinte de la ligne. Un fichier déposé à la main, même nommé comme une
//! archive de Hearth, n'est dans aucune ligne : il n'existe pas pour Hearth. Une archive de Hearth remplacée à
//! la main n'a plus le bon inode : elle n'est plus la sienne, et il n'y touche pas.
//!
//! Fonction pure : l'appelant dit ce qu'il a vu sur le disque ; le domaine juge.

/// Ce que la ligne du registre sait du fichier au moment où Hearth l'a écrit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registered {
    pub dev: u64,
    pub ino: u64,
    /// SHA-256 hexadécimal du fichier de manifeste tel que Hearth l'a écrit.
    pub manifest_sha256: String,
}

/// Ce que le disque montre à l'endroit inscrit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seen {
    /// Rien à cet endroit.
    Absent,
    /// Quelque chose qui n'est pas un fichier ordinaire (lien, dossier, tube...) : jamais le fichier de Hearth.
    NotRegular,
    File {
        dev: u64,
        ino: u64,
    },
}

/// Le jugement sur l'archive d'une ligne du registre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ownership {
    /// C'est le fichier que Hearth a écrit (même inode).
    Ours,
    /// Il y a un fichier, mais ce n'est plus celui que Hearth a écrit : il n'y touche pas (BR-BACKUP-013).
    Replaced,
    /// Le fichier n'existe plus : la ligne du registre peut être retirée (« disparue »).
    Missing,
}

/// L'archive inscrite est-elle encore la sienne ?
pub fn judge_archive(registered: &Registered, seen: &Seen) -> Ownership {
    match seen {
        Seen::Absent => Ownership::Missing,
        Seen::NotRegular => Ownership::Replaced,
        Seen::File { dev, ino } if (*dev, *ino) == (registered.dev, registered.ino) => {
            Ownership::Ours
        }
        Seen::File { .. } => Ownership::Replaced,
    }
}

/// Le manifeste voisin est-il celui que Hearth a écrit ? `seen_sha256` : l'empreinte du fichier lu, `None` s'il
/// est absent ou illisible. Comparaison à temps constant sur des empreintes publiques : la simple égalité suffit
/// (ce n'est pas un secret).
pub fn manifest_is_ours(registered: &Registered, seen_sha256: Option<&str>) -> bool {
    seen_sha256.is_some_and(|sha| sha.eq_ignore_ascii_case(&registered.manifest_sha256))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registered() -> Registered {
        Registered {
            dev: 7,
            ino: 4242,
            manifest_sha256: "ab".repeat(32),
        }
    }

    #[test]
    fn only_the_same_inode_is_ours() {
        let reg = registered();
        assert_eq!(
            judge_archive(&reg, &Seen::File { dev: 7, ino: 4242 }),
            Ownership::Ours
        );
        // Un fichier de même nom mais d'un autre inode : remplacé à la main.
        assert_eq!(
            judge_archive(&reg, &Seen::File { dev: 7, ino: 4243 }),
            Ownership::Replaced
        );
        // Même numéro d'inode sur un autre périphérique : pas le même fichier.
        assert_eq!(
            judge_archive(&reg, &Seen::File { dev: 8, ino: 4242 }),
            Ownership::Replaced
        );
    }

    #[test]
    fn a_link_or_a_special_file_is_never_ours_and_a_gap_is_missing() {
        let reg = registered();
        assert_eq!(judge_archive(&reg, &Seen::NotRegular), Ownership::Replaced);
        assert_eq!(judge_archive(&reg, &Seen::Absent), Ownership::Missing);
    }

    #[test]
    fn the_manifest_must_carry_the_registered_digest() {
        let reg = registered();
        assert!(manifest_is_ours(&reg, Some(&"ab".repeat(32))));
        assert!(manifest_is_ours(&reg, Some(&"AB".repeat(32))));
        assert!(!manifest_is_ours(&reg, Some(&"cd".repeat(32))));
        assert!(!manifest_is_ours(&reg, None));
    }
}
