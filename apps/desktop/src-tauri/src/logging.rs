//! Journal du client : fichier tournant dans le dossier de données de
//! l'application (`logs/`, un fichier par jour, 7 gardés). Initialisé avant
//! tout le reste ; les paniques y sont écrites. Les binaires livrés n'ont pas
//! de console : sans ce fichier, aucune erreur ne serait visible.

use std::backtrace::Backtrace;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use tracing_appender::rolling::{InitError, RollingFileAppender, Rotation};
use tracing_subscriber::EnvFilter;

use crate::domain::IDENTIFIER;
use crate::error::AppError;

/// Nombre de fichiers journaux gardés (rotation quotidienne).
pub const KEPT_FILES: usize = 7;
const FILE_PREFIX: &str = "hearth";
const FILE_SUFFIX: &str = "log";

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

/// Initialise le journal dans le dossier standard.
pub fn init() -> Result<(), LogError> {
    init_in(&log_dir())
}

/// Initialise le journal dans `dir` (niveau `info`, réglable par `RUST_LOG`)
/// et installe le crochet de panique qui y écrit.
pub fn init_in(dir: &Path) -> Result<(), LogError> {
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(FILE_PREFIX)
        .filename_suffix(FILE_SUFFIX)
        .max_log_files(KEPT_FILES)
        .build(dir)?;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    // Écriture synchrone : une panique n'attend pas qu'un tampon soit vidé.
    tracing_subscriber::fmt()
        .with_writer(Mutex::new(appender))
        .with_ansi(false)
        .with_env_filter(filter)
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
