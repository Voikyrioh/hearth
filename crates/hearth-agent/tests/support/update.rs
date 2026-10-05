//! Outils de test de la mise à jour de l'agent : téléchargeur simulé (avec une porte pour garder
//! une mise à jour « en cours »), machine en mémoire, vraie vérification minisign avec une clé
//! jetable, et le service assemblé sur tout cela.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::io::Cursor;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use hearth_agent::application::ports::{
    Downloader, FetchError, SignatureVerifier, SupervisorLock, UpdateFeed, UpdateHost,
    UpdateHostError,
};
use hearth_agent::application::update::{Timing, UpdateAdapters, UpdateEnv, UpdateService};
use hearth_agent::domain::install::Version;
use hearth_agent::domain::update::{Job, SupervisorState, UpdateRecord};
use hearth_agent::infrastructure::update::{BroadcastUpdateFeed, MinisignVerifier};
use hearth_proto::fingerprint::Fingerprint;
use sha2::{Digest, Sha256};
use tokio::sync::Notify;

use super::Env;

pub const CURRENT: Version = Version::new(0, 1, 0);

/// Une paire de clés minisign jetable, et de quoi signer.
pub struct Keys {
    pub public: String,
    secret: minisign::SecretKey,
}

impl Keys {
    pub fn generate() -> Self {
        let pair = minisign::KeyPair::generate_unencrypted_keypair().expect("clés");
        Self {
            public: pair.pk.to_box().expect("boîte").to_string(),
            secret: pair.sk,
        }
    }

    pub fn sign(&self, data: &[u8]) -> String {
        minisign::sign(None, &self.secret, Cursor::new(data), None, None)
            .expect("signature")
            .to_string()
    }
}

pub fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Ce que le téléchargeur simulé rend.
pub enum Download {
    Bytes(Vec<u8>),
    Unreachable,
    Failed,
}

pub struct FakeDownloader {
    pub result: Mutex<Download>,
    /// Tant que la porte est fermée, le téléchargement attend : la mise à jour reste « en cours ».
    pub gate: Option<Arc<Notify>>,
    pub fetched: Mutex<Vec<String>>,
}

#[async_trait]
impl Downloader for FakeDownloader {
    async fn fetch(
        &self,
        url: &str,
        _max_bytes: u64,
        progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
    ) -> Result<Vec<u8>, FetchError> {
        self.fetched
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(url.to_owned());
        let bytes = match &*self.result.lock().unwrap_or_else(PoisonError::into_inner) {
            Download::Bytes(bytes) => bytes.clone(),
            Download::Unreachable => return Err(FetchError::Unreachable("pas de réseau".into())),
            Download::Failed => return Err(FetchError::Failed("statut 404".into())),
        };
        let total = bytes.len() as u64;
        progress(0, Some(total));
        if let Some(gate) = &self.gate {
            gate.notified().await;
        }
        for quarter in 1..=4u64 {
            progress(total * quarter / 4, Some(total));
        }
        Ok(bytes)
    }
}

#[derive(Default)]
pub struct MemState {
    pub staged: Option<Vec<u8>>,
    pub last: Option<UpdateRecord>,
    pub state: Option<SupervisorState>,
    pub job: Option<Job>,
    pub launched: Vec<(PathBuf, PathBuf)>,
    pub supervisor_prepared: bool,
    pub cleared: u32,
    /// La version que le binaire déposé annonce.
    pub announced: Option<Version>,
    pub fail_stage: bool,
    pub fail_launch: bool,
    /// Chemins qui « existent » (la sauvegarde de l'ancien binaire, par exemple).
    pub existing: Vec<PathBuf>,
    pub db_backups: u32,
    pub db_restores: u32,
    /// Une copie de la base attend d'être remise.
    pub db_copy: bool,
    /// Les traces (`state.json`, `job.json`) existent mais ne se lisent pas.
    pub unreadable: bool,
    pub removed: Vec<PathBuf>,
}

/// La machine en mémoire : aucun fichier, aucun processus.
pub struct MemHost {
    pub inner: Mutex<MemState>,
    pub running: AtomicBool,
}

impl MemHost {
    pub fn new(announced: Version) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(MemState {
                announced: Some(announced),
                ..MemState::default()
            }),
            running: AtomicBool::new(false),
        })
    }

    pub fn with<R>(&self, f: impl FnOnce(&mut MemState) -> R) -> R {
        f(&mut self.inner.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

impl UpdateHost for MemHost {
    fn supervisor_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    fn take_supervisor_lock(&self) -> Result<SupervisorLock, UpdateHostError> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Err(UpdateHostError::AlreadyRunning);
        }
        Ok(SupervisorLock(Box::new(())))
    }

    fn read_last(&self) -> Result<Option<UpdateRecord>, UpdateHostError> {
        Ok(self.with(|s| s.last.clone()))
    }

    fn write_last(&self, record: &UpdateRecord) -> Result<(), UpdateHostError> {
        self.with(|s| s.last = Some(record.clone()));
        Ok(())
    }

    fn read_job(&self) -> Result<Option<Job>, UpdateHostError> {
        self.with(|s| {
            if s.unreadable {
                return Err(UpdateHostError::Other("job.json illisible".into()));
            }
            Ok(s.job.clone())
        })
    }

    fn database_size(&self) -> u64 {
        0
    }

    fn data_dir(&self) -> PathBuf {
        PathBuf::from("/var/lib/hearth")
    }

    fn database_copy_present(&self) -> bool {
        self.with(|s| s.db_copy)
    }

    fn remove_path(&self, path: &Path) -> Result<(), UpdateHostError> {
        self.with(|s| {
            s.existing.retain(|p| p != path);
            s.removed.push(path.to_owned());
        });
        Ok(())
    }

    fn discard_work_files(&self) {
        self.with(|s| {
            s.state = None;
            s.job = None;
        });
    }

    fn path_exists(&self, path: &Path) -> bool {
        self.with(|s| s.existing.iter().any(|p| p == path))
    }

    fn backup_database(&self) -> Result<(), UpdateHostError> {
        self.with(|s| s.db_backups += 1);
        Ok(())
    }

    fn restore_database(&self) -> Result<(), UpdateHostError> {
        self.with(|s| {
            s.db_restores += 1;
            s.db_copy = false;
        });
        Ok(())
    }

    fn read_state(&self) -> Result<Option<SupervisorState>, UpdateHostError> {
        self.with(|s| {
            if s.unreadable {
                return Err(UpdateHostError::Other("state.json illisible".into()));
            }
            Ok(s.state.clone())
        })
    }

    fn write_state(&self, state: &SupervisorState) -> Result<(), UpdateHostError> {
        self.with(|s| s.state = Some(state.clone()));
        Ok(())
    }

    fn stage(&self, bytes: &[u8]) -> Result<PathBuf, UpdateHostError> {
        self.with(|s| {
            if s.fail_stage {
                return Err(UpdateHostError::Other("disque plein".into()));
            }
            s.staged = Some(bytes.to_vec());
            Ok(PathBuf::from("/var/lib/hearth/update/hearth-agent.new"))
        })
    }

    fn staged_version(&self, _path: &Path) -> Result<Version, UpdateHostError> {
        self.with(|s| s.announced)
            .ok_or_else(|| UpdateHostError::Other("ne s'exécute pas".into()))
    }

    fn prepare_supervisor(&self) -> Result<PathBuf, UpdateHostError> {
        self.with(|s| s.supervisor_prepared = true);
        Ok(PathBuf::from("/var/lib/hearth/update/supervisor"))
    }

    fn write_job(&self, job: &Job) -> Result<PathBuf, UpdateHostError> {
        self.with(|s| s.job = Some(job.clone()));
        Ok(PathBuf::from("/var/lib/hearth/update/job.json"))
    }

    fn launch(&self, supervisor: &Path, job: &Path) -> Result<(), UpdateHostError> {
        self.with(|s| {
            if s.fail_launch {
                return Err(UpdateHostError::Other("systemd-run absent".into()));
            }
            s.launched.push((supervisor.to_owned(), job.to_owned()));
            Ok(())
        })
    }

    fn clear_staging(&self) {
        self.with(|s| {
            s.staged = None;
            s.job = None;
            s.state = None;
            s.db_copy = false;
            s.cleared += 1;
        });
    }
}

/// Tout ce qu'il faut pour piloter une mise à jour en test.
pub struct Rig {
    pub service: Arc<UpdateService>,
    pub keys: Arc<Keys>,
    pub host: Arc<MemHost>,
    pub downloader: Arc<FakeDownloader>,
    pub feed: Arc<BroadcastUpdateFeed>,
    pub gate: Option<Arc<Notify>>,
    verifier: Arc<dyn SignatureVerifier>,
    allowed: bool,
}

impl Rig {
    /// `allowed` : la mise à jour à distance est possible (ni gérée, ni sans systemd).
    pub fn new(env: &Env, allowed: bool, gated: bool) -> Self {
        Self::build(env, allowed, gated, MemHost::new(Version::new(0, 2, 0)))
    }

    /// Un autre service sur la même machine (le même dossier `update/`).
    pub fn with_host(env: &Env, allowed: bool, host: Arc<MemHost>) -> Self {
        Self::build(env, allowed, false, host)
    }

    fn build(env: &Env, allowed: bool, gated: bool, host: Arc<MemHost>) -> Self {
        let keys = Arc::new(Keys::generate());
        let verifier: Arc<dyn SignatureVerifier> =
            Arc::new(MinisignVerifier::new(&keys.public).expect("clé"));
        let gate = gated.then(|| Arc::new(Notify::new()));
        let downloader = Arc::new(FakeDownloader {
            result: Mutex::new(Download::Bytes(b"nouvel agent".to_vec())),
            gate: gate.clone(),
            fetched: Mutex::new(Vec::new()),
        });
        let feed = Arc::new(BroadcastUpdateFeed::new());
        let service = UpdateService::new(
            CURRENT,
            allowed,
            UpdateEnv {
                binary: PathBuf::from("/usr/local/bin/hearth-agent"),
                backup: PathBuf::from("/usr/local/bin/.hearth-agent.previous"),
                probe_addr: SocketAddr::from(([127, 0, 0, 1], 7341)),
                fingerprint: Fingerprint::from_hex(&"ab".repeat(32)).expect("empreinte"),
                timing: Timing {
                    grace: Duration::from_millis(1),
                    check_window: Duration::from_millis(50),
                    poll: Duration::from_millis(5),
                    watch: Duration::from_millis(5),
                },
                allow_local_addresses: false,
            },
            UpdateAdapters {
                downloader: downloader.clone(),
                verifier: verifier.clone(),
                host: host.clone(),
                feed: feed.clone() as Arc<dyn UpdateFeed>,
            },
            env.audit_sink.clone(),
            env.clock.clone(),
        );
        Self {
            service,
            keys,
            host,
            downloader,
            feed,
            gate,
            verifier,
            allowed,
        }
    }

    /// Les mêmes adaptateurs, pour un vrai agent (`app::start_with_all`).
    pub fn updating(&self) -> hearth_agent::app::Updating {
        hearth_agent::app::Updating {
            adapters: UpdateAdapters {
                downloader: self.downloader.clone(),
                verifier: self.verifier.clone(),
                host: self.host.clone(),
                feed: self.feed.clone() as Arc<dyn UpdateFeed>,
            },
            allowed: self.allowed,
            allow_local_addresses: false,
            timing: Timing {
                grace: Duration::from_millis(1),
                check_window: Duration::from_millis(50),
                poll: Duration::from_millis(5),
                watch: Duration::from_millis(5),
            },
        }
    }

    /// Un abonné à la progression, pris avant de lancer.
    pub fn feed_receiver(
        &self,
    ) -> tokio::sync::broadcast::Receiver<hearth_proto::api::update::UpdateProgress> {
        self.feed.subscribe()
    }

    /// Ouvre la porte du téléchargeur.
    pub fn release_gate(&self) {
        if let Some(gate) = &self.gate {
            gate.notify_one();
        }
    }

    /// Le corps d'une demande valable pour ces octets (somme et signature de la clé du banc).
    pub fn request(&self, version: &str, bytes: &[u8]) -> serde_json::Value {
        serde_json::json!({
            "version": version,
            "url": "https://exemple.org/hearth-agent",
            "signature": self.keys.sign(bytes),
            "sha256": sha256_hex(bytes),
        })
    }
}
