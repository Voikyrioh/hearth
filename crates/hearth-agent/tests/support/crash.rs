//! Une machine où le superviseur de mise à jour peut mourir à n'importe quel point (HRT-27).
//!
//! `CrashHost`, `CrashInstall` et `CrashService` enveloppent les vrais adaptateurs de fichiers
//! (`FsUpdateHost`, `SystemHost`) et un faux service ; chaque opération qui modifie quelque chose
//! passe par deux « points » : juste AVANT et juste APRÈS. Au point choisi, le processus « meurt »
//! (un `panic!` rattrapé par le test) : ce que l'opération a laissé sur le disque est exactement ce
//! que laisserait un signal reçu à cet instant.
//!
//! La machine simulée : le « nouvel agent » migre la base à son démarrage (`avant` devient
//! `migree`) ; l'« ancien agent » qui démarre sur une base migrée est LA panne que la remise de la
//! base dans l'ordre doit empêcher (`broken_pair`) ; l'ancien agent écrit dans la base une fois
//! démarré (`avant` devient `avant+vie`) : remettre la copie de la base après cela est un retour
//! arrière rejoué (`replayed`).
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::net::SocketAddr;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Once, PoisonError};
use std::time::Duration;

use hearth_agent::application::ports::{
    Answered, BinaryInstalled, ConfigSpec, FreeSpace, Greeting, HelloProbe, HostError, HostFacts,
    InstallHost, InstallLock, InstallPaths, ServiceError, ServiceKind, ServiceManager, ServiceSpec,
    ServiceState, SupervisorLock, UpdateHost, UpdateHostError,
};
use hearth_agent::application::update_supervisor::{SuperviseError, Supervised, Supervisor};
use hearth_agent::domain::install::{DataDirState, Version};
use hearth_agent::domain::update::{Job, Marker, SupervisorState, UpdateRecord};
use hearth_agent::infrastructure::clock::SystemClock;
use hearth_agent::infrastructure::install::SystemHost;
use hearth_agent::infrastructure::update::{FsUpdateHost, Launcher};
use hearth_proto::fingerprint::Fingerprint;

pub const OLD: &[u8] = b"old";

fn fingerprint() -> Fingerprint {
    Fingerprint::from_hex(&"ab".repeat(32)).unwrap()
}

/// Le message d'un point de mort : le seul que le crochet de panique tait.
const CRASH: &str = "crash-point";

static QUIET: Once = Once::new();

/// Les morts simulées ne polluent pas la sortie ; toute autre panique s'affiche comme d'habitude.
pub fn silence_simulated_deaths() {
    QUIET.call_once(|| {
        let default = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let simulated = info
                .payload()
                .downcast_ref::<&str>()
                .is_some_and(|text| *text == CRASH);
            if !simulated {
                default(info);
            }
        }));
    });
}

/// Le compteur de points, partagé par les trois enveloppes.
#[derive(Default)]
pub struct Crash {
    ticks: AtomicUsize,
    at: Mutex<Option<usize>>,
}

impl Crash {
    pub fn point(&self) {
        let n = self.ticks.fetch_add(1, Ordering::SeqCst);
        if *self.at.lock().unwrap_or_else(PoisonError::into_inner) == Some(n) {
            std::panic::panic_any(CRASH);
        }
    }

    /// Meurt au point `n` (compté depuis 0) ; `None` : ne meurt jamais.
    pub fn arm(&self, at: Option<usize>) {
        self.ticks.store(0, Ordering::SeqCst);
        *self.at.lock().unwrap_or_else(PoisonError::into_inner) = at;
    }

    pub fn ticks(&self) -> usize {
        self.ticks.load(Ordering::SeqCst)
    }
}

// ---------------------------------------------------------------------------------------------
// La machine : le service, l'agent qui répond
// ---------------------------------------------------------------------------------------------

#[derive(Default)]
pub struct MachineState {
    pub active: bool,
    pub calls: Vec<&'static str>,
    /// L'ancien agent a démarré sur une base déjà migrée.
    pub broken_pair: bool,
    /// L'ancien agent a tourné depuis la dernière remise de la base ou du binaire.
    pub old_ran: bool,
    /// Une remise a été faite (ou tentée) alors que l'ancien agent avait déjà vécu : rejouée.
    pub replayed: bool,
    /// Un échange a été refait alors que le binaire en place était déjà le nouveau.
    pub swap_replayed: bool,
    /// Le service est arrêté à la main (`systemctl stop`) : l'état dit « arrêté ».
    pub stopped_on_purpose: bool,
}

pub struct Machine {
    pub state: Arc<Mutex<MachineState>>,
    pub binary: PathBuf,
    pub database: PathBuf,
}

impl Machine {
    pub fn lock(&self) -> std::sync::MutexGuard<'_, MachineState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub struct CrashService {
    pub machine: Arc<Machine>,
    pub crash: Arc<Crash>,
    /// Les redémarrages échouent.
    pub fail_restart: bool,
}

impl ServiceManager for CrashService {
    fn kind(&self) -> ServiceKind {
        ServiceKind::Systemd
    }
    fn is_installed(&self) -> Result<bool, ServiceError> {
        Ok(true)
    }
    fn is_active(&self) -> Result<bool, ServiceError> {
        Ok(self.machine.lock().active)
    }
    fn state(&self) -> ServiceState {
        let state = self.machine.lock();
        if state.active {
            ServiceState::Active
        } else {
            ServiceState::Inactive
        }
    }
    fn is_enabled(&self) -> Result<bool, ServiceError> {
        Ok(true)
    }
    fn enable(&self) -> Result<(), ServiceError> {
        Ok(())
    }
    fn install(&self, _spec: &ServiceSpec) -> Result<(), ServiceError> {
        Ok(())
    }
    fn unit_text(&self) -> Result<Option<String>, ServiceError> {
        Ok(None)
    }
    fn restore_unit(&self, _text: &str) -> Result<(), ServiceError> {
        Ok(())
    }
    fn restart(&self) -> Result<(), ServiceError> {
        self.crash.point();
        let result = if self.fail_restart {
            Err(ServiceError::Command {
                command: "restart".into(),
                detail: "échec simulé".into(),
            })
        } else {
            let mut state = self.machine.lock();
            state.calls.push("restart");
            state.active = true;
            state.stopped_on_purpose = false;
            // L'agent dont le binaire est en place démarre et touche à la base.
            match fs::read(&self.machine.binary)
                .unwrap_or_default()
                .as_slice()
            {
                b"old" => {
                    let database = fs::read(&self.machine.database).unwrap_or_default();
                    if database == b"migree" {
                        state.broken_pair = true;
                    } else if database == b"avant" {
                        fs::write(&self.machine.database, b"avant+vie").unwrap();
                    }
                    state.old_ran = true;
                }
                _ => {
                    if fs::read(&self.machine.database).unwrap_or_default() == b"avant" {
                        fs::write(&self.machine.database, b"migree").unwrap();
                    }
                }
            }
            Ok(())
        };
        self.crash.point();
        result
    }
    fn stop(&self) -> Result<(), ServiceError> {
        self.crash.point();
        {
            let mut state = self.machine.lock();
            state.calls.push("stop");
            state.active = false;
        }
        self.crash.point();
        Ok(())
    }
    fn disable(&self) -> Result<(), ServiceError> {
        Ok(())
    }
    fn remove(&self) -> Result<(), ServiceError> {
        Ok(())
    }
}

pub struct CrashAgent {
    pub machine: Arc<Machine>,
}

impl HelloProbe for CrashAgent {
    fn hello(&self, _addr: SocketAddr, _timeout: Duration) -> Result<Greeting, String> {
        if !self.machine.lock().active {
            return Err("connexion refusée".into());
        }
        let content = fs::read(&self.machine.binary).map_err(|e| e.to_string())?;
        match content.as_slice() {
            b"old" => Ok(Greeting {
                version: Version::new(0, 1, 0),
                fingerprint: fingerprint(),
            }),
            b"new-good" => Ok(Greeting {
                version: Version::new(0, 2, 0),
                fingerprint: fingerprint(),
            }),
            _ => Err("pas de réponse".into()),
        }
    }
}

pub struct FakeSpace;

impl FreeSpace for FakeSpace {
    fn free_bytes(&self, _path: &Path) -> Result<u64, UpdateHostError> {
        Ok(1 << 30)
    }
}

// ---------------------------------------------------------------------------------------------
// Les enveloppes
// ---------------------------------------------------------------------------------------------

pub struct CrashHost {
    pub inner: FsUpdateHost,
    pub crash: Arc<Crash>,
    pub machine: Arc<Machine>,
    /// Le disque est plein : la remise de la base échoue (rien n'est touché).
    pub fail_restore_database: bool,
}

impl CrashHost {
    fn around<T>(&self, action: impl FnOnce() -> T) -> T {
        self.crash.point();
        let result = action();
        self.crash.point();
        result
    }
}

impl UpdateHost for CrashHost {
    fn supervisor_running(&self) -> bool {
        self.inner.supervisor_running()
    }
    fn supervisor_pending(&self) -> bool {
        self.inner.supervisor_pending()
    }
    fn existing_supervisor(&self) -> Option<PathBuf> {
        self.inner.existing_supervisor()
    }
    fn take_supervisor_lock(&self) -> Result<SupervisorLock, UpdateHostError> {
        self.inner.take_supervisor_lock()
    }
    fn read_last(&self) -> Result<Option<UpdateRecord>, UpdateHostError> {
        self.inner.read_last()
    }
    fn write_last(&self, record: &UpdateRecord) -> Result<(), UpdateHostError> {
        self.around(|| self.inner.write_last(record))
    }
    fn read_job(&self) -> Result<Option<Job>, UpdateHostError> {
        self.inner.read_job()
    }
    fn read_marker(&self) -> Result<Option<Marker>, UpdateHostError> {
        self.inner.read_marker()
    }
    fn write_marker(&self, marker: &Marker) -> Result<(), UpdateHostError> {
        self.around(|| self.inner.write_marker(marker))
    }
    fn discard_marker(&self) {
        self.around(|| self.inner.discard_marker());
    }
    fn same_content(&self, a: &Path, b: &Path) -> bool {
        self.inner.same_content(a, b)
    }
    fn path_exists(&self, path: &Path) -> bool {
        self.inner.path_exists(path)
    }
    fn backup_database(&self) -> Result<(), UpdateHostError> {
        self.around(|| self.inner.backup_database())
    }
    fn database_size(&self) -> u64 {
        self.inner.database_size()
    }
    fn data_dir(&self) -> PathBuf {
        self.inner.data_dir()
    }
    fn database_copy_present(&self) -> bool {
        self.inner.database_copy_present()
    }
    fn restore_database(&self) -> Result<(), UpdateHostError> {
        self.crash.point();
        if self.inner.database_copy_present() && self.machine.lock().old_ran {
            self.machine.lock().replayed = true;
        }
        if self.fail_restore_database {
            return Err(UpdateHostError::Other("disque plein (simulé)".into()));
        }
        let result = self.inner.restore_database();
        self.crash.point();
        result
    }
    fn remove_path(&self, path: &Path) -> Result<(), UpdateHostError> {
        self.inner.remove_path(path)
    }
    fn discard_work_files(&self) {
        self.around(|| self.inner.discard_work_files());
    }
    fn read_state(&self) -> Result<Option<SupervisorState>, UpdateHostError> {
        self.inner.read_state()
    }
    fn write_state(&self, state: &SupervisorState) -> Result<(), UpdateHostError> {
        self.inner.write_state(state)
    }
    fn stage(&self, bytes: &[u8]) -> Result<PathBuf, UpdateHostError> {
        self.inner.stage(bytes)
    }
    fn staged_version(&self, path: &Path) -> Result<Version, UpdateHostError> {
        self.inner.staged_version(path)
    }
    fn prepare_supervisor(&self) -> Result<PathBuf, UpdateHostError> {
        self.inner.prepare_supervisor()
    }
    fn write_job(&self, job: &Job) -> Result<PathBuf, UpdateHostError> {
        self.inner.write_job(job)
    }
    fn launch(&self, supervisor: &Path, job: &Path) -> Result<(), UpdateHostError> {
        self.inner.launch(supervisor, job)
    }
    fn clear_staging(&self) {
        self.around(|| self.inner.clear_staging());
    }
}

pub struct CrashInstall {
    pub inner: SystemHost,
    pub crash: Arc<Crash>,
    pub machine: Arc<Machine>,
}

impl InstallHost for CrashInstall {
    fn os(&self) -> String {
        self.inner.os()
    }
    fn arch(&self) -> String {
        self.inner.arch()
    }
    fn is_privileged(&self) -> bool {
        self.inner.is_privileged()
    }
    fn port_taken(&self, addr: std::net::IpAddr, port: u16) -> bool {
        self.inner.port_taken(addr, port)
    }
    fn free_bytes(&self, path: &Path) -> Result<u64, HostError> {
        InstallHost::free_bytes(&self.inner, path)
    }
    fn hostname(&self) -> String {
        self.inner.hostname()
    }
    fn inspect(&self, paths: &InstallPaths, source: &Path) -> Result<HostFacts, HostError> {
        self.inner.inspect(paths, source)
    }
    fn lock(&self, path: &Path) -> Result<InstallLock, HostError> {
        self.inner.lock(path)
    }
    fn install_binary(
        &self,
        source: &Path,
        dest: &Path,
        backup: &Path,
    ) -> Result<BinaryInstalled, HostError> {
        self.crash.point();
        // Refaire l'échange alors que le binaire en place EST déjà le nouveau perdrait l'ancien.
        if fs::read(dest).ok() == fs::read(source).ok() {
            self.machine.lock().swap_replayed = true;
        }
        let result = self.inner.install_binary(source, dest, backup);
        self.crash.point();
        result
    }
    fn restore_binary(&self, dest: &Path, installed: &BinaryInstalled) -> Result<(), HostError> {
        self.crash.point();
        if self.machine.lock().old_ran {
            self.machine.lock().replayed = true;
        }
        let result = self.inner.restore_binary(dest, installed);
        self.crash.point();
        result
    }
    fn discard_backup(&self, installed: &BinaryInstalled) {
        self.crash.point();
        self.inner.discard_backup(installed);
        self.crash.point();
    }
    fn ensure_data_dir(&self, dir: &Path) -> Result<bool, HostError> {
        self.inner.ensure_data_dir(dir)
    }
    fn write_config_new(&self, path: &Path, spec: &ConfigSpec) -> Result<bool, HostError> {
        self.inner.write_config_new(path, spec)
    }
    fn remove_file(&self, path: &Path) -> Result<(), HostError> {
        self.inner.remove_file(path)
    }
    fn list_dir(&self, path: &Path) -> Result<Vec<String>, HostError> {
        self.inner.list_dir(path)
    }
    fn data_dir_state(&self, path: &Path) -> DataDirState {
        self.inner.data_dir_state(path)
    }
    fn remove_dir_if_empty(&self, path: &Path) -> Result<(), HostError> {
        self.inner.remove_dir_if_empty(path)
    }
    fn wait_for_hello(
        &self,
        addr: SocketAddr,
        timeout: Duration,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Answered, HostError> {
        self.inner.wait_for_hello(addr, timeout, cancelled)
    }
}

// ---------------------------------------------------------------------------------------------
// Le banc
// ---------------------------------------------------------------------------------------------

pub struct World {
    pub dir: tempfile::TempDir,
    pub job: Job,
    pub crash: Arc<Crash>,
    pub machine: Arc<Machine>,
    pub host: CrashHost,
    pub install: CrashInstall,
    pub service: CrashService,
    pub agent: CrashAgent,
}

impl World {
    /// L'ancien binaire (`old`) installé, son service actif, la base `avant`, `staged` déposé.
    pub fn new(staged: &[u8]) -> Self {
        silence_simulated_deaths();
        // La mort est simulée par une panique, jamais par une coupure de courant : le disque de test peut
        // être de la mémoire (`/dev/shm`, où `fsync` ne coûte rien) quand il existe. Les `fsync` du code
        // réel (`FsUpdateHost`) restent ; seul le support de stockage du test change (la matrice de
        // morts prenait ~120 s sur un disque virtuel lent).
        let dir = if Path::new("/dev/shm").is_dir() {
            tempfile::Builder::new().tempdir_in("/dev/shm").unwrap()
        } else {
            tempfile::tempdir().unwrap()
        };
        let bin = dir.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        let binary = bin.join("hearth-agent");
        fs::write(&binary, OLD).unwrap();
        let database = dir.path().join("hearth.db");
        fs::write(&database, b"avant").unwrap();
        let crash = Arc::new(Crash::default());
        let machine = Arc::new(Machine {
            state: Arc::new(Mutex::new(MachineState {
                active: true,
                ..MachineState::default()
            })),
            binary: binary.clone(),
            database,
        });
        let inner = FsUpdateHost::new(dir.path(), binary.clone(), Launcher::Detached);
        let staged_path = inner.stage(staged).unwrap();
        let job = Job {
            version: "0.2.0".into(),
            previous: "0.1.0".into(),
            binary,
            staged: staged_path,
            backup: bin.join(".hearth-agent.previous"),
            probe_addr: "127.0.0.1:7341".into(),
            fingerprint: fingerprint().to_hex(),
            grace_ms: 0,
            check_window_ms: 80,
            poll_ms: 5,
            requested_by: Some("marie".into()),
            client_name: None,
            client_addr: None,
            recover: false,
        };
        // La demande de l'agent : son intention est écrite avant le superviseur.
        inner
            .write_state(&SupervisorState {
                version: job.version.clone(),
                step: hearth_proto::api::update::UpdateStep::Restart,
                previous: job.previous.clone(),
                requester: job.requester(),
            })
            .unwrap();
        inner.write_job(&job).unwrap();
        Self {
            host: CrashHost {
                inner,
                crash: crash.clone(),
                machine: machine.clone(),
                fail_restore_database: false,
            },
            install: CrashInstall {
                inner: SystemHost,
                crash: crash.clone(),
                machine: machine.clone(),
            },
            service: CrashService {
                machine: machine.clone(),
                crash: crash.clone(),
                fail_restart: false,
            },
            agent: CrashAgent {
                machine: machine.clone(),
            },
            dir,
            job,
            crash,
            machine,
        }
    }

    /// Un lancement du superviseur, comme le fait l'unité : il lit son travail sur le disque (rien à
    /// faire s'il n'y est plus). `Err(())` : il est mort.
    pub fn launch(&self) -> Result<Option<Result<Supervised, SuperviseError>>, ()> {
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            let Ok(Some(job)) = self.host.read_job() else {
                return None;
            };
            Some(
                Supervisor {
                    host: &self.host,
                    install: &self.install,
                    space: &FakeSpace,
                    service: &self.service,
                    probe: &self.agent,
                    clock: &SystemClock,
                }
                .run(&job),
            )
        }));
        outcome.map_err(|_| ())
    }

    pub fn binary(&self) -> Vec<u8> {
        fs::read(&self.job.binary).unwrap()
    }

    pub fn database(&self) -> Vec<u8> {
        fs::read(&self.machine.database).unwrap()
    }

    pub fn update_files(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.dir.path().join("update"))
            .map(|entries| {
                entries
                    .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    pub fn marker_path(&self) -> PathBuf {
        self.dir.path().join("update").join("phase.json")
    }
}
