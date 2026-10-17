//! `SafeFs` contre le VRAI système de fichiers (HRT-145). Ne tourne que sous Unix (CI Linux). Chaque test
//! plante des pièges (liens vers un dossier EXTÉRIEUR à la racine, tubes, fichiers à plusieurs noms) et
//! constate que l'extérieur n'a pas bougé.

use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rustix::process::{getegid, geteuid};

use super::unix::{UnixRoot, open_service_root, remove_service_root};
use crate::application::ports::{FsError, Ownership, Perms};
use crate::domain::files::RelPath;
use crate::domain::services::ServiceKey;
use crate::infrastructure::data_dir::private_tempdir;

const BUDGET: Duration = Duration::from_secs(10);

struct Fixture {
    _dir: crate::test_tmp::TestDir,
    data: PathBuf,
    /// `<données>/services/<clé>`
    root_path: PathBuf,
    /// Dossier voisin de `data`, HORS de la racine : ce que les pièges visent.
    outside: PathBuf,
    root: UnixRoot,
    key: ServiceKey,
}

fn key() -> ServiceKey {
    ServiceKey::parse("u_terraria").unwrap()
}

fn fixture() -> Fixture {
    let dir = private_tempdir();
    let data = dir.path().join("data");
    std::fs::create_dir(&data).unwrap();
    std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o700)).unwrap();
    let outside = dir.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("precious"), b"keep").unwrap();
    std::fs::create_dir(outside.join("deep")).unwrap();
    std::fs::write(outside.join("deep/inner"), b"keep").unwrap();
    let key = key();
    let root = open_service_root(&data, &key, true).unwrap();
    let root_path = data.join("services").join(key.as_str());
    Fixture {
        _dir: dir,
        data,
        root_path,
        outside,
        root,
        key,
    }
}

fn perms(mode: u32) -> Perms {
    Perms::new(
        Ownership::new(geteuid().as_raw(), getegid().as_raw()).unwrap(),
        mode,
    )
    .unwrap()
}

fn rel(text: &str) -> RelPath {
    RelPath::parse(text).unwrap()
}

fn mode_of(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

/// Rien n'a changé dehors : les mêmes entrées, les mêmes contenus.
fn assert_outside_intact(outside: &Path) {
    let mut names: Vec<String> = std::fs::read_dir(outside)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["deep", "precious"], "{names:?}");
    assert_eq!(std::fs::read(outside.join("precious")).unwrap(), b"keep");
    assert_eq!(std::fs::read(outside.join("deep/inner")).unwrap(), b"keep");
}

#[test]
fn the_services_folder_and_the_root_are_created_private() {
    let f = fixture();
    assert_eq!(mode_of(&f.data.join("services")), 0o700);
    assert_eq!(mode_of(&f.root_path), 0o700);
}

#[test]
fn opening_a_missing_root_without_creating_it_fails() {
    let f = fixture();
    let other = ServiceKey::parse("u_other").unwrap();
    assert!(matches!(
        open_service_root(&f.data, &other, false),
        Err(FsError::NotFound)
    ));
    assert!(!f.data.join("services/u_other").exists());
}

#[test]
fn a_data_folder_that_is_a_link_is_refused() {
    let f = fixture();
    let link = f.data.parent().unwrap().join("data-link");
    symlink(&f.data, &link).unwrap();
    assert!(matches!(
        open_service_root(&link, &f.key, false),
        Err(FsError::Blocked)
    ));
}

#[test]
fn a_root_another_account_can_write_is_refused() {
    let f = fixture();
    std::fs::set_permissions(&f.root_path, std::fs::Permissions::from_mode(0o770)).unwrap();
    assert!(matches!(
        open_service_root(&f.data, &f.key, false),
        Err(FsError::UnsafeRoot)
    ));
}

#[test]
fn mkdir_creates_parents_with_the_given_mode_and_is_idempotent() {
    let f = fixture();
    assert_eq!(f.root.make_dirs(&rel("a/b/c"), perms(0o750)), Ok(true));
    for path in ["a", "a/b", "a/b/c"] {
        assert_eq!(mode_of(&f.root_path.join(path)), 0o750, "{path}");
    }
    assert_eq!(f.root.make_dirs(&rel("a/b/c"), perms(0o700)), Ok(false));
    // Un dossier existant garde ses droits.
    assert_eq!(mode_of(&f.root_path.join("a/b/c")), 0o750);
}

#[test]
fn mkdir_through_a_planted_link_leaves_the_target_alone() {
    let f = fixture();
    symlink(&f.outside, f.root_path.join("data")).unwrap();
    assert_eq!(
        f.root.make_dirs(&rel("data/new"), perms(0o750)),
        Err(FsError::Blocked)
    );
    assert_outside_intact(&f.outside);
}

#[test]
fn mkdir_of_the_root_itself_is_refused() {
    let f = fixture();
    assert_eq!(
        f.root.make_dirs(&RelPath::root(), perms(0o750)),
        Err(FsError::Refused)
    );
}

#[test]
fn write_then_read_round_trips_and_leaves_no_staging_file() {
    let f = fixture();
    f.root.make_dirs(&rel("conf"), perms(0o750)).unwrap();
    f.root
        .write_file(&rel("conf/server.cfg"), b"port=7777", perms(0o640))
        .unwrap();
    f.root
        .write_file(&rel("conf/server.cfg"), b"port=7778", perms(0o600))
        .unwrap();
    assert_eq!(
        f.root.read_file(&rel("conf/server.cfg"), 1024).unwrap(),
        b"port=7778"
    );
    let path = f.root_path.join("conf/server.cfg");
    assert_eq!(mode_of(&path), 0o600);
    assert_eq!(std::fs::metadata(&path).unwrap().uid(), geteuid().as_raw());
    let names: Vec<_> = std::fs::read_dir(f.root_path.join("conf"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, ["server.cfg"]);
}

#[test]
fn a_written_file_never_carries_a_special_bit() {
    let f = fixture();
    // `Perms` refuse déjà setuid, setgid et sticky.
    assert!(Perms::new(perms(0o644).owner(), 0o4755).is_none());
    assert!(Perms::new(perms(0o644).owner(), 0o2755).is_none());
    assert!(Perms::new(perms(0o644).owner(), 0o1755).is_none());
    f.root
        .write_file(&rel("run.sh"), b"#!/bin/sh\n", perms(0o755))
        .unwrap();
    assert_eq!(mode_of(&f.root_path.join("run.sh")), 0o755);
}

#[test]
fn the_parent_must_exist() {
    let f = fixture();
    assert_eq!(
        f.root.write_file(&rel("missing/x"), b"x", perms(0o600)),
        Err(FsError::NotFound)
    );
}

#[test]
fn writing_through_a_planted_directory_link_leaves_the_target_alone() {
    let f = fixture();
    symlink(&f.outside, f.root_path.join("data")).unwrap();
    assert_eq!(
        f.root.write_file(&rel("data/x"), b"evil", perms(0o600)),
        Err(FsError::Blocked)
    );
    assert_eq!(
        f.root
            .write_file(&rel("data/deep/inner"), b"evil", perms(0o600)),
        Err(FsError::Blocked)
    );
    assert_outside_intact(&f.outside);
}

#[test]
fn a_link_in_place_of_the_file_is_refused_and_its_target_keeps_its_content() {
    let f = fixture();
    symlink(f.outside.join("precious"), f.root_path.join("file")).unwrap();
    assert_eq!(
        f.root.write_file(&rel("file"), b"evil", perms(0o600)),
        Err(FsError::Blocked)
    );
    assert_eq!(f.root.read_file(&rel("file"), 100), Err(FsError::Blocked));
    assert_outside_intact(&f.outside);
}

#[test]
fn a_dangling_link_with_an_outside_target_is_never_created_through() {
    let f = fixture();
    symlink(f.outside.join("not-yet"), f.root_path.join("file")).unwrap();
    assert_eq!(
        f.root.write_file(&rel("file"), b"evil", perms(0o600)),
        Err(FsError::Blocked)
    );
    assert!(!f.outside.join("not-yet").exists());
}

#[test]
fn a_file_with_several_names_is_readable_but_never_modified() {
    let f = fixture();
    std::fs::write(f.root_path.join("a"), b"shared").unwrap();
    std::fs::hard_link(f.root_path.join("a"), f.root_path.join("b")).unwrap();
    assert_eq!(
        f.root.write_file(&rel("a"), b"changed", perms(0o600)),
        Err(FsError::Hardlinked)
    );
    assert_eq!(f.root.read_file(&rel("b"), 100).unwrap(), b"shared");
    assert_eq!(std::fs::read(f.root_path.join("a")).unwrap(), b"shared");
}

#[test]
fn a_directory_in_place_of_the_file_is_refused() {
    let f = fixture();
    f.root.make_dirs(&rel("d"), perms(0o750)).unwrap();
    assert_eq!(
        f.root.write_file(&rel("d"), b"x", perms(0o600)),
        Err(FsError::NotRegular)
    );
}

#[cfg(target_os = "linux")]
#[test]
fn a_named_pipe_is_neither_read_nor_overwritten_and_nothing_blocks() {
    use rustix::fs::{CWD, FileType, Mode, mknodat};
    let f = fixture();
    mknodat(
        CWD,
        f.root_path.join("pipe"),
        FileType::Fifo,
        Mode::RUSR | Mode::WUSR,
        0,
    )
    .unwrap();
    assert_eq!(
        f.root.read_file(&rel("pipe"), 100),
        Err(FsError::NotRegular)
    );
    assert_eq!(
        f.root.write_file(&rel("pipe"), b"x", perms(0o600)),
        Err(FsError::NotRegular)
    );
}

#[test]
fn a_read_over_the_limit_is_refused_and_exactly_the_limit_is_accepted() {
    let f = fixture();
    f.root
        .write_file(&rel("big"), &[7u8; 100], perms(0o600))
        .unwrap();
    assert_eq!(f.root.read_file(&rel("big"), 99), Err(FsError::TooLarge));
    assert_eq!(f.root.read_file(&rel("big"), 100).unwrap().len(), 100);
    assert_eq!(
        f.root.read_file(&rel("absent"), 100),
        Err(FsError::NotFound)
    );
}

#[test]
fn measure_counts_blocks_and_a_multi_named_file_once() {
    let f = fixture();
    f.root.make_dirs(&rel("d"), perms(0o750)).unwrap();
    f.root
        .write_file(&rel("d/one"), &vec![1u8; 64 * 1024], perms(0o600))
        .unwrap();
    std::fs::hard_link(f.root_path.join("d/one"), f.root_path.join("d/two")).unwrap();
    let measure = f.root.measure_tree(&RelPath::root(), BUDGET).unwrap();
    assert!(!measure.partial);
    assert_eq!(measure.items, 3, "d, one, two");
    let file_blocks = std::fs::metadata(f.root_path.join("d/one"))
        .unwrap()
        .blocks()
        * 512;
    let dir_blocks = std::fs::metadata(f.root_path.join("d")).unwrap().blocks() * 512;
    assert_eq!(measure.bytes_on_disk, file_blocks + dir_blocks);
}

#[test]
fn measure_never_follows_a_link_out_of_the_root() {
    let f = fixture();
    symlink(&f.outside, f.root_path.join("out")).unwrap();
    symlink(f.outside.join("precious"), f.root_path.join("file")).unwrap();
    let measure = f.root.measure_tree(&RelPath::root(), BUDGET).unwrap();
    assert_eq!(measure.items, 2, "les deux liens, rien de leur cible");
    assert_eq!(
        f.root.measure_tree(&rel("out"), BUDGET),
        Err(FsError::Blocked)
    );
}

#[test]
fn an_exhausted_budget_gives_a_partial_measure() {
    let f = fixture();
    f.root.write_file(&rel("x"), b"data", perms(0o600)).unwrap();
    let measure = f
        .root
        .measure_tree(&RelPath::root(), Duration::ZERO)
        .unwrap();
    assert!(measure.partial);
    assert_eq!(measure.items, 0);
}

#[test]
fn remove_tree_unlinks_links_as_links_and_never_descends_into_them() {
    let f = fixture();
    f.root.make_dirs(&rel("t/sub"), perms(0o750)).unwrap();
    f.root
        .write_file(&rel("t/sub/f"), b"x", perms(0o600))
        .unwrap();
    symlink(&f.outside, f.root_path.join("t/to-dir")).unwrap();
    symlink(&f.outside, f.root_path.join("t/sub/to-dir-deep")).unwrap();
    symlink(f.outside.join("precious"), f.root_path.join("t/to-file")).unwrap();
    f.root.remove(&rel("t")).unwrap();
    assert!(!f.root_path.join("t").exists());
    assert_outside_intact(&f.outside);
}

#[test]
fn remove_tree_of_a_link_removes_only_the_link() {
    let f = fixture();
    symlink(&f.outside, f.root_path.join("out")).unwrap();
    f.root.remove(&rel("out")).unwrap();
    assert!(std::fs::symlink_metadata(f.root_path.join("out")).is_err());
    assert_outside_intact(&f.outside);
}

#[test]
fn remove_tree_refuses_the_root_and_reports_a_missing_target() {
    let f = fixture();
    assert_eq!(f.root.remove(&RelPath::root()), Err(FsError::Refused));
    assert!(f.root_path.is_dir());
    assert_eq!(f.root.remove(&rel("absent")), Err(FsError::NotFound));
    assert_eq!(f.root.remove(&rel("absent/deeper")), Err(FsError::NotFound));
}

#[test]
fn a_deeper_tree_than_the_bound_is_refused_not_followed_forever() {
    let f = fixture();
    // Trop long pour un seul appel sur certains systèmes : on construit par morceaux.
    let mut built = f.root_path.clone();
    for _ in 0..70 {
        built.push("d");
        std::fs::create_dir(&built).unwrap();
    }
    assert_eq!(f.root.remove(&rel("d")), Err(FsError::Refused));
    assert!(f.root_path.join("d").exists());
}

#[test]
fn a_mount_point_is_never_crossed() {
    let f = fixture();
    f.root.make_dirs(&rel("m/inner"), perms(0o750)).unwrap();
    f.root
        .write_file(&rel("m/inner/f"), b"x", perms(0o600))
        .unwrap();
    // Un périphérique attendu différent : chaque descendant ressemble à un autre système de fichiers.
    let wrong = open_service_root(&f.data, &f.key, false).unwrap();
    let wrong = wrong.with_dev(f.root.dev() + 1);
    assert_eq!(
        wrong.read_file(&rel("m/inner/f"), 10),
        Err(FsError::CrossesDevice)
    );
    assert_eq!(
        wrong.write_file(&rel("m/inner/g"), b"x", perms(0o600)),
        Err(FsError::CrossesDevice)
    );
    assert_eq!(
        wrong.make_dirs(&rel("m/new"), perms(0o750)),
        Err(FsError::CrossesDevice)
    );
    assert_eq!(wrong.remove(&rel("m")), Err(FsError::CrossesDevice));
    let measure = wrong.measure_tree(&RelPath::root(), BUDGET).unwrap();
    assert_eq!(measure.items, 0);
    assert!(f.root_path.join("m/inner/f").exists());
    assert!(!f.root_path.join("m/inner/g").exists());
}

#[test]
fn removing_the_service_root_takes_its_content_and_nothing_else() {
    let f = fixture();
    f.root.make_dirs(&rel("a/b"), perms(0o750)).unwrap();
    f.root
        .write_file(&rel("a/b/f"), b"x", perms(0o600))
        .unwrap();
    symlink(&f.outside, f.root_path.join("a/out")).unwrap();
    symlink(&f.outside, f.root_path.join("out2")).unwrap();
    let neighbour = ServiceKey::parse("u_neighbour").unwrap();
    let neighbour_root = open_service_root(&f.data, &neighbour, true).unwrap();
    neighbour_root
        .write_file(&rel("keep"), b"mine", perms(0o600))
        .unwrap();
    remove_service_root(&f.data, &f.key).unwrap();
    assert!(!f.root_path.exists());
    assert!(f.data.join("services").is_dir());
    assert_eq!(
        std::fs::read(f.data.join("services/u_neighbour/keep")).unwrap(),
        b"mine"
    );
    assert_outside_intact(&f.outside);
    assert!(matches!(
        remove_service_root(&f.data, &f.key),
        Err(FsError::NotFound)
    ));
}

#[test]
fn a_service_root_replaced_by_a_link_is_not_followed() {
    let f = fixture();
    let neighbour = ServiceKey::parse("u_linked").unwrap();
    symlink(&f.outside, f.data.join("services/u_linked")).unwrap();
    assert!(matches!(
        open_service_root(&f.data, &neighbour, false),
        Err(FsError::Blocked)
    ));
    assert!(matches!(
        open_service_root(&f.data, &neighbour, true),
        Err(FsError::Blocked)
    ));
    assert!(matches!(
        remove_service_root(&f.data, &neighbour),
        Err(FsError::Blocked)
    ));
    assert_outside_intact(&f.outside);
}

/// Course : pendant que l'agent écrit, le « service » remplace sans cesse un sous-dossier par un lien
/// vers l'extérieur, puis le remet. Quel que soit l'entrelacement, l'extérieur ne change JAMAIS.
#[test]
fn a_directory_swapped_for_a_link_during_writes_never_leaks_outside() {
    let f = fixture();
    let sub = f.root_path.join("sub");
    std::fs::create_dir(&sub).unwrap();
    let stop = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while !stop.load(Ordering::Relaxed) {
                let _ = std::fs::remove_dir_all(&sub);
                let _ = symlink(&f.outside, &sub);
                let _ = std::fs::remove_file(&sub);
                let _ = std::fs::create_dir(&sub);
            }
        });
        for round in 0..1500u32 {
            let _ = f.root.write_file(&rel("sub/file"), b"evil", perms(0o600));
            let _ = f.root.make_dirs(&rel("sub/made"), perms(0o750));
            let _ = f.root.read_file(&rel("sub/deep/inner"), 100);
            let _ = f.root.measure_tree(&rel("sub"), BUDGET);
            if round % 500 == 0 {
                let _ = f.root.remove(&rel("sub/file"));
            }
        }
        stop.store(true, Ordering::Relaxed);
    });
    assert_outside_intact(&f.outside);
}

/// Course sur la suppression : le dossier est remplacé par un lien vers l'extérieur pendant `remove`.
#[test]
fn a_directory_swapped_for_a_link_during_removal_never_deletes_outside() {
    let f = fixture();
    let stop = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let target = f.root_path.join("victim");
            while !stop.load(Ordering::Relaxed) {
                let _ = std::fs::remove_file(&target);
                let _ = std::fs::create_dir(&target);
                let _ = std::fs::create_dir(target.join("inner"));
                let _ = std::fs::remove_dir_all(&target);
                let _ = symlink(&f.outside, &target);
            }
        });
        for _ in 0..1500u32 {
            let _ = f.root.remove(&rel("victim"));
        }
        stop.store(true, Ordering::Relaxed);
    });
    assert_outside_intact(&f.outside);
}
