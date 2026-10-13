//! Les gestes par descripteur de `unix.rs`, contre le vrai système de fichiers (HRT-130). Ne tournent
//! que sous Unix (CI Linux). Un test qui exige root ou, au contraire, un utilisateur ordinaire se saute
//! en le disant sur la sortie d'erreur.

use std::ffi::OsStr;
use std::os::fd::OwnedFd;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;

use rustix::fs::{CWD, Mode, OFlags, mkdirat, openat};
use rustix::process::geteuid;

use super::unix::{
    Identity, check_parent, empty_dir, identity_of_fd, open_parent, remove_created_dir, switch,
    switch_failure, sync_tree,
};
use crate::domain::archive::ArchiveRefusal;
use crate::infrastructure::data_dir::private_tempdir;

fn open_dir(path: &Path) -> OwnedFd {
    openat(
        CWD,
        path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .unwrap()
}

fn open_in(parent: &OwnedFd, name: &str) -> OwnedFd {
    openat(
        parent,
        name,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .unwrap()
}

fn is_root() -> bool {
    geteuid().is_root()
}

/// Un dossier `stage` sous `parent`, avec fichiers, sous-dossiers et liens vers l'EXTÉRIEUR.
fn stage_with_traps(root: &Path) -> (OwnedFd, Identity, std::path::PathBuf) {
    let outside = root.join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("precious"), b"keep").unwrap();
    std::fs::create_dir(outside.join("deep")).unwrap();
    std::fs::write(outside.join("deep/inner"), b"keep").unwrap();
    let stage = root.join("stage");
    std::fs::create_dir(&stage).unwrap();
    std::fs::write(stage.join("file"), b"x").unwrap();
    std::fs::create_dir_all(stage.join("a/b")).unwrap();
    std::fs::write(stage.join("a/b/file"), b"x").unwrap();
    symlink(&outside, stage.join("link-to-dir")).unwrap();
    symlink(outside.join("precious"), stage.join("link-to-file")).unwrap();
    symlink(&outside, stage.join("a/link-to-dir")).unwrap();
    symlink(root.join("nowhere"), stage.join("dangling")).unwrap();
    let parent = open_dir(root);
    let stage_fd = open_in(&parent, "stage");
    let identity = identity_of_fd(&stage_fd).unwrap();
    (parent, identity, outside)
}

#[test]
fn cleanup_removes_what_the_staging_holds_and_never_follows_a_link() {
    let dir = private_tempdir();
    let (parent, identity, outside) = stage_with_traps(dir.path());
    remove_created_dir(&parent, OsStr::new("stage"), identity);
    assert!(
        !dir.path().join("stage").exists(),
        "dossier d'attente retiré"
    );
    assert_eq!(std::fs::read(outside.join("precious")).unwrap(), b"keep");
    assert_eq!(std::fs::read(outside.join("deep/inner")).unwrap(), b"keep");
}

#[test]
fn cleanup_touches_nothing_when_the_folder_is_not_the_one_that_was_created() {
    let dir = private_tempdir();
    let (parent, _identity, outside) = stage_with_traps(dir.path());
    // Un voisin porte le nom : l'identité relevée à la création n'est pas la sienne.
    let neighbour = dir.path().join("neighbour");
    std::fs::create_dir(&neighbour).unwrap();
    std::fs::write(neighbour.join("mine"), b"keep").unwrap();
    let wrong = identity_of_fd(&open_in(&parent, "neighbour")).unwrap();
    remove_created_dir(&parent, OsStr::new("stage"), wrong);
    assert!(dir.path().join("stage/file").exists(), "rien retiré");
    // Et l'inverse : le bon nom avec l'identité d'un AUTRE dossier ne retire pas le voisin.
    let stage_identity = identity_of_fd(&open_in(&parent, "stage")).unwrap();
    remove_created_dir(&parent, OsStr::new("neighbour"), stage_identity);
    assert_eq!(std::fs::read(neighbour.join("mine")).unwrap(), b"keep");
    assert_eq!(std::fs::read(outside.join("precious")).unwrap(), b"keep");
}

#[test]
fn cleanup_does_not_remove_a_link_standing_where_the_staging_was() {
    let dir = private_tempdir();
    let outside = dir.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("precious"), b"keep").unwrap();
    let parent = open_dir(dir.path());
    mkdirat(&parent, "stage", Mode::RWXU).unwrap();
    let identity = identity_of_fd(&open_in(&parent, "stage")).unwrap();
    // Le dossier d'attente est renommé, un lien prend sa place (un tiers qui pourrait écrire au parent).
    std::fs::rename(dir.path().join("stage"), dir.path().join("moved")).unwrap();
    symlink(&outside, dir.path().join("stage")).unwrap();
    remove_created_dir(&parent, OsStr::new("stage"), identity);
    assert_eq!(std::fs::read(outside.join("precious")).unwrap(), b"keep");
    assert!(dir.path().join("moved").exists());
}

#[test]
fn emptying_a_tree_and_syncing_it_work_by_descriptor() {
    let dir = private_tempdir();
    let (parent, _identity, outside) = stage_with_traps(dir.path());
    let stage = open_in(&parent, "stage");
    sync_tree(&stage, 0).unwrap();
    empty_dir(&stage, 0).unwrap();
    assert_eq!(
        std::fs::read_dir(dir.path().join("stage")).unwrap().count(),
        0
    );
    assert_eq!(std::fs::read(outside.join("precious")).unwrap(), b"keep");
}

#[test]
fn the_switch_never_replaces_an_existing_object() {
    let dir = private_tempdir();
    let parent = open_dir(dir.path());
    mkdirat(&parent, "stage", Mode::RWXU).unwrap();
    std::fs::write(dir.path().join("stage/new"), b"new").unwrap();

    // Un dossier VIDE existant (le seul que `rename` ordinaire remplacerait sans erreur) : refusé.
    std::fs::create_dir(dir.path().join("dest")).unwrap();
    let refusal = switch(&parent, OsStr::new("stage"), OsStr::new("dest")).unwrap_err();
    assert_ne!(refusal, ArchiveRefusal::Cancelled);
    assert!(matches!(
        refusal,
        ArchiveRefusal::WriteFailed | ArchiveRefusal::AtomicSwitchUnavailable
    ));
    assert!(dir.path().join("stage/new").exists());
    assert_eq!(
        std::fs::read_dir(dir.path().join("dest")).unwrap().count(),
        0
    );

    // Un fichier, un lien : refusés aussi.
    std::fs::remove_dir(dir.path().join("dest")).unwrap();
    std::fs::write(dir.path().join("dest"), b"file").unwrap();
    assert!(switch(&parent, OsStr::new("stage"), OsStr::new("dest")).is_err());
    assert_eq!(std::fs::read(dir.path().join("dest")).unwrap(), b"file");
    assert!(dir.path().join("stage/new").exists());

    // Libre : la bascule passe (sauf noyau sans l'option : refus net, voir le test suivant).
    std::fs::remove_file(dir.path().join("dest")).unwrap();
    match switch(&parent, OsStr::new("stage"), OsStr::new("dest")) {
        Ok(()) => assert_eq!(std::fs::read(dir.path().join("dest/new")).unwrap(), b"new"),
        Err(ArchiveRefusal::AtomicSwitchUnavailable) => {
            eprintln!("ce système de fichiers refuse RENAME_NOREPLACE : refus net, vérifié");
            assert!(dir.path().join("stage/new").exists());
            assert!(!dir.path().join("dest").exists());
        }
        Err(other) => panic!("bascule inattendue : {other:?}"),
    }
}

#[test]
fn a_system_without_the_no_replace_option_is_a_clean_refusal_never_a_silent_replacement() {
    use rustix::io::Errno;
    for errno in [Errno::INVAL, Errno::NOSYS, Errno::NOTSUP] {
        assert_eq!(
            switch_failure(errno),
            ArchiveRefusal::AtomicSwitchUnavailable
        );
    }
    for errno in [Errno::EXIST, Errno::NOENT, Errno::ACCESS, Errno::IO] {
        assert_eq!(switch_failure(errno), ArchiveRefusal::WriteFailed);
    }
}

#[test]
fn the_parent_must_be_the_agent_s_own_and_writable_by_nobody_else() {
    let dir = private_tempdir();
    let parent = dir.path().join("parent");
    std::fs::create_dir(&parent).unwrap();
    let check = |mode: u32| {
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(mode)).unwrap();
        check_parent(&open_dir(&parent))
    };
    assert_eq!(check(0o700), Ok(()));
    assert_eq!(check(0o755), Ok(()));
    assert_eq!(check(0o750), Ok(()));
    assert_eq!(check(0o770), Err(ArchiveRefusal::UnsafeDestination));
    assert_eq!(check(0o720), Err(ArchiveRefusal::UnsafeDestination));
    assert_eq!(check(0o707), Err(ArchiveRefusal::UnsafeDestination));
    assert_eq!(check(0o702), Err(ArchiveRefusal::UnsafeDestination));
    assert_eq!(check(0o777), Err(ArchiveRefusal::UnsafeDestination));
    // Le bit « sticky » ne rend pas un dossier de tous sûr (cas de /tmp).
    assert_eq!(check(0o1777), Err(ArchiveRefusal::UnsafeDestination));
}

#[test]
fn a_parent_owned_by_someone_else_is_refused() {
    if is_root() {
        eprintln!("ignoré : ce test suppose un utilisateur ordinaire (« / » appartient à root)");
        return;
    }
    // « / » appartient à root, n'est pas inscriptible par les autres : seul le propriétaire cloche.
    let root = open_parent(Path::new("/"), &[]).unwrap();
    assert_eq!(check_parent(&root), Err(ArchiveRefusal::UnsafeDestination));
}

#[test]
fn the_parent_is_opened_component_by_component_without_following_a_link() {
    let dir = private_tempdir();
    let real = dir.path().join("real");
    std::fs::create_dir_all(real.join("inner")).unwrap();
    symlink(&real, dir.path().join("link")).unwrap();
    std::fs::write(dir.path().join("file"), b"x").unwrap();
    let inner = OsStr::new("inner");

    open_parent(dir.path(), &[OsStr::new("real"), inner]).unwrap();
    for refused in [
        // un lien en dernier composant de la racine
        open_parent(&dir.path().join("link"), &[]),
        // un lien en composant intermédiaire
        open_parent(dir.path(), &[OsStr::new("link"), inner]),
        // un fichier là où un dossier est attendu
        open_parent(dir.path(), &[OsStr::new("file")]),
        // un composant absent
        open_parent(dir.path(), &[OsStr::new("absent")]),
    ] {
        assert_eq!(refused.unwrap_err(), ArchiveRefusal::UnsafeDestination);
    }
}
