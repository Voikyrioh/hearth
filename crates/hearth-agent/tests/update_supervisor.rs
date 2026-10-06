//! Le superviseur de mise à jour (BR-UPDATE-015, BR-UPDATE-018) sur de vrais fichiers temporaires :
//! arrêt, échange atomique, redémarrage, contrôle, retour à l'ancien binaire **exactement** tel
//! qu'il était. Le service et l'agent qui répond sont simulés : le « nouvel agent » répond selon le
//! contenu du binaire en place (et seulement si le service tourne), comme un vrai processus.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use hearth_agent::application::ports::{
    FreeSpace, Greeting, HelloProbe, ServiceError, ServiceKind, ServiceManager, ServiceSpec,
    UpdateHost, UpdateHostError,
};
use hearth_agent::application::update_supervisor::{SuperviseError, Supervisor};
use hearth_agent::domain::install::Version;
use hearth_agent::domain::update::Job;
use hearth_agent::infrastructure::clock::SystemClock;
use hearth_agent::infrastructure::install::SystemHost;
use hearth_agent::infrastructure::update::{FsUpdateHost, Launcher};
use hearth_proto::api::update::{UpdateOutcome, UpdateReason};
use hearth_proto::fingerprint::Fingerprint;

const OLD: &[u8] = b"old";

fn fingerprint(byte: u8) -> Fingerprint {
    Fingerprint::from_hex(&format!("{byte:02x}").repeat(32)).unwrap()
}

#[derive(Default)]
struct ServiceState {
    active: bool,
    calls: Vec<&'static str>,
    /// Les redémarrages à partir du n-ième échouent.
    fail_restart_from: Option<usize>,
    restarts: usize,
    /// Le premier redémarrage (le nouvel agent) migre la base : il y écrit « migree ».
    migrate_on_first_restart: Option<PathBuf>,
}

struct FakeService(Arc<Mutex<ServiceState>>);

impl FakeService {
    fn state(&self) -> std::sync::MutexGuard<'_, ServiceState> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl ServiceManager for FakeService {
    fn kind(&self) -> ServiceKind {
        ServiceKind::Systemd
    }
    fn is_installed(&self) -> Result<bool, ServiceError> {
        Ok(true)
    }
    fn is_active(&self) -> Result<bool, ServiceError> {
        Ok(self.state().active)
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
        let mut state = self.state();
        state.calls.push("restart");
        state.restarts += 1;
        if state.restarts == 1
            && let Some(path) = &state.migrate_on_first_restart
        {
            fs::write(path, b"migree").unwrap();
        }
        if state.fail_restart_from.is_some_and(|n| state.restarts >= n) {
            return Err(ServiceError::Command {
                command: "restart".into(),
                detail: "échec simulé".into(),
            });
        }
        state.active = true;
        Ok(())
    }
    fn stop(&self) -> Result<(), ServiceError> {
        let mut state = self.state();
        state.calls.push("stop");
        state.active = false;
        Ok(())
    }
    fn disable(&self) -> Result<(), ServiceError> {
        Ok(())
    }
    fn remove(&self) -> Result<(), ServiceError> {
        Ok(())
    }
}

/// L'agent qui répond : celui dont le binaire est en place, s'il tourne.
struct FakeAgent {
    binary: PathBuf,
    service: Arc<Mutex<ServiceState>>,
}

impl HelloProbe for FakeAgent {
    fn hello(&self, _addr: SocketAddr, _timeout: Duration) -> Result<Greeting, String> {
        if !self
            .service
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .active
        {
            return Err("connexion refusée".into());
        }
        let content = fs::read(&self.binary).map_err(|e| e.to_string())?;
        match content.as_slice() {
            b"old" => Ok(Greeting {
                version: Version::new(0, 1, 0),
                fingerprint: fingerprint(0xab),
            }),
            b"new-good" => Ok(Greeting {
                version: Version::new(0, 2, 0),
                fingerprint: fingerprint(0xab),
            }),
            b"new-other-certificate" => Ok(Greeting {
                version: Version::new(0, 2, 0),
                fingerprint: fingerprint(0xcd),
            }),
            // Un nouveau binaire qui redémarre sur l'ancienne version.
            b"new-still-old" => Ok(Greeting {
                version: Version::new(0, 1, 0),
                fingerprint: fingerprint(0xab),
            }),
            _ => Err("pas de réponse".into()),
        }
    }
}

/// L'espace libre qu'on dit : jamais le vrai disque (un disque presque plein de la machine de
/// test ne doit pas faire échouer la copie de la base, ni l'inverse).
struct FakeSpace(Mutex<Result<u64, String>>);

impl FreeSpace for FakeSpace {
    fn free_bytes(&self, _path: &Path) -> Result<u64, UpdateHostError> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .map_err(UpdateHostError::Other)
    }
}

struct Bench {
    dir: tempfile::TempDir,
    space: FakeSpace,
    job: Job,
    host: FsUpdateHost,
    service: FakeService,
    agent: FakeAgent,
}

impl Bench {
    /// L'ancien binaire (`old`) installé et son service actif ; `staged` déposé dans `update/`.
    fn new(staged: Option<&[u8]>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        let binary = bin.join("hearth-agent");
        fs::write(&binary, OLD).unwrap();
        let host = FsUpdateHost::new(dir.path(), binary.clone(), Launcher::Detached);
        let staged_path = match staged {
            Some(bytes) => host.stage(bytes).unwrap(),
            None => dir.path().join("update").join("hearth-agent.new"),
        };
        let job = Job {
            version: "0.2.0".into(),
            previous: "0.1.0".into(),
            binary: binary.clone(),
            staged: staged_path,
            backup: bin.join(".hearth-agent.previous"),
            probe_addr: "127.0.0.1:7341".into(),
            fingerprint: fingerprint(0xab).to_hex(),
            grace_ms: 1,
            check_window_ms: 120,
            poll_ms: 5,
            requested_by: Some("marie".into()),
            client_name: Some("PC".into()),
            client_addr: Some("10.0.0.7".into()),
            recover: false,
        };
        fs::write(dir.path().join("hearth.db"), b"avant").unwrap();
        let service_state = Arc::new(Mutex::new(ServiceState {
            migrate_on_first_restart: Some(dir.path().join("hearth.db")),
            active: true,
            ..ServiceState::default()
        }));
        Self {
            space: FakeSpace(Mutex::new(Ok(1 << 30))),
            agent: FakeAgent {
                binary,
                service: service_state.clone(),
            },
            service: FakeService(service_state),
            dir,
            job,
            host,
        }
    }

    fn run(
        &self,
    ) -> Result<hearth_agent::application::update_supervisor::Supervised, SuperviseError> {
        Supervisor {
            host: &self.host,
            install: &SystemHost,
            space: &self.space,
            service: &self.service,
            probe: &self.agent,
            clock: &SystemClock,
        }
        .run(&self.job)
    }

    fn binary(&self) -> Vec<u8> {
        fs::read(&self.job.binary).unwrap()
    }

    fn leftovers(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.job.binary.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

#[test]
fn a_new_agent_that_answers_with_the_new_version_and_the_same_certificate_succeeds() {
    let bench = Bench::new(Some(b"new-good"));
    let done = bench.run().unwrap();
    assert_eq!(done.outcome, UpdateOutcome::Succeeded);
    assert_eq!(bench.binary(), b"new-good");
    assert_eq!(
        bench.leftovers(),
        ["hearth-agent"],
        "ni sauvegarde ni temporaire ne reste"
    );
    assert!(bench.service.state().active);
    assert_eq!(bench.service.state().calls, ["stop", "restart"]);

    let last = bench.host.read_last().unwrap().unwrap();
    assert_eq!(last.outcome, UpdateOutcome::Succeeded);
    assert_eq!(
        (last.version.as_deref().unwrap(), last.previous.as_str()),
        ("0.2.0", "0.1.0")
    );
    assert_eq!(last.requested_by.as_deref(), Some("marie"));
    assert!(!last.reported, "le nouvel agent l'annoncera");
    assert_eq!(
        bench.host.read_state().unwrap(),
        None,
        "l'étape en cours est effacée avec le dépôt"
    );
    assert!(
        !bench.job.staged.exists(),
        "le binaire déposé est retiré après l'échange"
    );
}

#[test]
fn a_new_agent_that_never_answers_is_rolled_back_to_the_exact_old_binary() {
    let bench = Bench::new(Some(b"new-mute"));
    let done = bench.run().unwrap();
    assert_eq!(done.outcome, UpdateOutcome::RolledBack);
    assert_eq!(done.reason, Some(UpdateReason::NoAnswer));
    assert_eq!(bench.binary(), OLD, "exactement l'ancien binaire");
    assert_eq!(bench.leftovers(), ["hearth-agent"], "aucun reste");
    assert!(
        bench.service.state().active,
        "l'ancien agent tourne de nouveau"
    );
    assert_eq!(
        bench.service.state().calls,
        ["stop", "restart", "stop", "restart"]
    );

    let last = bench.host.read_last().unwrap().unwrap();
    assert_eq!(last.outcome, UpdateOutcome::RolledBack);
    assert_eq!(last.reason, Some(UpdateReason::NoAnswer));
    assert!(!last.reported);
}

#[test]
fn a_new_agent_with_another_certificate_is_rolled_back_at_once() {
    // Fenêtre d'une minute : si le retour attendait la fin de la fenêtre, la raison serait
    // `no_answer` (et le test durerait une minute), pas `identity_changed`.
    let mut bench = Bench::new(Some(b"new-other-certificate"));
    bench.job.check_window_ms = 60_000;
    let done = bench.run().unwrap();
    assert_eq!(done.outcome, UpdateOutcome::RolledBack);
    assert_eq!(done.reason, Some(UpdateReason::IdentityChanged));
    assert_eq!(bench.binary(), OLD);
}

#[test]
fn an_agent_that_still_reports_the_old_version_is_not_a_success() {
    let bench = Bench::new(Some(b"new-still-old"));
    let done = bench.run().unwrap();
    assert_eq!(done.outcome, UpdateOutcome::RolledBack);
    assert_eq!(done.reason, Some(UpdateReason::NoAnswer));
    assert_eq!(bench.binary(), OLD);
}

#[test]
fn a_swap_that_cannot_happen_leaves_the_old_binary_and_restarts_the_service() {
    // Le binaire déposé a disparu : l'échange échoue, l'ancien reste en place.
    let bench = Bench::new(None);
    let done = bench.run().unwrap();
    assert_eq!(done.outcome, UpdateOutcome::Failed);
    assert_eq!(done.reason, Some(UpdateReason::Swap));
    assert_eq!(bench.binary(), OLD);
    assert!(bench.service.state().active, "le service est relancé");
    assert_eq!(bench.leftovers(), ["hearth-agent"]);
}

#[test]
fn not_enough_room_for_the_database_copy_leaves_everything_as_it_was() {
    let bench = Bench::new(Some(b"new-good"));
    *bench.space.0.lock().unwrap() = Ok(1024);
    let done = bench.run().unwrap();
    assert_eq!(done.outcome, UpdateOutcome::Failed);
    assert_eq!(done.reason, Some(UpdateReason::Swap));
    assert_eq!(bench.binary(), OLD, "aucun échange");
    assert!(bench.service.state().active, "l'ancien agent repart");
    assert_eq!(bench.service.state().calls, ["stop", "restart"]);
    assert_eq!(bench.leftovers(), ["hearth-agent"], "ni copie ni dépôt");
}

#[test]
fn a_room_that_cannot_be_measured_refuses_before_any_swap_like_a_full_disk() {
    let bench = Bench::new(Some(b"new-good"));
    *bench.space.0.lock().unwrap() = Err("df introuvable".into());
    let done = bench.run().unwrap();
    assert_eq!(done.outcome, UpdateOutcome::Failed);
    assert_eq!(done.reason, Some(UpdateReason::Swap));
    assert_eq!(bench.binary(), OLD, "aucun échange");
    assert!(bench.service.state().active);
    assert_eq!(bench.service.state().calls, ["stop", "restart"]);
    assert_eq!(bench.leftovers(), ["hearth-agent"], "ni copie ni dépôt");
}

#[test]
fn a_rollback_that_cannot_restart_the_old_agent_says_so_and_keeps_the_binary_restored() {
    let bench = Bench::new(Some(b"new-mute"));
    // Le premier redémarrage (le nouvel agent) passe, le second (le retour) échoue.
    bench.service.state().fail_restart_from = Some(2);
    let done = bench.run().unwrap();
    assert_eq!(done.outcome, UpdateOutcome::Failed);
    assert_eq!(done.reason, Some(UpdateReason::RollbackFailed));
    assert_eq!(
        bench.binary(),
        OLD,
        "le binaire d'avant est remis malgré tout"
    );
    let last = bench.host.read_last().unwrap().unwrap();
    assert_eq!(last.reason, Some(UpdateReason::RollbackFailed));
}

#[test]
fn two_supervisors_never_work_at_once() {
    let bench = Bench::new(Some(b"new-good"));
    let _held = bench.host.take_supervisor_lock().unwrap();
    assert!(matches!(
        bench.run(),
        Err(SuperviseError::Host(UpdateHostError::AlreadyRunning))
    ));
    assert_eq!(bench.binary(), OLD, "rien n'a été touché");
    assert!(bench.service.state().calls.is_empty());
}

#[test]
fn a_job_with_an_unreadable_address_or_version_is_refused_before_touching_anything() {
    for (version, addr, fingerprint_hex) in [
        ("x.y", "127.0.0.1:7341", "ab".repeat(32)),
        ("0.2.0", "pas-une-adresse", "ab".repeat(32)),
        ("0.2.0", "127.0.0.1:7341", "zz".to_owned()),
    ] {
        let mut bench = Bench::new(Some(b"new-good"));
        bench.job.version = version.into();
        bench.job.probe_addr = addr.into();
        bench.job.fingerprint = fingerprint_hex;
        assert!(matches!(bench.run(), Err(SuperviseError::Job(_))));
        assert_eq!(bench.binary(), OLD);
        assert!(bench.service.state().calls.is_empty());
    }
}

#[test]
fn the_data_beside_the_binary_is_never_touched() {
    // Comptes, journal, identité : dans le dossier de données, jamais dans celui du binaire.
    let bench = Bench::new(Some(b"new-mute"));
    let data = bench.dir.path().join("hearth.db");
    fs::write(&data, b"comptes et journal").unwrap();
    let cert = bench.dir.path().join("cert.pem");
    fs::write(&cert, b"certificat").unwrap();
    bench.run().unwrap();
    assert_eq!(fs::read(&data).unwrap(), b"comptes et journal");
    assert_eq!(fs::read(&cert).unwrap(), b"certificat");
    let _: &Path = &data;
}

// ---------------------------------------------------------------------------------------------
// La base migrée par la nouvelle version (BR-UPDATE-029)
// ---------------------------------------------------------------------------------------------

impl Bench {
    fn database(&self) -> Vec<u8> {
        fs::read(self.dir.path().join("hearth.db")).unwrap()
    }

    fn update_files(&self) -> Vec<String> {
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
}

#[test]
fn a_rollback_puts_the_database_back_as_it_was_before_the_swap() {
    // Le nouvel agent a migré la base à son démarrage : l'ancien binaire ne doit jamais se
    // retrouver devant elle.
    let bench = Bench::new(Some(b"new-mute"));
    assert_eq!(bench.database(), b"avant");
    let done = bench.run().unwrap();
    assert_eq!(done.outcome, UpdateOutcome::RolledBack);
    assert_eq!(bench.binary(), OLD);
    assert_eq!(bench.database(), b"avant", "la base d'avant la migration");
    assert!(
        !bench
            .update_files()
            .iter()
            .any(|name| name.contains("before")),
        "la copie de la base est retirée : {:?}",
        bench.update_files()
    );
}

#[test]
fn a_success_keeps_the_migrated_database_and_drops_its_copy() {
    let bench = Bench::new(Some(b"new-good"));
    let done = bench.run().unwrap();
    assert_eq!(done.outcome, UpdateOutcome::Succeeded);
    assert_eq!(bench.database(), b"migree");
    assert!(
        !bench
            .update_files()
            .iter()
            .any(|name| name.contains("before")),
        "{:?}",
        bench.update_files()
    );
}

// ---------------------------------------------------------------------------------------------
// Reprise d'un travail orphelin (BR-UPDATE-028) : l'échange a eu lieu, personne n'a conclu
// ---------------------------------------------------------------------------------------------

/// L'état laissé par un superviseur tué après l'échange : le binaire installé est `installed`,
/// l'ancien est gardé à côté, la base a été migrée (sa copie d'avant est dans `update/`).
fn killed_after_the_swap(installed: &[u8]) -> Bench {
    let mut bench = Bench::new(None);
    // Pas de migration à un redémarrage : elle a déjà eu lieu avant la mort du superviseur.
    bench.service.state().migrate_on_first_restart = None;
    fs::write(&bench.job.binary, installed).unwrap();
    fs::write(&bench.job.backup, OLD).unwrap();
    fs::write(bench.dir.path().join("hearth.db"), b"migree").unwrap();
    let update = bench.dir.path().join("update");
    fs::create_dir_all(&update).unwrap();
    fs::write(update.join("hearth.db.before"), b"avant").unwrap();
    bench.job.recover = true;
    bench.job.check_window_ms = 150;
    bench
}

#[test]
fn a_recovery_keeps_a_new_agent_that_answers_and_never_stops_the_service() {
    let bench = killed_after_the_swap(b"new-good");
    let done = bench.run().unwrap();
    assert_eq!(done.outcome, UpdateOutcome::Succeeded);
    assert_eq!(bench.binary(), b"new-good");
    assert!(
        bench.service.state().calls.is_empty(),
        "ni arrêt ni redémarrage : l'agent qui répond est celui qui a lancé la reprise"
    );
    assert_eq!(
        bench.leftovers(),
        ["hearth-agent"],
        "la sauvegarde de l'ancien binaire est retirée"
    );
    assert_eq!(bench.database(), b"migree");
    let last = bench.host.read_last().unwrap().unwrap();
    assert_eq!(last.outcome, UpdateOutcome::Succeeded);
    assert!(!last.reported, "l'agent l'annoncera");
}

#[test]
fn a_recovery_puts_back_the_exact_old_binary_and_database_when_the_new_agent_does_not_hold() {
    let bench = killed_after_the_swap(b"new-mute");
    let done = bench.run().unwrap();
    assert_eq!(done.outcome, UpdateOutcome::RolledBack);
    assert_eq!(done.reason, Some(UpdateReason::NoAnswer));
    assert_eq!(bench.binary(), OLD, "exactement l'ancien binaire");
    assert_eq!(bench.database(), b"avant", "la base d'avant la migration");
    assert!(bench.service.state().active);
    assert_eq!(bench.leftovers(), ["hearth-agent"]);
}

#[test]
fn a_supervisor_waits_a_second_for_a_lock_the_agent_holds_for_an_instant() {
    use std::thread;
    let bench = Bench::new(Some(b"new-good"));
    let held = bench.host.take_supervisor_lock().unwrap();
    // L'agent qui teste le verrou le relâche presque aussitôt : le superviseur n'abandonne pas.
    let releaser = thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));
        drop(held);
    });
    let done = bench.run().expect("le superviseur a insisté");
    releaser.join().unwrap();
    assert_eq!(done.outcome, UpdateOutcome::Succeeded);
}
