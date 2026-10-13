//! Retirer ce qu'une opération interrompue avait créé pour elle-même, après avoir revérifié que c'est
//! toujours LUI (HRT-83, ADR-0045, BR-EDIT-031).
//!
//! C'est le seul nettoyage automatique de l'agent au démarrage. Il est borné :
//!
//! - **seulement un objet nommé au journal** (racine connue + chemin relatif sans `..`), jamais une
//!   recherche ;
//! - **seulement si le numéro d'inode et le périphérique sont ceux notés** au journal, et que ce n'est ni
//!   un lien symbolique, ni un autre genre de fichier : un dossier remplacé entre-temps n'est pas touché
//!   (`NotOurs`) ;
//! - chaque dossier parent est vérifié (jamais un lien symbolique) ;
//! - **sans plate-forme Unix, rien n'est retiré** (`Unverifiable`) : pas d'inode, pas de retrait.
//!
//! Limite assumée de cette tranche : pas de résolution relative à un descripteur (`openat2`,
//! `O_NOFOLLOW`) : la couche `SafeFs` arrive avec HRT-85 et reprendra ce retrait. Entre la vérification
//! de l'inode et le retrait, un processus qui a déjà les droits root sur le serveur pourrait remplacer
//! l'objet ; ce n'est pas la menace de cette garde (elle protège d'une erreur, pas de root).
//!
//! Code d'inode et de droits : Linux. Il compile partout, il ne s'exécute pas sous Windows (tests
//! `cfg(unix)`).

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::application::ports::{CreatedObjects, RemoveOutcome};
use crate::domain::ops::{CreatedObject, ObjectKind, ObjectRoot};

pub struct FsCreatedObjects {
    roots: HashMap<ObjectRoot, PathBuf>,
}

impl FsCreatedObjects {
    /// Les racines que l'agent connaît. Une racine absente rend `Unverifiable` : rien n'est retiré.
    pub fn new(roots: impl IntoIterator<Item = (ObjectRoot, PathBuf)>) -> Self {
        Self {
            roots: roots.into_iter().collect(),
        }
    }
}

#[async_trait]
impl CreatedObjects for FsCreatedObjects {
    async fn remove(&self, object: &CreatedObject) -> RemoveOutcome {
        let Some(root) = self.roots.get(&object.root).cloned() else {
            return RemoveOutcome::Unverifiable;
        };
        let object = object.clone();
        tokio::task::spawn_blocking(move || remove_blocking(&root, &object))
            .await
            .unwrap_or(RemoveOutcome::Unverifiable)
    }
}

#[cfg(unix)]
fn identity_of(meta: &fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    Some((meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
fn identity_of(_meta: &fs::Metadata) -> Option<(u64, u64)> {
    None
}

fn remove_blocking(root: &Path, object: &CreatedObject) -> RemoveOutcome {
    if !object.is_well_formed() {
        return RemoveOutcome::Unverifiable;
    }
    // Chaque dossier entre la racine et l'objet doit être un vrai dossier, jamais un lien.
    let mut current = root.to_path_buf();
    let parts: Vec<&str> = object.rel.split('/').collect();
    for part in &parts[..parts.len() - 1] {
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {}
            Ok(_) => return RemoveOutcome::NotOurs,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return RemoveOutcome::AlreadyGone;
            }
            Err(_) => return RemoveOutcome::Unverifiable,
        }
    }
    current.push(parts[parts.len() - 1]);
    let meta = match fs::symlink_metadata(&current) {
        Ok(meta) => meta,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return RemoveOutcome::AlreadyGone,
        Err(_) => return RemoveOutcome::Unverifiable,
    };
    let Some((dev, ino)) = identity_of(&meta) else {
        return RemoveOutcome::Unverifiable;
    };
    if (dev, ino) != (object.dev, object.ino) || meta.file_type().is_symlink() {
        return RemoveOutcome::NotOurs;
    }
    let expected_dir = match object.kind {
        ObjectKind::PartialArchive | ObjectKind::ReceivedPart => false,
        ObjectKind::ImportDir | ObjectKind::StagingDir => true,
    };
    if meta.is_dir() != expected_dir || (!expected_dir && !meta.is_file()) {
        return RemoveOutcome::NotOurs;
    }
    let removed = if expected_dir {
        // `remove_dir_all` ne suit pas les liens symboliques : un lien dans le dossier est retiré comme lien.
        fs::remove_dir_all(&current)
    } else {
        fs::remove_file(&current)
    };
    match removed {
        Ok(()) => RemoveOutcome::Removed,
        Err(error) if error.kind() == io::ErrorKind::NotFound => RemoveOutcome::AlreadyGone,
        Err(_) => RemoveOutcome::Unverifiable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::data_dir::private_tempdir;

    fn object(rel: &str, kind: ObjectKind, meta: &fs::Metadata) -> CreatedObject {
        let (dev, ino) = identity_of(meta).unwrap_or((0, 0));
        CreatedObject {
            root: ObjectRoot::Staging,
            rel: rel.to_owned(),
            dev,
            ino,
            kind,
        }
    }

    #[test]
    fn an_object_that_is_already_gone_is_not_an_error() {
        let dir = private_tempdir();
        let ghost = CreatedObject {
            root: ObjectRoot::Staging,
            rel: "never/existed.part".into(),
            dev: 1,
            ino: 2,
            kind: ObjectKind::PartialArchive,
        };
        assert_eq!(
            remove_blocking(dir.path(), &ghost),
            RemoveOutcome::AlreadyGone
        );
    }

    #[test]
    fn a_path_outside_the_rules_is_never_touched() {
        let dir = private_tempdir();
        let outside = dir.path().join("keep");
        fs::write(&outside, b"x").unwrap();
        let root = dir.path().join("root");
        fs::create_dir(&root).unwrap();
        for rel in ["../keep", "/abs", "a/../../keep", "", "a\\b"] {
            let bad = CreatedObject {
                root: ObjectRoot::Staging,
                rel: rel.to_owned(),
                dev: 0,
                ino: 0,
                kind: ObjectKind::PartialArchive,
            };
            assert_eq!(
                remove_blocking(&root, &bad),
                RemoveOutcome::Unverifiable,
                "{rel:?}"
            );
        }
        assert!(outside.exists());
    }

    #[test]
    fn a_root_the_agent_does_not_know_removes_nothing() {
        let objects = FsCreatedObjects::new([]);
        let any = CreatedObject {
            root: ObjectRoot::Backups,
            rel: "x".into(),
            dev: 1,
            ino: 1,
            kind: ObjectKind::PartialArchive,
        };
        let outcome = tokio_test_block(objects.remove(&any));
        assert_eq!(outcome, RemoveOutcome::Unverifiable);
    }

    fn tokio_test_block<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(future)
    }

    #[cfg(unix)]
    #[test]
    fn the_object_of_the_journal_is_removed_file_or_directory() {
        let dir = private_tempdir();
        let file = dir.path().join("a.part");
        fs::write(&file, b"partial").unwrap();
        let record = object(
            "a.part",
            ObjectKind::PartialArchive,
            &fs::metadata(&file).unwrap(),
        );
        assert_eq!(remove_blocking(dir.path(), &record), RemoveOutcome::Removed);
        assert!(!file.exists());
        assert_eq!(
            remove_blocking(dir.path(), &record),
            RemoveOutcome::AlreadyGone
        );

        let import = dir.path().join("import-01J");
        fs::create_dir_all(import.join("sub")).unwrap();
        fs::write(import.join("sub/world.dat"), b"x").unwrap();
        let record = object(
            "import-01J",
            ObjectKind::ImportDir,
            &fs::metadata(&import).unwrap(),
        );
        assert_eq!(remove_blocking(dir.path(), &record), RemoveOutcome::Removed);
        assert!(!import.exists());
    }

    /// Le cœur de la garde : un objet de même nom mais d'un autre inode n'est pas celui de l'opération.
    #[cfg(unix)]
    #[test]
    fn a_replacement_with_another_inode_is_left_alone() {
        let dir = private_tempdir();
        let file = dir.path().join("a.part");
        fs::write(&file, b"partial").unwrap();
        let record = object(
            "a.part",
            ObjectKind::PartialArchive,
            &fs::metadata(&file).unwrap(),
        );
        // L'objet est remplacé (retiré puis recréé : un autre inode, ou le même réutilisé par
        // le système de fichiers — on force la différence en changeant le numéro noté).
        let mut stale = record.clone();
        stale.ino = stale.ino.wrapping_add(1);
        assert_eq!(remove_blocking(dir.path(), &stale), RemoveOutcome::NotOurs);
        let mut other_device = record;
        other_device.dev = other_device.dev.wrapping_add(1);
        assert_eq!(
            remove_blocking(dir.path(), &other_device),
            RemoveOutcome::NotOurs
        );
        assert!(file.exists(), "rien n'est retiré");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_a_wrong_kind_or_a_linked_parent_is_never_followed_or_removed() {
        let dir = private_tempdir();
        let precious = dir.path().join("precious");
        fs::create_dir(&precious).unwrap();
        fs::write(precious.join("data"), b"keep").unwrap();
        // Un lien à la place de l'objet, même avec l'inode de sa cible.
        let link = dir.path().join("import-01J");
        std::os::unix::fs::symlink(&precious, link).unwrap();
        let target = object(
            "import-01J",
            ObjectKind::ImportDir,
            &fs::metadata(&precious).unwrap(),
        );
        assert_eq!(remove_blocking(dir.path(), &target), RemoveOutcome::NotOurs);
        assert!(precious.join("data").exists());
        // Un dossier là où le journal dit « archive » (autre genre).
        let as_file = object(
            "precious",
            ObjectKind::PartialArchive,
            &fs::metadata(&precious).unwrap(),
        );
        assert_eq!(
            remove_blocking(dir.path(), &as_file),
            RemoveOutcome::NotOurs
        );
        // Un parent qui est un lien : on ne descend pas par lui.
        let inside = object(
            "import-01J/data",
            ObjectKind::PartialArchive,
            &fs::metadata(precious.join("data")).unwrap(),
        );
        assert_eq!(remove_blocking(dir.path(), &inside), RemoveOutcome::NotOurs);
        assert!(precious.join("data").exists());
        // Un lien DANS un dossier d'import part comme lien, sans toucher sa cible.
        let import = dir.path().join("import-02K");
        fs::create_dir(&import).unwrap();
        std::os::unix::fs::symlink(&precious, import.join("out")).unwrap();
        let record = object(
            "import-02K",
            ObjectKind::ImportDir,
            &fs::metadata(&import).unwrap(),
        );
        assert_eq!(remove_blocking(dir.path(), &record), RemoveOutcome::Removed);
        assert!(
            precious.join("data").exists(),
            "la cible du lien est intacte"
        );
    }

    #[cfg(not(unix))]
    #[test]
    fn without_inodes_nothing_is_removed() {
        let dir = private_tempdir();
        let file = dir.path().join("a.part");
        fs::write(&file, b"partial").unwrap();
        let record = object(
            "a.part",
            ObjectKind::PartialArchive,
            &fs::metadata(&file).unwrap(),
        );
        assert_eq!(
            remove_blocking(dir.path(), &record),
            RemoveOutcome::Unverifiable
        );
        assert!(file.exists());
    }
}
