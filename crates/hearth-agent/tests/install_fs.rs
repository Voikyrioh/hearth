//! Installation et désinstallation sur un **vrai système de fichiers** (dossier temporaire) avec
//! l'hôte de production (`SystemHost`) : seuls le service, l'identité et les comptes sont des
//! doubles (ils écrivent de vrais fichiers aux vrais noms). Ces tests prouvent ce que la machine
//! simulée de `install_flow.rs` ne peut pas prouver : une identité à moitié là n'est jamais
//! supprimée, une purge ne supprime que les fichiers que Hearth connaît (jamais un dossier
//! partagé), un retour en arrière ne touche pas ce qui existait.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::VecDeque;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use hearth_agent::application::install::Installer;
use hearth_agent::application::ports::{
    AdminAccounts, AdminAccountsError, AdminCredential, Answered, BinaryInstalled, ConfigSpec,
    HostError, HostFacts, IdentityError, IdentityStore, InstallHost, InstallLock, InstallPaths,
    PublicIdentity, ServiceError, ServiceKind, ServiceManager, ServiceSpec,
};
use hearth_agent::domain::accounts::Username;
use hearth_agent::domain::install::{DataDirState, Version};
use hearth_agent::domain::install_id::InstallId;
use hearth_agent::domain::secret::Secret;
use hearth_agent::entrypoint::cli::{InstallArgs, UninstallArgs};
use hearth_agent::entrypoint::install::{self, Context, InstallCliError, Prompter};
use hearth_agent::infrastructure::install::SystemHost;
use hearth_proto::fingerprint::Fingerprint;

const TARGET: Version = Version::new(0, 2, 0);

fn fingerprint() -> Fingerprint {
    Fingerprint::from_bytes([0xAB; 32])
}

/// L'hôte de production, sauf ce qu'un test ne peut pas avoir : les droits, le port, l'attente du
/// vrai service.
struct Host(SystemHost);

impl InstallHost for Host {
    fn os(&self) -> String {
        "linux".to_owned()
    }
    fn arch(&self) -> String {
        "x86_64".to_owned()
    }
    fn is_privileged(&self) -> bool {
        true
    }
    fn port_taken(&self, _addr: IpAddr, _port: u16) -> bool {
        false
    }
    fn free_bytes(&self, _path: &Path) -> Result<u64, HostError> {
        Ok(u64::MAX)
    }
    fn hostname(&self) -> String {
        "serveur".to_owned()
    }
    fn inspect(&self, paths: &InstallPaths, source: &Path) -> Result<HostFacts, HostError> {
        self.0.inspect(paths, source)
    }
    fn lock(&self, path: &Path) -> Result<InstallLock, HostError> {
        self.0.lock(path)
    }
    fn install_binary(
        &self,
        source: &Path,
        dest: &Path,
        backup: &Path,
    ) -> Result<BinaryInstalled, HostError> {
        self.0.install_binary(source, dest, backup)
    }
    fn restore_binary(&self, dest: &Path, installed: &BinaryInstalled) -> Result<(), HostError> {
        self.0.restore_binary(dest, installed)
    }
    fn discard_backup(&self, installed: &BinaryInstalled) {
        self.0.discard_backup(installed);
    }
    fn ensure_data_dir(&self, dir: &Path) -> Result<bool, HostError> {
        self.0.ensure_data_dir(dir)
    }
    fn write_config_new(&self, path: &Path, spec: &ConfigSpec) -> Result<bool, HostError> {
        self.0.write_config_new(path, spec)
    }
    fn remove_file(&self, path: &Path) -> Result<(), HostError> {
        self.0.remove_file(path)
    }
    fn list_dir(&self, path: &Path) -> Result<Vec<String>, HostError> {
        self.0.list_dir(path)
    }
    fn data_dir_state(&self, path: &Path) -> DataDirState {
        // Le propriétaire d'un dossier temporaire n'est pas root : le reste est celui du disque.
        let real = self.0.data_dir_state(path);
        DataDirState {
            owned_by_root: true,
            ..real
        }
    }
    fn remove_dir_if_empty(&self, path: &Path) -> Result<(), HostError> {
        self.0.remove_dir_if_empty(path)
    }
    fn wait_for_hello(
        &self,
        _addr: SocketAddr,
        _timeout: Duration,
        _cancelled: &dyn Fn() -> bool,
    ) -> Result<Answered, HostError> {
        Ok(Answered {
            served: fingerprint(),
        })
    }
}

#[derive(Default)]
struct ServiceState {
    unit: Option<String>,
    active: bool,
    fail_install: bool,
}

struct Service(Arc<Mutex<ServiceState>>);

impl ServiceManager for Service {
    fn kind(&self) -> ServiceKind {
        ServiceKind::Systemd
    }
    fn is_installed(&self) -> Result<bool, ServiceError> {
        Ok(self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .unit
            .is_some())
    }
    fn is_active(&self) -> Result<bool, ServiceError> {
        Ok(self.0.lock().unwrap_or_else(PoisonError::into_inner).active)
    }
    fn install(&self, _spec: &ServiceSpec) -> Result<(), ServiceError> {
        let mut state = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if state.fail_install {
            return Err(ServiceError::Command {
                command: "enable --now".to_owned(),
                detail: "échec simulé".to_owned(),
            });
        }
        state.unit = Some("unite-v2".to_owned());
        state.active = true;
        Ok(())
    }
    fn unit_text(&self) -> Result<Option<String>, ServiceError> {
        Ok(self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .unit
            .clone())
    }
    fn restore_unit(&self, text: &str) -> Result<(), ServiceError> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).unit = Some(text.to_owned());
        Ok(())
    }
    fn restart(&self) -> Result<(), ServiceError> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).active = true;
        Ok(())
    }
    fn stop(&self) -> Result<(), ServiceError> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).active = false;
        Ok(())
    }
    fn disable(&self) -> Result<(), ServiceError> {
        Ok(())
    }
    fn remove(&self) -> Result<(), ServiceError> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).unit = None;
        Ok(())
    }
}

/// Écrit de vrais fichiers, aux vrais noms, dans le vrai dossier de données.
struct Identity(PathBuf);

impl IdentityStore for Identity {
    fn load_or_create(&self) -> Result<PublicIdentity, IdentityError> {
        for name in ["cert.pem", "key.pem", "install_id"] {
            let path = self.0.join(name);
            if !path.exists() {
                std::fs::write(&path, format!("cree-par-le-test:{name}")).map_err(|source| {
                    IdentityError::Storage {
                        path: path.display().to_string(),
                        source,
                    }
                })?;
            }
        }
        Ok(PublicIdentity {
            install_id: InstallId::from_bytes([9; 16]),
            fingerprint: fingerprint(),
        })
    }
}

struct Admins(PathBuf);

#[async_trait]
impl AdminAccounts for Admins {
    fn check_hash(&self, _hash: &Secret) -> Result<(), AdminAccountsError> {
        Ok(())
    }
    async fn admin_count(&self) -> Result<u64, AdminAccountsError> {
        let text = std::fs::read_to_string(self.0.join("hearth.db")).unwrap_or_default();
        Ok(u64::from(text.contains("admin")))
    }
    async fn create_admin(
        &self,
        _name: &Username,
        _credential: AdminCredential,
    ) -> Result<(), AdminAccountsError> {
        std::fs::write(self.0.join("hearth.db"), "admin")
            .map_err(|error| AdminAccountsError::Failed(error.to_string()))
    }
    async fn remove_created(&self, _name: &Username) -> Result<(), AdminAccountsError> {
        let _ = std::fs::remove_file(self.0.join("hearth.db"));
        Ok(())
    }
}

#[derive(Default)]
struct NoPrompt(VecDeque<String>);

impl Prompter for NoPrompt {
    fn line(&mut self, _label: &str) -> io::Result<String> {
        self.0
            .pop_front()
            .ok_or_else(|| io::Error::other("aucune question attendue"))
    }
    fn secret(&mut self, _label: &str) -> io::Result<Secret> {
        Err(io::Error::other("aucune question attendue"))
    }
}

struct Machine {
    root: tempfile::TempDir,
    paths: InstallPaths,
    service: Arc<Mutex<ServiceState>>,
    interrupted: AtomicBool,
}

impl Machine {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("dossier temporaire");
        let base = root.path().to_path_buf();
        std::fs::create_dir_all(base.join("bin")).expect("bin");
        std::fs::write(base.join("source"), b"binaire").expect("source");
        let paths = InstallPaths {
            binary: base.join("bin").join("hearth-agent"),
            config: base.join("etc").join("agent.toml"),
            data_dir: base.join("data"),
            lock: base.join("install.lock"),
        };
        Self {
            root,
            paths,
            service: Arc::new(Mutex::new(ServiceState::default())),
            interrupted: AtomicBool::new(false),
        }
    }

    /// Un dossier de données déjà là, fermé aux autres comme celui d'un vrai serveur.
    fn make_data_dir(&self) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(&self.paths.data_dir).unwrap();
        std::fs::set_permissions(&self.paths.data_dir, std::fs::Permissions::from_mode(0o700))
            .unwrap();
    }

    fn data(&self, name: &str) -> PathBuf {
        self.paths.data_dir.join(name)
    }

    /// Tous les fichiers sous la racine, avec leur contenu : l'état complet de la « machine ».
    fn snapshot(&self) -> Vec<(String, String)> {
        fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, String)>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.strip_prefix(base).unwrap().display().to_string();
                if name == "install.lock" {
                    continue;
                }
                if path.is_dir() {
                    out.push((format!("{name}/"), String::new()));
                    walk(&path, base, out);
                } else {
                    out.push((name, std::fs::read_to_string(&path).unwrap_or_default()));
                }
            }
        }
        let mut out = Vec::new();
        walk(self.root.path(), self.root.path(), &mut out);
        out.sort();
        out
    }

    async fn install(&self, admin: bool) -> Result<String, InstallCliError> {
        let env = move |name: &str| match name {
            "HEARTH_ADMIN_USER" if admin => Some("marie".to_owned()),
            "HEARTH_ADMIN_PASSWORD" if admin => Some("Cheval-Agrafe-42".to_owned()),
            _ => None,
        };
        let (host, service, identity, admins) = (
            Host(SystemHost),
            Service(self.service.clone()),
            Identity(self.paths.data_dir.clone()),
            Admins(self.paths.data_dir.clone()),
        );
        let installer = Installer {
            host: &host,
            service: &service,
            identity: &identity,
            admins: &admins,
            paths: self.paths.clone(),
            interrupted: &self.interrupted,
        };
        let context = Context {
            env: &env,
            interactive: false,
            source_binary: self.root.path().join("source"),
            listen_addr: IpAddr::from([0, 0, 0, 0]),
            data_dir_override: None,
            target: TARGET,
            command_line: "hearth-agent install".to_owned(),
            systemd_missing: false,
        };
        let mut out = Vec::new();
        install::install(
            &InstallArgs {
                port: None,
                managed: false,
                yes: true,
            },
            &context,
            &installer,
            &mut NoPrompt::default(),
            &mut out,
        )
        .await?;
        Ok(String::from_utf8(out).unwrap())
    }

    async fn uninstall(&self, purge: bool) -> Result<String, InstallCliError> {
        let env = |_: &str| None;
        let (host, service, identity, admins) = (
            Host(SystemHost),
            Service(self.service.clone()),
            Identity(self.paths.data_dir.clone()),
            Admins(self.paths.data_dir.clone()),
        );
        let installer = Installer {
            host: &host,
            service: &service,
            identity: &identity,
            admins: &admins,
            paths: self.paths.clone(),
            interrupted: &self.interrupted,
        };
        let context = Context {
            env: &env,
            interactive: false,
            source_binary: self.root.path().join("source"),
            listen_addr: IpAddr::from([0, 0, 0, 0]),
            data_dir_override: None,
            target: TARGET,
            command_line: "hearth-agent uninstall".to_owned(),
            systemd_missing: false,
        };
        let mut out = Vec::new();
        install::uninstall(
            &UninstallArgs {
                keep_data: !purge,
                purge,
                yes: true,
                managed: false,
            },
            &context,
            &installer,
            &mut NoPrompt::default(),
            &mut out,
        )
        .await?;
        Ok(String::from_utf8(out).unwrap())
    }
}

#[tokio::test]
async fn a_partial_identity_is_refused_before_any_write_and_nothing_is_deleted() {
    let machine = Machine::new();
    machine.make_data_dir();
    // Un certificat et une clé sans identifiant d'installation.
    std::fs::write(machine.data("cert.pem"), "mon-certificat").unwrap();
    std::fs::write(machine.data("key.pem"), "ma-cle").unwrap();
    let before = machine.snapshot();

    let error = machine.install(true).await.unwrap_err().to_string();
    assert!(
        error.contains("identité du serveur est incomplète"),
        "{error}"
    );
    assert!(error.contains("Rien n'a été modifié"), "{error}");
    assert_eq!(
        machine.snapshot(),
        before,
        "chaque fichier, chaque octet, au même endroit"
    );
    assert!(!machine.paths.binary.exists());
    assert!(!machine.paths.config.exists());
}

#[tokio::test]
async fn a_failure_after_the_identity_never_deletes_what_existed_before() {
    let machine = Machine::new();
    // Une installation complète existe : identité, base avec un compte, configuration, unité.
    machine.install(true).await.unwrap();
    std::fs::write(machine.data("notes.txt"), "pas à Hearth").unwrap();
    // Réinstallation avec un autre binaire, et le service refuse l'unité.
    std::fs::write(machine.root.path().join("source"), b"autre binaire").unwrap();
    let before = machine.snapshot();
    let unit_before = machine.service.lock().unwrap().unit.clone();
    machine.service.lock().unwrap().fail_install = true;
    let error = machine.install(false).await.unwrap_err().to_string();
    assert!(error.contains("Une erreur s'est produite"), "{error}");
    assert!(
        error.contains("Aucune modification n'a été apportée"),
        "{error}"
    );

    assert_eq!(
        machine.snapshot(),
        before,
        "identité, base, configuration, binaire : intacts"
    );
    assert_eq!(machine.service.lock().unwrap().unit, unit_before);
}

#[tokio::test]
async fn a_purge_removes_only_the_files_hearth_knows_and_leaves_a_shared_directory() {
    let machine = Machine::new();
    machine.install(true).await.unwrap();
    // Le dossier de données est partagé avec autre chose.
    std::fs::write(machine.data("notes.txt"), "pas à Hearth").unwrap();
    std::fs::create_dir(machine.data("autre-dossier")).unwrap();
    std::fs::write(machine.data("autre-dossier").join("x"), "x").unwrap();

    let out = machine.uninstall(true).await.unwrap();
    for name in ["cert.pem", "key.pem", "install_id", "hearth.db"] {
        assert!(!machine.data(name).exists(), "{name} devrait être supprimé");
    }
    assert!(!machine.paths.config.exists());
    assert!(!machine.paths.binary.exists());
    assert_eq!(
        std::fs::read_to_string(machine.data("notes.txt")).unwrap(),
        "pas à Hearth",
        "le fichier d'un autre survit"
    );
    assert!(machine.data("autre-dossier").join("x").exists());
    assert!(out.contains("notes.txt"), "{out}");
    assert!(out.contains("autre-dossier"), "{out}");
    assert!(out.contains("n'ont pas été touchés"), "{out}");
}

#[tokio::test]
async fn a_purge_of_a_directory_that_holds_only_hearth_files_removes_the_directory() {
    let machine = Machine::new();
    machine.install(true).await.unwrap();
    let out = machine.uninstall(true).await.unwrap();
    assert!(!machine.paths.data_dir.exists());
    assert!(
        out.contains("Aucune trace de l'agent ne reste sur ta machine."),
        "{out}"
    );
}

#[tokio::test]
async fn a_failed_first_installation_removes_what_it_created_and_nothing_else() {
    let machine = Machine::new();
    // Le dossier de données existe, partagé ; la configuration aussi (autre fichier voisin).
    machine.make_data_dir();
    std::fs::write(machine.data("notes.txt"), "pas à Hearth").unwrap();
    let before = machine.snapshot();
    machine.service.lock().unwrap().fail_install = true;
    let error = machine.install(true).await.unwrap_err().to_string();
    assert!(
        error.contains("Aucune modification n'a été apportée"),
        "{error}"
    );
    assert_eq!(machine.snapshot(), before);
}
