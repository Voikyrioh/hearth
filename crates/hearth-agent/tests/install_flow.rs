//! Installation et désinstallation de bout en bout **sur une machine simulée** : les ports
//! (machine, service, identité, comptes) sont remplacés par des doubles qui tiennent l'état d'une
//! machine ; le vrai flux (`entrypoint::install`) et le vrai cas d'usage (`Installer`) tournent.
//! On vérifie ce qui est dit, ce qui est fait, et surtout l'état de la machine après un échec ou
//! une interruption (BR-INSTALL-008). Le bout en bout réel, dans un conteneur, est
//! `cargo xtask e2e-install`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::VecDeque;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use hearth_agent::application::install::Installer;
use hearth_agent::application::ports::{
    AdminAccounts, AdminAccountsError, AdminCredential, Answered, BinaryInstalled, ConfigSpec,
    HostError, HostFacts, IdentityError, IdentityStore, InstallHost, InstallLock, InstallPaths,
    PublicIdentity, ServiceError, ServiceKind, ServiceManager, ServiceSpec,
};
use hearth_agent::domain::accounts::Username;
use hearth_agent::domain::install::{BinaryState, DataDirState, DataState, Version};
use hearth_agent::domain::install_id::InstallId;
use hearth_agent::domain::secret::Secret;
use hearth_agent::entrypoint::cli::{InstallArgs, UninstallArgs};
use hearth_agent::entrypoint::install::{self, Context, InstallCliError, Prompter};
use hearth_proto::fingerprint::Fingerprint;

const TARGET: Version = Version::new(0, 2, 0);
const GOOD_PASSWORD: &str = "Cheval-Agrafe-42";

fn fingerprint() -> Fingerprint {
    Fingerprint::from_bytes([0xAB; 32])
}

/// L'état de la machine simulée.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
struct World {
    privileged: bool,
    arch: String,
    port_taken: bool,
    locked: bool,
    binary: Option<(Version, Vec<u8>)>,
    backup: Option<(Version, Vec<u8>)>,
    unit: bool,
    unit_text: String,
    active: bool,
    enabled: bool,
    data_dir: bool,
    identity: bool,
    database: bool,
    config_port: Option<u16>,
    config_managed: Option<bool>,
    admins: Vec<String>,
    /// Étapes qui échouent (« binary », « config », « identity », « admin », « service »,
    /// « hello »).
    fail: Vec<&'static str>,
    /// Le service répond avec un autre certificat.
    wrong_certificate: bool,
    /// Levé quand l'étape nommée est franchie : simule Ctrl+C.
    interrupt_after: Option<&'static str>,
    /// Appels de `restart`.
    restarts: u32,
    log: Vec<String>,
}

#[derive(Clone)]
struct Machine {
    world: Arc<Mutex<World>>,
    interrupted: Arc<AtomicBool>,
}

impl Machine {
    fn new() -> Self {
        let world = World {
            privileged: true,
            arch: "x86_64".to_owned(),
            ..World::default()
        };
        Self {
            world: Arc::new(Mutex::new(world)),
            interrupted: Arc::new(AtomicBool::new(false)),
        }
    }

    fn world(&self) -> MutexGuard<'_, World> {
        self.world.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn snapshot(&self) -> World {
        let mut world = self.world().clone();
        world.log.clear();
        world.restarts = 0;
        world
    }

    fn step(&self, name: &'static str) -> Result<(), String> {
        let mut world = self.world();
        world.log.push(name.to_owned());
        if world.fail.contains(&name) {
            return Err(format!("échec simulé : {name}"));
        }
        if world.interrupt_after == Some(name) {
            self.interrupted
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(())
    }

    /// Une installation complète et à jour, avec un compte.
    fn installed(&self, version: Version, active: bool) {
        let mut world = self.world();
        world.binary = Some((version, b"binaire".to_vec()));
        world.unit = true;
        world.unit_text = "unite-v1".to_owned();
        world.enabled = true;
        world.active = active;
        world.data_dir = true;
        world.identity = true;
        world.database = true;
        world.config_port = Some(7341);
        world.config_managed = Some(false);
        world.admins = vec!["marie".to_owned()];
    }
}

fn host_error(message: String) -> HostError {
    HostError::Other(message)
}

struct FakeHost(Machine);

impl InstallHost for FakeHost {
    fn os(&self) -> String {
        "linux".to_owned()
    }

    fn arch(&self) -> String {
        self.0.world().arch.clone()
    }

    fn is_privileged(&self) -> bool {
        self.0.world().privileged
    }

    fn port_taken(&self, _addr: IpAddr, _port: u16) -> bool {
        self.0.world().port_taken
    }

    fn free_bytes(&self, _path: &Path) -> Result<u64, HostError> {
        Ok(u64::MAX)
    }

    fn hostname(&self) -> String {
        "serveur".to_owned()
    }

    fn inspect(&self, _paths: &InstallPaths, _source: &Path) -> Result<HostFacts, HostError> {
        let world = self.0.world();
        Ok(HostFacts {
            binary: match &world.binary {
                Some((version, bytes)) => BinaryState::Present {
                    version: Some(*version),
                    identical: bytes == b"nouveau",
                },
                None => BinaryState::Absent,
            },
            data: DataState {
                dir_exists: world.data_dir,
                identity: world.identity,
                database: world.database,
                identity_partial: false,
            },
            config_exists: world.config_port.is_some(),
            configured_port: world.config_port,
        })
    }

    fn lock(&self, _path: &Path) -> Result<InstallLock, HostError> {
        let mut world = self.0.world();
        if world.locked {
            return Err(HostError::AlreadyRunning);
        }
        world.locked = true;
        Ok(InstallLock(Box::new(LockGuard(self.0.clone()))))
    }

    fn install_binary(
        &self,
        _source: &Path,
        dest: &Path,
        _backup: &Path,
    ) -> Result<BinaryInstalled, HostError> {
        self.0.step("binary").map_err(host_error)?;
        let mut world = self.0.world();
        let previous = world.binary.replace((TARGET, b"nouveau".to_vec()));
        let had = previous.is_some();
        world.backup = previous;
        Ok(BinaryInstalled {
            backup: had.then(|| dest.with_extension("previous")),
        })
    }

    fn restore_binary(&self, _dest: &Path, _installed: &BinaryInstalled) -> Result<(), HostError> {
        let mut world = self.0.world();
        world.log.push("restore-binary".to_owned());
        world.binary = world.backup.take();
        Ok(())
    }

    fn discard_backup(&self, _installed: &BinaryInstalled) {
        self.0.world().backup = None;
    }

    fn ensure_data_dir(&self, _dir: &Path) -> Result<bool, HostError> {
        let mut world = self.0.world();
        let created = !world.data_dir;
        world.data_dir = true;
        Ok(created)
    }

    fn write_config_new(&self, _path: &Path, spec: &ConfigSpec) -> Result<bool, HostError> {
        self.0.step("config").map_err(host_error)?;
        let mut world = self.0.world();
        if world.config_port.is_some() {
            return Ok(false);
        }
        world.config_port = Some(spec.port);
        world.config_managed = Some(spec.managed);
        Ok(true)
    }

    fn remove_file(&self, path: &Path) -> Result<(), HostError> {
        let mut world = self.0.world();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        match name {
            "cert.pem" | "key.pem" | "install_id" => world.identity = false,
            "hearth.db" => {
                world.database = false;
                world.admins.clear();
            }
            "agent.toml" => {
                world.config_port = None;
                world.config_managed = None;
            }
            "hearth-agent" => world.binary = None,
            _ => {}
        }
        Ok(())
    }

    fn list_dir(&self, _path: &Path) -> Result<Vec<String>, HostError> {
        Ok(Vec::new())
    }

    fn data_dir_state(&self, _path: &Path) -> DataDirState {
        DataDirState::ABSENT
    }

    fn remove_dir_if_empty(&self, path: &Path) -> Result<(), HostError> {
        if path.ends_with("hearth") && self.0.world().data_dir {
            // Le dossier de données : vide une fois ses fichiers retirés un à un.
            self.0.world().data_dir = false;
        }
        Ok(())
    }

    fn wait_for_hello(
        &self,
        _addr: SocketAddr,
        _timeout: Duration,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Answered, HostError> {
        if cancelled() {
            return Err(host_error("interrompu".to_owned()));
        }
        self.0.step("hello").map_err(host_error)?;
        let world = self.0.world();
        Ok(Answered {
            served: if world.wrong_certificate {
                Fingerprint::from_bytes([0x11; 32])
            } else {
                fingerprint()
            },
        })
    }
}

struct LockGuard(Machine);

impl Drop for LockGuard {
    fn drop(&mut self) {
        self.0.world().locked = false;
    }
}

struct FakeService(Machine);

fn service_error(message: String) -> ServiceError {
    ServiceError::Command {
        command: "simulé".to_owned(),
        detail: message,
    }
}

impl ServiceManager for FakeService {
    fn kind(&self) -> ServiceKind {
        ServiceKind::Systemd
    }

    fn is_installed(&self) -> Result<bool, ServiceError> {
        Ok(self.0.world().unit)
    }

    fn is_active(&self) -> Result<bool, ServiceError> {
        Ok(self.0.world().active)
    }

    fn install(&self, _spec: &ServiceSpec) -> Result<(), ServiceError> {
        self.0.step("service").map_err(service_error)?;
        let mut world = self.0.world();
        world.unit = true;
        world.unit_text = "unite-v2".to_owned();
        world.enabled = true;
        world.active = true;
        Ok(())
    }

    fn unit_text(&self) -> Result<Option<String>, ServiceError> {
        let world = self.0.world();
        Ok(world.unit.then(|| world.unit_text.clone()))
    }

    fn restore_unit(&self, text: &str) -> Result<(), ServiceError> {
        let mut world = self.0.world();
        world.unit = true;
        world.unit_text = text.to_owned();
        Ok(())
    }

    fn restart(&self) -> Result<(), ServiceError> {
        let mut world = self.0.world();
        world.restarts += 1;
        world.active = true;
        Ok(())
    }

    fn stop(&self) -> Result<(), ServiceError> {
        self.0.world().active = false;
        Ok(())
    }

    fn disable(&self) -> Result<(), ServiceError> {
        self.0.world().enabled = false;
        Ok(())
    }

    fn remove(&self) -> Result<(), ServiceError> {
        let mut world = self.0.world();
        world.unit = false;
        world.unit_text.clear();
        Ok(())
    }
}

struct FakeIdentity(Machine);

impl IdentityStore for FakeIdentity {
    fn load_or_create(&self) -> Result<PublicIdentity, IdentityError> {
        self.0.step("identity").map_err(IdentityError::Generation)?;
        self.0.world().identity = true;
        Ok(PublicIdentity {
            install_id: InstallId::from_bytes([9; 16]),
            fingerprint: fingerprint(),
        })
    }
}

struct FakeAdmins(Machine);

#[async_trait]
impl AdminAccounts for FakeAdmins {
    fn check_hash(&self, hash: &Secret) -> Result<(), AdminAccountsError> {
        if hash.expose().starts_with("$argon2id$") {
            Ok(())
        } else {
            Err(AdminAccountsError::Failed("haché illisible".to_owned()))
        }
    }

    async fn admin_count(&self) -> Result<u64, AdminAccountsError> {
        Ok(self.0.world().admins.len() as u64)
    }

    async fn create_admin(
        &self,
        name: &Username,
        credential: AdminCredential,
    ) -> Result<(), AdminAccountsError> {
        self.0.step("admin").map_err(AdminAccountsError::Failed)?;
        let mut world = self.0.world();
        world.database = true;
        world.admins.push(name.to_string());
        world.log.push(match credential {
            AdminCredential::Password(_) => "admin-password".to_owned(),
            AdminCredential::Hash(_) => "admin-hash".to_owned(),
        });
        Ok(())
    }

    async fn remove_created(&self, name: &Username) -> Result<(), AdminAccountsError> {
        self.0.world().admins.retain(|admin| admin != name.as_str());
        Ok(())
    }
}

/// Les réponses de l'utilisateur, dans l'ordre ; ce qui est demandé est consigné.
#[derive(Default)]
struct Script {
    lines: VecDeque<String>,
    secrets: VecDeque<String>,
    asked: Vec<String>,
}

impl Prompter for Script {
    fn line(&mut self, label: &str) -> io::Result<String> {
        self.asked.push(label.to_owned());
        self.lines
            .pop_front()
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "plus de réponse"))
    }

    fn secret(&mut self, label: &str) -> io::Result<Secret> {
        self.asked.push(label.to_owned());
        self.secrets
            .pop_front()
            .map(Secret::new)
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "plus de réponse"))
    }
}

fn script(lines: &[&str], secrets: &[&str]) -> Script {
    Script {
        lines: lines.iter().map(|s| (*s).to_owned()).collect(),
        secrets: secrets.iter().map(|s| (*s).to_owned()).collect(),
        asked: Vec::new(),
    }
}

struct Run {
    result: Result<(), InstallCliError>,
    out: String,
    script: Script,
}

fn install_args() -> InstallArgs {
    InstallArgs {
        port: None,
        managed: false,
        yes: false,
    }
}

fn uninstall_args() -> UninstallArgs {
    UninstallArgs {
        keep_data: false,
        purge: false,
        yes: false,
        managed: false,
    }
}

fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
    let map: Vec<(String, String)> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    move |name| map.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
}

fn paths() -> InstallPaths {
    InstallPaths {
        binary: PathBuf::from("/usr/local/bin/hearth-agent"),
        config: PathBuf::from("/etc/hearth/agent.toml"),
        data_dir: PathBuf::from("/var/lib/hearth"),
        lock: PathBuf::from("/run/hearth-agent-install.lock"),
    }
}

async fn run_install(
    machine: &Machine,
    args: &InstallArgs,
    env: &[(&str, &str)],
    interactive: bool,
    mut script: Script,
) -> Run {
    let env = env_of(env);
    let (host, service, identity, admins) = (
        FakeHost(machine.clone()),
        FakeService(machine.clone()),
        FakeIdentity(machine.clone()),
        FakeAdmins(machine.clone()),
    );
    let installer = Installer {
        host: &host,
        service: &service,
        identity: &identity,
        admins: &admins,
        paths: paths(),
        interrupted: &machine.interrupted,
    };
    let context = Context {
        env: &env,
        interactive,
        source_binary: PathBuf::from("/tmp/hearth-agent"),
        listen_addr: IpAddr::from([0, 0, 0, 0]),
        data_dir_override: None,
        target: TARGET,
        command_line: "hearth-agent install".to_owned(),
        systemd_missing: false,
    };
    let mut out = Vec::new();
    let result = install::install(args, &context, &installer, &mut script, &mut out).await;
    Run {
        result,
        out: String::from_utf8(out).expect("UTF-8"),
        script,
    }
}

async fn run_uninstall(
    machine: &Machine,
    args: &UninstallArgs,
    interactive: bool,
    mut script: Script,
) -> Run {
    let env = env_of(&[]);
    let (host, service, identity, admins) = (
        FakeHost(machine.clone()),
        FakeService(machine.clone()),
        FakeIdentity(machine.clone()),
        FakeAdmins(machine.clone()),
    );
    let installer = Installer {
        host: &host,
        service: &service,
        identity: &identity,
        admins: &admins,
        paths: paths(),
        interrupted: &machine.interrupted,
    };
    let context = Context {
        env: &env,
        interactive,
        source_binary: PathBuf::from("/tmp/hearth-agent"),
        listen_addr: IpAddr::from([0, 0, 0, 0]),
        data_dir_override: None,
        target: TARGET,
        command_line: "hearth-agent uninstall".to_owned(),
        systemd_missing: false,
    };
    let mut out = Vec::new();
    let result = install::uninstall(args, &context, &installer, &mut script, &mut out).await;
    Run {
        result,
        out: String::from_utf8(out).expect("UTF-8"),
        script,
    }
}

const ADMIN_ENV: [(&str, &str); 2] = [
    ("HEARTH_ADMIN_USER", "marie"),
    ("HEARTH_ADMIN_PASSWORD", GOOD_PASSWORD),
];

impl Run {
    fn error(&self) -> String {
        self.result
            .as_ref()
            .err()
            .map_or_else(String::new, ToString::to_string)
    }
}

// ----------------------------------------------------------------------------------------------
// Première installation
// ----------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_non_interactive_first_installation_creates_everything_and_says_so() {
    let machine = Machine::new();
    let run = run_install(
        &machine,
        &install_args(),
        &ADMIN_ENV,
        false,
        Script::default(),
    )
    .await;
    assert!(run.result.is_ok(), "{}", run.error());
    let world = machine.world();
    assert_eq!(world.binary.as_ref().map(|b| b.0), Some(TARGET));
    assert!(
        world.unit && world.enabled && world.active,
        "BR-INSTALL-005"
    );
    assert!(world.identity && world.database && world.data_dir);
    assert_eq!(world.config_port, Some(7341));
    assert_eq!(world.admins, ["marie"], "BR-INSTALL-002");
    assert!(world.log.contains(&"admin-password".to_owned()));
    for line in [
        "Installation de l'agent Hearth",
        "Vérification de l'architecture système...",
        "Vérification de la disponibilité du port...",
        "Installation en cours...",
        "Démarrage du service...",
        "Installation réussie. L'agent démarre automatiquement avec ton serveur.",
        "Empreinte du serveur : ABAB ABAB ABAB ABAB ABAB ABAB ABAB ABAB",
        "Note cette empreinte. Elle apparaîtra dans le client lors de la première connexion. Compare-la avec celle que le client affichera à la première connexion.",
        "Adresse à saisir dans le client : serveur:7341",
    ] {
        assert!(run.out.contains(line), "manque : {line}\n{}", run.out);
    }
    assert!(
        !run.out.contains(GOOD_PASSWORD),
        "aucun mot de passe affiché"
    );
    assert!(!run.out.contains('—'), "pas de tiret cadratin");
}

#[tokio::test]
async fn the_port_comes_from_the_option_then_the_variable_and_ends_up_in_the_configuration() {
    let machine = Machine::new();
    let mut args = install_args();
    args.port = Some("9100".to_owned());
    let mut env = ADMIN_ENV.to_vec();
    env.push(("HEARTH_PORT", "9200"));
    let run = run_install(&machine, &args, &env, false, Script::default()).await;
    assert!(run.result.is_ok(), "{}", run.error());
    assert_eq!(machine.world().config_port, Some(9100));
    assert!(run.out.contains("serveur:9100"));
}

#[tokio::test]
async fn a_password_hash_creates_the_account_without_any_password() {
    let machine = Machine::new();
    let env = [
        ("HEARTH_ADMIN_USER", "marie"),
        (
            "HEARTH_ADMIN_PASSWORD_HASH",
            "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$aGFzaGhhc2hoYXNoaGFzaGhhc2hoYXNoaGFzaGhhc2g",
        ),
    ];
    let run = run_install(&machine, &install_args(), &env, false, Script::default()).await;
    assert!(run.result.is_ok(), "{}", run.error());
    assert!(machine.world().log.contains(&"admin-hash".to_owned()));
    assert!(!run.out.contains("argon2"), "le haché n'est pas affiché");
}

#[tokio::test]
async fn an_interactive_installation_asks_the_name_the_password_twice_and_the_port() {
    let machine = Machine::new();
    let run = run_install(
        &machine,
        &install_args(),
        &[],
        true,
        script(&["", "marie"], &[GOOD_PASSWORD, GOOD_PASSWORD]),
    )
    .await;
    // Le port d'abord (entrée vide : valeur par défaut), puis le nom.
    assert!(run.result.is_ok(), "{}", run.error());
    assert_eq!(
        run.script.asked,
        [
            "Numéro de port [7341]:",
            "Nom du compte administrateur:",
            "Mot de passe:",
            "Confirme le mot de passe:"
        ]
    );
    assert!(run.out.contains(
        "Utilise des lettres minuscules, des chiffres, des tirets ou des underscores. Minimum 3 caractères."
    ));
    assert!(
        run.out
            .contains("Minimum 12 caractères, au moins 1 majuscule, 1 minuscule et 1 chiffre.")
    );
    assert_eq!(machine.world().admins, ["marie"]);
    assert_eq!(machine.world().config_port, Some(7341));
}

#[tokio::test]
async fn invalid_entries_are_explained_and_asked_again_without_leaving() {
    let machine = Machine::new();
    let run = run_install(
        &machine,
        &install_args(),
        &[],
        true,
        // port vide, nom vide, nom invalide, nom correct
        script(
            &["", "", "Marie Dupont", "marie"],
            &[
                "court",
                "",
                GOOD_PASSWORD,
                "autre",
                GOOD_PASSWORD,
                GOOD_PASSWORD,
            ],
        ),
    )
    .await;
    assert!(run.result.is_ok(), "{}", run.error());
    for line in [
        "Le nom du compte est requis.",
        "Le nom du compte contient des caractères non autorisés.",
        "Le mot de passe est trop court.",
        "Le mot de passe est requis.",
        "Les deux mots de passe ne correspondent pas.",
    ] {
        assert!(run.out.contains(line), "manque : {line}\n{}", run.out);
    }
    assert_eq!(machine.world().admins, ["marie"]);
}

#[tokio::test]
async fn without_a_terminal_and_without_variables_nothing_is_written() {
    let machine = Machine::new();
    let before = machine.snapshot();
    let run = run_install(&machine, &install_args(), &[], false, Script::default()).await;
    assert!(
        matches!(run.result, Err(InstallCliError::Refused(_))),
        "{}",
        run.error()
    );
    assert!(run.error().contains("HEARTH_ADMIN_USER"));
    assert_eq!(machine.snapshot(), before, "BR-INSTALL-006 : rien d'écrit");
}

#[tokio::test]
async fn a_bad_variable_is_refused_before_any_write() {
    for (user, password, expected) in [
        (
            "Marie Dupont",
            GOOD_PASSWORD,
            "Le nom du compte contient des caractères non autorisés.",
        ),
        ("marie", "court", "Le mot de passe est trop court."),
        (
            "marie",
            "pasdemajuscule12",
            "Le mot de passe est trop court.",
        ),
    ] {
        let machine = Machine::new();
        let before = machine.snapshot();
        let env = [
            ("HEARTH_ADMIN_USER", user),
            ("HEARTH_ADMIN_PASSWORD", password),
        ];
        let run = run_install(&machine, &install_args(), &env, false, Script::default()).await;
        assert!(run.error().contains(expected), "{}", run.error());
        assert_eq!(machine.snapshot(), before, "{user} / {password}");
    }
    let machine = Machine::new();
    let env = [
        ("HEARTH_ADMIN_USER", "marie"),
        ("HEARTH_ADMIN_PASSWORD_HASH", "pas-un-hache"),
    ];
    let run = run_install(&machine, &install_args(), &env, false, Script::default()).await;
    assert!(run.error().contains("haché Argon2id"), "{}", run.error());
    assert_eq!(machine.snapshot(), Machine::new().snapshot());
}

#[tokio::test]
async fn a_bad_port_is_refused_before_any_write() {
    let machine = Machine::new();
    let mut args = install_args();
    args.port = Some("99999".to_owned());
    let run = run_install(&machine, &args, &ADMIN_ENV, false, Script::default()).await;
    assert!(matches!(run.result, Err(InstallCliError::Refused(_))));
    assert_eq!(machine.snapshot(), Machine::new().snapshot());
}

// ----------------------------------------------------------------------------------------------
// Prérequis : rien n'est écrit
// ----------------------------------------------------------------------------------------------

#[tokio::test]
async fn without_administration_rights_it_stops_with_the_exact_command_and_writes_nothing() {
    let machine = Machine::new();
    machine.world().privileged = false;
    let before = machine.snapshot();
    let run = run_install(
        &machine,
        &install_args(),
        &ADMIN_ENV,
        false,
        Script::default(),
    )
    .await;
    let error = run.error();
    assert!(
        error.starts_with("Droits d'administration requis. Relance cette commande avec les droits d'administration."),
        "{error}"
    );
    assert!(error.contains("sudo hearth-agent install"), "{error}");
    assert_eq!(machine.snapshot(), before, "BR-INSTALL-001");
}

#[tokio::test]
async fn an_unsupported_architecture_is_refused_and_nothing_is_written() {
    let machine = Machine::new();
    machine.world().arch = "riscv64".to_owned();
    let before = machine.snapshot();
    let run = run_install(
        &machine,
        &install_args(),
        &ADMIN_ENV,
        false,
        Script::default(),
    )
    .await;
    assert_eq!(
        run.error(),
        "Cette architecture n'est pas prise en charge. Architecture supportée : x86_64 (arm64 viendra plus tard)."
    );
    assert_eq!(machine.snapshot(), before, "BR-INSTALL-012");
}

#[tokio::test]
async fn a_taken_port_is_refused_with_the_command_to_relaunch_and_nothing_is_written() {
    let machine = Machine::new();
    machine.world().port_taken = true;
    let before = machine.snapshot();
    let run = run_install(
        &machine,
        &install_args(),
        &ADMIN_ENV,
        false,
        Script::default(),
    )
    .await;
    let error = run.error();
    assert!(
        error.starts_with(
            "Le port configuré est déjà utilisé. Relance en choisissant un autre port."
        ),
        "{error}"
    );
    assert!(
        error.contains("hearth-agent install --port 7342"),
        "{error}"
    );
    assert_eq!(machine.snapshot(), before, "BR-INSTALL-006");
}

#[tokio::test]
async fn a_second_simultaneous_installation_is_refused() {
    let machine = Machine::new();
    machine.world().locked = true;
    let run = run_install(
        &machine,
        &install_args(),
        &ADMIN_ENV,
        false,
        Script::default(),
    )
    .await;
    assert_eq!(
        run.error(),
        "Une installation est déjà en cours sur ce serveur. Attends qu'elle se termine, puis relance."
    );
    assert!(
        machine.world().locked,
        "le verrou de l'autre n'est pas touché"
    );
}

#[tokio::test]
async fn systemd_missing_without_the_managed_flag_is_refused_before_any_write() {
    let machine = Machine::new();
    let env = env_of(&ADMIN_ENV);
    let (host, service, identity, admins) = (
        FakeHost(machine.clone()),
        FakeService(machine.clone()),
        FakeIdentity(machine.clone()),
        FakeAdmins(machine.clone()),
    );
    let installer = Installer {
        host: &host,
        service: &service,
        identity: &identity,
        admins: &admins,
        paths: paths(),
        interrupted: &machine.interrupted,
    };
    let context = Context {
        env: &env,
        interactive: false,
        source_binary: PathBuf::from("/tmp/hearth-agent"),
        listen_addr: IpAddr::from([0, 0, 0, 0]),
        data_dir_override: None,
        target: TARGET,
        command_line: "hearth-agent install".to_owned(),
        systemd_missing: true,
    };
    let mut out = Vec::new();
    let result = install::install(
        &install_args(),
        &context,
        &installer,
        &mut Script::default(),
        &mut out,
    )
    .await;
    assert!(result.unwrap_err().to_string().contains("--managed"));
    assert_eq!(machine.snapshot(), Machine::new().snapshot());
}

// ----------------------------------------------------------------------------------------------
// Retour en arrière (BR-INSTALL-008)
// ----------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_failure_at_any_step_of_a_first_installation_leaves_the_machine_as_before() {
    for step in ["binary", "config", "identity", "admin", "service", "hello"] {
        let machine = Machine::new();
        machine.world().fail.push(step);
        let before = machine.snapshot();
        let run = run_install(
            &machine,
            &install_args(),
            &ADMIN_ENV,
            false,
            Script::default(),
        )
        .await;
        let error = run.error();
        assert!(
            error.starts_with("Une erreur s'est produite : "),
            "{step} : {error}"
        );
        assert!(
            error.ends_with("Aucune modification n'a été apportée à ta machine."),
            "{step} : {error}"
        );
        let mut after = machine.snapshot();
        after.fail.clear();
        let mut expected = before;
        expected.fail.clear();
        assert_eq!(after, expected, "BR-INSTALL-008 : {step}");
    }
}

#[tokio::test]
async fn an_interruption_after_each_step_leaves_the_machine_as_before() {
    for step in ["binary", "config", "identity", "admin", "service"] {
        let machine = Machine::new();
        machine.world().interrupt_after = Some(step);
        let before = machine.snapshot();
        let run = run_install(
            &machine,
            &install_args(),
            &ADMIN_ENV,
            false,
            Script::default(),
        )
        .await;
        assert!(
            matches!(run.result, Err(InstallCliError::Interrupted)),
            "{step} : {}",
            run.error()
        );
        assert_eq!(
            run.error(),
            "Installation interrompue. Aucune modification n'a été apportée à ta machine."
        );
        let mut after = machine.snapshot();
        after.interrupt_after = None;
        let mut expected = before;
        expected.interrupt_after = None;
        assert_eq!(after, expected, "{step}");
    }
}

#[tokio::test]
async fn a_certificate_that_is_not_the_installed_identity_fails_and_is_undone() {
    let machine = Machine::new();
    machine.world().wrong_certificate = true;
    let before = machine.snapshot();
    let run = run_install(
        &machine,
        &install_args(),
        &ADMIN_ENV,
        false,
        Script::default(),
    )
    .await;
    assert!(run.error().contains("certificat"), "{}", run.error());
    let mut after = machine.snapshot();
    after.wrong_certificate = false;
    let mut expected = before;
    expected.wrong_certificate = false;
    assert_eq!(after, expected);
}

#[tokio::test]
async fn a_failed_reinstallation_restores_the_binary_and_keeps_data_and_accounts() {
    let machine = Machine::new();
    machine.installed(Version::new(0, 1, 0), true);
    machine.world().fail.push("service");
    let before = machine.snapshot();
    let run = run_install(&machine, &install_args(), &[], false, Script::default()).await;
    assert!(run.result.is_err());
    let mut after = machine.snapshot();
    after.fail.clear();
    let mut expected = before;
    expected.fail.clear();
    assert_eq!(
        after, expected,
        "ancien binaire rétabli, comptes et identité intacts"
    );
    assert!(
        machine.world().restarts >= 1,
        "le service repart sur l'ancien binaire"
    );
}

// ----------------------------------------------------------------------------------------------
// Réinstallation, mise à niveau, réparation
// ----------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_service_that_was_stopped_stays_stopped_with_its_old_unit_after_a_failure() {
    let machine = Machine::new();
    machine.installed(Version::new(0, 1, 0), false);
    machine.world().fail.push("hello");
    let run = run_install(&machine, &install_args(), &[], false, Script::default()).await;
    assert!(run.result.is_err());
    let world = machine.world();
    assert!(!world.active, "le service arrêté n'a pas été relancé");
    assert_eq!(world.unit_text, "unite-v1", "l'unité d'avant, telle quelle");
    assert_eq!(world.restarts, 0);
}

#[tokio::test]
async fn a_service_that_was_running_runs_again_on_its_old_unit_after_a_failure() {
    let machine = Machine::new();
    machine.installed(Version::new(0, 1, 0), true);
    machine.world().fail.push("hello");
    let run = run_install(&machine, &install_args(), &[], false, Script::default()).await;
    assert!(run.result.is_err());
    let world = machine.world();
    assert!(world.active);
    assert_eq!(world.unit_text, "unite-v1");
    assert!(world.restarts >= 1);
}

#[tokio::test]
async fn a_reinstallation_asks_nothing_keeps_everything_and_restarts_on_the_new_binary() {
    let machine = Machine::new();
    machine.installed(TARGET, true);
    let run = run_install(&machine, &install_args(), &[], false, Script::default()).await;
    assert!(run.result.is_ok(), "{}", run.error());
    let world = machine.world();
    assert_eq!(
        world.admins,
        ["marie"],
        "BR-INSTALL-003 : pas de nouveau compte"
    );
    assert!(world.identity && world.database);
    assert_eq!(world.config_port, Some(7341));
    assert!(run.out.contains("Mise à jour de l'agent Hearth"));
    assert!(
        run.out
            .contains("Agent détecté. Vérification de la version...")
    );
    assert!(
        run.out
            .contains("L'agent est déjà à jour. Service redémarré.")
    );
    assert!(
        run.out
            .contains("Mise à jour réussie. Tes comptes et données sont conservés.")
    );
    assert!(
        run.out.contains("Empreinte du serveur : ABAB"),
        "BR-INSTALL-004 : même empreinte"
    );
    assert!(!run.out.contains("Note cette empreinte"));
    assert!(run.script.asked.is_empty());
}

#[tokio::test]
async fn the_same_binary_on_a_running_service_does_not_interrupt_it() {
    let machine = Machine::new();
    machine.installed(TARGET, true);
    // Le binaire installé a exactement les octets du nouveau.
    machine.world().binary = Some((TARGET, b"nouveau".to_vec()));
    let run = run_install(&machine, &install_args(), &[], false, Script::default()).await;
    assert!(run.result.is_ok(), "{}", run.error());
    let world = machine.world();
    assert_eq!(world.restarts, 0, "BR-INSTALL-007");
    assert!(
        !world.log.contains(&"service".to_owned()),
        "l'unité n'est pas réécrite"
    );
    assert!(!world.log.contains(&"binary".to_owned()));
    assert!(
        run.out
            .contains("L'agent est déjà à jour. Le service n'a pas été interrompu.")
    );
}

#[tokio::test]
async fn an_older_version_is_upgraded_and_the_existing_port_is_kept() {
    let machine = Machine::new();
    machine.installed(Version::new(0, 1, 0), true);
    machine.world().config_port = Some(9000);
    let mut args = install_args();
    args.port = Some("9500".to_owned());
    let run = run_install(&machine, &args, &[], false, Script::default()).await;
    assert!(run.result.is_ok(), "{}", run.error());
    let world = machine.world();
    assert_eq!(world.binary.as_ref().map(|b| b.0), Some(TARGET));
    assert_eq!(
        world.config_port,
        Some(9000),
        "la configuration n'est jamais écrasée"
    );
    assert!(world.restarts >= 1);
    assert!(run.out.contains("Mise à jour depuis la version 0.1.0..."));
    assert!(
        run.out
            .contains("Le port de l'installation existante (9000) est conservé.")
    );
    assert!(run.out.contains("serveur:9000"));
}

#[tokio::test]
async fn a_newer_installed_version_is_refused_and_nothing_changes() {
    let machine = Machine::new();
    machine.installed(Version::new(0, 9, 0), true);
    let before = machine.snapshot();
    let run = run_install(&machine, &install_args(), &[], false, Script::default()).await;
    assert!(run.error().contains("plus récente"), "{}", run.error());
    assert_eq!(machine.snapshot(), before);
}

#[tokio::test]
async fn a_damaged_installation_is_repaired_with_its_data() {
    let machine = Machine::new();
    machine.installed(TARGET, false);
    {
        let mut world = machine.world();
        world.binary = None;
        world.unit = false;
        world.enabled = false;
    }
    let run = run_install(&machine, &install_args(), &[], false, Script::default()).await;
    assert!(run.result.is_ok(), "{}", run.error());
    let world = machine.world();
    assert!(world.binary.is_some() && world.unit && world.active);
    assert_eq!(world.admins, ["marie"]);
    assert!(
        run.out
            .contains("Réparation réussie. Tes comptes et données sont conservés.")
    );
}

#[tokio::test]
async fn a_managed_installation_writes_no_unit_copies_no_binary_and_says_so() {
    let machine = Machine::new();
    let mut args = install_args();
    args.managed = true;
    let run = run_install(&machine, &args, &ADMIN_ENV, false, Script::default()).await;
    assert!(run.result.is_ok(), "{}", run.error());
    let world = machine.world();
    assert!(!world.unit && !world.active && world.binary.is_none());
    assert_eq!(
        world.config_managed,
        Some(true),
        "l'agent saura qu'il est géré"
    );
    assert_eq!(world.admins, ["marie"]);
    assert!(
        run.out.contains("aucune unité n'a été écrite"),
        "{}",
        run.out
    );
    assert!(run.out.contains("Empreinte du serveur : ABAB"));
}

// ----------------------------------------------------------------------------------------------
// Désinstallation
// ----------------------------------------------------------------------------------------------

#[tokio::test]
async fn uninstalling_with_keep_stops_the_service_and_keeps_accounts_and_journal() {
    let machine = Machine::new();
    machine.installed(TARGET, true);
    let mut args = uninstall_args();
    args.keep_data = true;
    let run = run_uninstall(&machine, &args, false, Script::default()).await;
    assert!(run.result.is_ok(), "{}", run.error());
    let world = machine.world();
    assert!(!world.active && !world.enabled && !world.unit && world.binary.is_none());
    assert!(world.database && world.identity && world.config_port.is_some());
    assert_eq!(world.admins, ["marie"], "BR-INSTALL-011");
    assert!(run.out.contains("Désinstallation de l'agent Hearth"));
    assert!(
        run.out
            .contains("Service arrêté. Tes comptes et données sont conservés sur ta machine.")
    );
}

#[tokio::test]
async fn uninstalling_with_purge_removes_everything() {
    let machine = Machine::new();
    machine.installed(TARGET, true);
    let mut args = uninstall_args();
    args.purge = true;
    let run = run_uninstall(&machine, &args, false, Script::default()).await;
    assert!(run.result.is_ok(), "{}", run.error());
    let world = machine.world();
    assert!(world.binary.is_none() && !world.unit && !world.active);
    assert!(!world.data_dir && !world.database && !world.identity);
    assert!(world.config_port.is_none() && world.admins.is_empty());
    assert!(
        run.out
            .contains("Aucune trace de l'agent ne reste sur ta machine.")
    );
}

#[tokio::test]
async fn the_interactive_question_is_asked_until_the_answer_is_valid() {
    let machine = Machine::new();
    machine.installed(TARGET, true);
    let run = run_uninstall(
        &machine,
        &uninstall_args(),
        true,
        script(&["peut-être", "", "Supprimer"], &[]),
    )
    .await;
    assert!(run.result.is_ok(), "{}", run.error());
    assert_eq!(
        run.script.asked,
        ["Supprimer aussi les comptes, le journal et la configuration ? (conserver/supprimer)"; 3]
    );
    assert_eq!(
        run.out
            .matches("Réponds 'conserver' ou 'supprimer'.")
            .count(),
        2
    );
    assert!(!machine.world().database);
}

#[tokio::test]
async fn without_a_terminal_the_choice_is_required_unless_yes_which_keeps() {
    let machine = Machine::new();
    machine.installed(TARGET, true);
    let before = machine.snapshot();
    let run = run_uninstall(&machine, &uninstall_args(), false, Script::default()).await;
    assert!(matches!(run.result, Err(InstallCliError::Refused(_))));
    assert_eq!(machine.snapshot(), before);

    let mut args = uninstall_args();
    args.yes = true;
    let run = run_uninstall(&machine, &args, false, Script::default()).await;
    assert!(run.result.is_ok(), "{}", run.error());
    assert!(machine.world().database, "--yes seul conserve");
}

#[tokio::test]
async fn nothing_installed_means_nothing_to_uninstall() {
    let machine = Machine::new();
    let run = run_uninstall(&machine, &uninstall_args(), false, Script::default()).await;
    assert!(run.result.is_ok());
    assert!(
        run.out
            .contains("L'agent n'est pas installé sur ce serveur. Rien à désinstaller.")
    );
}

#[tokio::test]
async fn uninstalling_without_administration_rights_is_refused() {
    let machine = Machine::new();
    machine.installed(TARGET, true);
    machine.world().privileged = false;
    let before = machine.snapshot();
    let mut args = uninstall_args();
    args.purge = true;
    let run = run_uninstall(&machine, &args, false, Script::default()).await;
    assert!(run.error().starts_with("Droits d'administration requis."));
    assert_eq!(machine.snapshot(), before);
}

#[tokio::test]
async fn data_kept_by_an_uninstall_are_found_again_by_the_next_installation() {
    let machine = Machine::new();
    run_install(
        &machine,
        &install_args(),
        &ADMIN_ENV,
        false,
        Script::default(),
    )
    .await
    .result
    .expect("installation");
    let accounts = machine.world().admins.clone();
    let mut args = uninstall_args();
    args.keep_data = true;
    run_uninstall(&machine, &args, false, Script::default())
        .await
        .result
        .expect("désinstallation");

    let run = run_install(&machine, &install_args(), &[], false, Script::default()).await;
    assert!(run.result.is_ok(), "{}", run.error());
    assert_eq!(
        machine.world().admins,
        accounts,
        "mêmes comptes, aucun compte demandé"
    );
    assert!(run.out.contains("Empreinte du serveur : ABAB"));
    assert!(
        run.out
            .contains("Réparation réussie. Tes comptes et données sont conservés.")
    );
}
