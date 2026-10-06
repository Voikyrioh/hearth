//! Mémoire durable de la mise à jour : un petit fichier JSON dans le dossier de données de
//! l'application (`update.json`). Ni la WebView ni l'interface n'y touchent : vider la WebView ne
//! remet donc pas à zéro la règle « une fois par jour au plus » (ADR-0017).

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::domain::UpdateRecord;
use super::ports::{Clock, UpdateStore};

/// Nom du fichier, dans le dossier de données de l'application.
pub const FILE_NAME: &str = "update.json";

pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0)
    }
}

pub struct FileUpdateStore {
    path: PathBuf,
}

impl FileUpdateStore {
    pub fn new(dir: &Path) -> Self {
        Self {
            path: dir.join(FILE_NAME),
        }
    }
}

impl UpdateStore for FileUpdateStore {
    fn load(&self) -> UpdateRecord {
        match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|error| {
                tracing::warn!(%error, "fichier de mise à jour illisible, repart de zéro");
                UpdateRecord::default()
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => UpdateRecord::default(),
            Err(error) => {
                tracing::warn!(%error, "fichier de mise à jour inaccessible, repart de zéro");
                UpdateRecord::default()
            }
        }
    }

    /// Écriture atomique : fichier voisin puis renommage, pour qu'un arrêt en cours d'écriture ne
    /// laisse jamais un fichier coupé.
    fn save(&self, record: &UpdateRecord) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(record).map_err(|error| error.to_string())?;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        }
        let temporary = self.path.with_extension("json.tmp");
        std::fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
        std::fs::rename(&temporary, &self.path).map_err(|error| {
            let _ = std::fs::remove_file(&temporary);
            error.to_string()
        })
    }
}
