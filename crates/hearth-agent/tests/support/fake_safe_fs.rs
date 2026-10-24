#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]
//! Faux `SafeFs` en mémoire (HRT-145) : mêmes règles visibles que le vrai (suite `safe_fs_contract.rs`),
//! sans système de fichiers. Sert aux tests des cas d'usage qui écriront sous la racine d'un service
//! (déploiement, suppression) sur le poste de développement. Il ne simule ni lien, ni montage, ni course :
//! ces pièges ne se testent que contre le vrai (`tests/safe_fs.rs`, `infrastructure/fs/tests_unix.rs`).

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

use hearth_agent::application::ports::{Erased, FsError, Measure, Perms, SafeFs, ServiceRoot};
use hearth_agent::domain::files::RelPath;
use hearth_agent::domain::services::ServiceKey;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    Dir,
    /// Le contenu, puis propriétaire (uid, gid) et droits posés à l'écriture.
    File(Vec<u8>, (u32, u32, u32)),
}

/// `racine de service -> (chemin relatif -> noeud)`.
type Tree = BTreeMap<String, Node>;

/// Les crochets de test d'un effacement (HRT-85) : un chemin qui refuse de partir, une pause, un journal.
#[derive(Debug, Default)]
struct EraseHooks {
    /// Chemins relatifs qui « refusent de partir » (droits refusés) : l'arbre dessous reste aussi.
    stuck: BTreeSet<String>,
    /// L'effacement attend ici tant que `released` est faux.
    held: bool,
    /// Chaque chemin dont l'effacement est demandé, dans l'ordre.
    calls: Vec<String>,
    /// Combien de fois la racine d'un service a été retirée vide.
    empty_root_removals: u32,
}

#[derive(Debug, Default, Clone)]
pub struct FakeSafeFs {
    roots: Arc<Mutex<BTreeMap<String, Tree>>>,
    hooks: Arc<(Mutex<EraseHooks>, Condvar)>,
}

impl FakeSafeFs {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Tree>> {
        self.roots.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Le contenu d'un fichier, pour les assertions d'un test.
    pub fn content(&self, key: &ServiceKey, rel: &str) -> Option<Vec<u8>> {
        match self.lock().get(key.as_str())?.get(rel)? {
            Node::File(bytes, _) => Some(bytes.clone()),
            Node::Dir => None,
        }
    }

    /// Propriétaire (uid, gid) et droits d'un fichier écrit, pour les assertions d'un test.
    pub fn perms_of(&self, key: &ServiceKey, rel: &str) -> Option<(u32, u32, u32)> {
        match self.lock().get(key.as_str())?.get(rel)? {
            Node::File(_, perms) => Some(*perms),
            Node::Dir => None,
        }
    }

    /// Les chemins de la racine d'un service, pour les assertions d'un test.
    pub fn paths(&self, key: &ServiceKey) -> Vec<String> {
        self.lock()
            .get(key.as_str())
            .map(|tree| tree.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Le service a-t-il une racine ?
    pub fn has_root(&self, key: &ServiceKey) -> bool {
        self.lock().contains_key(key.as_str())
    }

    fn hooks(&self) -> std::sync::MutexGuard<'_, EraseHooks> {
        self.hooks.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Ce chemin refuse de partir à l'effacement (droits refusés), lui et tout ce qu'il contient.
    pub fn refuse_to_erase(&self, rel: &str) {
        self.hooks().stuck.insert(rel.to_owned());
    }

    /// Suspend les effacements jusqu'à `release_erasure` ou jusqu'à la chute du jeton rendu : de quoi regarder
    /// l'état « en cours de suppression ». Le jeton relâche tout seul, même si le test échoue avant de le faire
    /// (sinon le fil bloqué du faux ferait attendre la fin du test pour toujours).
    pub fn hold_erasure(&self) -> ErasureHold {
        self.hooks().held = true;
        ErasureHold(self.clone())
    }

    pub fn release_erasure(&self) {
        self.hooks().held = false;
        self.hooks.1.notify_all();
    }

    /// Les chemins dont l'effacement a été demandé, dans l'ordre.
    pub fn erase_calls(&self) -> Vec<String> {
        self.hooks().calls.clone()
    }

    /// Combien de fois la racine d'un service a été retirée parce qu'elle était vide.
    pub fn empty_root_removals(&self) -> u32 {
        self.hooks().empty_root_removals
    }

    /// Crée un dossier (et ses parents) avec un fichier dedans : de quoi semer des données de service.
    pub fn seed_file(&self, key: &ServiceKey, rel: &str, bytes: &[u8]) {
        let mut roots = self.lock();
        let tree = roots.entry(key.as_str().to_owned()).or_default();
        let mut path = String::new();
        let parts: Vec<&str> = rel.split('/').collect();
        for (index, part) in parts.iter().enumerate() {
            if !path.is_empty() {
                path.push('/');
            }
            path.push_str(part);
            if index + 1 == parts.len() {
                tree.insert(path.clone(), Node::File(bytes.to_vec(), (0, 0, 0o644)));
            } else {
                tree.entry(path.clone()).or_insert(Node::Dir);
            }
        }
    }
}

/// L'effacement est suspendu tant que ce jeton existe.
pub struct ErasureHold(FakeSafeFs);

impl Drop for ErasureHold {
    fn drop(&mut self) {
        self.0.release_erasure();
    }
}

struct FakeRoot {
    fs: FakeSafeFs,
    key: String,
}

impl FakeRoot {
    fn with<T>(&self, f: impl FnOnce(&mut Tree) -> Result<T, FsError>) -> Result<T, FsError> {
        let mut roots = self.fs.lock();
        f(roots.get_mut(&self.key).ok_or(FsError::NotFound)?)
    }
}

fn parent_must_be_dir(tree: &Tree, rel: &RelPath) -> Result<(), FsError> {
    let (_, parents) = rel.split_last().ok_or(FsError::Refused)?;
    let mut path = String::new();
    for part in parents {
        if !path.is_empty() {
            path.push('/');
        }
        path.push_str(part);
        match tree.get(&path) {
            Some(Node::Dir) => {}
            Some(Node::File(..)) => return Err(FsError::Blocked),
            None => return Err(FsError::NotFound),
        }
    }
    Ok(())
}

impl SafeFs for FakeSafeFs {
    fn open_service_root(&self, key: &ServiceKey) -> Result<Box<dyn ServiceRoot>, FsError> {
        if !self.lock().contains_key(key.as_str()) {
            return Err(FsError::NotFound);
        }
        Ok(Box::new(FakeRoot {
            fs: self.clone(),
            key: key.as_str().to_owned(),
        }))
    }

    fn remove_empty_service_root(&self, key: &ServiceKey) -> Result<bool, FsError> {
        let mut roots = self.lock();
        match roots.get(key.as_str()) {
            None => Ok(false),
            Some(tree) if !tree.is_empty() => Ok(false),
            Some(_) => {
                roots.remove(key.as_str());
                drop(roots);
                self.hooks().empty_root_removals += 1;
                Ok(true)
            }
        }
    }

    fn ensure_service_root(&self, key: &ServiceKey) -> Result<Box<dyn ServiceRoot>, FsError> {
        self.lock().entry(key.as_str().to_owned()).or_default();
        self.open_service_root(key)
    }

    fn remove_service_root(&self, key: &ServiceKey) -> Result<(), FsError> {
        self.lock()
            .remove(key.as_str())
            .map(|_| ())
            .ok_or(FsError::NotFound)
    }
}

impl ServiceRoot for FakeRoot {
    fn mkdir(&self, rel: &RelPath, _perms: Perms) -> Result<bool, FsError> {
        if rel.is_root() {
            return Err(FsError::Refused);
        }
        self.with(|tree| {
            let mut path = String::new();
            let mut created = false;
            for part in rel.components() {
                if !path.is_empty() {
                    path.push('/');
                }
                path.push_str(part);
                match tree.get(&path) {
                    Some(Node::Dir) => created = false,
                    Some(Node::File(..)) => return Err(FsError::Blocked),
                    None => {
                        tree.insert(path.clone(), Node::Dir);
                        created = true;
                    }
                }
            }
            Ok(created)
        })
    }

    fn write_atomic(&self, rel: &RelPath, bytes: &[u8], perms: Perms) -> Result<(), FsError> {
        self.with(|tree| {
            parent_must_be_dir(tree, rel)?;
            let path = rel.to_string();
            if tree.get(&path) == Some(&Node::Dir) {
                return Err(FsError::NotRegular);
            }
            let owner = perms.owner();
            tree.insert(
                path,
                Node::File(bytes.to_vec(), (owner.uid(), owner.gid(), perms.mode())),
            );
            Ok(())
        })
    }

    fn read_bounded(&self, rel: &RelPath, max: u64) -> Result<Vec<u8>, FsError> {
        self.with(|tree| {
            if rel.is_root() {
                return Err(FsError::Refused);
            }
            parent_must_be_dir(tree, rel)?;
            match tree.get(&rel.to_string()) {
                None => Err(FsError::NotFound),
                Some(Node::Dir) => Err(FsError::NotRegular),
                Some(Node::File(bytes, _)) if bytes.len() as u64 > max => Err(FsError::TooLarge),
                Some(Node::File(bytes, _)) => Ok(bytes.clone()),
            }
        })
    }

    fn measure(&self, rel: &RelPath, _budget: Duration) -> Result<Measure, FsError> {
        self.with(|tree| {
            if !rel.is_root() && tree.get(&rel.to_string()) != Some(&Node::Dir) {
                return Err(FsError::NotFound);
            }
            let prefix = if rel.is_root() {
                String::new()
            } else {
                format!("{rel}/")
            };
            let mut measure = Measure::default();
            for node in tree
                .iter()
                .filter(|(p, _)| p.starts_with(&prefix))
                .map(|(_, node)| node)
            {
                measure.items += 1;
                if let Node::File(bytes, _) = node {
                    measure.bytes_on_disk += bytes.len() as u64;
                }
            }
            Ok(measure)
        })
    }

    fn remove_tree(&self, rel: &RelPath) -> Result<(), FsError> {
        if rel.is_root() {
            return Err(FsError::Refused);
        }
        self.with(|tree| {
            parent_must_be_dir(tree, rel)?;
            let path = rel.to_string();
            if !tree.contains_key(&path) {
                return Err(FsError::NotFound);
            }
            let below = format!("{path}/");
            tree.retain(|p, _| p != &path && !p.starts_with(&below));
            Ok(())
        })
    }

    fn erase_tree(
        &self,
        rel: &RelPath,
        progress: &mut (dyn FnMut(u64) + Send),
    ) -> Result<Erased, FsError> {
        if rel.is_root() {
            return Err(FsError::Refused);
        }
        {
            let mut hooks = self.fs.hooks();
            hooks.calls.push(rel.to_string());
            while hooks.held {
                hooks = self
                    .fs
                    .hooks
                    .1
                    .wait(hooks)
                    .unwrap_or_else(PoisonError::into_inner);
            }
        }
        let stuck: Vec<String> = self.fs.hooks().stuck.iter().cloned().collect();
        let report = self.with(|tree| {
            parent_must_be_dir(tree, rel)?;
            let path = rel.to_string();
            if !tree.contains_key(&path) {
                return Err(FsError::NotFound);
            }
            let below = format!("{path}/");
            let in_tree = |p: &String| p == &path || p.starts_with(&below);
            // Ce qui refuse de partir garde aussi ses parents (un dossier non vide ne se retire pas).
            let mut kept: BTreeSet<String> = BTreeSet::new();
            for p in tree.keys().filter(|p| in_tree(p)) {
                let blocked = stuck
                    .iter()
                    .any(|s| p == s || p.starts_with(&format!("{s}/")));
                if blocked {
                    kept.insert(p.clone());
                    let mut ancestor = p.as_str();
                    while let Some((parent, _)) = ancestor.rsplit_once('/') {
                        kept.insert(parent.to_owned());
                        ancestor = parent;
                    }
                }
            }
            let doomed: Vec<String> = tree
                .keys()
                .filter(|p| in_tree(p) && !kept.contains(*p))
                .cloned()
                .collect();
            let left = stuck
                .iter()
                .filter(|s| tree.contains_key(*s) && in_tree(s))
                .count() as u64;
            for p in &doomed {
                tree.remove(p);
            }
            Ok(Erased {
                removed: doomed.len() as u64,
                left,
                first_error: (left > 0)
                    .then_some(FsError::Io(std::io::ErrorKind::PermissionDenied)),
            })
        })?;
        progress(report.removed);
        Ok(report)
    }
}
