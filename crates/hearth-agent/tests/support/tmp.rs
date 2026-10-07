//! Dossiers temporaires des tests (HRT-18, T43) : source unique, partagée par `#[path]` avec
//! `hearth-link` et la coquille du client.
//!
//! Deux garanties que `tempfile::tempdir()` seul ne donne pas :
//!
//! - **racine connue** : les dossiers vivent dans `<target>/hearth-test-tmp/`, pas en vrac dans
//!   `%TEMP%`. Une fuite ne remplit plus le disque système et se compte d'un coup d'œil
//!   (`cargo xtask test-tmp-check`, étape de la CI).
//! - **suppression qui compte** : sous Windows, un dossier qui contient une base SQLite encore
//!   ouverte ne se supprime pas, et `TempDir` avale l'erreur. Or, à la fin d'un test, la base est
//!   encore ouverte : les connexions rendues au pool le sont par des tâches que l'exécuteur du test
//!   n'a pas eu le temps d'exécuter, et l'exécuteur lui-même ne s'arrête qu'après le `Drop` des
//!   variables du test. `Drop` ne peut donc pas fermer la base (il bloquerait l'exécuteur qui doit
//!   libérer les connexions). Il tente la suppression tout de suite (l'espace est rendu au plus tôt)
//!   ET confie le dossier au fil du test, qui le supprime une seconde fois à sa sortie, une fois
//!   l'exécuteur arrêté et les fichiers libres (nouvelles tentatives pendant 10 s). Cette seconde
//!   passe couvre aussi une tâche de fond (gestionnaire de lien) qui réécrit son fichier après la
//!   première suppression et recrée le dossier. Ce qui reste après cela est une vraie fuite :
//!   `cargo xtask test-tmp-check` échoue.
#![allow(dead_code)]

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Nom du dossier racine, sous le dossier `target` de cargo.
pub const ROOT_NAME: &str = "hearth-test-tmp";

/// Dossier racine des dossiers de test : le `target` de cargo (repéré à son `CACHEDIR.TAG` ou à son
/// `.rustc_info.json`, en remontant depuis l'exécutable de test), sinon `%TEMP%`.
pub fn root() -> PathBuf {
    let base = std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.ancestors()
                .skip(1)
                .find(|dir| {
                    dir.join("CACHEDIR.TAG").is_file() || dir.join(".rustc_info.json").is_file()
                })
                .map(Path::to_path_buf)
        })
        .unwrap_or_else(std::env::temp_dir);
    let root = base.join(ROOT_NAME);
    std::fs::create_dir_all(&root).expect("racine des dossiers de test");
    root
}

/// Dossier de test, supprimé à la fin (voir le module).
#[derive(Debug)]
pub struct TestDir {
    dir: Option<tempfile::TempDir>,
}

/// Même signature que `tempfile::tempdir()` : les appels existants gardent leur `unwrap`/`expect`.
pub fn tempdir() -> std::io::Result<TestDir> {
    let dir = tempfile::Builder::new().tempdir_in(root())?;
    Ok(TestDir { dir: Some(dir) })
}

/// Pour les tests de mort simulée, qui font beaucoup de `fsync` : la mémoire (`/dev/shm`) quand elle
/// existe (Linux), sinon comme `tempdir`. Même discipline de suppression ; `xtask test-tmp-check`
/// surveille aussi cette racine.
pub fn tempdir_fast() -> std::io::Result<TestDir> {
    let shm = Path::new("/dev/shm");
    if !shm.is_dir() {
        return tempdir();
    }
    let root = shm.join(ROOT_NAME);
    std::fs::create_dir_all(&root)?;
    let dir = tempfile::Builder::new().tempdir_in(root)?;
    Ok(TestDir { dir: Some(dir) })
}

impl TestDir {
    pub fn path(&self) -> &Path {
        self.dir.as_ref().expect("dossier présent").path()
    }
}

impl AsRef<Path> for TestDir {
    fn as_ref(&self) -> &Path {
        self.path()
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let Some(dir) = self.dir.take() else { return };
        // `keep` : sinon `TempDir::drop` réessaierait en silence, sans que personne le sache.
        let path = dir.keep();
        let _ = remove(&path);
        DEFERRED.with(|deferred| deferred.borrow_mut().paths.push(path));
    }
}

/// Supprime `path` ; `Ok` aussi s'il n'existe plus.
fn remove(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(path) {
        Err(_) if !path.exists() => Ok(()),
        other => other,
    }
}

thread_local! {
    /// Dossiers dont le `TestDir` a été détruit sur ce fil : supprimés (encore, s'il en reste) à la
    /// sortie du fil du test.
    static DEFERRED: RefCell<Deferred> = const { RefCell::new(Deferred { paths: Vec::new() }) };
}

struct Deferred {
    paths: Vec<PathBuf>,
}

impl Drop for Deferred {
    fn drop(&mut self) {
        // Le fil de test sort : son exécuteur est arrêté, les connexions fermées ou en train de
        // l'être (le fil de travail de SQLite referme le fichier un instant après).
        let deadline = Instant::now() + Duration::from_secs(10);
        for path in self.paths.drain(..) {
            loop {
                match remove(&path) {
                    Ok(()) => break,
                    Err(error) if Instant::now() >= deadline => {
                        eprintln!(
                            "dossier de test non supprimé : {} ({error}) ; un fichier reste ouvert \
                             (base SQLite, journal) après la fin du test",
                            path.display()
                        );
                        break;
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(25)),
                }
            }
        }
    }
}
