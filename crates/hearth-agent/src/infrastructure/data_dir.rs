//! Dossier de données de l'agent : le seul endroit qui le crée et qui en garantit les droits.
//!
//! Sous Unix le dossier est en 0700 (créé ainsi, ou resserré s'il existe avec des droits plus
//! larges) et les fichiers privés (`hearth.db`, `key.pem`…) ne sont lisibles que par le
//! propriétaire. Sous Windows, le dossier de l'utilisateur est déjà protégé par ses ACL.
//! La racine de composition l'appelle avant d'ouvrir la base et le magasin d'identité ; chaque
//! adaptateur le rappelle (sans effet si tout est déjà en ordre) pour ne jamais dépendre de
//! l'ordre des appels.

use std::io;
use std::path::Path;

#[cfg(unix)]
const DIR_MODE: u32 = 0o700;
#[cfg(unix)]
const FILE_MODE: u32 = 0o600;

/// Crée le dossier (et ses parents) s'il manque, et le resserre à 0700 s'il est plus ouvert.
/// Échoue clairement si les droits ne peuvent pas être corrigés.
#[cfg(unix)]
pub fn ensure(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(DIR_MODE)
        .create(dir)?;
    let mode = std::fs::metadata(dir)?.permissions().mode();
    if mode & 0o077 != 0 {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(DIR_MODE)).map_err(
            |source| {
                io::Error::new(
                    source.kind(),
                    format!(
                        "droits du dossier de données trop larges ({:o}) et impossibles à resserrer en 0700 : {source}",
                        mode & 0o777
                    ),
                )
            },
        )?;
    }
    Ok(())
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

#[cfg(all(test, unix))]
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
    fn a_wider_existing_directory_is_tightened() {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        ensure(root.path()).unwrap();
        assert_eq!(mode(root.path()), 0o700);
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
