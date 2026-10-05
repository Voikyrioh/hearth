//! Journal du client : fichier tournant dans le dossier de données de
//! l'application (`logs/`, un fichier par jour, 7 gardés, 16 Mio au plus au
//! total). Initialisé avant tout le reste ; les paniques y sont écrites
//! directement (boîte de message si le journal est indisponible). Les
//! binaires livrés n'ont pas de console : sans ce fichier, aucune erreur ne
//! serait visible. Niveau fixé par le code (`info` livré, `debug` en
//! développement, où `RUST_LOG` est honoré) : un utilisateur ne peut pas
//! l'augmenter par l'environnement.

use std::backtrace::Backtrace;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use tracing_appender::rolling::{InitError, RollingFileAppender, Rotation};

use crate::domain::IDENTIFIER;
use crate::error::AppError;
use crate::texts;

/// Nombre de fichiers journaux gardés (rotation quotidienne).
pub const KEPT_FILES: usize = 7;
/// Taille totale maximale du dossier des journaux : au-delà, les plus anciens
/// fichiers sont supprimés, puis les écritures sont abandonnées.
pub const MAX_TOTAL_BYTES: u64 = 16 * 1024 * 1024;
const FILE_PREFIX: &str = "hearth";
const FILE_SUFFIX: &str = "log";
/// Octets écrits entre deux vérifications de la taille du dossier.
const CHECK_EVERY: usize = 64 * 1024;
/// Une fois le plafond atteint : délai minimal entre deux vérifications (place
/// libérée, changement de jour).
const RECHECK_WHEN_FULL: Duration = Duration::from_secs(1);
/// Dernière ligne écrite quand le plafond de taille est atteint.
pub const FULL_LINE: &str = "journal plein : les messages suivants sont abandonnés jusqu'au changement de jour ou jusqu'à ce que de la place soit libérée\n";
/// Fichier créé par le crochet de panique s'il n'y a encore aucun journal.
const PANIC_FILE: &str = "hearth.panic.log";

/// Raison pour laquelle le journal n'a pas pu s'ouvrir, gardée pour la boîte de
/// message d'un éventuel échec de démarrage.
static INIT_FAILURE: OnceLock<String> = OnceLock::new();

/// Dossier où le crochet de panique écrit directement (sans passer par `tracing`).
static PANIC_DIR: OnceLock<PathBuf> = OnceLock::new();

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
    let dir = log_dir();
    let result = init_in(&dir);
    if let Err(error) = &result {
        let _ = INIT_FAILURE.set(error.to_string());
        // Le crochet tente quand même d'écrire dans le dossier ; sinon, boîte de message.
        let _ = PANIC_DIR.set(dir);
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
    let _ = PANIC_DIR.set(dir.to_path_buf());
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
        let report = format!(
            "panique : {info}\nbacktrace :\n{}\n",
            Backtrace::force_capture()
        );
        // Direct dans le fichier : aucun abonné `tracing` n'est nécessaire. Si le
        // journal est indisponible, la panique est montrée avant la sortie.
        if !write_panic_report(PANIC_DIR.get().map(PathBuf::as_path), &report) {
            rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Error)
                .set_title(texts::PANIC_TITLE)
                .set_description(texts::panic_body(&info.to_string()))
                .show();
        }
        previous(info);
    }));
}

/// Ajoute `report` au fichier journal le plus récent de `dir` (ou crée
/// `hearth.panic.log`). Vrai si l'écriture a réussi.
pub fn write_panic_report(dir: Option<&Path>, report: &str) -> bool {
    let Some(dir) = dir else {
        return false;
    };
    let _ = std::fs::create_dir_all(dir);
    let target = list_logs(dir)
        .into_iter()
        .max_by_key(|file| file.modified)
        .map_or_else(|| dir.join(PANIC_FILE), |file| dir.join(file.name));
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(target)
        .and_then(|mut file| {
            file.write_all(report.as_bytes())?;
            file.flush()
        })
        .is_ok()
}

/// Écrivain qui borne la taille totale du dossier des journaux. Au plafond, il
/// écrit une dernière ligne (`FULL_LINE`) puis abandonne les messages ; la
/// rotation quotidienne de `inner` et la place libérée le rouvrent.
pub struct BoundedWriter<W: Write> {
    inner: W,
    dir: PathBuf,
    cap: u64,
    since_check: usize,
    full: bool,
    recheck: Duration,
    last_check: Instant,
}

impl<W: Write> BoundedWriter<W> {
    pub fn new(inner: W, dir: PathBuf, cap: u64) -> Self {
        Self::with_recheck(inner, dir, cap, RECHECK_WHEN_FULL)
    }

    /// Comme `new`, avec le délai entre deux vérifications une fois plein.
    pub fn with_recheck(inner: W, dir: PathBuf, cap: u64, recheck: Duration) -> Self {
        let mut writer = Self {
            inner,
            dir,
            cap,
            since_check: 0,
            full: false,
            recheck,
            last_check: Instant::now(),
        };
        writer.refresh();
        writer
    }

    /// Recalcule l'état « plein » ; écrit la dernière ligne à la bascule.
    fn refresh(&mut self) {
        self.since_check = 0;
        self.last_check = Instant::now();
        let full = enforce_cap(&self.dir, self.cap);
        if full && !self.full {
            let _ = self.inner.write_all(FULL_LINE.as_bytes());
        }
        self.full = full;
    }
}

impl<W: Write> Write for BoundedWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.since_check += buf.len();
        if self.full {
            if self.last_check.elapsed() >= self.recheck {
                // Écrire zéro octet suffit à `inner` pour tourner au changement de jour.
                let _ = self.inner.write(&[]);
                self.refresh();
            }
        } else if self.since_check >= CHECK_EVERY {
            self.refresh();
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

struct LogFile {
    name: String,
    len: u64,
    modified: SystemTime,
}

fn list_logs(dir: &Path) -> Vec<LogFile> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !(name.starts_with(FILE_PREFIX) && name.ends_with(FILE_SUFFIX)) {
                return None;
            }
            // `std::fs::metadata` sur le chemin, pas `DirEntry::metadata` : sous Windows les
            // métadonnées d'une entrée de dossier ne suivent pas la taille d'un fichier encore
            // ouvert en écriture (celui du jour).
            let meta = std::fs::metadata(entry.path()).ok()?;
            Some(LogFile {
                name,
                len: meta.len(),
                modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            })
        })
        .collect()
}

/// Supprime les plus anciens journaux de `dir` tant que le total dépasse `cap`.
/// « Plus ancien » = date de modification (le nom ne suffit pas : une horloge
/// reculée donne un nom plus ancien à un fichier encore ouvert) ; le fichier
/// modifié en dernier n'est jamais supprimé. Renvoie vrai si le total dépasse
/// encore `cap`, c'est-à-dire s'il faut abandonner les écritures.
pub fn enforce_cap(dir: &Path, cap: u64) -> bool {
    let mut files = list_logs(dir);
    // Du plus ancien au plus récent ; à date égale, le nom (qui porte la date) départage.
    files.sort_by(|a, b| {
        a.modified
            .cmp(&b.modified)
            .then_with(|| a.name.cmp(&b.name))
    });
    let mut total: u64 = files.iter().map(|file| file.len).sum();
    while total > cap && files.len() > 1 {
        let oldest = files.remove(0);
        if std::fs::remove_file(dir.join(&oldest.name)).is_ok() {
            total -= oldest.len;
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
