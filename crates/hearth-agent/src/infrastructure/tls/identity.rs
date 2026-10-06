//! Identité persistée dans le dossier de données : `cert.pem`, `key.pem`, `install_id`.
//!
//! La décision (créer, réutiliser, refuser, nettoyer) vient de `domain::identity_policy` :
//! cet adaptateur observe le dossier et exécute. La création est protégée par un verrou de
//! fichier du système (`File::try_lock`), relâché par le noyau si le processus meurt : le
//! fichier `identity.lock` peut rester sur disque, sa seule présence ne bloque rien.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use hearth_proto::fingerprint::Fingerprint;
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use time::{Duration as TimeDuration, OffsetDateTime};

use crate::application::ports::{IdentityError, IdentityStore, PublicIdentity};
use crate::domain::identity_policy::{self, IdentityAction, StoreObservation};
use crate::domain::install_id::InstallId;
use crate::infrastructure::file_lock::{HeldLock, LockError};

use crate::domain::install::{
    CERT_FILE, IDENTITY_LOCK_FILE as LOCK_FILE, INSTALL_ID_FILE, KEY_FILE, is_identity_temporary,
};
const VALIDITY_DAYS: i64 = 3650;
/// Attente maximale du verrou de création, puis erreur claire.
const LOCK_TIMEOUT: Duration = Duration::from_secs(10);
const LOCK_RETRY: Duration = Duration::from_millis(25);

/// Matériel TLS chargé depuis le disque. Ne quitte pas `infrastructure/tls`.
pub(super) struct TlsMaterial {
    pub certificate: CertificateDer<'static>,
    pub private_key: PrivateKeyDer<'static>,
}

pub struct FileIdentityStore {
    dir: PathBuf,
    lock_timeout: Duration,
}

impl FileIdentityStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            lock_timeout: LOCK_TIMEOUT,
        }
    }

    #[cfg(test)]
    fn with_lock_timeout(mut self, timeout: Duration) -> Self {
        self.lock_timeout = timeout;
        self
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    fn observe(&self) -> StoreObservation {
        StoreObservation {
            has_certificate: self.path(CERT_FILE).exists(),
            has_key: self.path(KEY_FILE).exists(),
            has_install_id: self.path(INSTALL_ID_FILE).exists(),
        }
    }

    fn refuse(&self, missing: Vec<identity_policy::IdentityPart>) -> IdentityError {
        IdentityError::Incomplete {
            dir: self.dir.display().to_string(),
            missing,
        }
    }

    fn read_public(&self) -> Result<PublicIdentity, IdentityError> {
        let certificate = read_certificate(&self.path(CERT_FILE))?;
        let id_path = self.path(INSTALL_ID_FILE);
        let text = fs::read_to_string(&id_path).map_err(storage(&id_path))?;
        let install_id = InstallId::parse(&text)
            .map_err(|e| IdentityError::Corrupt(format!("{INSTALL_ID_FILE} : {e}")))?;
        Ok(PublicIdentity {
            fingerprint: Fingerprint::of_certificate_der(certificate.as_ref()),
            install_id,
        })
    }

    /// Charge certificat et clé. À appeler après `load_or_create`.
    pub(super) fn read_tls_material(&self) -> Result<TlsMaterial, IdentityError> {
        let certificate = read_certificate(&self.path(CERT_FILE))?;
        let key_path = self.path(KEY_FILE);
        let key_pem = fs::read(&key_path).map_err(storage(&key_path))?;
        let private_key = PrivateKeyDer::from_pem_slice(&key_pem)
            .map_err(|e| IdentityError::Corrupt(format!("{KEY_FILE} : {e}")))?;
        Ok(TlsMaterial {
            certificate,
            private_key,
        })
    }

    fn create(&self) -> Result<PublicIdentity, IdentityError> {
        let mut random = [0u8; 16];
        rustls::crypto::ring::default_provider()
            .secure_random
            .fill(&mut random)
            .map_err(|_| IdentityError::Generation("source aléatoire indisponible".to_owned()))?;
        let install_id = InstallId::from_bytes(random);

        let generated = generate_certificate(&install_id)
            .map_err(|e| IdentityError::Generation(e.to_string()))?;

        // Le certificat est écrit en dernier : sa présence valide toute l'identité.
        let id_path = self.path(INSTALL_ID_FILE);
        let key_path = self.path(KEY_FILE);
        let cert_path = self.path(CERT_FILE);
        write_file(&id_path, install_id.as_str().as_bytes(), false).map_err(storage(&id_path))?;
        write_file(&key_path, generated.key_pem.as_bytes(), true).map_err(storage(&key_path))?;
        write_file(&cert_path, generated.cert_pem.as_bytes(), false)
            .map_err(storage(&cert_path))?;

        Ok(PublicIdentity {
            fingerprint: Fingerprint::of_certificate_der(&generated.cert_der),
            install_id,
        })
    }

    fn remove_leftovers(&self) -> Result<(), IdentityError> {
        for name in [KEY_FILE, INSTALL_ID_FILE] {
            remove_if_present(&self.path(name))?;
        }
        Ok(())
    }

    /// Supprime les temporaires d'écriture (`key.pem.<pid>.<n>.tmp`…) d'un processus mort : ils
    /// peuvent contenir une clé privée. À appeler sous le verrou seulement.
    fn remove_stale_temporaries(&self) -> Result<(), IdentityError> {
        let entries = fs::read_dir(&self.dir).map_err(storage(&self.dir))?;
        for entry in entries {
            let entry = entry.map_err(storage(&self.dir))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if is_identity_temporary(&name) {
                tracing::warn!(file = %name, "temporaire d'identité orphelin supprimé");
                remove_if_present(&entry.path())?;
            }
        }
        Ok(())
    }
}

fn remove_if_present(path: &Path) -> Result<(), IdentityError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(storage(path)(e)),
    }
}

/// Convertit une erreur d'E/S en erreur d'identité qui nomme le chemin concerné.
fn storage(path: &Path) -> impl FnOnce(io::Error) -> IdentityError {
    let path = path.display().to_string();
    move |source| IdentityError::Storage { path, source }
}

impl IdentityStore for FileIdentityStore {
    fn load_or_create(&self) -> Result<PublicIdentity, IdentityError> {
        // Chemin courant : identité déjà là, aucune écriture et aucun verrou.
        match identity_policy::decide(self.observe()) {
            IdentityAction::Reuse => return self.read_public(),
            IdentityAction::Refuse { missing } => return Err(self.refuse(missing)),
            IdentityAction::Create | IdentityAction::CleanThenCreate => {}
        }

        create_data_dir(&self.dir).map_err(storage(&self.dir))?;
        let _lock = CreationLock::acquire(&self.path(LOCK_FILE), self.lock_timeout)?;

        // Un autre processus a pu finir pendant l'attente : on observe de nouveau.
        let action = identity_policy::decide(self.observe());
        if matches!(
            action,
            IdentityAction::Create | IdentityAction::CleanThenCreate
        ) {
            self.remove_stale_temporaries()?;
        }
        match action {
            IdentityAction::Reuse => self.read_public(),
            IdentityAction::Refuse { missing } => Err(self.refuse(missing)),
            IdentityAction::CleanThenCreate => {
                tracing::warn!(dir = %self.dir.display(), "restes d'une création interrompue : nettoyage");
                self.remove_leftovers()?;
                self.create()
            }
            IdentityAction::Create => {
                tracing::info!(dir = %self.dir.display(), "première exécution : génération du certificat");
                self.create()
            }
        }
    }
}

/// Verrou exclusif entre processus sur `identity.lock` (brique partagée `file_lock`) : relâché
/// explicitement à la destruction, et par le noyau si le processus meurt.
struct CreationLock {
    _held: HeldLock,
}

impl CreationLock {
    fn acquire(path: &Path, timeout: Duration) -> Result<Self, IdentityError> {
        let file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(path)
            .map_err(storage(path))?;
        match HeldLock::acquire(file, timeout, LOCK_RETRY) {
            Ok(held) => Ok(Self { _held: held }),
            Err(LockError::Busy) => {
                tracing::warn!(path = %path.display(), "verrou de création tenu par un autre processus");
                Err(IdentityError::LockTimeout)
            }
            Err(LockError::Io(error)) => Err(storage(path)(error)),
        }
    }
}

fn read_certificate(path: &Path) -> Result<CertificateDer<'static>, IdentityError> {
    let pem = fs::read(path).map_err(storage(path))?;
    CertificateDer::from_pem_slice(&pem)
        .map_err(|e| IdentityError::Corrupt(format!("{CERT_FILE} : {e}")))
}

struct Generated {
    cert_der: Vec<u8>,
    cert_pem: String,
    key_pem: String,
}

fn generate_certificate(install_id: &InstallId) -> Result<Generated, rcgen::Error> {
    let key_pair = KeyPair::generate()?;
    let mut params = CertificateParams::new(vec!["localhost".to_owned()])?;
    let mut name = DistinguishedName::new();
    name.push(DnType::CommonName, format!("hearth-{install_id}"));
    params.distinguished_name = name;
    let now = OffsetDateTime::now_utc();
    params.not_before = now - TimeDuration::days(1);
    params.not_after = now + TimeDuration::days(VALIDITY_DAYS);
    let certificate = params.self_signed(&key_pair)?;
    Ok(Generated {
        cert_der: certificate.der().to_vec(),
        cert_pem: certificate.pem(),
        key_pem: key_pair.serialize_pem(),
    })
}

fn create_data_dir(dir: &Path) -> io::Result<()> {
    crate::infrastructure::data_dir::ensure(dir)
}

/// Écrit via un fichier temporaire propre au processus puis renomme, pour ne jamais laisser un
/// fichier tronqué. `private` : permissions 0600 sous Unix (sans effet sous Windows, où le
/// dossier de l'utilisateur est déjà protégé par ses ACL).
fn write_file(path: &Path, bytes: &[u8], private: bool) -> io::Result<()> {
    use std::io::Write;

    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let suffix = format!(
        "{}.{}.tmp",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let mut tmp_name = path.as_os_str().to_owned();
    tmp_name.push(".");
    tmp_name.push(suffix);
    let tmp = PathBuf::from(tmp_name);

    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if private { 0o600 } else { 0o644 });
    }
    #[cfg(not(unix))]
    let _ = private;

    let result = (|| {
        let mut file = options.open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use std::sync::Barrier;
    use std::thread;
    use std::time::Instant;

    use super::*;
    use crate::domain::identity_policy::IdentityPart;

    /// Vrai si le verrou est libre (on le prend puis on le rend). Quelques essais : un autre test
    /// qui lance un processus au même instant en garde un instant une copie du descripteur (entre
    /// `fork` et `exec`), et un verrou `flock` dure tant qu'une copie existe.
    fn can_lock(path: &Path) -> bool {
        let file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(path)
            .expect("open");
        for _ in 0..50 {
            if file.try_lock().is_ok() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    /// FIX:01M46N01GMK08NXQHCZ28A2KQ1 : un enfant en cours de lancement porte une copie du
    /// descripteur ; elle ne retient pas le verrou de création après sa libération.
    #[test]
    fn the_creation_lock_is_released_while_a_copy_of_its_descriptor_is_still_alive() {
        let dir = tempfile::tempdir().expect("dossier");
        let path = dir.path().join(LOCK_FILE);
        let lock = CreationLock::acquire(&path, Duration::ZERO).expect("pris");
        let child_copy = lock._held.descriptor_copy();
        drop(lock);
        assert!(
            CreationLock::acquire(&path, Duration::ZERO).is_ok(),
            "libre alors que la copie vit encore"
        );
        drop(child_copy);
    }

    fn cert_fingerprint(dir: &Path) -> Fingerprint {
        let pem = fs::read(dir.join(CERT_FILE)).expect("read");
        let der = CertificateDer::from_pem_slice(&pem).expect("pem");
        Fingerprint::of_certificate_der(der.as_ref())
    }

    #[test]
    fn first_run_creates_the_files_and_releases_the_lock() {
        let dir = crate::infrastructure::data_dir::private_tempdir();
        let data = dir.path().join("data");
        let identity = FileIdentityStore::new(&data)
            .load_or_create()
            .expect("creation");
        for name in [CERT_FILE, KEY_FILE, INSTALL_ID_FILE] {
            assert!(data.join(name).is_file(), "{name}");
        }
        assert!(can_lock(&data.join(LOCK_FILE)), "verrou libéré");
        assert_eq!(identity.fingerprint, cert_fingerprint(&data));
    }

    #[test]
    fn second_load_returns_the_same_identity() {
        let dir = crate::infrastructure::data_dir::private_tempdir();
        let first = FileIdentityStore::new(dir.path())
            .load_or_create()
            .expect("creation");
        let second = FileIdentityStore::new(dir.path())
            .load_or_create()
            .expect("chargement");
        assert_eq!(first, second);
    }

    #[test]
    fn two_installations_have_different_identities() {
        let a = crate::infrastructure::data_dir::private_tempdir();
        let b = crate::infrastructure::data_dir::private_tempdir();
        let ia = FileIdentityStore::new(a.path())
            .load_or_create()
            .expect("a");
        let ib = FileIdentityStore::new(b.path())
            .load_or_create()
            .expect("b");
        assert_ne!(ia.fingerprint, ib.fingerprint);
        assert_ne!(ia.install_id, ib.install_id);
    }

    #[test]
    fn existing_certificate_without_key_is_an_error_not_a_regeneration() {
        let dir = crate::infrastructure::data_dir::private_tempdir();
        let store = FileIdentityStore::new(dir.path());
        let first = store.load_or_create().expect("creation");
        fs::remove_file(dir.path().join(KEY_FILE)).expect("remove");
        let err = store.load_or_create().expect_err("doit échouer");
        assert!(
            matches!(&err, IdentityError::Incomplete { missing, .. } if missing == &[IdentityPart::Key]),
            "{err}"
        );
        assert_eq!(cert_fingerprint(dir.path()), first.fingerprint);
    }

    #[test]
    fn leftovers_of_an_interrupted_creation_are_replaced() {
        let dir = crate::infrastructure::data_dir::private_tempdir();
        fs::write(dir.path().join(KEY_FILE), "reste").expect("write");
        FileIdentityStore::new(dir.path())
            .load_or_create()
            .expect("creation");
        let store = FileIdentityStore::new(dir.path());
        assert!(store.read_tls_material().is_ok());
    }

    #[test]
    fn corrupt_files_are_reported() {
        let dir = crate::infrastructure::data_dir::private_tempdir();
        let store = FileIdentityStore::new(dir.path());
        store.load_or_create().expect("creation");
        fs::write(dir.path().join(INSTALL_ID_FILE), "pas-un-id").expect("write");
        assert!(matches!(
            store.load_or_create(),
            Err(IdentityError::Corrupt(_))
        ));
    }

    #[test]
    fn simultaneous_creations_yield_one_identity() {
        const THREADS: usize = 8;
        let dir = crate::infrastructure::data_dir::private_tempdir();
        let barrier = Barrier::new(THREADS);
        let identities: Vec<PublicIdentity> = thread::scope(|scope| {
            let handles: Vec<_> = (0..THREADS)
                .map(|_| {
                    scope.spawn(|| {
                        barrier.wait();
                        FileIdentityStore::new(dir.path())
                            .load_or_create()
                            .expect("création concurrente")
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("thread"))
                .collect()
        });
        assert!(identities.windows(2).all(|pair| pair[0] == pair[1]));
        // Ce qui est sur le disque est cohérent avec ce que tout le monde a reçu.
        assert_eq!(identities[0].fingerprint, cert_fingerprint(dir.path()));
        assert!(can_lock(&dir.path().join(LOCK_FILE)));
    }

    #[test]
    fn a_lock_held_by_another_handle_ends_in_a_timeout_then_frees_up() {
        let dir = crate::infrastructure::data_dir::private_tempdir();
        let lock_path = dir.path().join(LOCK_FILE);
        let holder = fs::File::create(&lock_path).expect("create");
        holder.try_lock().expect("verrou pris par le test");

        let timeout = Duration::from_millis(200);
        let store = FileIdentityStore::new(dir.path()).with_lock_timeout(timeout);
        let start = Instant::now();
        let err = store.load_or_create().expect_err("doit échouer");
        assert!(matches!(err, IdentityError::LockTimeout), "{err}");
        assert!(start.elapsed() >= timeout);
        assert!(!dir.path().join(CERT_FILE).exists());

        drop(holder);
        store.load_or_create().expect("création après libération");
    }

    #[test]
    fn a_leftover_lock_file_that_nobody_holds_does_not_block() {
        let dir = crate::infrastructure::data_dir::private_tempdir();
        fs::write(dir.path().join(LOCK_FILE), "reste d'un processus mort").expect("write");
        let store =
            FileIdentityStore::new(dir.path()).with_lock_timeout(Duration::from_millis(200));
        store.load_or_create().expect("création");
    }

    #[test]
    fn the_lock_is_released_even_when_creation_fails() {
        let dir = crate::infrastructure::data_dir::private_tempdir();
        // Un dossier à la place du fichier install_id fait échouer l'écriture.
        fs::create_dir(dir.path().join(INSTALL_ID_FILE)).expect("mkdir");
        let err = FileIdentityStore::new(dir.path())
            .load_or_create()
            .expect_err("doit échouer");
        assert!(can_lock(&dir.path().join(LOCK_FILE)));
        // L'erreur nomme le chemin concerné.
        assert!(err.to_string().contains(INSTALL_ID_FILE), "{err}");
    }

    #[test]
    fn stale_temporaries_of_a_dead_process_are_removed_before_creation() {
        let dir = crate::infrastructure::data_dir::private_tempdir();
        for name in [
            "key.pem.4242.0.tmp",
            "cert.pem.4242.1.tmp",
            "install_id.4242.2.tmp",
        ] {
            fs::write(dir.path().join(name), "clé orpheline").expect("write");
        }
        fs::write(dir.path().join("autre.tmp"), "pas à nous").expect("write");
        FileIdentityStore::new(dir.path())
            .load_or_create()
            .expect("creation");
        let left: Vec<String> = fs::read_dir(dir.path())
            .expect("read_dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert_eq!(left, vec!["autre.tmp".to_owned()]);
    }

    #[test]
    fn missing_file_errors_name_the_path() {
        let dir = crate::infrastructure::data_dir::private_tempdir();
        let store = FileIdentityStore::new(dir.path());
        store.load_or_create().expect("creation");
        fs::remove_file(dir.path().join(INSTALL_ID_FILE)).expect("remove");
        // Un dossier à la place de install_id : présent pour la décision, illisible à la lecture.
        fs::create_dir(dir.path().join(INSTALL_ID_FILE)).expect("mkdir");
        let err = store.load_or_create().expect_err("lecture impossible");
        assert!(err.to_string().contains(INSTALL_ID_FILE), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn private_key_is_readable_by_its_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = crate::infrastructure::data_dir::private_tempdir();
        FileIdentityStore::new(dir.path())
            .load_or_create()
            .expect("creation");
        let mode = fs::metadata(dir.path().join(KEY_FILE))
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
