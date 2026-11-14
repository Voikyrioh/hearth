//! Les sauvegardes d'un service : les règles, sans E/S (HRT-97, ADR-0047, BR-BACKUP-*).
//!
//! - [`ignore`] : les éléments que Hearth ne lit pas (BR-BACKUP-003) ; le nom n'y est qu'une PROPOSITION.
//! - [`manifest`] : le manifeste d'une sauvegarde, sa forme, ses bornes, sa cohérence (BR-BACKUP-012, 014).
//! - [`space`] : l'espace nécessaire, la réserve, le seuil pendant la copie (BR-BACKUP-009).
//! - [`ownership`] : ce que Hearth reconnaît comme SON fichier (registre, inode, empreinte du manifeste) :
//!   la règle absolue de BR-BACKUP-004 (a) et (e), BR-BACKUP-013.
//! - [`overlap`] : la racine des sauvegardes ne chevauche aucun dossier de données (BR-BACKUP-004 (b)).
//! - [`read`] : la lecture d'une archive comme hostile : chaque entrée doit avoir sa ligne du manifeste, les
//!   totaux sont bornés par lui (ADR-0047 point 7).
//!
//! Rien ici ne lit un disque ni ne parle au moteur. Aucun type n'a de champ pour un contenu de fichier ni
//! pour la note d'un compte.

pub mod ignore;
pub mod manifest;
pub mod overlap;
pub mod ownership;
pub mod read;
pub mod space;

use std::fmt;

/// L'identifiant d'une sauvegarde : un ULID en majuscules (26 caractères). Il entre dans un nom de fichier :
/// il est jugé, jamais pris tel quel d'une requête.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BackupId(String);

impl BackupId {
    /// `None` si le texte n'est pas exactement un ULID en majuscules.
    pub fn parse(text: &str) -> Option<Self> {
        let valid = text.len() == 26
            && text
                .bytes()
                .all(|b| b.is_ascii_digit() || (b.is_ascii_uppercase() && !b"ILOU".contains(&b)));
        valid.then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Le nom de l'archive : `<id>.tar.gz`.
    pub fn archive_name(&self) -> String {
        format!("{}.tar.gz", self.0)
    }

    /// Le nom de l'archive en cours d'écriture : `<id>.tar.gz.part`.
    pub fn part_name(&self) -> String {
        format!("{}.tar.gz.part", self.0)
    }

    /// Le nom du manifeste voisin : `<id>.manifest.json`.
    pub fn manifest_name(&self) -> String {
        format!("{}.manifest.json", self.0)
    }
}

impl fmt::Display for BackupId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Un dossier du service qui entre dans la sauvegarde : son rang, où il est monté dans le service (son nom
/// pour l'humain) et son dossier relatif à la racine du service. Le nom du dossier dans l'archive est
/// `dir` (un composant, unique dans la sauvegarde).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedRoot {
    pub id: u32,
    /// La cible de montage dans le service (`/data`).
    pub target: String,
    /// Le nom de ce dossier dans l'archive.
    pub dir: String,
}

/// Le nom d'un dossier dans l'archive d'après sa cible de montage : le dernier composant, sans caractère
/// douteux, rendu unique parmi `taken`. Toujours un composant valable (`root` à défaut).
pub fn archive_dir_name(target: &str, taken: &[String]) -> String {
    let last = target
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default();
    let clean: String = last
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let base = if clean.is_empty() || clean == "." || clean == ".." {
        "root".to_owned()
    } else {
        clean
    };
    if !taken.contains(&base) {
        return base;
    }
    (2_u32..)
        .map(|n| format!("{base}-{n}"))
        .find(|candidate| !taken.contains(candidate))
        .unwrap_or(base)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_is_an_upper_case_ulid_and_nothing_else() {
        assert!(BackupId::parse("01J9ZY0G3Q8M2K6W4T7V5N1B9D").is_some());
        for bad in [
            "",
            "01j9zy0g3q8m2k6w4t7v5n1b9d",
            "../../etc/passwd",
            "01J9ZY0G3Q8M2K6W4T7V5N1B9",
            "01J9ZY0G3Q8M2K6W4T7V5N1B9DD",
            "01J9ZY0G3Q8M2K6W4T7V5N1B9I",
            "01J9ZY0G3Q8M2K6W4T7V5N1B9/",
        ] {
            assert!(BackupId::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn the_file_names_derive_from_the_id() {
        let id = BackupId::parse("01J9ZY0G3Q8M2K6W4T7V5N1B9D").expect("id");
        assert_eq!(id.archive_name(), "01J9ZY0G3Q8M2K6W4T7V5N1B9D.tar.gz");
        assert_eq!(id.part_name(), "01J9ZY0G3Q8M2K6W4T7V5N1B9D.tar.gz.part");
        assert_eq!(
            id.manifest_name(),
            "01J9ZY0G3Q8M2K6W4T7V5N1B9D.manifest.json"
        );
    }

    #[test]
    fn a_folder_name_in_the_archive_is_one_safe_unique_component() {
        assert_eq!(archive_dir_name("/data", &[]), "data");
        assert_eq!(archive_dir_name("/srv/terraria/", &[]), "terraria");
        assert_eq!(archive_dir_name("/data", &["data".to_owned()]), "data-2");
        assert_eq!(archive_dir_name("/", &[]), "root");
        assert_eq!(archive_dir_name("/a b/é..", &[]), "_..");
        assert_eq!(archive_dir_name("/..", &[]), "root");
    }
}
