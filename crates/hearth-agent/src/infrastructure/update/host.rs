//! La machine vue par la mise à jour : le dossier `update/` du dossier de données (binaire déposé,
//! copie du superviseur, travail, étape, résultat, verrou) et le lancement du superviseur.
//!
//! Le dossier `update/` est dans le dossier de données et non dans `/tmp` : l'unité a
//! `PrivateTmp=yes` (un fichier de `/tmp` n'y serait pas visible du superviseur) et seul le
//! dossier de données est écrivable par l'agent (ADR-0012). Tous les fichiers sont écrits par un
//! fichier voisin puis un renommage, droits 0600 (0700 pour les binaires).
//!
//! Le superviseur est lancé par `systemd-run` : une unité transitoire, hors du groupe de contrôle
//! du service, qui survit à `systemctl stop hearth-agent` (qui tue tout le groupe de contrôle,
//! y compris un processus « détaché » qui en fait partie).

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::application::ports::{SupervisorLock, UpdateHost, UpdateHostError};
use crate::domain::install::{
    DATABASE_FILE, DATABASE_FILES, UPDATE_DB_BACKUP_FILE, UPDATE_DIR, UPDATE_JOB_FILE,
    UPDATE_LAST_FILE, UPDATE_LOCK_FILE, UPDATE_STAGED_FILE, UPDATE_STATE_FILE,
    UPDATE_SUPERVISOR_FILE, UPDATE_WAL_BACKUP_FILE, Version,
};
use crate::domain::update::{Job, SupervisorState, UpdateRecord};
use crate::infrastructure::install::scrub::scrubbed;

/// Nom de l'unité transitoire du superviseur : un seul à la fois, par construction.
pub const SUPERVISOR_UNIT: &str = "hearth-agent-update";

/// Combien de temps le nouveau binaire a pour répondre à `--version`.
const VERSION_TIMEOUT: Duration = Duration::from_secs(10);

/// Comment le superviseur est lancé.
#[derive(Debug, Clone)]
pub enum Launcher {
    /// `systemd-run` (le chemin du programme) : le cas de production.
    SystemdRun(OsString),
    /// Un processus détaché du groupe de processus de l'agent : développement et tests, sans
    /// systemd. Ne survit pas à un `systemctl stop` : jamais en production.
    Detached,
}

/// Combien de temps un superviseur insiste pour prendre le verrou avant de renoncer : l'agent le
/// teste (`supervisor_running`) chaque seconde, et ce test le tient un instant.
const LOCK_PATIENCE: Duration = Duration::from_secs(1);
const LOCK_RETRY: Duration = Duration::from_millis(20);

pub struct FsUpdateHost {
    data_dir: PathBuf,
    dir: PathBuf,
    /// Le binaire en cours d'exécution, copié pour jouer le superviseur.
    current_exe: PathBuf,
    launcher: Launcher,
}

impl FsUpdateHost {
    pub fn new(data_dir: &Path, current_exe: PathBuf, launcher: Launcher) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
            dir: data_dir.join(UPDATE_DIR),
            current_exe,
            launcher,
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    fn io(action: &'static str, path: &Path) -> impl FnOnce(std::io::Error) -> UpdateHostError {
        let path = path.display().to_string();
        move |source| UpdateHostError::Io {
            action,
            path,
            source,
        }
    }

    /// Crée `update/` (0700) s'il manque.
    fn ensure_dir(&self) -> Result<(), UpdateHostError> {
        fs::create_dir_all(&self.dir).map_err(Self::io("création du dossier", &self.dir))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.dir, fs::Permissions::from_mode(0o700))
                .map_err(Self::io("droits du dossier", &self.dir))?;
        }
        Ok(())
    }

    /// Écrit `bytes` dans `name` : fichier voisin, droits, renommage.
    fn write_atomic(
        &self,
        name: &str,
        bytes: &[u8],
        mode: u32,
    ) -> Result<PathBuf, UpdateHostError> {
        self.ensure_dir()?;
        let path = self.path(name);
        // Un nom par processus : l'agent et le superviseur écrivent parfois le même fichier.
        let temporary = self.path(&format!("{name}.{}.tmp", std::process::id()));
        let write = || -> std::io::Result<()> {
            let mut options = OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(mode);
            }
            #[cfg(not(unix))]
            let _ = mode;
            let mut file = options.open(&temporary)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, &path)?;
            // Le renommage lui-même doit survivre à une coupure de courant : fsync du dossier.
            #[cfg(unix)]
            File::open(&self.dir)?.sync_all()?;
            Ok(())
        };
        write().map_err(|source| {
            let _ = fs::remove_file(&temporary);
            Self::io("écriture", &path)(source)
        })?;
        Ok(path)
    }

    fn write_json<T: Serialize>(&self, name: &str, value: &T) -> Result<PathBuf, UpdateHostError> {
        let bytes = serde_json::to_vec_pretty(value)
            .map_err(|error| UpdateHostError::Other(error.to_string()))?;
        self.write_atomic(name, &bytes, 0o600)
    }

    fn read_json<T: DeserializeOwned>(&self, name: &str) -> Result<Option<T>, UpdateHostError> {
        let path = self.path(name);
        match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|error| {
                UpdateHostError::Other(format!("{} illisible : {error}", path.display()))
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(Self::io("lecture", &path)(error)),
        }
    }

    fn lock_file(&self) -> Result<File, UpdateHostError> {
        self.ensure_dir()?;
        let path = self.path(UPDATE_LOCK_FILE);
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(Self::io("ouverture du verrou", &path))
    }
}

impl UpdateHost for FsUpdateHost {
    fn supervisor_running(&self) -> bool {
        // Une lecture ne crée rien : sans fichier de verrou, aucun superviseur n'a jamais travaillé
        // ici (et `GET /agent/update` ne doit pas laisser de trace sur le disque).
        let Ok(file) = OpenOptions::new()
            .read(true)
            .write(true)
            .open(self.path(UPDATE_LOCK_FILE))
        else {
            return false;
        };
        match file.try_lock() {
            Ok(()) => {
                let _ = file.unlock();
                false
            }
            Err(TryLockError::WouldBlock) => true,
            Err(TryLockError::Error(_)) => false,
        }
    }

    fn take_supervisor_lock(&self) -> Result<SupervisorLock, UpdateHostError> {
        let file = self.lock_file()?;
        // Il insiste une seconde : un test de l'agent (`supervisor_running`) tient le verrou un
        // instant, et le superviseur ne doit pas renoncer à cause de lui.
        let deadline = Instant::now() + LOCK_PATIENCE;
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(SupervisorLock(Box::new(file))),
                Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                    std::thread::sleep(LOCK_RETRY);
                }
                Err(TryLockError::WouldBlock) => return Err(UpdateHostError::AlreadyRunning),
                Err(TryLockError::Error(source)) => {
                    return Err(Self::io(
                        "verrou du superviseur",
                        &self.path(UPDATE_LOCK_FILE),
                    )(source));
                }
            }
        }
    }

    fn read_last(&self) -> Result<Option<UpdateRecord>, UpdateHostError> {
        self.read_json(UPDATE_LAST_FILE)
    }

    fn write_last(&self, record: &UpdateRecord) -> Result<(), UpdateHostError> {
        self.write_json(UPDATE_LAST_FILE, record).map(|_| ())
    }

    fn read_state(&self) -> Result<Option<SupervisorState>, UpdateHostError> {
        self.read_json(UPDATE_STATE_FILE)
    }

    fn read_job(&self) -> Result<Option<Job>, UpdateHostError> {
        self.read_json(UPDATE_JOB_FILE)
    }

    fn path_exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn backup_database(&self) -> Result<(), UpdateHostError> {
        self.ensure_dir()?;
        if !self.data_dir.join(DATABASE_FILE).is_file() {
            return Ok(());
        }
        // Service arrêté (le superviseur vient de l'arrêter) : la base est fermée, son journal est
        // vide ou absent. Copie dans un fichier voisin puis renommage : jamais une copie à moitié
        // écrite sous le nom de la sauvegarde.
        for (name, backup) in [
            (DATABASE_FILE, UPDATE_DB_BACKUP_FILE),
            (DATABASE_FILES[1], UPDATE_WAL_BACKUP_FILE),
        ] {
            let source = self.data_dir.join(name);
            let target = self.path(backup);
            if source.is_file() {
                copy_atomic(&source, &target, &self.dir)?;
            } else {
                let _ = fs::remove_file(&target);
            }
        }
        Ok(())
    }

    fn database_size(&self) -> u64 {
        DATABASE_FILES[..2]
            .iter()
            .filter_map(|name| fs::metadata(self.data_dir.join(name)).ok())
            .map(|meta| meta.len())
            .sum()
    }

    fn data_dir(&self) -> PathBuf {
        self.data_dir.clone()
    }

    fn database_copy_present(&self) -> bool {
        self.path(UPDATE_DB_BACKUP_FILE).is_file()
    }

    fn remove_path(&self, path: &Path) -> Result<(), UpdateHostError> {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(Self::io("retrait", path)(error)),
        }
    }

    fn discard_work_files(&self) {
        for name in [UPDATE_STATE_FILE, UPDATE_JOB_FILE] {
            let _ = fs::remove_file(self.path(name));
        }
    }

    fn restore_database(&self) -> Result<(), UpdateHostError> {
        let saved = self.path(UPDATE_DB_BACKUP_FILE);
        if !saved.is_file() {
            // Aucune base à l'époque (ou aucune copie : l'échange n'a pas eu lieu, ou elle a déjà
            // servi).
            return Ok(());
        }
        // 1. Tout est copié dans des fichiers voisins : un disque plein ou une coupure ici laisse la
        // base vivante intacte (le service reste arrêté, rien n'est perdu).
        let database = self.data_dir.join(DATABASE_FILE);
        let wal = self.data_dir.join(DATABASE_FILES[1]);
        let saved_wal = self.path(UPDATE_WAL_BACKUP_FILE);
        let staged_database = neighbour(&database);
        let staged_wal = neighbour(&wal);
        let staged = (|| -> Result<(), UpdateHostError> {
            copy_synced(&saved, &staged_database)?;
            if saved_wal.is_file() {
                copy_synced(&saved_wal, &staged_wal)?;
            }
            Ok(())
        })();
        if let Err(error) = staged {
            let _ = fs::remove_file(&staged_database);
            let _ = fs::remove_file(&staged_wal);
            return Err(error);
        }
        // 2. Les fichiers d'après la copie (journal, mémoire partagée) sont ceux de la base migrée :
        // ils partent, puis les renommages (atomiques) mettent la copie en place.
        for name in &DATABASE_FILES[1..] {
            let path = self.data_dir.join(name);
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(Self::io("retrait d'un fichier de la base", &path)(error));
                }
            }
        }
        fs::rename(&staged_database, &database)
            .map_err(Self::io("restauration de la base", &database))?;
        if saved_wal.is_file() {
            fs::rename(&staged_wal, &wal).map_err(Self::io("restauration du journal", &wal))?;
        }
        sync_dir(&self.data_dir);
        // 3. Elle ne sert qu'une fois : retirée tout de suite, une reprise ultérieure ne peut pas
        // recopier une base périmée par-dessus ce qui s'est écrit depuis (BR-UPDATE-029).
        let _ = fs::remove_file(&saved);
        let _ = fs::remove_file(&saved_wal);
        Ok(())
    }

    fn write_state(&self, state: &SupervisorState) -> Result<(), UpdateHostError> {
        self.write_json(UPDATE_STATE_FILE, state).map(|_| ())
    }

    fn stage(&self, bytes: &[u8]) -> Result<PathBuf, UpdateHostError> {
        self.write_atomic(UPDATE_STAGED_FILE, bytes, 0o700)
    }

    fn staged_version(&self, path: &Path) -> Result<Version, UpdateHostError> {
        let mut child = scrubbed(path)
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(Self::io("exécution de --version", path))?;
        let deadline = Instant::now() + VERSION_TIMEOUT;
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => break,
                Ok(Some(status)) => {
                    return Err(UpdateHostError::Other(format!("--version : {status}")));
                }
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(UpdateHostError::Other(
                        "--version : délai dépassé".to_owned(),
                    ));
                }
                Err(source) => return Err(Self::io("attente de --version", path)(source)),
            }
        }
        let mut output = String::new();
        if let Some(stdout) = child.stdout.as_mut() {
            let _ = stdout.take(4096).read_to_string(&mut output);
        }
        Version::parse_version_output(&output)
            .map_err(|error| UpdateHostError::Other(error.to_string()))
    }

    fn prepare_supervisor(&self) -> Result<PathBuf, UpdateHostError> {
        let bytes = fs::read(&self.current_exe)
            .map_err(Self::io("lecture du binaire en cours", &self.current_exe))?;
        self.write_atomic(UPDATE_SUPERVISOR_FILE, &bytes, 0o700)
    }

    fn write_job(&self, job: &Job) -> Result<PathBuf, UpdateHostError> {
        self.write_json(UPDATE_JOB_FILE, job)
    }

    fn launch(&self, supervisor: &Path, job: &Path) -> Result<(), UpdateHostError> {
        match &self.launcher {
            Launcher::SystemdRun(program) => {
                let output = scrubbed(program)
                    .arg("--unit")
                    .arg(SUPERVISOR_UNIT)
                    .arg("--description")
                    .arg("Mise à jour de l'agent Hearth")
                    .arg("--collect")
                    .arg("--quiet")
                    // Échoue tout de suite si le binaire ne s'exécute pas (dossier de données
                    // monté `noexec`) au lieu de répondre « lancé ».
                    .arg("--service-type=exec")
                    // Durcissement de l'unité transitoire : le superviseur écrit dans le dossier
                    // du binaire et dans `update/`, lance `systemctl`, rien d'autre.
                    .arg("--property=NoNewPrivileges=yes")
                    .arg("--property=PrivateTmp=yes")
                    .arg("--property=ProtectHome=yes")
                    .arg(supervisor)
                    .arg("update-supervise")
                    .arg("--job")
                    .arg(job)
                    .stdin(Stdio::null())
                    .output()
                    .map_err(Self::io("lancement de systemd-run", supervisor))?;
                if output.status.success() {
                    Ok(())
                } else {
                    Err(UpdateHostError::Other(format!(
                        "systemd-run : {} : {}",
                        output.status,
                        String::from_utf8_lossy(&output.stderr).trim()
                    )))
                }
            }
            Launcher::Detached => detached(supervisor, job),
        }
    }

    fn clear_staging(&self) {
        for name in [
            UPDATE_STAGED_FILE,
            UPDATE_SUPERVISOR_FILE,
            UPDATE_JOB_FILE,
            UPDATE_STATE_FILE,
            UPDATE_DB_BACKUP_FILE,
            UPDATE_WAL_BACKUP_FILE,
        ] {
            let path = self.path(name);
            if let Err(error) = fs::remove_file(&path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(%error, path = %path.display(), "fichier de mise à jour non retiré");
            }
        }
    }
}

/// `hearth.db` devient `hearth.db.<pid>.tmp` (le même dossier : le renommage est atomique).
fn neighbour(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".{}.tmp", std::process::id()));
    PathBuf::from(name)
}

/// Copie `from` dans `to` (fichier voisin du même dossier `dir`, puis renommage), `fsync` compris.
fn copy_atomic(from: &Path, to: &Path, dir: &Path) -> Result<(), UpdateHostError> {
    let staged = neighbour(to);
    let result = copy_synced(from, &staged)
        .and_then(|()| fs::rename(&staged, to).map_err(FsUpdateHost::io("copie de la base", to)));
    if result.is_err() {
        let _ = fs::remove_file(&staged);
        return result;
    }
    sync_dir(dir);
    Ok(())
}

fn copy_synced(from: &Path, to: &Path) -> Result<(), UpdateHostError> {
    fs::copy(from, to).map_err(FsUpdateHost::io("copie de la base", from))?;
    OpenOptions::new()
        .write(true)
        .open(to)
        .and_then(|file| file.sync_all())
        .map_err(FsUpdateHost::io("écriture de la copie de la base", to))
}

/// `fsync` du dossier (Unix) : le renommage lui-même survit à une coupure de courant.
fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    if let Ok(file) = File::open(dir) {
        let _ = file.sync_all();
    }
    #[cfg(not(unix))]
    let _ = dir;
}

#[cfg(unix)]
fn detached(supervisor: &Path, job: &Path) -> Result<(), UpdateHostError> {
    use std::os::unix::process::CommandExt;
    scrubbed(supervisor)
        .arg("update-supervise")
        .arg("--job")
        .arg(job)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // Son propre groupe de processus : il ne reçoit pas les signaux du groupe de l'agent.
        .process_group(0)
        .spawn()
        .map(|_| ())
        .map_err(FsUpdateHost::io("lancement du superviseur", supervisor))
}

#[cfg(not(unix))]
fn detached(_supervisor: &Path, _job: &Path) -> Result<(), UpdateHostError> {
    Err(UpdateHostError::Other(
        "le lancement détaché n'existe que sous Unix".to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use hearth_proto::api::update::{UpdateOutcome, UpdateStep};

    use super::*;

    fn host() -> (tempfile::TempDir, FsUpdateHost) {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("agent-courant");
        fs::write(&exe, b"binaire courant").unwrap();
        let host = FsUpdateHost::new(dir.path(), exe, Launcher::Detached);
        (dir, host)
    }

    #[test]
    fn the_last_result_and_the_state_survive_a_new_host_on_the_same_directory() {
        let (dir, host) = host();
        assert_eq!(host.read_last().unwrap(), None);
        let record = UpdateRecord {
            version: "0.2.0".into(),
            previous: "0.1.0".into(),
            outcome: UpdateOutcome::Succeeded,
            reason: None,
            at: "2026-10-05T10:00:00Z".into(),
            requested_by: Some("marie".into()),
            client_name: None,
            client_addr: None,
            reported: false,
        };
        host.write_last(&record).unwrap();
        host.write_state(&SupervisorState {
            version: "0.2.0".into(),
            step: UpdateStep::Check,
            previous: "0.1.0".into(),
            requester: Default::default(),
        })
        .unwrap();
        // Un autre processus (le nouvel agent) relit le même dossier.
        let other = FsUpdateHost::new(dir.path(), dir.path().join("x"), Launcher::Detached);
        assert_eq!(other.read_last().unwrap(), Some(record));
        assert_eq!(other.read_state().unwrap().unwrap().step, UpdateStep::Check);
        assert!(!dir.path().join("update").join("last.json.tmp").exists());
    }

    #[test]
    fn clearing_the_staging_keeps_the_result_and_the_lock() {
        let (dir, host) = host();
        host.stage(b"nouveau").unwrap();
        host.prepare_supervisor().unwrap();
        host.write_state(&SupervisorState {
            version: "0.2.0".into(),
            step: UpdateStep::Restart,
            previous: "0.1.0".into(),
            requester: Default::default(),
        })
        .unwrap();
        host.write_last(&UpdateRecord {
            version: "0.2.0".into(),
            previous: "0.1.0".into(),
            outcome: UpdateOutcome::Failed,
            reason: None,
            at: "x".into(),
            requested_by: None,
            client_name: None,
            client_addr: None,
            reported: true,
        })
        .unwrap();
        host.clear_staging();
        let update = dir.path().join("update");
        assert!(!update.join("hearth-agent.new").exists());
        assert!(!update.join("supervisor").exists());
        assert!(!update.join("state.json").exists());
        assert!(update.join("last.json").exists(), "le résultat reste");
    }

    #[test]
    fn reading_the_state_creates_nothing_on_the_disk() {
        let (dir, host) = host();
        assert!(!host.supervisor_running());
        assert_eq!(host.read_last().unwrap(), None);
        assert_eq!(host.read_state().unwrap(), None);
        assert!(!dir.path().join("update").exists());
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .map(|entries| {
                entries
                    .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    #[test]
    fn the_database_copy_is_written_aside_then_renamed_with_its_journal_and_leaves_no_temporary() {
        let (dir, host) = host();
        fs::write(dir.path().join("hearth.db"), b"base").unwrap();
        fs::write(dir.path().join("hearth.db-wal"), b"journal").unwrap();
        assert_eq!(host.database_size(), 4 + 7);
        assert!(!host.database_copy_present());
        host.backup_database().unwrap();
        assert!(host.database_copy_present());
        let update = dir.path().join("update");
        assert_eq!(fs::read(update.join("hearth.db.before")).unwrap(), b"base");
        assert_eq!(
            fs::read(update.join("hearth.db-wal.before")).unwrap(),
            b"journal"
        );
        assert!(
            names(&update).iter().all(|n| !n.ends_with(".tmp")),
            "{:?}",
            names(&update)
        );
    }

    #[test]
    fn the_database_is_restored_once_then_the_copy_is_gone_and_a_later_restore_changes_nothing() {
        let (dir, host) = host();
        let db = dir.path().join("hearth.db");
        fs::write(&db, b"avant").unwrap();
        host.backup_database().unwrap();
        // La nouvelle version migre, écrit un journal et une mémoire partagée.
        fs::write(&db, b"migree").unwrap();
        fs::write(dir.path().join("hearth.db-wal"), b"j").unwrap();
        fs::write(dir.path().join("hearth.db-shm"), b"s").unwrap();
        host.restore_database().unwrap();
        assert_eq!(fs::read(&db).unwrap(), b"avant");
        assert!(!dir.path().join("hearth.db-wal").exists());
        assert!(!dir.path().join("hearth.db-shm").exists());
        assert!(
            !host.database_copy_present(),
            "la copie ne sert qu'une fois"
        );
        assert!(
            names(dir.path()).iter().all(|n| !n.ends_with(".tmp")),
            "{:?}",
            names(dir.path())
        );
        // Les écritures d'après le retour arrière ne sont jamais écrasées par une copie périmée.
        fs::write(&db, b"ecritures-d-apres").unwrap();
        host.restore_database().unwrap();
        assert_eq!(fs::read(&db).unwrap(), b"ecritures-d-apres");
    }

    #[test]
    fn a_restore_that_cannot_copy_leaves_the_live_database_untouched() {
        let (dir, host) = host();
        let db = dir.path().join("hearth.db");
        fs::write(&db, b"vivante").unwrap();
        // Le fichier voisin où la copie doit être écrite est impossible (un dossier à sa place) : la
        // copie échoue avant tout retrait ou renommage, comme un disque plein.
        host.backup_database().unwrap();
        fs::write(&db, b"migree-vivante").unwrap();
        fs::create_dir(neighbour(&db)).unwrap();
        assert!(host.restore_database().is_err());
        assert_eq!(fs::read(&db).unwrap(), b"migree-vivante");
        assert!(
            host.database_copy_present(),
            "la copie est gardée pour un nouvel essai"
        );
    }

    #[test]
    fn the_supervisor_lock_is_exclusive_and_released_on_drop() {
        let (_dir, host) = host();
        assert!(!host.supervisor_running());
        let lock = host.take_supervisor_lock().unwrap();
        assert!(host.supervisor_running());
        assert!(matches!(
            host.take_supervisor_lock(),
            Err(UpdateHostError::AlreadyRunning)
        ));
        drop(lock);
        assert!(!host.supervisor_running(), "relâché avec le processus");
        assert!(host.take_supervisor_lock().is_ok());
    }

    #[test]
    fn the_supervisor_is_a_copy_of_the_running_binary_and_the_staged_file_is_private() {
        let (dir, host) = host();
        let supervisor = host.prepare_supervisor().unwrap();
        assert_eq!(fs::read(&supervisor).unwrap(), b"binaire courant");
        let staged = host.stage(b"x").unwrap();
        assert_eq!(staged, dir.path().join("update").join("hearth-agent.new"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&staged), 0o700);
            assert_eq!(mode(&supervisor), 0o700);
            assert_eq!(mode(&dir.path().join("update")), 0o700);
        }
    }

    #[cfg(unix)]
    #[test]
    fn the_staged_version_comes_from_dash_dash_version_and_a_broken_file_is_an_error() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, host) = host();
        let script = dir.path().join("nouveau");
        fs::write(&script, "#!/bin/sh\necho \"hearth-agent 0.2.0\"\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(host.staged_version(&script).unwrap(), Version::new(0, 2, 0));

        let broken = dir.path().join("casse");
        fs::write(&broken, "pas un binaire").unwrap();
        fs::set_permissions(&broken, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(host.staged_version(&broken).is_err());

        let failing = dir.path().join("echec");
        fs::write(&failing, "#!/bin/sh\nexit 3\n").unwrap();
        fs::set_permissions(&failing, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(host.staged_version(&failing).is_err());
    }
}
