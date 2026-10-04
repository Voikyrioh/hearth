//! Identité persistée dans le dossier de données : `cert.pem`, `key.pem`, `install_id`.
//!
//! La décision (créer, réutiliser, refuser, nettoyer) vient de `domain::identity_policy` :
//! cet adaptateur observe le dossier et exécute. La création est protégée par un fichier
//! verrou pour qu'un seul processus à la fois écrive l'identité.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use hearth_proto::fingerprint::Fingerprint;
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use time::{Duration as TimeDuration, OffsetDateTime};

use crate::application::ports::{IdentityError, IdentityStore, PublicIdentity};
use crate::domain::identity_policy::{self, IdentityAction, StoreObservation};
use crate::domain::install_id::InstallId;

const CERT_FILE: &str = "cert.pem";
const KEY_FILE: &str = "key.pem";
const INSTALL_ID_FILE: &str = "install_id";
const LOCK_FILE: &str = "identity.lock";
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
        let text = fs::read_to_string(self.path(INSTALL_ID_FILE))?;
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
        let key_pem = fs::read(self.path(KEY_FILE))?;
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
        write_file(
            &self.path(INSTALL_ID_FILE),
            install_id.as_str().as_bytes(),
            false,
        )?;
        write_file(&self.path(KEY_FILE), generated.key_pem.as_bytes(), true)?;
        write_file(&self.path(CERT_FILE), generated.cert_pem.as_bytes(), false)?;

        Ok(PublicIdentity {
            fingerprint: Fingerprint::of_certificate_der(&generated.cert_der),
            install_id,
        })
    }

    fn remove_leftovers(&self) -> io::Result<()> {
        for name in [KEY_FILE, INSTALL_ID_FILE] {
            match fs::remove_file(self.path(name)) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

impl IdentityStore for FileIdentityStore {
    fn load_or_create(&self) -> Result<PublicIdentity, IdentityError> {
        // Chemin courant : identité déjà là, aucune écriture et aucun verrou.
        match identity_policy::decide(self.observe()) {
            IdentityAction::Reuse => return self.read_public(),
            IdentityAction::Refuse { missing } => return Err(self.refuse(missing)),
            IdentityAction::Create | IdentityAction::CleanThenCreate => {}
        }

        create_data_dir(&self.dir)?;
        let _lock = CreationLock::acquire(&self.path(LOCK_FILE), self.lock_timeout)?;

        // Un autre processus a pu finir pendant l'attente : on observe de nouveau.
        match identity_policy::decide(self.observe()) {
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

/// Verrou entre processus : un fichier créé en exclusivité, supprimé à la libération
/// (y compris sur erreur, via `Drop`).
struct CreationLock {
    path: PathBuf,
}

impl CreationLock {
    fn acquire(path: &Path, timeout: Duration) -> Result<Self, IdentityError> {
        let deadline = Instant::now() + timeout;
        loop {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
            {
                Ok(_) => {
                    return Ok(Self {
                        path: path.to_owned(),
                    });
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    if Instant::now() >= deadline {
                        return Err(IdentityError::LockTimeout(path.display().to_string()));
                    }
                    // Le démarrage est synchrone : une courte attente bloquante suffit.
                    thread::sleep(LOCK_RETRY);
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
}

impl Drop for CreationLock {
    fn drop(&mut self) {
        if let Err(e) = fs::remove_file(&self.path) {
            tracing::warn!(path = %self.path.display(), error = %e, "suppression du verrou impossible");
        }
    }
}

fn read_certificate(path: &Path) -> Result<CertificateDer<'static>, IdentityError> {
    let pem = fs::read(path)?;
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

#[cfg(unix)]
fn create_data_dir(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
}

#[cfg(not(unix))]
fn create_data_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)
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

    use super::*;
    use crate::domain::identity_policy::IdentityPart;

    fn cert_fingerprint(dir: &Path) -> Fingerprint {
        let pem = fs::read(dir.join(CERT_FILE)).expect("read");
        let der = CertificateDer::from_pem_slice(&pem).expect("pem");
        Fingerprint::of_certificate_der(der.as_ref())
    }

    #[test]
    fn first_run_creates_the_files_and_releases_the_lock() {
        let dir = tempfile::tempdir().expect("tempdir");
        let data = dir.path().join("data");
        let identity = FileIdentityStore::new(&data)
            .load_or_create()
            .expect("creation");
        for name in [CERT_FILE, KEY_FILE, INSTALL_ID_FILE] {
            assert!(data.join(name).is_file(), "{name}");
        }
        assert!(!data.join(LOCK_FILE).exists(), "verrou libéré");
        assert_eq!(identity.fingerprint, cert_fingerprint(&data));
    }

    #[test]
    fn second_load_returns_the_same_identity() {
        let dir = tempfile::tempdir().expect("tempdir");
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
        let a = tempfile::tempdir().expect("tempdir");
        let b = tempfile::tempdir().expect("tempdir");
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
        let dir = tempfile::tempdir().expect("tempdir");
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
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join(KEY_FILE), "reste").expect("write");
        FileIdentityStore::new(dir.path())
            .load_or_create()
            .expect("creation");
        let store = FileIdentityStore::new(dir.path());
        assert!(store.read_tls_material().is_ok());
    }

    #[test]
    fn corrupt_files_are_reported() {
        let dir = tempfile::tempdir().expect("tempdir");
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
        let dir = tempfile::tempdir().expect("tempdir");
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
        assert!(!dir.path().join(LOCK_FILE).exists());
    }

    #[test]
    fn a_held_lock_ends_in_a_clear_error_and_is_not_removed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lock = dir.path().join(LOCK_FILE);
        fs::write(&lock, "").expect("write");
        let start = Instant::now();
        let timeout = Duration::from_millis(200);
        let err = FileIdentityStore::new(dir.path())
            .with_lock_timeout(timeout)
            .load_or_create()
            .expect_err("doit échouer");
        assert!(matches!(err, IdentityError::LockTimeout(_)), "{err}");
        assert!(start.elapsed() >= timeout);
        assert!(lock.exists(), "le verrou d'un autre n'est pas supprimé");
        assert!(!dir.path().join(CERT_FILE).exists());
    }

    #[test]
    fn the_lock_is_released_even_when_creation_fails() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Un dossier à la place du fichier install_id fait échouer l'écriture.
        fs::create_dir(dir.path().join(INSTALL_ID_FILE)).expect("mkdir");
        let err = FileIdentityStore::new(dir.path()).load_or_create();
        assert!(err.is_err());
        assert!(!dir.path().join(LOCK_FILE).exists());
    }

    #[cfg(unix)]
    #[test]
    fn private_key_is_readable_by_its_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
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
