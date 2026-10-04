//! Dossier de données de l'agent : le seul endroit qui le crée et qui contrôle ses droits.
//!
//! Sous Unix : un dossier absent est créé en 0700. Un dossier qui existe déjà n'est jamais
//! modifié en silence : s'il est ouvert à d'autres utilisateurs (droits au-delà de 0700), c'est
//! une erreur qui dit quoi faire (`chmod 700`), sauf s'il est vide et qu'on peut en changer les
//! droits (on en est le propriétaire) : il est alors resserré, puisqu'il ne contient encore rien
//! d'exposé. Les fichiers privés (`hearth.db`, `key.pem`…) ne sont lisibles que par leur
//! propriétaire.
//!
//! Sous Windows, ce module crée le dossier avec les droits hérités de son parent et ne
//! vérifie ni ne change aucun droit : la confidentialité vient de l'emplacement par défaut
//! (`%LOCALAPPDATA%`, protégé par les ACL du profil de l'utilisateur) ; un dossier choisi ailleurs
//! n'est pas protégé par ce code. Windows ne sert qu'au développement.
//!
//! La racine de composition l'appelle avant d'ouvrir la base et le magasin d'identité ; chaque
//! adaptateur le rappelle (sans effet si tout est déjà en ordre) pour ne jamais dépendre de
//! l'ordre des appels.

use std::io;
use std::path::Path;

#[cfg(unix)]
const DIR_MODE: u32 = 0o700;
#[cfg(unix)]
const FILE_MODE: u32 = 0o600;

/// Garantit que le dossier de données existe et n'est pas ouvert aux autres utilisateurs.
#[cfg(unix)]
pub fn ensure(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;

    match std::fs::metadata(dir) {
        Ok(meta) if meta.is_dir() => check_existing(dir),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} existe et n'est pas un dossier", dir.display()),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => std::fs::DirBuilder::new()
            .recursive(true)
            .mode(DIR_MODE)
            .create(dir),
        Err(error) => Err(error),
    }
}

/// Dossier déjà là : on ne le resserre que s'il est vide (et seulement si les droits le permettent,
/// ce qui revient à en être le propriétaire) ; sinon l'opérateur décide.
#[cfg(unix)]
fn check_existing(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mode = std::fs::metadata(dir)?.permissions().mode() & 0o777;
    if mode & 0o077 == 0 {
        return Ok(());
    }
    let advice = format!(
        "le dossier de données {} est ouvert aux autres utilisateurs (droits {mode:o}) ; corrige-le avec `chmod 700 {}` ou choisis un autre dossier",
        dir.display(),
        dir.display()
    );
    let is_empty = std::fs::read_dir(dir)?.next().is_none();
    if !is_empty {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, advice));
    }
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(DIR_MODE))
        .map_err(|source| io::Error::new(source.kind(), format!("{advice} ({source})")))
}

#[cfg(not(unix))]
pub fn ensure(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)
}

/// Garantit que le fichier existe et n'est lisible que par son propriétaire (0600 sous Unix).
/// SQLite donne aux fichiers `-wal` et `-shm` les droits du fichier de base : les créer ainsi
/// avant l'ouverture règle aussi leur cas.
#[cfg(unix)]
pub fn ensure_private_file(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(FILE_MODE)
        .open(path)?;
    let mode = std::fs::metadata(path)?.permissions().mode();
    if mode & 0o177 != 0 {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(FILE_MODE))?;
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn ensure_private_file(path: &Path) -> io::Result<()> {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map(|_| ())
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn mode(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn a_missing_directory_is_created_private_with_its_parents() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("a").join("b");
        ensure(&dir).unwrap();
        assert_eq!(mode(&dir), 0o700);
    }

    #[test]
    fn an_empty_wider_directory_we_own_is_tightened() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        ensure(root.path()).unwrap();
        assert_eq!(mode(root.path()), 0o700);
    }

    #[test]
    fn a_wider_directory_with_content_is_refused_and_left_untouched() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("hearth.db"), b"x").unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        let error = ensure(root.path()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        let message = error.to_string();
        assert!(message.contains("chmod 700"), "{message}");
        assert!(
            message.contains(&root.path().display().to_string()),
            "{message}"
        );
        assert!(message.contains("755"), "{message}");
        assert_eq!(mode(root.path()), 0o755);
    }

    #[test]
    fn a_private_directory_with_content_is_accepted_as_is() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("hearth.db"), b"x").unwrap();
        ensure(root.path()).unwrap();
        assert_eq!(mode(root.path()), 0o700);
    }

    #[test]
    fn a_file_in_place_of_the_directory_is_an_error() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("data");
        std::fs::write(&file, b"x").unwrap();
        assert!(ensure(&file).is_err());
    }

    #[test]
    fn a_private_file_is_created_0600_and_a_wider_one_is_tightened() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("hearth.db");
        ensure_private_file(&file).unwrap();
        assert_eq!(mode(&file), 0o600);
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        ensure_private_file(&file).unwrap();
        assert_eq!(mode(&file), 0o600);
    }
}
