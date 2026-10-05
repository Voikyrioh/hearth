//! Journal du client : fichier tournant dans le dossier de données de
//! l'application (`logs/`, un fichier par jour, 7 gardés, 16 Mio au plus au
//! total). Initialisé avant tout le reste ; les paniques y sont écrites. Les
//! binaires livrés n'ont pas de console : sans ce fichier, aucune erreur ne
//! serait visible. Niveau fixé par le code (`info` livré, `debug` en
//! développement, où `RUST_LOG` est honoré) : un utilisateur ne peut pas
//! l'augmenter par l'environnement.

use std::backtrace::Backtrace;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use tracing_appender::rolling::{InitError, RollingFileAppender, Rotation};

use crate::domain::IDENTIFIER;
use crate::error::AppError;

/// Nombre de fichiers journaux gardés (rotation quotidienne).
pub const KEPT_FILES: usize = 7;
/// Taille totale maximale du dossier des journaux : au-delà, les plus anciens
/// fichiers sont supprimés, puis les écritures sont abandonnées.
pub const MAX_TOTAL_BYTES: u64 = 16 * 1024 * 1024;
const FILE_PREFIX: &str = "hearth";
const FILE_SUFFIX: &str = "log";
/// Octets écrits entre deux vérifications de la taille du dossier.
const CHECK_EVERY: usize = 64 * 1024;

/// Raison pour laquelle le journal n'a pas pu s'ouvrir, gardée pour la boîte de
/// message d'un éventuel échec de démarrage.
static INIT_FAILURE: OnceLock<String> = OnceLock::new();

#[derive(Debug, thiserror::Error)]
pub enum LogError {
    #[error("fichier journal impossible à ouvrir : {0}")]
    Appender(#[from] InitError),
    #[error("journal déjà initialisé ou invalide : {0}")]
    Subscriber(String),
}

/// Dossier de données de l'application : `%APPDATA%\fr.voikyrioh.hearth`,
/// le même que celui du greffon des réglages.
pub fn data_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join(IDENTIFIER)
}

/// Dossier des journaux : `<données>\logs`.
pub fn log_dir() -> PathBuf {
    data_dir().join("logs")
}

/// Pourquoi le journal n'a pas pu s'ouvrir, si c'est le cas.
pub fn init_failure() -> Option<&'static str> {
    INIT_FAILURE.get().map(String::as_str)
}

/// Initialise le journal dans le dossier standard. Le crochet de panique est
/// posé dans tous les cas ; si le journal ne s'ouvre pas, l'erreur est gardée
/// (`init_failure`) et rendue, l'application démarre quand même.
pub fn init() -> Result<(), LogError> {
    let result = init_in(&log_dir());
    if let Err(error) = &result {
        let _ = INIT_FAILURE.set(error.to_string());
        install_panic_hook();
    }
    result
}

/// Initialise le journal dans `dir` et installe le crochet de panique qui y écrit.
pub fn init_in(dir: &Path) -> Result<(), LogError> {
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(FILE_PREFIX)
        .filename_suffix(FILE_SUFFIX)
        .max_log_files(KEPT_FILES)
        .build(dir)?;
    let writer = BoundedWriter::new(appender, dir.to_path_buf(), MAX_TOTAL_BYTES);
    // Écriture synchrone : une panique n'attend pas qu'un tampon soit vidé.
    let builder = tracing_subscriber::fmt()
        .with_writer(Mutex::new(writer))
        .with_ansi(false);
    #[cfg(debug_assertions)]
    let builder = builder.with_env_filter(
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug")),
    );
    #[cfg(not(debug_assertions))]
    let builder = builder.with_max_level(tracing::Level::INFO);
    builder
        .try_init()
        .map_err(|error| LogError::Subscriber(error.to_string()))?;
    install_panic_hook();
    Ok(())
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(
            panic = %info,
            backtrace = %Backtrace::force_capture(),
            "panique"
        );
        previous(info);
    }));
}

/// Écrivain qui borne la taille totale du dossier des journaux.
struct BoundedWriter<W: Write> {
    inner: W,
    dir: PathBuf,
    cap: u64,
    since_check: usize,
    full: bool,
}

impl<W: Write> BoundedWriter<W> {
    fn new(inner: W, dir: PathBuf, cap: u64) -> Self {
        let full = enforce_cap(&dir, cap);
        Self {
            inner,
            dir,
            cap,
            since_check: 0,
            full,
        }
    }
}

impl<W: Write> Write for BoundedWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.since_check += buf.len();
        if self.since_check >= CHECK_EVERY {
            self.since_check = 0;
            self.full = enforce_cap(&self.dir, self.cap);
        }
        if self.full {
            return Ok(buf.len());
        }
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Supprime les plus anciens journaux de `dir` tant que le total dépasse `cap`
/// (le plus récent n'est jamais supprimé : il est ouvert). Renvoie vrai si le
/// total dépasse encore `cap`, c'est-à-dire s'il faut abandonner les écritures.
pub fn enforce_cap(dir: &Path, cap: u64) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    let mut files: Vec<(String, u64)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_log = name.starts_with(FILE_PREFIX) && name.ends_with(FILE_SUFFIX);
            let len = entry.metadata().ok()?.len();
            is_log.then_some((name, len))
        })
        .collect();
    // Le nom porte la date (`hearth.2026-10-04.log`) : l'ordre alphabétique est chronologique.
    files.sort();
    let mut total: u64 = files.iter().map(|(_, len)| len).sum();
    while total > cap && files.len() > 1 {
        let (name, len) = files.remove(0);
        if std::fs::remove_file(dir.join(&name)).is_ok() {
            total -= len;
        }
    }
    total > cap
}

/// Ouvre le dossier des journaux dans l'Explorateur (le crée au besoin).
pub fn open_log_dir() -> Result<(), AppError> {
    let dir = log_dir();
    std::fs::create_dir_all(&dir).map_err(|error| AppError::Logs(error.to_string()))?;
    Command::new("explorer")
        .arg(&dir)
        .spawn()
        .map(drop)
        .map_err(|error| AppError::Logs(error.to_string()))
}
