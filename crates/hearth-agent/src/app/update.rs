//! Composition de la mise à jour de l'agent à distance : les adaptateurs de production
//! (`Updating::production`) et le superviseur détaché (`update-supervise`).

use std::ffi::OsString;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use super::AppError;
use crate::application::ports::{
    Clock, FreeSpace, InstallHost, ServiceManager, UpdateFeed, UpdateHost, UpdateHostError,
};
use crate::application::update::format_time;
use crate::application::update::{Timing, UpdateAdapters, UpdateEnv, UpdateService};
use crate::application::update_supervisor::{End, SuperviseError, Supervisor};
use crate::domain::install::Version;
use crate::domain::update::{Job, UpdateRecord};
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
///
/// **Code de sortie** (le superviseur tourne dans une unité transitoire `Restart=on-failure`,
/// BR-UPDATE-030) :
/// - 0 : il n'y a plus rien à reprendre. Résultat écrit (réussite, retour arrière, échec), reprise
///   abandonnée (copies gardées, journalisée une fois), service arrêté à la main (rien n'est défait),
///   un autre superviseur travaille déjà, travail absent ou illisible (une relance n'y changerait
///   rien) : systemd ne relance pas ;
/// - non nul : une étape n'a pas pu être enregistrée (disque, droits) ; systemd relance, le
///   superviseur reprend là où son marqueur dit qu'il en était, et le nombre de reprises est borné.
pub fn run_supervisor(job_path: &Path) -> Result<(), AppError> {
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
    let text = match std::fs::read_to_string(job_path) {
        Ok(text) => text,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            tracing::info!(job = %job_path.display(), "aucun travail de mise à jour : rien à faire");
            return Ok(());
        }
        Err(source) => {
            return Err(AppError::Supervise {
                detail: format!("travail {} illisible : {source}", job_path.display()),
            });
        }
    };
    let job: Job = match serde_json::from_str(&text) {
        Ok(job) => job,
        Err(error) => {
            // Un travail illisible le reste à la relance : pas de reprise. Mais jamais en silence : le
            // résultat « échec » est écrit (l'agent l'annonce et le consigne au journal d'activité),
            // puis sortie en 0. Si le résultat ne s'écrit pas, sortie non nulle (bornée par systemd).
            tracing::error!(job = %job_path.display(), %error, "travail de mise à jour illisible, rien n'est touché");
            return write_unreadable_result(&host, None);
        }
    };
    let install = SystemHost;
    let service = Systemd::system();
    let probe = AgentHelloProbe;
    let clock = SystemClock;
    let supervisor = Supervisor {
        host: &host,
        install: &install as &dyn InstallHost,
        space: &install as &dyn FreeSpace,
        service: &service as &dyn ServiceManager,
        probe: &probe,
        clock: &clock,
    };
    match supervisor.run(&job) {
        Ok(done) => {
            match done.end {
                End::Concluded => {
                    tracing::info!(outcome = ?done.outcome, reason = ?done.reason, "mise à jour terminée");
                }
                End::Abandoned => tracing::error!(
                    backup = %job.backup.display(),
                    "retour en arrière impossible : l'ancien binaire et la copie de la base sont gardés"
                ),
                End::Paused | End::Nothing => {}
            }
            Ok(())
        }
        Err(SuperviseError::Host(UpdateHostError::AlreadyRunning)) => {
            tracing::info!("un superviseur de mise à jour travaille déjà : rien à faire");
            Ok(())
        }
        Err(SuperviseError::Job(detail)) => {
            tracing::error!(%detail, "travail de mise à jour illisible, rien n'est touché");
            write_unreadable_result(&host, Some(&job))
        }
        Err(error) => Err(AppError::Supervise {
            detail: error.to_string(),
        }),
    }
}

/// Le travail ne se lit pas : rien n'est touché, le résultat « échec, interrompue » est écrit (version
/// connue si le travail se lisait en partie). Le client le voit comme toute tentative interrompue
/// (`failed` / `interrupted`).
fn write_unreadable_result(host: &FsUpdateHost, job: Option<&Job>) -> Result<(), AppError> {
    let record = UpdateRecord {
        version: job.map(|job| job.version.clone()),
        previous: job.map(|job| job.previous.clone()).unwrap_or_default(),
        outcome: UpdateOutcome::Failed,
        reason: Some(UpdateReason::Interrupted),
        at: format_time(SystemClock.now()),
        requested_by: job.and_then(|job| job.requested_by.clone()),
        client_name: job.and_then(|job| job.client_name.clone()),
        client_addr: job.and_then(|job| job.client_addr.clone()),
        reported: false,
    };
    host.write_last(&record)
        .map_err(|error| AppError::Supervise {
            detail: format!("résultat non écrit : {error}"),
        })?;
    // Le résultat est écrit : le travail illisible et la trace d'étape partent, pour que l'agent qui
    // démarre ne conclue pas une seconde fois la même panne. Les copies (ancien binaire, base d'avant,
    // marqueur) restent : rien n'est perdu pour la reprise à la main.
    host.discard_work_files();
    Ok(())
}
