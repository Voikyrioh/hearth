//! Le secret d'empreinte des requêtes suivies, dans un fichier du dossier de données
//! (`request_fingerprint.key`, HRT-32, ADR-0034).
//!
//! - **Absent : créé.** 32 octets du hasard du système, écrits dans un fichier voisin ouvert en
//!   0600 **avant** d'y écrire le moindre octet (aucune fenêtre où le contenu serait lisible),
//!   synchronisé, puis posé sous son nom par un lien dur : le lien échoue si le nom existe, donc
//!   deux démarrages simultanés ne créent jamais deux secrets (le perdant relit celui du gagnant),
//!   et le fichier final n'existe jamais à moitié écrit. Aucun verrou à tenir ni à nettoyer.
//! - **Présent et valide : lu.** Exactement 32 octets, droits sans accès pour les autres.
//! - **Présent mais inutilisable (mauvaise taille, droits ouverts, illisible) : l'agent refuse de
//!   démarrer** avec un message clair. Jamais régénéré par-dessus : l'opérateur décide.
//!
//! Le propriétaire est l'utilisateur qui lance l'agent (le service) : seul le service crée le
//! fichier, jamais les sous-commandes `account` ni `attack-mode`. Sous Windows (développement),
//! la confidentialité vient du dossier du profil, comme pour `key.pem` (`data_dir`).

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use zeroize::Zeroize;

use crate::application::ports::{FingerprintSecretError, FingerprintSecretStore};
use crate::domain::fingerprint_secret::{FingerprintSecret, SECRET_LEN};
use crate::domain::install::{FINGERPRINT_SECRET_FILE, is_fingerprint_secret_temporary};
use crate::infrastructure::data_dir;

pub struct FileFingerprintSecretStore {
    dir: PathBuf,
}

impl FileFingerprintSecretStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn path(&self) -> PathBuf {
        self.dir.join(FINGERPRINT_SECRET_FILE)
    }
}

fn storage(path: &Path) -> impl FnOnce(io::Error) -> FingerprintSecretError {
    let path = path.display().to_string();
    move |source| FingerprintSecretError::Storage { path, source }
}

/// Lit le secret s'il existe ; `None` si le fichier est absent.
fn read(path: &Path) -> Result<Option<FingerprintSecret>, FingerprintSecretError> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(storage(path)(error)),
    };
    let metadata = file.metadata().map_err(storage(path))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            return Err(FingerprintSecretError::TooOpen {
                path: path.display().to_string(),
                mode,
            });
        }
    }
    let wrong_size = |len: u64| FingerprintSecretError::WrongSize {
        path: path.display().to_string(),
        len,
        expected: SECRET_LEN,
    };
    if metadata.len() != SECRET_LEN as u64 {
        return Err(wrong_size(metadata.len()));
    }
    // Un octet de plus que prévu suffit à voir un fichier qui aurait grossi entre-temps.
    let mut bytes = Vec::with_capacity(SECRET_LEN + 1);
    file.take(SECRET_LEN as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(storage(path))?;
    let secret = FingerprintSecret::from_slice(&bytes);
    let len = bytes.len() as u64;
    bytes.zeroize();
    secret.map(Some).map_err(|_| wrong_size(len))
}

/// Âge à partir duquel un temporaire du secret est un reste d'un arrêt brutal : une création normale
/// dure des millisecondes, aucun démarrage vivant ne le retient aussi longtemps.
const STALE_AFTER: std::time::Duration = std::time::Duration::from_secs(600);

/// Retire les temporaires du secret (`request_fingerprint.key.<pid>.<n>.tmp`) laissés par un arrêt
/// brutal : ils contiennent un secret en clair (0600). Sans verrou, on ne touche qu'à ceux assez
/// vieux pour ne plus appartenir à une création en cours. Best effort : un échec n'arrête rien.
fn remove_stale_temporaries(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !is_fingerprint_secret_temporary(&name) {
            continue;
        }
        let old = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age >= STALE_AFTER);
        if old {
            tracing::warn!(file = %name, "temporaire de secret orphelin supprimé");
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Écrit `bytes` dans un fichier voisin ouvert en 0600, le synchronise, et rend son chemin.
fn write_neighbour(
    path: &Path,
    bytes: &[u8; SECRET_LEN],
) -> Result<PathBuf, FingerprintSecretError> {
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
            Err(storage(&tmp)(error))
        }
    }
}

impl FingerprintSecretStore for FileFingerprintSecretStore {
    fn load_or_create(&self) -> Result<FingerprintSecret, FingerprintSecretError> {
        let path = self.path();
        if let Some(secret) = read(&path)? {
            return Ok(secret);
        }
        data_dir::ensure(&self.dir).map_err(storage(&self.dir))?;

        remove_stale_temporaries(&self.dir);
        let mut bytes = [0_u8; SECRET_LEN];
        getrandom::fill(&mut bytes)
            .map_err(|error| FingerprintSecretError::Generation(error.to_string()))?;
        let result = (|| {
            let tmp = write_neighbour(&path, &bytes)?;
            // Le lien échoue si le nom existe déjà : un seul démarrage gagne.
            let linked = fs::hard_link(&tmp, &path);
            let _ = fs::remove_file(&tmp);
            match linked {
                Ok(()) => {
                    tracing::info!(file = %path.display(), "secret d'empreinte des requêtes suivies créé");
                    Ok(FingerprintSecret::from_bytes(bytes))
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    // Un autre démarrage a créé le secret entre-temps : c'est le sien qui compte.
                    read(&path)?.ok_or_else(|| {
                        storage(&path)(io::Error::new(
                            io::ErrorKind::NotFound,
                            "le secret a disparu pendant sa création",
                        ))
                    })
                }
                Err(error) => Err(storage(&path)(error)),
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

    fn store(dir: &Path) -> FileFingerprintSecretStore {
        FileFingerprintSecretStore::new(dir)
    }

    #[test]
    fn a_missing_file_is_created_with_32_bytes_and_read_back_the_same() {
        let dir = private_tempdir();
        let first = store(dir.path()).load_or_create().unwrap();
        let on_disk = fs::read(dir.path().join(FINGERPRINT_SECRET_FILE)).unwrap();
        assert_eq!(on_disk.len(), SECRET_LEN);
        assert_eq!(first.expose().as_slice(), on_disk.as_slice());
        let second = store(dir.path()).load_or_create().unwrap();
        assert_eq!(first.expose(), second.expose());
        // Deux installations ne partagent pas de secret.
        let other = private_tempdir();
        let third = store(other.path()).load_or_create().unwrap();
        assert_ne!(first.expose(), third.expose());
    }

    #[test]
    fn an_old_orphan_temporary_is_removed_at_creation_and_a_fresh_one_is_left_alone() {
        let dir = private_tempdir();
        let old = dir.path().join("request_fingerprint.key.4242.0.tmp");
        let fresh = dir.path().join("request_fingerprint.key.4243.0.tmp");
        let other = dir.path().join("notes.tmp");
        for path in [&old, &fresh, &other] {
            fs::write(path, [9_u8; SECRET_LEN]).unwrap();
        }
        let long_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        fs::File::options()
            .write(true)
            .open(&old)
            .unwrap()
            .set_modified(long_ago)
            .unwrap();
        store(dir.path()).load_or_create().unwrap();
        assert!(!old.exists(), "reste d'un arrêt brutal");
        assert!(fresh.exists(), "peut être une création en cours");
        assert!(other.exists(), "pas à nous");
    }

    #[test]
    fn a_missing_data_directory_is_created_private() {
        let root = private_tempdir();
        let dir = root.path().join("data");
        store(&dir).load_or_create().unwrap();
        assert!(dir.join(FINGERPRINT_SECRET_FILE).is_file());
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_0600_and_leaves_no_temporary() {
        use std::os::unix::fs::PermissionsExt;
        let dir = private_tempdir();
        store(dir.path()).load_or_create().unwrap();
        let mode = fs::metadata(dir.path().join(FINGERPRINT_SECRET_FILE))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        let names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, [FINGERPRINT_SECRET_FILE], "aucun temporaire");
    }

    /// Les droits sont posés à l'ouverture, avant l'écriture : même sous un masque permissif, le
    /// fichier voisin n'a jamais été lisible par d'autres.
    #[cfg(unix)]
    #[test]
    fn the_neighbour_file_is_private_from_its_creation() {
        use std::os::unix::fs::PermissionsExt;
        let dir = private_tempdir();
        let path = dir.path().join(FINGERPRINT_SECRET_FILE);
        let tmp = write_neighbour(&path, &[3; SECRET_LEN]).unwrap();
        let mode = fs::metadata(&tmp).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        fs::remove_file(tmp).unwrap();
    }

    #[test]
    fn concurrent_first_starts_agree_on_one_secret() {
        let dir = private_tempdir();
        let barrier = Arc::new(Barrier::new(8));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let path = dir.path().to_owned();
                let barrier = barrier.clone();
                thread::spawn(move || {
                    barrier.wait();
                    *store(&path).load_or_create().unwrap().expose()
                })
            })
            .collect();
        let secrets: Vec<[u8; SECRET_LEN]> =
            handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert!(secrets.windows(2).all(|pair| pair[0] == pair[1]));
        let on_disk = fs::read(dir.path().join(FINGERPRINT_SECRET_FILE)).unwrap();
        assert_eq!(on_disk.as_slice(), secrets[0].as_slice());
        let names = fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(names, 1, "aucun temporaire laissé par la course");
    }

    #[test]
    fn a_file_of_the_wrong_size_refuses_to_start_and_is_never_overwritten() {
        for content in [&b""[..], &[1; 31][..], &[1; 33][..], &[1; 64][..]] {
            let dir = private_tempdir();
            let path = dir.path().join(FINGERPRINT_SECRET_FILE);
            fs::write(&path, content).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            }
            let error = store(dir.path()).load_or_create().unwrap_err();
            assert!(
                matches!(error, FingerprintSecretError::WrongSize { len, .. } if len == content.len() as u64),
                "{error}"
            );
            let message = error.to_string();
            assert!(message.contains("32"), "{message}");
            assert!(message.contains(FINGERPRINT_SECRET_FILE), "{message}");
            assert_eq!(
                fs::read(&path).unwrap(),
                content,
                "jamais régénéré par-dessus"
            );
        }
    }

    #[test]
    fn an_unreadable_file_refuses_to_start_and_is_left_alone() {
        // Un dossier à la place du fichier : présent, illisible comme secret.
        let dir = private_tempdir();
        let path = dir.path().join(FINGERPRINT_SECRET_FILE);
        fs::create_dir(&path).unwrap();
        assert!(store(dir.path()).load_or_create().is_err());
        assert!(path.is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn a_secret_open_to_other_users_is_refused_and_left_alone() {
        use std::os::unix::fs::PermissionsExt;
        let dir = private_tempdir();
        let path = dir.path().join(FINGERPRINT_SECRET_FILE);
        fs::write(&path, [5_u8; SECRET_LEN]).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        let error = store(dir.path()).load_or_create().unwrap_err();
        assert!(
            matches!(error, FingerprintSecretError::TooOpen { mode: 0o644, .. }),
            "{error}"
        );
        assert!(error.to_string().contains("chmod 600"));
        assert_eq!(fs::read(&path).unwrap(), [5_u8; SECRET_LEN]);
    }

    #[test]
    fn errors_never_carry_the_secret() {
        let dir = private_tempdir();
        let path = dir.path().join(FINGERPRINT_SECRET_FILE);
        fs::write(&path, [0xAB_u8; 40]).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let error = store(dir.path()).load_or_create().unwrap_err();
        let shown = format!("{error} {error:?}");
        assert!(!shown.contains("171") && !shown.to_lowercase().contains("abab"));
    }

    /// Capte tout ce que le suivi écrit pendant la création puis la relecture du secret.
    #[derive(Clone, Default)]
    struct Captured(Arc<std::sync::Mutex<Vec<u8>>>);

    impl Write for Captured {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
        type Writer = Self;
        fn make_writer(&'a self) -> Self {
            self.clone()
        }
    }

    #[test]
    fn the_secret_appears_in_no_log_line_debug_or_error() {
        let captured = Captured::default();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(captured.clone())
            .finish();
        let dir = private_tempdir();
        let secret = tracing::subscriber::with_default(subscriber, || {
            let created = store(dir.path()).load_or_create().unwrap();
            let read_back = store(dir.path()).load_or_create().unwrap();
            assert_eq!(created.expose(), read_back.expose());
            created
        });
        let logs = String::from_utf8_lossy(&captured.0.lock().unwrap()).into_owned();
        // (Le suivi capté peut être vide si un autre test a figé l'intérêt des points de journal :
        // ce test ne prouve que ce qu'il voit, l'absence se prouve aussi par la lecture du code.)
        let bytes = secret.expose();
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let decimal = bytes
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        for shown in [logs, format!("{secret:?}"), format!("{secret:#?}")] {
            assert!(!shown.to_lowercase().contains(&hex), "{shown}");
            assert!(!shown.contains(&decimal), "{shown}");
        }
    }
}
