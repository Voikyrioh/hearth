//! La clé de scellement des secrets des services, dans un fichier du dossier de données
//! (`services.key`, HRT-83, ADR-0044). Modèle : `fingerprint_secret.rs` (ADR-0034), mais ici
//! **lire et créer sont deux gestes séparés** : l'agent ne crée la clé qu'à la première nécessité, et
//! seulement si le coffre (`application::secret_vault`) a établi que rien ne dépend d'une clé.
//!
//! - **Lire** ne crée, ne corrige et ne supprime rien. Absent → `Absent`. Présent et valide (exactement 32
//!   octets, droits sans accès pour les autres) → la clé. Présent mais inutilisable (mauvaise taille,
//!   droits ouverts, illisible) → `Unusable`, et le fichier n'est **jamais** touché.
//! - **Créer** écrit 32 octets du hasard du système dans un fichier voisin ouvert en `0600` **avant**
//!   d'y écrire le moindre octet, le synchronise, puis le pose sous son nom par un lien dur : le lien
//!   échoue si le nom existe, donc deux démarrages simultanés ne créent jamais deux clés et un fichier
//!   existant n'est jamais remplacé. Le fichier final n'existe jamais à moitié écrit.
//!
//! **La lecture se méfie du chemin** : ouverture sans suivre de lien symbolique (`O_NOFOLLOW`) et sans
//! bloquer sur un tube nommé (`O_NONBLOCK`), puis, sur le descripteur ouvert (`fstat`) et avant toute
//! lecture : fichier ordinaire, propriétaire = utilisateur effectif de l'agent, droits sans accès pour les
//! autres, taille. À la création, le fichier PUIS le dossier sont synchronisés avant de rendre la clé.
//!
//! Sous Windows (développement) la confidentialité vient du dossier du profil ; les contrôles de droits
//! et la création `0600` sont du code Linux, prouvé par la CI seulement.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use zeroize::Zeroize;

use crate::application::ports::{KeyRead, KeyStoreError, ServicesKeyStore};
use crate::domain::install::{SERVICES_KEY_FILE, is_services_key_temporary};
use crate::domain::sealed::{KEY_LEN, ServicesKey, UnusableKey};
use crate::infrastructure::data_dir;

pub struct FileServicesKeyStore {
    dir: PathBuf,
}

impl FileServicesKeyStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn path(&self) -> PathBuf {
        self.dir.join(SERVICES_KEY_FILE)
    }
}

fn storage(path: &Path, error: &io::Error) -> KeyStoreError {
    KeyStoreError::Storage {
        path: path.display().to_string(),
        message: error.to_string(),
    }
}

/// Ouvre un fichier en lecture sans jamais suivre un lien symbolique (`O_NOFOLLOW`) et sans jamais
/// bloquer sur un tube nommé posé à sa place (`O_NONBLOCK` : l'ouverture rend la main, le contrôle de
/// genre vient avant toute lecture). Aussi utilisée pour ouvrir une archive (`archive_fs.rs`).
pub(crate) fn open_read_no_follow(path: &Path) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    options.open(path)
}

/// L'utilisateur EFFECTIF de l'agent : deuxième valeur de la ligne `Uid:` de `/proc/self/status` (même
/// source que `is_privileged` de l'installation ; aucun `unsafe`, donc pas de `geteuid`).
#[cfg(unix)]
fn parse_effective_uid(status: &str) -> Option<u32> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

#[cfg(unix)]
fn effective_uid() -> Option<u32> {
    parse_effective_uid(&fs::read_to_string("/proc/self/status").ok()?)
}

/// Un lien symbolique refusé par `O_NOFOLLOW` répond `ELOOP`.
#[cfg(unix)]
pub(crate) fn is_symlink_refusal(error: &io::Error) -> bool {
    error.raw_os_error() == Some(libc::ELOOP)
}

#[cfg(not(unix))]
pub(crate) fn is_symlink_refusal(_error: &io::Error) -> bool {
    false
}

/// Lit la clé. Tous les contrôles portent sur le DESCRIPTEUR ouvert (`fstat`), jamais sur le chemin :
/// genre (fichier ordinaire, avant toute lecture), propriétaire (l'utilisateur effectif de l'agent),
/// droits (aucun accès pour les autres), taille.
fn read_file(path: &Path) -> KeyRead {
    let file = match open_read_no_follow(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return KeyRead::Absent,
        Err(error) if is_symlink_refusal(&error) => {
            return KeyRead::Unusable(UnusableKey::NotRegular);
        }
        Err(_) => return KeyRead::Unusable(UnusableKey::Unreadable),
    };
    let Ok(metadata) = file.metadata() else {
        return KeyRead::Unusable(UnusableKey::Unreadable);
    };
    if !metadata.is_file() {
        return KeyRead::Unusable(UnusableKey::NotRegular);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        // Un utilisateur effectif qu'on ne sait pas établir ne passe pas pour le bon.
        if effective_uid() != Some(metadata.uid()) {
            return KeyRead::Unusable(UnusableKey::WrongOwner);
        }
        let mode = metadata.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            return KeyRead::Unusable(UnusableKey::TooOpen { mode });
        }
    }
    if metadata.len() != KEY_LEN as u64 {
        return KeyRead::Unusable(UnusableKey::WrongSize {
            len: metadata.len(),
        });
    }
    // Un octet de plus que prévu suffit à voir un fichier qui aurait grossi entre-temps.
    let mut bytes = Vec::with_capacity(KEY_LEN + 1);
    let read = file.take(KEY_LEN as u64 + 1).read_to_end(&mut bytes);
    let key = match read {
        Ok(_) => ServicesKey::from_slice(&bytes),
        Err(_) => {
            bytes.zeroize();
            return KeyRead::Unusable(UnusableKey::Unreadable);
        }
    };
    let len = bytes.len() as u64;
    bytes.zeroize();
    match key {
        Some(key) => KeyRead::Present(key),
        None => KeyRead::Unusable(UnusableKey::WrongSize { len }),
    }
}

/// Âge à partir duquel un temporaire de la clé est un reste d'un arrêt brutal : une création normale dure
/// des millisecondes.
const STALE_AFTER: std::time::Duration = std::time::Duration::from_secs(600);

/// Retire les temporaires de la clé laissés par un arrêt brutal (ils contiennent la clé en clair, `0600`).
/// Sans verrou, on ne touche qu'à ceux assez vieux pour ne plus appartenir à une création en cours.
fn remove_stale_temporaries(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !is_services_key_temporary(&name) {
            continue;
        }
        let old = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age >= STALE_AFTER);
        if old {
            tracing::warn!(file = %name, "temporaire de la clé des services orphelin supprimé");
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Synchronise le DOSSIER : sans cela, l'entrée du fichier peut manquer après une coupure de courant
/// alors que la base garde déjà la vérification de la clé (secrets perdus pour de bon).
fn sync_dir(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        fs::File::open(dir)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        Ok(())
    }
}

/// Écrit `bytes` dans un fichier voisin ouvert en `0600`, le synchronise, et rend son chemin.
fn write_neighbour(path: &Path, bytes: &[u8; KEY_LEN]) -> Result<PathBuf, KeyStoreError> {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let mut name = path.as_os_str().to_owned();
    name.push(format!(
        ".{}.{}.tmp",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let tmp = PathBuf::from(name);

    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Les droits sont posés par l'ouverture elle-même : aucun octet n'est écrit avant.
        options.mode(0o600);
    }
    let written = (|| {
        let mut file = options.open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()
    })();
    match written {
        Ok(()) => Ok(tmp),
        Err(error) => {
            let _ = fs::remove_file(&tmp);
            Err(storage(&tmp, &error))
        }
    }
}

impl ServicesKeyStore for FileServicesKeyStore {
    fn read(&self) -> KeyRead {
        read_file(&self.path())
    }

    fn create(&self) -> Result<ServicesKey, KeyStoreError> {
        let path = self.path();
        data_dir::ensure(&self.dir).map_err(|error| storage(&self.dir, &error))?;
        remove_stale_temporaries(&self.dir);
        let mut bytes = [0_u8; KEY_LEN];
        getrandom::fill(&mut bytes)
            .map_err(|error| KeyStoreError::Generation(error.to_string()))?;
        let result = (|| {
            let tmp = write_neighbour(&path, &bytes)?;
            // Le lien échoue si le nom existe déjà : un seul démarrage gagne, un fichier existant n'est
            // JAMAIS remplacé.
            let linked = fs::hard_link(&tmp, &path);
            let _ = fs::remove_file(&tmp);
            match linked {
                Ok(()) => {
                    // Le fichier est déjà synchronisé (voisin) ; le dossier l'est ICI, avant de rendre la
                    // clé : aucun scellé ni vérification ne dépend d'une entrée qui pourrait manquer.
                    sync_dir(&self.dir).map_err(|error| storage(&self.dir, &error))?;
                    tracing::info!(file = %path.display(), "clé de scellement des secrets des services créée");
                    Ok(ServicesKey::from_bytes(bytes))
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    // Un autre démarrage l'a créée entre-temps : c'est la sienne qui compte. Si elle est
                    // inutilisable, c'est une erreur : on ne la remplace pas.
                    match read_file(&path) {
                        KeyRead::Present(key) => {
                            sync_dir(&self.dir).map_err(|error| storage(&self.dir, &error))?;
                            Ok(key)
                        }
                        _ => Err(storage(
                            &path,
                            &io::Error::other(
                                "le fichier existe mais n'est pas une clé utilisable",
                            ),
                        )),
                    }
                }
                Err(error) => Err(storage(&path, &error)),
            }
        })();
        bytes.zeroize();
        result
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};
    use std::thread;

    use super::*;
    use crate::infrastructure::data_dir::private_tempdir;

    fn store(dir: &Path) -> FileServicesKeyStore {
        FileServicesKeyStore::new(dir)
    }

    #[test]
    fn reading_never_creates_anything() {
        let dir = private_tempdir();
        assert!(matches!(store(dir.path()).read(), KeyRead::Absent));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn a_created_key_is_32_bytes_read_back_the_same_and_leaves_no_temporary() {
        let dir = private_tempdir();
        let created = store(dir.path()).create().unwrap();
        let on_disk = fs::read(dir.path().join(SERVICES_KEY_FILE)).unwrap();
        assert_eq!(on_disk.len(), KEY_LEN);
        assert_eq!(created.expose().as_slice(), on_disk.as_slice());
        let KeyRead::Present(read) = store(dir.path()).read() else {
            panic!("la clé créée doit se relire");
        };
        assert_eq!(read.expose(), created.expose());
        let names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, [SERVICES_KEY_FILE]);
        // Deux installations ne partagent pas de clé.
        let other = private_tempdir();
        let third = store(other.path()).create().unwrap();
        assert_ne!(created.expose(), third.expose());
    }

    #[test]
    fn creating_never_overwrites_an_existing_file_whatever_it_holds() {
        for content in [&b""[..], &[1; 31][..], &[1; 33][..], &[9; KEY_LEN][..]] {
            let dir = private_tempdir();
            let path = dir.path().join(SERVICES_KEY_FILE);
            fs::write(&path, content).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            }
            let result = store(dir.path()).create();
            // Un fichier de 32 octets est la clé d'un autre démarrage : rendue. Les autres : erreur.
            assert_eq!(
                result.is_ok(),
                content.len() == KEY_LEN,
                "{} octets",
                content.len()
            );
            assert_eq!(fs::read(&path).unwrap(), content, "jamais écrasé");
        }
    }

    #[test]
    fn a_file_of_the_wrong_size_is_unusable_and_left_alone() {
        for content in [&b""[..], &[1; 31][..], &[1; 33][..], &[1; 64][..]] {
            let dir = private_tempdir();
            let path = dir.path().join(SERVICES_KEY_FILE);
            fs::write(&path, content).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            }
            let KeyRead::Unusable(UnusableKey::WrongSize { len }) = store(dir.path()).read() else {
                panic!("{} octets devraient être refusés", content.len());
            };
            assert_eq!(len, content.len() as u64);
            assert_eq!(fs::read(&path).unwrap(), content);
        }
    }

    #[test]
    fn a_directory_in_its_place_is_unusable_and_left_alone() {
        let dir = private_tempdir();
        let path = dir.path().join(SERVICES_KEY_FILE);
        fs::create_dir(&path).unwrap();
        assert!(matches!(store(dir.path()).read(), KeyRead::Unusable(_)));
        assert!(store(dir.path()).create().is_err());
        assert!(path.is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_0600_and_a_key_open_to_others_is_unusable_and_left_alone() {
        use std::os::unix::fs::PermissionsExt;
        let dir = private_tempdir();
        store(dir.path()).create().unwrap();
        let path = dir.path().join(SERVICES_KEY_FILE);
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        let before = fs::read(&path).unwrap();
        assert!(matches!(
            store(dir.path()).read(),
            KeyRead::Unusable(UnusableKey::TooOpen { mode: 0o644 })
        ));
        assert!(store(dir.path()).create().is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    /// Les droits sont posés à l'ouverture, avant l'écriture : même sous un masque permissif, le fichier
    /// voisin n'a jamais été lisible par d'autres.
    #[cfg(unix)]
    #[test]
    fn the_neighbour_file_is_private_from_its_creation() {
        use std::os::unix::fs::PermissionsExt;
        let dir = private_tempdir();
        let path = dir.path().join(SERVICES_KEY_FILE);
        let tmp = write_neighbour(&path, &[3; KEY_LEN]).unwrap();
        let mode = fs::metadata(&tmp).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        fs::remove_file(tmp).unwrap();
    }

    #[test]
    fn concurrent_first_creations_agree_on_one_key() {
        let dir = private_tempdir();
        let barrier = Arc::new(Barrier::new(8));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let path = dir.path().to_owned();
                let barrier = barrier.clone();
                thread::spawn(move || {
                    barrier.wait();
                    *store(&path).create().unwrap().expose()
                })
            })
            .collect();
        let keys: Vec<[u8; KEY_LEN]> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert!(keys.windows(2).all(|pair| pair[0] == pair[1]));
        assert_eq!(
            fs::read(dir.path().join(SERVICES_KEY_FILE)).unwrap(),
            keys[0]
        );
        assert_eq!(
            fs::read_dir(dir.path()).unwrap().count(),
            1,
            "aucun temporaire"
        );
    }

    #[test]
    fn an_old_orphan_temporary_is_removed_and_a_fresh_one_is_left_alone() {
        let dir = private_tempdir();
        let old = dir.path().join("services.key.4242.0.tmp");
        let fresh = dir.path().join("services.key.4243.0.tmp");
        let other = dir.path().join("notes.tmp");
        for path in [&old, &fresh, &other] {
            fs::write(path, [9_u8; KEY_LEN]).unwrap();
        }
        let long_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        fs::File::options()
            .write(true)
            .open(&old)
            .unwrap()
            .set_modified(long_ago)
            .unwrap();
        store(dir.path()).create().unwrap();
        assert!(!old.exists(), "reste d'un arrêt brutal");
        assert!(fresh.exists(), "peut être une création en cours");
        assert!(other.exists(), "pas à nous");
    }

    /// Un lien symbolique n'est jamais suivi, même vers une vraie clé valide : refusé, cible intacte.
    #[cfg(unix)]
    #[test]
    fn a_symbolic_link_is_never_followed() {
        let dir = private_tempdir();
        let target = dir.path().join("elsewhere.key");
        fs::write(&target, [7_u8; KEY_LEN]).unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let link = dir.path().join(SERVICES_KEY_FILE);
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(matches!(
            store(dir.path()).read(),
            KeyRead::Unusable(UnusableKey::NotRegular)
        ));
        assert!(store(dir.path()).create().is_err(), "jamais remplacé");
        assert!(link.is_symlink());
        // Un lien pendant est refusé de la même façon (et non lu comme « absent »).
        fs::remove_file(&target).unwrap();
        assert!(matches!(
            store(dir.path()).read(),
            KeyRead::Unusable(UnusableKey::NotRegular)
        ));
    }

    /// Un tube nommé à la place de la clé ne bloque pas l'agent : refusé avant toute lecture.
    #[cfg(unix)]
    #[test]
    fn a_named_pipe_in_its_place_is_refused_without_blocking() {
        let dir = private_tempdir();
        let path = dir.path().join(SERVICES_KEY_FILE);
        let made = std::process::Command::new("mkfifo")
            .arg(path)
            .status()
            .unwrap();
        assert!(made.success());
        assert!(matches!(
            store(dir.path()).read(),
            KeyRead::Unusable(UnusableKey::NotRegular)
        ));
        assert!(store(dir.path()).create().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn the_effective_user_is_the_second_uid_of_the_status_file() {
        let status = "Name:\thearth\nUid:\t1000\t0\t1000\t1000\nGid:\t1000\t1000\t1000\t1000\n";
        assert_eq!(parse_effective_uid(status), Some(0));
        assert_eq!(parse_effective_uid("Uid:\t7\n"), None);
        assert_eq!(parse_effective_uid("Name:\tx\n"), None);
        // Sur ce système, le propriétaire d'un fichier créé par le test est bien l'utilisateur effectif.
        let dir = private_tempdir();
        store(dir.path()).create().unwrap();
        assert!(matches!(store(dir.path()).read(), KeyRead::Present(_)));
    }

    #[test]
    fn a_missing_data_directory_is_created() {
        let root = private_tempdir();
        let dir = root.path().join("data");
        store(&dir).create().unwrap();
        assert!(dir.join(SERVICES_KEY_FILE).is_file());
    }

    #[test]
    fn the_key_is_not_in_a_debug_output_or_an_error_message() {
        let dir = private_tempdir();
        let created = store(dir.path()).create().unwrap();
        assert_eq!(format!("{created:?}"), "ServicesKey(***)");
        let path = dir.path().join(SERVICES_KEY_FILE);
        fs::remove_file(&path).unwrap();
        fs::write(&path, [0xAB_u8; 40]).unwrap();
        let shown = format!("{:?}", store(dir.path()).read());
        assert!(
            !shown.contains("171") && !shown.to_lowercase().contains("abab"),
            "{shown}"
        );
    }
}
