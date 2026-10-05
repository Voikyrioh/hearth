//! Composition de la mise à jour de l'agent à distance : les adaptateurs de production
//! (`Updating::production`) et le superviseur détaché (`update-supervise`).

use std::ffi::OsString;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use super::AppError;
use crate::application::ports::{Clock, InstallHost, ServiceManager, UpdateFeed};
use crate::application::update::{Timing, UpdateAdapters, UpdateEnv, UpdateService};
use crate::application::update_supervisor::{SuperviseError, Supervised, Supervisor};
use crate::domain::install::Version;
use crate::domain::update::Job;
use crate::infrastructure::clock::SystemClock;
use crate::infrastructure::config::AgentConfig;
use crate::infrastructure::install::{AgentHelloProbe, SystemHost};
use crate::infrastructure::service::Systemd;
use crate::infrastructure::update::{
    BroadcastUpdateFeed, FsUpdateHost, HttpsDownloader, Launcher, MinisignVerifier,
};
use hearth_proto::api::update::{UpdateOutcome, UpdateReason};
use hearth_proto::fingerprint::Fingerprint;

/// Ce dont la mise à jour a besoin, injectable : l'agent de production en reçoit les adaptateurs
/// réels, les tests des faux.
pub struct Updating {
    pub adapters: UpdateAdapters,
    /// La mise à jour à distance est possible ici : ni installation gérée, ni sans systemd.
    pub allowed: bool,
    pub timing: Timing,
    /// Adresses locales permises pour le téléchargement (tests de bout en bout seulement).
    pub allow_local_addresses: bool,
}

impl Updating {
    /// Téléchargement HTTPS, clé minisign embarquée, dossier `update/`, `systemd-run`.
    pub fn production(config: &AgentConfig) -> Result<Self, AppError> {
        let exe = std::env::current_exe().map_err(AppError::CurrentExe)?;
        let feed: Arc<dyn UpdateFeed> = Arc::new(BroadcastUpdateFeed::new());
        Ok(Self {
            adapters: UpdateAdapters {
                downloader: Arc::new(HttpsDownloader::new(
                    crate::build_info::ALLOW_LOCAL_DOWNLOADS,
                )),
                verifier: Arc::new(MinisignVerifier::embedded().map_err(AppError::Update)?),
                host: Arc::new(FsUpdateHost::new(
                    &config.data_dir,
                    exe,
                    Launcher::SystemdRun(OsString::from("systemd-run")),
                )),
                feed,
            },
            allowed: !config.managed && Systemd::is_running_here(),
            timing: Timing::default(),
            allow_local_addresses: crate::build_info::ALLOW_LOCAL_DOWNLOADS,
        })
    }
}

/// Le service de mise à jour d'un agent qui écoute sur `addr`, avec cette identité.
pub fn update_service(
    updating: Updating,
    addr: SocketAddr,
    fingerprint: Fingerprint,
    sink: Arc<dyn crate::application::ports::AuditSink>,
    clock: Arc<dyn Clock>,
) -> Result<Arc<UpdateService>, AppError> {
    let binary = std::env::current_exe().map_err(AppError::CurrentExe)?;
    let current = Version::parse(crate::build_info::VERSION).map_err(AppError::Version)?;
    let probe_ip = crate::application::install::probe_ip(addr.ip());
    Ok(UpdateService::new(
        current,
        updating.allowed,
        UpdateEnv {
            backup: crate::application::ports::InstallPaths::backup_of(&binary),
            binary,
            probe_addr: SocketAddr::new(probe_ip, addr.port()),
            fingerprint,
            timing: updating.timing,
            allow_local_addresses: updating.allow_local_addresses,
        },
        updating.adapters,
        sink,
        clock,
    ))
}

/// `hearth-agent update-supervise --job FICHIER` : le travail du superviseur, jusqu'au résultat.
/// Code de sortie 0 : l'agent tourne (nouvelle version, ou retour à l'ancienne). Erreur : le
/// retour en arrière lui-même a échoué (l'ancien binaire est à `job.backup`).
pub fn run_supervisor(job_path: &Path) -> Result<(), AppError> {
    let text = std::fs::read_to_string(job_path).map_err(|source| AppError::Supervise {
        detail: format!("travail {} illisible : {source}", job_path.display()),
    })?;
    let job: Job = serde_json::from_str(&text).map_err(|error| AppError::Supervise {
        detail: format!("travail {} illisible : {error}", job_path.display()),
    })?;
    // Le dossier du travail est `update/` : l'hôte en déduit tout le reste.
    let data_dir = job_path
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| AppError::Supervise {
            detail: "dossier de données introuvable".to_owned(),
        })?;
    let exe = std::env::current_exe().map_err(AppError::CurrentExe)?;
    let host = FsUpdateHost::new(
        data_dir,
        exe,
        Launcher::SystemdRun(OsString::from("systemd-run")),
    );
    let install = SystemHost;
    let service = Systemd::system();
    let probe = AgentHelloProbe;
    let clock = SystemClock;
    let supervisor = Supervisor {
        host: &host,
        install: &install as &dyn InstallHost,
        service: &service as &dyn ServiceManager,
        probe: &probe,
        clock: &clock,
    };
    match supervisor.run(&job) {
        Ok(Supervised {
            outcome: UpdateOutcome::Failed,
            reason: Some(UpdateReason::RollbackFailed),
        }) => Err(AppError::Supervise {
            detail: format!(
                "retour en arrière impossible : l'ancien binaire est resté à {}",
                job.backup.display()
            ),
        }),
        Ok(done) => {
            tracing::info!(outcome = ?done.outcome, reason = ?done.reason, "mise à jour terminée");
            Ok(())
        }
        Err(SuperviseError::Job(detail)) => Err(AppError::Supervise { detail }),
        Err(error) => Err(AppError::Supervise {
            detail: error.to_string(),
        }),
    }
}
