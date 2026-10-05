//! Stockage en fichiers JSON : le carnet de serveurs et la dernière vue de chacun.
//!
//! Écriture atomique : le contenu va dans un fichier temporaire voisin, puis le renommage remplace
//! l'ancien d'un coup. Un arrêt en pleine écriture laisse l'ancien fichier intact. Un fichier
//! illisible est ignoré avec un avertissement (et mis de côté en `.corrupt` pour ne rien perdre
//! en silence) : jamais de panique, jamais de blocage au démarrage.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::AsyncWriteExt as _;
use tokio::sync::Mutex;

use crate::domain::pending_ops::PendingOp;
use crate::domain::server::{LastKnown, ServerId, ServerRecord};
use crate::ports::operation_store::{LoadedOperations, OperationStore};
use crate::ports::server_store::{ServerStore, StoreError};
use crate::ports::snapshot_store::SnapshotStore;

const FORMAT_VERSION: u32 = 1;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize, Deserialize)]
struct ServersFile {
    version: u32,
    servers: Vec<ServerRecord>,
}

fn io_error(context: &str, error: &std::io::Error) -> StoreError {
    StoreError(format!("{context} : {}", error.kind()))
}

/// Écrit `bytes` dans `path` de façon atomique (fichier temporaire, synchronisation, renommage).
pub async fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| io_error("création du dossier", &e))?;
    }
    let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut temp = path.as_os_str().to_owned();
    temp.push(format!(".tmp-{}-{sequence}", std::process::id()));
    let temp = PathBuf::from(temp);
    let written = async {
        let mut file = tokio::fs::File::create(&temp).await?;
        file.write_all(bytes).await?;
        file.sync_all().await?;
        drop(file);
        tokio::fs::rename(&temp, path).await
    }
    .await;
    if let Err(error) = written {
        let _ = tokio::fs::remove_file(&temp).await;
        return Err(io_error("écriture", &error));
    }
    Ok(())
}

/// Met un fichier illisible de côté (`.corrupt`) pour ne pas l'écraser sans trace.
async fn set_aside(path: &Path) {
    let mut aside = path.as_os_str().to_owned();
    aside.push(".corrupt");
    let _ = tokio::fs::rename(path, PathBuf::from(aside)).await;
}

pub struct FileServerStore {
    path: PathBuf,
    /// Une écriture à la fois : lecture, modification et remplacement ne s'entrelacent pas.
    lock: Mutex<()>,
}

impl FileServerStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            lock: Mutex::new(()),
        }
    }

    async fn read(&self) -> Result<Vec<ServerRecord>, StoreError> {
        let bytes = match tokio::fs::read(&self.path).await {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(io_error("lecture du carnet", &error)),
        };
        let parsed: Result<Value, _> = serde_json::from_slice(&bytes);
        let Some(entries) = parsed
            .ok()
            .and_then(|value| value.get("servers").and_then(Value::as_array).cloned())
        else {
            tracing::warn!(file = %self.path.display(), "carnet de serveurs illisible, ignoré");
            set_aside(&self.path).await;
            return Ok(Vec::new());
        };
        let mut records = Vec::with_capacity(entries.len());
        let mut skipped = 0usize;
        for entry in entries {
            match serde_json::from_value::<ServerRecord>(entry) {
                Ok(record) => records.push(record),
                Err(_) => skipped += 1,
            }
        }
        if skipped > 0 {
            tracing::warn!(file = %self.path.display(), skipped, "entrées du carnet illisibles, ignorées");
            // On garde une copie de l'original avant de le réécrire sans ces entrées.
            let mut aside = self.path.as_os_str().to_owned();
            aside.push(".corrupt");
            let _ = tokio::fs::write(PathBuf::from(aside), &bytes).await;
        }
        Ok(records)
    }

    async fn write(&self, servers: Vec<ServerRecord>) -> Result<(), StoreError> {
        let file = ServersFile {
            version: FORMAT_VERSION,
            servers,
        };
        let bytes = serde_json::to_vec_pretty(&file)
            .map_err(|e| StoreError(format!("sérialisation : {e}")))?;
        write_atomic(&self.path, &bytes).await
    }
}

#[async_trait]
impl ServerStore for FileServerStore {
    async fn list(&self) -> Result<Vec<ServerRecord>, StoreError> {
        let _guard = self.lock.lock().await;
        self.read().await
    }

    async fn save(&self, record: &ServerRecord) -> Result<(), StoreError> {
        let _guard = self.lock.lock().await;
        let mut servers = self.read().await?;
        match servers.iter_mut().find(|existing| existing.id == record.id) {
            Some(existing) => *existing = record.clone(),
            None => servers.push(record.clone()),
        }
        self.write(servers).await
    }

    async fn remove(&self, id: &ServerId) -> Result<(), StoreError> {
        let _guard = self.lock.lock().await;
        let mut servers = self.read().await?;
        let before = servers.len();
        servers.retain(|existing| &existing.id != id);
        if servers.len() == before {
            return Ok(());
        }
        self.write(servers).await
    }
}

/// Une vue par serveur : `{dossier}/{id}.json` (l'identifiant ne porte aucun caractère de chemin).
pub struct FileSnapshotStore {
    dir: PathBuf,
}

impl FileSnapshotStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn path(&self, id: &ServerId) -> PathBuf {
        self.dir.join(format!("{}.json", id.as_str()))
    }
}

#[async_trait]
impl SnapshotStore for FileSnapshotStore {
    async fn load(&self, id: &ServerId) -> Result<Option<LastKnown>, StoreError> {
        let path = self.path(id);
        let bytes = match tokio::fs::read(&path).await {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error("lecture de la dernière vue", &error)),
        };
        match serde_json::from_slice(&bytes) {
            Ok(view) => Ok(Some(view)),
            Err(_) => {
                tracing::warn!(file = %path.display(), "dernière vue illisible, ignorée");
                Ok(None)
            }
        }
    }

    async fn save(&self, id: &ServerId, view: &LastKnown) -> Result<(), StoreError> {
        let bytes =
            serde_json::to_vec(view).map_err(|e| StoreError(format!("sérialisation : {e}")))?;
        write_atomic(&self.path(id), &bytes).await
    }

    async fn remove(&self, id: &ServerId) -> Result<(), StoreError> {
        match tokio::fs::remove_file(self.path(id)).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io_error("suppression de la dernière vue", &error)),
        }
    }
}

/// Les opérations en suspens : `{dossier}/{id}.json`, une liste (au plus 256 entrées, voir
/// `domain::pending_ops::MAX_PENDING`).
pub struct FileOperationStore {
    dir: PathBuf,
}

impl FileOperationStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn path(&self, id: &ServerId) -> PathBuf {
        self.dir.join(format!("{}.json", id.as_str()))
    }
}

#[async_trait]
impl OperationStore for FileOperationStore {
    async fn load(&self, id: &ServerId) -> Result<LoadedOperations, StoreError> {
        let path = self.path(id);
        let bytes = match tokio::fs::read(&path).await {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(LoadedOperations::default());
            }
            Err(error) => {
                // Illisible (droits, partage, disque) : mis de côté comme un fichier abîmé, sans
                // quoi la prochaine écriture l'écraserait sans trace.
                tracing::warn!(file = %path.display(), error = %error.kind(), "opérations en suspens non lues, mises de côté");
                set_aside(&path).await;
                return Ok(LoadedOperations {
                    operations: Vec::new(),
                    damaged: true,
                });
            }
        };
        let parsed: Result<Vec<Value>, _> = serde_json::from_slice(&bytes);
        let Ok(entries) = parsed else {
            tracing::warn!(file = %path.display(), "opérations en suspens illisibles, mises de côté");
            set_aside(&path).await;
            return Ok(LoadedOperations {
                operations: Vec::new(),
                damaged: true,
            });
        };
        let mut operations = Vec::with_capacity(entries.len());
        let mut skipped = 0usize;
        for entry in entries {
            match serde_json::from_value::<PendingOp>(entry) {
                Ok(operation) => operations.push(operation),
                Err(_) => skipped += 1,
            }
        }
        if skipped > 0 {
            tracing::warn!(file = %path.display(), skipped, "entrées d'opérations illisibles, ignorées");
            let mut aside = path.as_os_str().to_owned();
            aside.push(".corrupt");
            let _ = tokio::fs::write(PathBuf::from(aside), &bytes).await;
        }
        Ok(LoadedOperations {
            operations,
            damaged: skipped > 0,
        })
    }

    async fn save(&self, id: &ServerId, operations: &[PendingOp]) -> Result<(), StoreError> {
        if operations.is_empty() {
            return self.remove(id).await;
        }
        let bytes = serde_json::to_vec(operations)
            .map_err(|e| StoreError(format!("sérialisation : {e}")))?;
        write_atomic(&self.path(id), &bytes).await
    }

    async fn remove(&self, id: &ServerId) -> Result<(), StoreError> {
        match tokio::fs::remove_file(self.path(id)).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io_error("suppression des opérations", &error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use hearth_proto::fingerprint::Fingerprint;

    use super::*;
    use crate::domain::time::WallTime;

    fn record(id: &str, name: &str) -> ServerRecord {
        ServerRecord {
            id: ServerId::parse(id).unwrap(),
            name: name.into(),
            color: "#7aa2f7".into(),
            host: "forge.lan".into(),
            port: 7341,
            fingerprint: Fingerprint::from_bytes([7; 32]),
            username: "marie".into(),
            remember: false,
            mac_addresses: vec![],
            last_contact_at: None,
            signed_out: false,
        }
    }

    #[tokio::test]
    async fn servers_are_saved_replaced_listed_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileServerStore::new(dir.path().join("servers.json"));
        assert!(store.list().await.unwrap().is_empty());
        store.save(&record("a", "Forge")).await.unwrap();
        store.save(&record("b", "Salon")).await.unwrap();
        store.save(&record("a", "Forge 2")).await.unwrap();
        let listed = store.list().await.unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].name, "Forge 2");
        store.remove(&ServerId::parse("a").unwrap()).await.unwrap();
        store
            .remove(&ServerId::parse("zzz").unwrap())
            .await
            .unwrap();
        assert_eq!(store.list().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_corrupt_file_is_ignored_with_a_copy_kept_and_the_store_keeps_working() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("servers.json");
        tokio::fs::write(&path, b"{ ceci n'est pas du json")
            .await
            .unwrap();
        let store = FileServerStore::new(&path);
        assert!(store.list().await.unwrap().is_empty());
        assert!(dir.path().join("servers.json.corrupt").exists());
        store.save(&record("a", "Forge")).await.unwrap();
        assert_eq!(store.list().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn an_unreadable_entry_is_skipped_but_the_others_survive() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("servers.json");
        let good = serde_json::to_value(record("a", "Forge")).unwrap();
        let file = serde_json::json!({ "version": 1, "servers": [good, { "id": "a/b" }, 42] });
        tokio::fs::write(&path, file.to_string()).await.unwrap();
        let store = FileServerStore::new(&path);
        let listed = store.list().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert!(dir.path().join("servers.json.corrupt").exists());
    }

    #[tokio::test]
    async fn an_empty_or_truncated_file_never_panics() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("servers.json");
        for content in [
            &b""[..],
            b"{",
            b"null",
            b"[]",
            b"{\"servers\":null}",
            &[0xff, 0xfe, 0x00],
        ] {
            tokio::fs::write(&path, content).await.unwrap();
            let store = FileServerStore::new(&path);
            assert!(store.list().await.unwrap().is_empty(), "{content:?}");
        }
    }

    #[tokio::test]
    async fn writes_leave_no_temporary_file_behind() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileServerStore::new(dir.path().join("servers.json"));
        for n in 0..5 {
            store.save(&record("a", &format!("v{n}"))).await.unwrap();
        }
        let names: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["servers.json"]);
    }

    #[tokio::test]
    async fn the_last_view_round_trips_and_a_corrupt_one_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSnapshotStore::new(dir.path().join("snapshots"));
        let id = ServerId::parse("a").unwrap();
        assert_eq!(store.load(&id).await.unwrap(), None);
        let view = LastKnown {
            at: WallTime::from_millis(1_790_000_000_000),
            machine: None,
            history: vec![],
        };
        store.save(&id, &view).await.unwrap();
        assert_eq!(store.load(&id).await.unwrap(), Some(view));
        tokio::fs::write(dir.path().join("snapshots").join("a.json"), b"{{{")
            .await
            .unwrap();
        assert_eq!(store.load(&id).await.unwrap(), None);
        store.remove(&id).await.unwrap();
        store.remove(&id).await.unwrap();
    }

    #[tokio::test]
    async fn an_operations_file_that_cannot_be_read_is_set_aside_and_reported() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("operations");
        let store = FileOperationStore::new(&folder);
        let id = ServerId::parse("a").unwrap();
        // Un dossier à la place du fichier : la lecture échoue autrement que par « absent ».
        tokio::fs::create_dir_all(folder.join("a.json"))
            .await
            .unwrap();
        let loaded = store.load(&id).await.unwrap();
        assert!(loaded.operations.is_empty() && loaded.damaged);
        assert!(folder.join("a.json.corrupt").exists());
        assert!(!folder.join("a.json").exists());
    }

    #[tokio::test]
    async fn pending_operations_round_trip_and_an_empty_list_erases() {
        use crate::domain::pending_ops::{OperationId, PendingOps};
        use crate::domain::time::WallTime;
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("operations");
        let store = FileOperationStore::new(&folder);
        let id = ServerId::parse("a").unwrap();
        assert_eq!(store.load(&id).await.unwrap(), LoadedOperations::default());
        let mut ops = PendingOps::new();
        ops.register(
            OperationId::parse("01J9").unwrap(),
            "PUT /me/password".into(),
            WallTime::from_millis(5),
        )
        .unwrap();
        store.save(&id, &ops.snapshot()).await.unwrap();
        let loaded = store.load(&id).await.unwrap();
        assert_eq!(loaded.operations.len(), 1);
        assert!(!loaded.damaged);

        // Illisible : mis de côté (pas supprimé), signalé.
        tokio::fs::write(folder.join("a.json"), b"[{")
            .await
            .unwrap();
        let loaded = store.load(&id).await.unwrap();
        assert!(loaded.operations.is_empty() && loaded.damaged);
        assert!(folder.join("a.json.corrupt").exists());

        // Une entrée invalide n'emporte pas les autres.
        let good = serde_json::to_value(ops.snapshot()).unwrap();
        let mixed = serde_json::json!([good[0], { "id": "a/b" }, 7]);
        tokio::fs::write(folder.join("a.json"), mixed.to_string())
            .await
            .unwrap();
        let loaded = store.load(&id).await.unwrap();
        assert_eq!(loaded.operations.len(), 1);
        assert!(loaded.damaged);

        store.save(&id, &ops.snapshot()).await.unwrap();
        store.save(&id, &[]).await.unwrap();
        assert!(!folder.join("a.json").exists());
    }
}
