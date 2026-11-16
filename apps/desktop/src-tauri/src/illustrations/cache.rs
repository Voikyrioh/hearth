//! Le cache disque des images de jeux (HRT-239, HRT-240, ADR-0061) : dossier `catalog-illustrations/` du dossier de données
//! local. Deux sortes de fichiers, et rien d'autre : `steam/{app_id}-header.jpg` (+ `.meta`) et `agent/{hex}.jpg`
//! (adressé par contenu, partagé entre serveurs). Le nom d'un fichier se construit depuis un `u32` ou une empreinte déjà
//! validés, jamais depuis un texte reçu. Écriture atomique (fichier temporaire puis renommage) ; une copie Steam est
//! revérifiée (signature, dimensions) à la lecture, une copie de l'agent est revérifiée par son empreinte SHA-256 (un
//! fichier altéré est supprimé) ; plafond de 32 Mio, les fichiers les moins récemment servis partent d'abord.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use super::sha256_hex;
use super::steam::check_image;

/// Une copie Steam de moins de 30 jours est servie telle quelle.
pub const FRESH_FOR: Duration = Duration::from_secs(30 * 24 * 3600);
/// Plafond du dossier, en octets.
pub const CAP_BYTES: u64 = 32 * 1024 * 1024;

/// Les validateurs d'une copie Steam, pour la relecture conditionnelle.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    /// Quand la copie a été lue ou confirmée, en secondes depuis l'époque Unix.
    pub fetched_s: u64,
}

pub struct Cached {
    pub bytes: Vec<u8>,
    pub meta: Meta,
}

#[derive(Debug, Clone)]
pub struct Cache {
    root: PathBuf,
}

fn now_s(now: SystemTime) -> u64 {
    now.duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Cache {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn steam_dir(&self) -> PathBuf {
        self.root.join("steam")
    }

    fn agent_dir(&self) -> PathBuf {
        self.root.join("agent")
    }

    fn steam_file(&self, app_id: u32) -> PathBuf {
        self.steam_dir().join(format!("{app_id}-header.jpg"))
    }

    fn meta_file(&self, app_id: u32) -> PathBuf {
        self.steam_dir().join(format!("{app_id}-header.meta"))
    }

    fn agent_file(&self, hex: &str) -> PathBuf {
        self.agent_dir().join(format!("{hex}.jpg"))
    }

    /// La copie Steam d'un jeu, si elle existe ET reste une image valable (une copie altérée est supprimée).
    pub fn read_steam(&self, app_id: u32) -> Option<Cached> {
        let path = self.steam_file(app_id);
        let bytes = fs::read(&path).ok()?;
        if check_image(&bytes).is_err() {
            let _ = fs::remove_file(&path);
            let _ = fs::remove_file(self.meta_file(app_id));
            return None;
        }
        let meta = fs::read(self.meta_file(app_id))
            .ok()
            .and_then(|raw| serde_json::from_slice::<Meta>(&raw).ok())
            .unwrap_or_default();
        touch(&path);
        Some(Cached { bytes, meta })
    }

    /// Une copie de moins de 30 jours : servie sans relire Steam.
    pub fn is_fresh(meta: &Meta, now: SystemTime) -> bool {
        meta.fetched_s != 0 && now_s(now).saturating_sub(meta.fetched_s) < FRESH_FOR.as_secs()
    }

    pub fn write_steam(&self, app_id: u32, bytes: &[u8], meta: &Meta) -> std::io::Result<()> {
        atomic_write(&self.steam_file(app_id), bytes)?;
        let raw = serde_json::to_vec(meta).unwrap_or_default();
        atomic_write(&self.meta_file(app_id), &raw)?;
        self.enforce_cap();
        Ok(())
    }

    /// Une relecture conditionnelle confirmée (`304`) : la copie est de nouveau fraîche.
    pub fn refresh_steam(&self, app_id: u32, meta: &Meta, now: SystemTime) {
        let renewed = Meta {
            fetched_s: now_s(now),
            ..meta.clone()
        };
        let raw = serde_json::to_vec(&renewed).unwrap_or_default();
        let _ = atomic_write(&self.meta_file(app_id), &raw);
    }

    /// Une illustration de l'agent, relue par son empreinte : un fichier dont le SHA-256 n'est pas celui du nom est supprimé.
    pub fn read_agent(&self, hex: &str) -> Option<Vec<u8>> {
        let path = self.agent_file(hex);
        let bytes = fs::read(&path).ok()?;
        if sha256_hex(&bytes) != hex {
            let _ = fs::remove_file(&path);
            return None;
        }
        touch(&path);
        Some(bytes)
    }

    pub fn write_agent(&self, hex: &str, bytes: &[u8]) -> std::io::Result<()> {
        atomic_write(&self.agent_file(hex), bytes)?;
        self.enforce_cap();
        Ok(())
    }

    /// Oublie toutes les images Steam gardées (le réglage vient d'être coupé, BR-CLIENT-015).
    pub fn purge_steam(&self) {
        let _ = fs::remove_dir_all(self.steam_dir());
    }

    /// Ramène le dossier sous le plafond en retirant d'abord les fichiers les moins récemment servis.
    pub fn enforce_cap(&self) {
        let mut files: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
        for dir in [self.steam_dir(), self.agent_dir()] {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let Ok(meta) = entry.metadata() else { continue };
                if meta.is_file() {
                    files.push((
                        meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                        meta.len(),
                        entry.path(),
                    ));
                }
            }
        }
        let mut total: u64 = files.iter().map(|(_, size, _)| size).sum();
        if total <= CAP_BYTES {
            return;
        }
        files.sort_by_key(|(served, _, _)| *served);
        for (_, size, path) in files {
            if total <= CAP_BYTES {
                break;
            }
            if fs::remove_file(&path).is_ok() {
                total = total.saturating_sub(size);
            }
        }
    }
}

/// Écrit dans un fichier temporaire du même dossier puis renomme : jamais un fichier à moitié écrit sous son nom.
fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::other("dossier"))?;
    fs::create_dir_all(dir)?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("x");
    let temp = dir.join(format!(".{name}.tmp"));
    {
        let mut file = fs::File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    if let Err(error) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    Ok(())
}

/// Marque un fichier comme servi à l'instant (pour la purge des moins récemment servis).
fn touch(path: &Path) {
    if let Ok(file) = fs::OpenOptions::new().write(true).open(path) {
        let _ = file.set_modified(SystemTime::now());
    }
}

#[cfg(test)]
mod tests {

    /// Un numéro unique par test : deux tests ne partagent jamais un dossier (l'horloge de Windows tourne par 100 ns).
    fn next_id() -> u64 {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }
    use super::super::steam::test_server::jpeg;
    use super::*;

    fn cache() -> (Cache, PathBuf) {
        // Sous `<target>/hearth-test-tmp/` (jamais en vrac dans %TEMP%) : `<target>/debug/deps/<exécutable de test>`.
        let target = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.ancestors().nth(3).map(Path::to_path_buf))
            .unwrap_or_else(std::env::temp_dir);
        let root = target.join("hearth-test-tmp").join(format!(
            "illustrations-{}-{}-{}",
            std::process::id(),
            rand_suffix(),
            next_id()
        ));
        (Cache::new(root.clone()), root)
    }

    fn rand_suffix() -> u128 {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    }

    #[test]
    fn a_steam_copy_round_trips_atomically_with_its_validators() {
        let (cache, root) = cache();
        let bytes = jpeg(460, 215, 3000);
        let meta = Meta {
            etag: Some("\"e\"".into()),
            last_modified: None,
            fetched_s: now_s(SystemTime::now()),
        };
        cache.write_steam(105_600, &bytes, &meta).unwrap();
        let read = cache.read_steam(105_600).unwrap();
        assert_eq!(read.bytes, bytes);
        assert_eq!(read.meta, meta);
        assert!(Cache::is_fresh(&read.meta, SystemTime::now()));
        assert!(!Cache::is_fresh(
            &read.meta,
            SystemTime::now() + FRESH_FOR + Duration::from_secs(5)
        ));
        // Aucun fichier temporaire ne reste.
        let leftovers: Vec<_> = fs::read_dir(root.join("steam"))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_tampered_steam_copy_is_deleted_and_never_served() {
        let (cache, root) = cache();
        cache
            .write_steam(1, &jpeg(460, 215, 500), &Meta::default())
            .unwrap();
        fs::write(root.join("steam").join("1-header.jpg"), b"pas une image").unwrap();
        assert!(cache.read_steam(1).is_none());
        assert!(!root.join("steam").join("1-header.jpg").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn an_agent_copy_is_addressed_by_content_and_a_tampered_one_is_deleted() {
        let (cache, root) = cache();
        let bytes = jpeg(920, 430, 4000);
        let hex = sha256_hex(&bytes);
        cache.write_agent(&hex, &bytes).unwrap();
        assert_eq!(cache.read_agent(&hex).unwrap(), bytes);
        fs::write(
            root.join("agent").join(format!("{hex}.jpg")),
            b"autre chose",
        )
        .unwrap();
        assert!(cache.read_agent(&hex).is_none());
        assert!(!root.join("agent").join(format!("{hex}.jpg")).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn purge_steam_forgets_only_steam() {
        let (cache, root) = cache();
        cache
            .write_steam(1, &jpeg(460, 215, 500), &Meta::default())
            .unwrap();
        let bytes = jpeg(920, 430, 500);
        cache.write_agent(&sha256_hex(&bytes), &bytes).unwrap();
        cache.purge_steam();
        assert!(cache.read_steam(1).is_none());
        assert!(cache.read_agent(&sha256_hex(&bytes)).is_some());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn the_cap_removes_the_least_recently_served_first() {
        let (cache, root) = cache();
        let big = jpeg(460, 215, 12 * 1024 * 1024);
        for id in 1..=3u32 {
            cache.write_steam(id, &big, &Meta::default()).unwrap();
            std::thread::sleep(Duration::from_millis(30));
        }
        // 3 x 12 Mio dépassent 32 Mio : le plus ancien est parti, les plus récents restent.
        assert!(!root.join("steam").join("1-header.jpg").exists());
        assert!(root.join("steam").join("3-header.jpg").exists());
        let _ = fs::remove_dir_all(root);
    }
}
