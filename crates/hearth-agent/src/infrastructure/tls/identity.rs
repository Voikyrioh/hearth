//! Identité persistée dans le dossier de données : `cert.pem`, `key.pem`, `install_id`.
//!
//! Le certificat est le point de validation de la création : s'il existe, l'identité est
//! considérée comme émise et ne sera jamais régénérée (BR-INSTALL-004).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use time::{Duration, OffsetDateTime};

use crate::application::ports::{Identity, IdentityError, IdentityStore};
use crate::domain::fingerprint::Fingerprint;
use crate::domain::install_id::InstallId;

const CERT_FILE: &str = "cert.pem";
const KEY_FILE: &str = "key.pem";
const INSTALL_ID_FILE: &str = "install_id";
const VALIDITY_DAYS: i64 = 3650;

pub struct FileIdentityStore {
    dir: PathBuf,
}

impl FileIdentityStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    fn load(&self) -> Result<Identity, IdentityError> {
        let cert_pem = fs::read(self.path(CERT_FILE))?;
        let key_pem = read_required(&self.dir, KEY_FILE)?;
        let id_text = String::from_utf8(read_required(&self.dir, INSTALL_ID_FILE)?)
            .map_err(|e| IdentityError::Corrupt(e.to_string()))?;

        let certificate = CertificateDer::from_pem_slice(&cert_pem)
            .map_err(|e| IdentityError::Corrupt(format!("{CERT_FILE} : {e}")))?;
        let key = PrivateKeyDer::from_pem_slice(&key_pem)
            .map_err(|e| IdentityError::Corrupt(format!("{KEY_FILE} : {e}")))?;
        let install_id = InstallId::parse(&id_text)
            .map_err(|e| IdentityError::Corrupt(format!("{INSTALL_ID_FILE} : {e}")))?;

        Ok(Identity {
            fingerprint: Fingerprint::of_certificate_der(certificate.as_ref()),
            certificate_der: certificate.as_ref().to_vec(),
            private_key_der: key.secret_der().to_vec(),
            install_id,
        })
    }

    fn create(&self) -> Result<Identity, IdentityError> {
        create_data_dir(&self.dir)?;

        let mut random = [0u8; 16];
        rustls::crypto::ring::default_provider()
            .secure_random
            .fill(&mut random)
            .map_err(|_| IdentityError::Generation("source aléatoire indisponible".to_owned()))?;
        let install_id = InstallId::from_bytes(random);

        let (cert_der, cert_pem, key_der, key_pem) = generate_certificate(&install_id)
            .map_err(|e| IdentityError::Generation(e.to_string()))?;

        // Le certificat est écrit en dernier : sa présence valide toute l'identité.
        write_file(
            &self.path(INSTALL_ID_FILE),
            install_id.as_str().as_bytes(),
            false,
        )?;
        write_file(&self.path(KEY_FILE), key_pem.as_bytes(), true)?;
        write_file(&self.path(CERT_FILE), cert_pem.as_bytes(), false)?;

        Ok(Identity {
            fingerprint: Fingerprint::of_certificate_der(&cert_der),
            certificate_der: cert_der,
            private_key_der: key_der,
            install_id,
        })
    }
}

impl IdentityStore for FileIdentityStore {
    fn load_or_create(&self) -> Result<Identity, IdentityError> {
        if self.path(CERT_FILE).exists() {
            return self.load();
        }
        // Pas de certificat : au pire des restes d'une création interrompue, écrasés ici.
        tracing::info!(dir = %self.dir.display(), "première exécution : génération du certificat");
        self.create()
    }
}

fn read_required(dir: &Path, name: &str) -> Result<Vec<u8>, IdentityError> {
    match fs::read(dir.join(name)) {
        Ok(bytes) => Ok(bytes),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(IdentityError::Incomplete(
            dir.display().to_string(),
            name.to_owned(),
        )),
        Err(e) => Err(e.into()),
    }
}

/// Retourne (certificat DER, certificat PEM, clé PKCS#8 DER, clé PEM).
fn generate_certificate(
    install_id: &InstallId,
) -> Result<(Vec<u8>, String, Vec<u8>, String), rcgen::Error> {
    let key_pair = KeyPair::generate()?;
    let mut params = CertificateParams::new(vec!["localhost".to_owned()])?;
    let mut name = DistinguishedName::new();
    name.push(DnType::CommonName, format!("hearth-{install_id}"));
    params.distinguished_name = name;
    let now = OffsetDateTime::now_utc();
    params.not_before = now - Duration::days(1);
    params.not_after = now + Duration::days(VALIDITY_DAYS);
    let certificate = params.self_signed(&key_pair)?;
    Ok((
        certificate.der().to_vec(),
        certificate.pem(),
        key_pair.serialize_der(),
        key_pair.serialize_pem(),
    ))
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

/// Écrit via un fichier temporaire puis renomme, pour ne jamais laisser un fichier tronqué.
/// `private` : permissions 0600 sous Unix (sans effet sous Windows, où le dossier de
/// l'utilisateur est déjà protégé par ses ACL).
fn write_file(path: &Path, bytes: &[u8], private: bool) -> io::Result<()> {
    use std::io::Write;

    let tmp = path.with_extension("tmp");
    let _ = fs::remove_file(&tmp);
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if private { 0o600 } else { 0o644 });
    }
    #[cfg(not(unix))]
    let _ = private;
    let mut file = options.open(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_run_creates_the_three_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = FileIdentityStore::new(dir.path().join("data"));
        let identity = store.load_or_create().expect("creation");
        for name in [CERT_FILE, KEY_FILE, INSTALL_ID_FILE] {
            assert!(dir.path().join("data").join(name).is_file(), "{name}");
        }
        assert_eq!(
            identity.fingerprint,
            Fingerprint::of_certificate_der(&identity.certificate_der)
        );
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
        assert_eq!(first.fingerprint, second.fingerprint);
        assert_eq!(first.install_id, second.install_id);
        assert_eq!(first.certificate_der, second.certificate_der);
        assert_eq!(first.private_key_der, second.private_key_der);
    }

    #[test]
    fn two_installations_have_different_fingerprints() {
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
        assert!(matches!(err, IdentityError::Incomplete(..)), "{err}");
        // Le certificat n'a pas été touché.
        let cert = fs::read(dir.path().join(CERT_FILE)).expect("read");
        let der = CertificateDer::from_pem_slice(&cert).expect("pem");
        assert_eq!(der.as_ref(), first.certificate_der.as_slice());
    }

    #[test]
    fn leftovers_of_an_interrupted_creation_are_replaced() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join(KEY_FILE), "reste").expect("write");
        let identity = FileIdentityStore::new(dir.path())
            .load_or_create()
            .expect("creation");
        assert!(!identity.private_key_der.is_empty());
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
