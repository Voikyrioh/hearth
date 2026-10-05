//! La machine pour l'installation : ce qu'elle observe et les fichiers qu'elle écrit. Que de la
//! bibliothèque standard : l'architecture vient de la compilation, les droits de `/proc`, l'espace
//! libre de `df`, le port d'un essai d'écoute. Aucun interpréteur de commandes : `df` et le
//! binaire installé (`--version`) sont lancés avec une liste d'arguments.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener};
use std::path::Path;
use std::process::Stdio;
use std::thread;
use std::time::{Duration, Instant};

use super::probe;
use super::scrub::scrubbed;
use crate::application::ports::{
    Answered, BinaryInstalled, ConfigSpec, HostError, HostFacts, InstallHost, InstallLock,
    InstallPaths,
};
use crate::domain::install::{
    BinaryState, DATABASE_FILE, DataDirState, DataState, IDENTITY_CONTENT_FILES, Version,
    binary_temporary_name,
};
use crate::infrastructure::config;
use crate::infrastructure::data_dir;

/// Pause entre deux essais pendant qu'on attend l'agent.
const RETRY: Duration = Duration::from_millis(300);
/// Au-delà, `--version` d'un binaire installé est considéré comme sans réponse.
const VERSION_TIMEOUT: Duration = Duration::from_secs(5);
pub struct SystemHost;

fn io_error(action: &'static str, path: &Path) -> impl FnOnce(io::Error) -> HostError {
    let path = path.display().to_string();
    move |source| HostError::Io {
        action,
        path,
        source,
    }
}

impl InstallHost for SystemHost {
    fn os(&self) -> String {
        std::env::consts::OS.to_owned()
    }

    fn arch(&self) -> String {
        std::env::consts::ARCH.to_owned()
    }

    fn is_privileged(&self) -> bool {
        // Linux : l'utilisateur effectif est la deuxième valeur de la ligne `Uid:`.
        fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|status| {
                status
                    .lines()
                    .find_map(|line| line.strip_prefix("Uid:"))
                    .and_then(|ids| ids.split_whitespace().nth(1).map(|uid| uid == "0"))
            })
            .unwrap_or(false)
    }

    fn port_taken(&self, addr: IpAddr, port: u16) -> bool {
        matches!(
            TcpListener::bind((addr, port)),
            Err(error) if error.kind() == io::ErrorKind::AddrInUse
        )
    }

    fn free_bytes(&self, path: &Path) -> Result<u64, HostError> {
        let existing = path
            .ancestors()
            .find(|candidate| candidate.exists())
            .unwrap_or(Path::new("/"));
        let output = scrubbed("df")
            .args(["-Pk"])
            .arg(existing)
            .stdin(Stdio::null())
            .output()
            .map_err(io_error("lecture de l'espace libre", existing))?;
        parse_df(&String::from_utf8_lossy(&output.stdout)).ok_or_else(|| {
            HostError::Other(format!(
                "espace libre illisible pour {}",
                existing.display()
            ))
        })
    }

    fn hostname(&self) -> String {
        gethostname::gethostname().to_string_lossy().into_owned()
    }

    fn inspect(&self, paths: &InstallPaths, source: &Path) -> Result<HostFacts, HostError> {
        let binary = if paths.binary.is_file() {
            BinaryState::Present {
                version: installed_version(&paths.binary),
                identical: same_bytes(source, &paths.binary),
            }
        } else {
            BinaryState::Absent
        };
        let data_dir = &paths.data_dir;
        let identity_present = IDENTITY_CONTENT_FILES
            .iter()
            .filter(|name| data_dir.join(name).exists())
            .count();
        let data = DataState {
            dir_exists: data_dir.is_dir(),
            identity: identity_present == 3,
            database: data_dir.join(DATABASE_FILE).is_file(),
            identity_partial: identity_present > 0 && identity_present < 3,
        };
        Ok(HostFacts {
            binary,
            data,
            config_exists: paths.config.is_file(),
            configured_port: config::read_port(&paths.config),
        })
    }

    fn lock(&self, path: &Path) -> Result<InstallLock, HostError> {
        let mut options = OpenOptions::new();
        options.create(true).write(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(path)
            .map_err(io_error("ouverture du verrou d'installation", path))?;
        match file.try_lock() {
            Ok(()) => Ok(InstallLock(Box::new(HeldLock(file)))),
            Err(fs::TryLockError::WouldBlock) => Err(HostError::AlreadyRunning),
            Err(fs::TryLockError::Error(error)) => {
                Err(io_error("verrou d'installation", path)(error))
            }
        }
    }

    fn install_binary(
        &self,
        source: &Path,
        dest: &Path,
        backup: &Path,
    ) -> Result<BinaryInstalled, HostError> {
        let temporary = dest.with_file_name(binary_temporary_name(std::process::id()));
        let result = (|| -> Result<BinaryInstalled, HostError> {
            // Fichier voisin, droits 0755 dès la création, puis renommage : jamais un binaire à
            // moitié copié sous le nom définitif.
            let mut options = OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o755);
            }
            let mut input = File::open(source).map_err(io_error("lecture du binaire", source))?;
            let mut output = options
                .open(&temporary)
                .map_err(io_error("écriture du binaire", &temporary))?;
            io::copy(&mut input, &mut output)
                .and_then(|_| output.sync_all())
                .map_err(io_error("copie du binaire", &temporary))?;
            drop(output);

            // L'ancien binaire reste lisible sous un second nom pendant le remplacement.
            let had_previous = dest.is_file();
            if had_previous {
                let _ = fs::remove_file(backup);
                if fs::hard_link(dest, backup).is_err() {
                    fs::copy(dest, backup).map_err(io_error("sauvegarde du binaire", backup))?;
                }
            }
            fs::rename(&temporary, dest).map_err(io_error("remplacement du binaire", dest))?;
            Ok(BinaryInstalled {
                backup: had_previous.then(|| backup.to_path_buf()),
            })
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn restore_binary(&self, dest: &Path, installed: &BinaryInstalled) -> Result<(), HostError> {
        match &installed.backup {
            Some(backup) => {
                fs::rename(backup, dest).map_err(io_error("rétablissement du binaire", dest))
            }
            None => self.remove_file(dest),
        }
    }

    fn discard_backup(&self, installed: &BinaryInstalled) {
        if let Some(backup) = &installed.backup {
            let _ = fs::remove_file(backup);
        }
    }

    fn ensure_data_dir(&self, dir: &Path) -> Result<bool, HostError> {
        let existed = dir.is_dir();
        data_dir::ensure(dir).map_err(io_error("dossier de données", dir))?;
        Ok(!existed)
    }

    fn write_config_new(&self, path: &Path, spec: &ConfigSpec) -> Result<bool, HostError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(io_error("dossier de configuration", parent))?;
        }
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            // La configuration ne contient aucun secret : lisible, jamais modifiable par d'autres.
            options.mode(0o644);
        }
        let mut file = match options.open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Ok(false),
            Err(error) => return Err(io_error("écriture de la configuration", path)(error)),
        };
        let text = config::render_file(spec.port, spec.managed, spec.data_dir.as_deref());
        file.write_all(text.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(io_error("écriture de la configuration", path))?;
        Ok(true)
    }

    fn remove_file(&self, path: &Path) -> Result<(), HostError> {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io_error("suppression", path)(error)),
        }
    }

    fn list_dir(&self, path: &Path) -> Result<Vec<String>, HostError> {
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(io_error("lecture du dossier", path)(error)),
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        Ok(names)
    }

    fn data_dir_state(&self, path: &Path) -> DataDirState {
        let Ok(meta) = fs::metadata(path) else {
            return DataDirState::ABSENT;
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            DataDirState {
                exists: true,
                directory: meta.is_dir(),
                owned_by_root: meta.uid() == 0,
                private: meta.permissions().mode() & 0o077 == 0,
            }
        }
        #[cfg(not(unix))]
        {
            DataDirState {
                exists: true,
                directory: meta.is_dir(),
                owned_by_root: true,
                private: true,
            }
        }
    }

    fn remove_dir_if_empty(&self, path: &Path) -> Result<(), HostError> {
        match fs::remove_dir(path) {
            Ok(()) => Ok(()),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::DirectoryNotEmpty
                ) =>
            {
                Ok(())
            }
            Err(error) => Err(io_error("suppression du dossier", path)(error)),
        }
    }

    fn wait_for_hello(
        &self,
        addr: SocketAddr,
        timeout: Duration,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Answered, HostError> {
        let deadline = Instant::now() + timeout;
        loop {
            if cancelled() {
                return Err(HostError::Other("interrompu".to_owned()));
            }
            match probe::hello(addr, Duration::from_secs(3)) {
                Ok(served) => return Ok(Answered { served }),
                Err(error) if Instant::now() >= deadline => {
                    return Err(HostError::Other(error.to_string()));
                }
                Err(_) => thread::sleep(RETRY),
            }
        }
    }
}

/// La version d'un binaire installé, par `--version` (avec un délai) ; `None` si illisible.
fn installed_version(binary: &Path) -> Option<Version> {
    let mut child = scrubbed(binary)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + VERSION_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) | Err(_) => return None,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => thread::sleep(Duration::from_millis(20)),
        }
    }
    let mut output = String::new();
    child.stdout.take()?.read_to_string(&mut output).ok()?;
    Version::parse_version_output(&output).ok()
}

/// Deux fichiers ont-ils exactement les mêmes octets ?
fn same_bytes(a: &Path, b: &Path) -> bool {
    let (Ok(mut a), Ok(mut b)) = (File::open(a), File::open(b)) else {
        return false;
    };
    let (Ok(meta_a), Ok(meta_b)) = (a.metadata(), b.metadata()) else {
        return false;
    };
    if meta_a.len() != meta_b.len() {
        return false;
    }
    let (mut left, mut right) = (vec![0_u8; 64 * 1024], vec![0_u8; 64 * 1024]);
    loop {
        let read_a = fill(&mut a, &mut left);
        let read_b = fill(&mut b, &mut right);
        match (read_a, read_b) {
            (Ok(n), Ok(m)) if n == m && left[..n] == right[..m] => {
                if n == 0 {
                    return true;
                }
            }
            _ => return false,
        }
    }
}

fn fill(file: &mut File, buffer: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        let n = file.read(&mut buffer[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    Ok(filled)
}

/// Octets disponibles d'après la sortie de `df -Pk` (colonne « Available », en Kio).
fn parse_df(output: &str) -> Option<u64> {
    let line = output.lines().nth(1)?;
    let kib: u64 = line.split_whitespace().nth(3)?.parse().ok()?;
    kib.checked_mul(1024)
}

/// Le verrou tenu : relâché **explicitement** à la destruction, pas seulement par la fermeture du
/// descripteur.
///
/// Un `flock` appartient à la description de fichier ouverte, pas au descripteur. Si un autre
/// thread lance un sous-processus à cet instant, l'enfant porte une copie du descripteur entre le
/// `fork` et l'`exec` (qui le ferme : `O_CLOEXEC`) ; fermer le nôtre ne suffit alors pas, le
/// verrou reste pris le temps de cet `exec`, et l'opération suivante est refusée à tort.
/// `unlock` agit sur la description : elle libère le verrou quels que soient les copies vivantes.
struct HeldLock(File);

impl Drop for HeldLock {
    fn drop(&mut self) {
        // FIX:01M46N01GMK08NXQHCZ28A2KQ1 : relâche le verrou même si un enfant en cours de
        // lancement garde encore une copie du descripteur (docs/bugs/FIX-01M46N01GMK08NXQHCZ28A2KQ1.md).
        // Échec ignoré : la fermeture qui suit le relâche de toute façon, et on ne panique pas
        // dans un `drop`.
        let _ = self.0.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn df_output_is_read_in_kibibytes() {
        let output = "Filesystem 1024-blocks Used Available Capacity Mounted on\n/dev/sda1 1000 400 600 40% /\n";
        assert_eq!(parse_df(output), Some(600 * 1024));
        assert_eq!(parse_df(""), None);
        assert_eq!(parse_df("a\nb c d e"), None);
    }

    #[test]
    fn this_machine_is_described_by_the_standard_library() {
        let host = SystemHost;
        assert_eq!(host.os(), std::env::consts::OS);
        assert_eq!(host.arch(), std::env::consts::ARCH);
        assert!(!host.hostname().is_empty());
    }

    #[test]
    fn a_listening_port_is_taken_and_a_free_one_is_not() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("écoute");
        let port = listener.local_addr().expect("adresse").port();
        let host = SystemHost;
        assert!(host.port_taken(IpAddr::from([127, 0, 0, 1]), port));
        drop(listener);
        assert!(!host.port_taken(IpAddr::from([127, 0, 0, 1]), port));
    }

    #[test]
    fn identical_files_are_recognised_by_their_bytes() {
        let dir = tempfile::tempdir().expect("dossier");
        let (a, b, c) = (
            dir.path().join("a"),
            dir.path().join("b"),
            dir.path().join("c"),
        );
        std::fs::write(&a, vec![7_u8; 200_000]).expect("a");
        std::fs::write(&b, vec![7_u8; 200_000]).expect("b");
        let mut other = vec![7_u8; 200_000];
        other[199_999] = 8;
        std::fs::write(&c, other).expect("c");
        assert!(same_bytes(&a, &b));
        assert!(!same_bytes(&a, &c));
        assert!(!same_bytes(&a, &dir.path().join("missing")));
    }

    #[test]
    fn the_config_is_written_once_and_never_overwritten() {
        let dir = tempfile::tempdir().expect("dossier");
        let path = dir.path().join("etc").join("agent.toml");
        let host = SystemHost;
        let first = ConfigSpec {
            port: 7341,
            managed: false,
            data_dir: None,
        };
        assert!(host.write_config_new(&path, &first).expect("première"));
        let before = std::fs::read_to_string(&path).expect("lecture");
        let second = ConfigSpec {
            port: 9000,
            ..first
        };
        assert!(!host.write_config_new(&path, &second).expect("seconde"));
        assert_eq!(std::fs::read_to_string(&path).expect("lecture"), before);
    }

    #[test]
    fn a_directory_is_listed_by_name_and_never_removed_with_its_content() {
        let dir = tempfile::tempdir().expect("dossier");
        let host = SystemHost;
        assert_eq!(
            host.list_dir(&dir.path().join("none")).expect("absent"),
            Vec::<String>::new()
        );
        let shared = dir.path().join("shared");
        std::fs::create_dir(&shared).expect("dossier");
        std::fs::write(shared.join("b.txt"), "x").expect("fichier");
        std::fs::write(shared.join("a.txt"), "x").expect("fichier");
        assert_eq!(host.list_dir(&shared).expect("liste"), ["a.txt", "b.txt"]);
        host.remove_dir_if_empty(&shared)
            .expect("non vide : sans effet");
        assert!(shared.join("a.txt").exists() && shared.join("b.txt").exists());
    }

    #[test]
    fn removing_what_is_absent_is_not_an_error() {
        let dir = tempfile::tempdir().expect("dossier");
        let host = SystemHost;
        host.remove_file(&dir.path().join("none")).expect("fichier");
        host.remove_dir_if_empty(&dir.path().join("none"))
            .expect("dossier");
        let full = dir.path().join("full");
        std::fs::create_dir(&full).expect("dossier");
        std::fs::write(full.join("x"), "x").expect("fichier");
        host.remove_dir_if_empty(&full)
            .expect("non vide : sans effet");
        assert!(full.join("x").exists());
    }
}

#[cfg(test)]
#[cfg(unix)]
mod unix_tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn write(path: &Path, bytes: &[u8]) {
        std::fs::write(path, bytes).expect("écriture");
    }

    #[test]
    fn a_first_binary_is_installed_executable_without_leftovers() {
        let dir = tempfile::tempdir().expect("dossier");
        let (source, dest, backup) = (
            dir.path().join("source"),
            dir.path().join("hearth-agent"),
            dir.path().join(".previous"),
        );
        write(&source, b"nouveau");
        let installed = SystemHost
            .install_binary(&source, &dest, &backup)
            .expect("installation");
        assert_eq!(installed.backup, None);
        assert_eq!(std::fs::read(&dest).expect("lecture"), b"nouveau");
        let mode = std::fs::metadata(&dest).expect("méta").permissions().mode() & 0o777;
        assert_eq!(mode, 0o755);
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .expect("dossier")
            .map(|e| {
                e.expect("entrée")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(names.len(), 2, "source et binaire seulement : {names:?}");
    }

    #[test]
    fn replacing_a_binary_keeps_the_old_one_aside_and_restoring_brings_it_back() {
        let dir = tempfile::tempdir().expect("dossier");
        let (source, dest, backup) = (
            dir.path().join("source"),
            dir.path().join("hearth-agent"),
            dir.path().join(".previous"),
        );
        write(&source, b"nouveau");
        write(&dest, b"ancien");
        let installed = SystemHost
            .install_binary(&source, &dest, &backup)
            .expect("remplacement");
        assert_eq!(installed.backup.as_deref(), Some(backup.as_path()));
        assert_eq!(std::fs::read(&dest).expect("lecture"), b"nouveau");
        assert_eq!(std::fs::read(&backup).expect("sauvegarde"), b"ancien");

        SystemHost
            .restore_binary(&dest, &installed)
            .expect("retour");
        assert_eq!(std::fs::read(&dest).expect("lecture"), b"ancien");
        assert!(!backup.exists());
    }

    #[test]
    fn restoring_after_a_first_install_removes_the_binary() {
        let dir = tempfile::tempdir().expect("dossier");
        let (source, dest, backup) = (
            dir.path().join("source"),
            dir.path().join("hearth-agent"),
            dir.path().join(".previous"),
        );
        write(&source, b"nouveau");
        let installed = SystemHost
            .install_binary(&source, &dest, &backup)
            .expect("installation");
        SystemHost
            .restore_binary(&dest, &installed)
            .expect("retour");
        assert!(!dest.exists());
    }

    #[test]
    fn a_failed_copy_leaves_no_temporary_and_the_old_binary_in_place() {
        let dir = tempfile::tempdir().expect("dossier");
        let dest = dir.path().join("hearth-agent");
        write(&dest, b"ancien");
        let error = SystemHost
            .install_binary(
                &dir.path().join("absent"),
                &dest,
                &dir.path().join(".previous"),
            )
            .unwrap_err();
        assert!(error.to_string().contains("absent"), "{error}");
        assert_eq!(std::fs::read(&dest).expect("lecture"), b"ancien");
        assert_eq!(std::fs::read_dir(dir.path()).expect("dossier").count(), 1);
    }

    #[test]
    fn the_version_is_read_from_the_installed_binary() {
        let dir = tempfile::tempdir().expect("dossier");
        let script = dir.path().join("hearth-agent");
        write(&script, b"#!/bin/sh\necho 'hearth-agent 3.4.5'\n");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("droits");
        assert_eq!(installed_version(&script), Some(Version::new(3, 4, 5)));
        write(&script, b"#!/bin/sh\necho rien\n");
        assert_eq!(installed_version(&script), None);
        write(&script, b"#!/bin/sh\nexit 1\n");
        assert_eq!(installed_version(&script), None);
    }

    #[test]
    fn inspect_describes_what_is_on_disk_and_changes_nothing() {
        let dir = tempfile::tempdir().expect("dossier");
        let paths = InstallPaths {
            binary: dir.path().join("hearth-agent"),
            config: dir.path().join("agent.toml"),
            data_dir: dir.path().join("data"),
            lock: dir.path().join("lock"),
        };
        let source = dir.path().join("source");
        write(&source, b"binaire");
        let facts = SystemHost.inspect(&paths, &source).expect("relevé");
        assert_eq!(facts.binary, BinaryState::Absent);
        assert_eq!(facts.data, DataState::default());
        assert!(!facts.config_exists);
        assert!(!paths.data_dir.exists(), "rien n'est créé");

        std::fs::create_dir(&paths.data_dir).expect("données");
        for name in ["cert.pem", "key.pem", "install_id", "hearth.db"] {
            write(&paths.data_dir.join(name), b"x");
        }
        write(&paths.config, b"port = 9100\n");
        let facts = SystemHost.inspect(&paths, &source).expect("relevé");
        assert!(facts.data.identity && facts.data.database && facts.data.dir_exists);
        assert!(!facts.data.identity_partial);
        assert_eq!(facts.configured_port, Some(9100));
        // Une identité à moitié là est signalée, rien n'est touché.
        std::fs::remove_file(paths.data_dir.join("install_id")).expect("retrait");
        let facts = SystemHost.inspect(&paths, &source).expect("relevé");
        assert!(facts.data.identity_partial && !facts.data.identity);
        assert!(paths.data_dir.join("cert.pem").exists());
    }

    #[test]
    fn a_second_installation_cannot_take_the_lock() {
        let dir = tempfile::tempdir().expect("dossier");
        let path = dir.path().join("install.lock");
        let first = SystemHost.lock(&path).expect("premier verrou");
        let second = SystemHost.lock(&path);
        assert!(matches!(second, Err(HostError::AlreadyRunning)));
        drop(first);
        assert!(SystemHost.lock(&path).is_ok(), "libéré avec le verrou");
    }

    /// FIX:01M46N01GMK08NXQHCZ28A2KQ1 : un enfant en cours de lancement (`fork` avant `exec`)
    /// porte une copie du descripteur ; elle ne doit pas retenir le verrou après sa libération.
    #[test]
    fn the_install_lock_is_released_while_a_copy_of_its_descriptor_is_still_alive() {
        let dir = tempfile::tempdir().expect("dossier");
        let path = dir.path().join("install.lock");
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .expect("ouverture");
        file.try_lock().expect("verrou pris");
        // Même description de fichier ouverte, comme le descripteur hérité par un enfant forké.
        let child_copy = file.try_clone().expect("copie du descripteur");

        drop(HeldLock(file));

        assert!(
            SystemHost.lock(&path).is_ok(),
            "le verrou doit être libre alors que la copie vit encore"
        );
        drop(child_copy);
    }

    #[test]
    fn the_data_dir_is_created_private() {
        let dir = tempfile::tempdir().expect("dossier");
        let data = dir.path().join("hearth");
        assert!(SystemHost.ensure_data_dir(&data).expect("création"));
        let mode = std::fs::metadata(&data).expect("méta").permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
        assert!(!SystemHost.ensure_data_dir(&data).expect("existant"));
    }
}
