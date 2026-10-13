//! L'écriture de l'extraction (HRT-130, ADR-0055, `BR-ARCH-007`, `BR-ARCH-008`, `BR-ARCH-009`) contre le
//! VRAI système de fichiers : jamais de lien suivi, jamais de droit repris de l'archive, un dossier
//! d'attente puis une bascule sans remplacement, un parent de confiance, rien de partiel laissé. Écrire
//! n'existe que sous Unix : ces tests ne tournent qu'en CI Linux (le poste de développement Windows ne
//! les compile pas ; il vérifie seulement que l'écriture y est refusée). Un test qui exige root ou un
//! utilisateur ordinaire se saute en le disant.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "support/tmp.rs"]
mod tmp;

use std::path::Path;

use hearth_agent::domain::archive::{ArchiveLimits, ArchiveRefusal};
use hearth_agent::infrastructure::archive_fs::{
    Destination, FileOwner, WriteSpec, extract_file, inspect_file,
};

fn never() -> bool {
    false
}

/// Un en-tête tar `ustar` ; `mode` est le champ de droits tel que l'archive l'écrit.
fn tar_entry(name: &str, flag: u8, mode: &str, data: &[u8]) -> Vec<u8> {
    let mut block = [0u8; 512];
    block[..name.len()].copy_from_slice(name.as_bytes());
    block[100..100 + mode.len()].copy_from_slice(mode.as_bytes());
    block[108..115].copy_from_slice(b"0000000");
    block[116..123].copy_from_slice(b"0000000");
    block[124..135].copy_from_slice(format!("{:011o}", data.len()).as_bytes());
    block[136..147].copy_from_slice(b"00000000000");
    block[156] = flag;
    block[257..263].copy_from_slice(b"ustar\0");
    block[263..265].copy_from_slice(b"00");
    block[148..156].copy_from_slice(b"        ");
    let sum: u32 = block.iter().map(|&b| u32::from(b)).sum();
    block[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    let mut out = block.to_vec();
    out.extend(data);
    out.extend(vec![0u8; (512 - data.len() % 512) % 512]);
    out
}

fn tar_of(entries: Vec<Vec<u8>>) -> Vec<u8> {
    let mut out: Vec<u8> = entries.concat();
    out.extend(vec![0u8; 1024]);
    out
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// L'archive écrite dans un dossier de test, et un dossier `work` qui servira de parent de confiance
/// (`0755` posé explicitement : le `umask` du poste ne doit pas le rendre inscriptible au groupe).
fn setup(archive: &[u8]) -> (tmp::TestDir, std::path::PathBuf, std::path::PathBuf) {
    let dir = tmp::tempdir().unwrap();
    let archive_path = dir.path().join("pack.tar");
    std::fs::write(&archive_path, archive).unwrap();
    let work = dir.path().join("work");
    std::fs::create_dir(&work).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&work, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    (dir, archive_path, work)
}

fn spec_with(uid: u32, gid: u32, file_mode: u32, dir_mode: u32) -> WriteSpec {
    WriteSpec::new(FileOwner::new(uid, gid).unwrap(), file_mode, dir_mode).unwrap()
}

/// Le propriétaire de la fiche : l'utilisateur effectif du test, de façon que `fchown` réussisse sans
/// root.
#[cfg(unix)]
fn my_ids() -> (u32, u32) {
    (
        rustix::process::geteuid().as_raw(),
        rustix::process::getegid().as_raw(),
    )
}

#[cfg(unix)]
fn my_spec() -> WriteSpec {
    let (uid, gid) = my_ids();
    spec_with(uid, gid, 0o664, 0o775)
}

#[test]
fn a_refused_archive_creates_nothing_not_even_a_staging_folder() {
    let archive = tar_of(vec![
        tar_entry("fine.txt", b'0', "0000644", b"fine"),
        tar_entry("evil", b'2', "0000777", b""),
    ]);
    let (_dir, archive_path, work) = setup(&archive);
    let refusal = inspect_file(&archive_path, &ArchiveLimits::default(), &never).unwrap_err();
    assert_eq!(refusal.code(), "SYMBOLIC_LINK");
    assert!(names_in(&work).is_empty());
}

#[test]
fn an_error_never_quotes_a_server_path() {
    let (_dir, archive_path, work) = setup(b"not an archive");
    let refusal = inspect_file(&archive_path, &ArchiveLimits::default(), &never).unwrap_err();
    let missing =
        inspect_file(&work.join("absent.zip"), &ArchiveLimits::default(), &never).unwrap_err();
    for text in [
        refusal.to_string(),
        missing.to_string(),
        format!("{missing:?}"),
    ] {
        assert!(
            !text.contains("work") && !text.contains("hearth-test-tmp"),
            "{text}"
        );
    }
}

#[test]
fn the_write_parameters_are_validated_where_they_are_built() {
    assert!(FileOwner::new(0, 0).is_some());
    assert!(
        FileOwner::new(u32::MAX, 0).is_none(),
        "u32::MAX = ne pas changer"
    );
    assert!(FileOwner::new(0, u32::MAX).is_none());
    let owner = FileOwner::new(1000, 1000).unwrap();
    assert!(WriteSpec::new(owner, 0o640, 0o750).is_some());
    for bad_file in [0o755, 0o4644, 0o2644, 0o1644, 0o100, 0o200, 0o1000] {
        assert!(
            WriteSpec::new(owner, bad_file, 0o750).is_none(),
            "{bad_file:o}"
        );
    }
    for bad_dir in [0o4755, 0o2755, 0o1755, 0o550, 0o650, 0o10_000] {
        assert!(
            WriteSpec::new(owner, 0o640, bad_dir).is_none(),
            "{bad_dir:o}"
        );
    }
}

#[test]
fn a_destination_needs_an_absolute_trusted_root_and_plain_relative_names() {
    let root = std::env::temp_dir();
    assert!(Destination::new(&root, Path::new("restored")).is_some());
    assert!(Destination::new(&root, Path::new("a/b/restored")).is_some());
    for bad in ["", ".", "..", "../x", "a/../x", "/abs", "./x/.."] {
        assert!(Destination::new(&root, Path::new(bad)).is_none(), "{bad:?}");
    }
    assert!(Destination::new(Path::new("relative/root"), Path::new("x")).is_none());
    let deep = vec!["d"; 65].join("/");
    assert!(Destination::new(&root, Path::new(&deep)).is_none());
}

#[cfg(not(unix))]
#[test]
fn writing_is_refused_outside_unix_there_is_no_unsafe_fallback() {
    let archive = tar_of(vec![tar_entry("a.txt", b'0', "0000644", b"aaa")]);
    let (_dir, archive_path, work) = setup(&archive);
    let limits = ArchiveLimits::default();
    let inspection = inspect_file(&archive_path, &limits, &never).unwrap();
    let destination = Destination::new(&work, Path::new("out")).unwrap();
    let spec = spec_with(1000, 1000, 0o640, 0o750);
    assert_eq!(
        extract_file(
            &archive_path,
            &inspection,
            &destination,
            &spec,
            &limits,
            &never
        )
        .unwrap_err(),
        ArchiveRefusal::AtomicSwitchUnavailable
    );
    assert!(names_in(&work).is_empty());
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::io::Write;
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

    use hearth_agent::domain::archive::{ArchivePath, ExtractSink, SinkError};
    use hearth_agent::infrastructure::archive_fs::{DirectorySink, read_file_entry};

    fn path_of(text: &str) -> ArchivePath {
        // Un `ArchivePath` ne se fabrique que par le jugement d'une archive : on passe par une vraie.
        let archive = tar_of(vec![tar_entry(text, b'0', "0000644", b"x")]);
        let dir = tmp::tempdir().unwrap();
        let file = dir.path().join("p.tar");
        std::fs::write(&file, archive).unwrap();
        let inspection = inspect_file(&file, &ArchiveLimits::default(), &never).unwrap();
        inspection.entries()[0].path.clone()
    }

    fn place<'a>(work: &'a Path, relative: &'a str) -> Destination<'a> {
        Destination::new(work, Path::new(relative)).unwrap()
    }

    fn is_root() -> bool {
        rustix::process::geteuid().is_root()
    }

    fn open_dir(path: &Path) -> std::fs::File {
        std::fs::File::open(path).unwrap()
    }

    fn two_file_archive() -> Vec<u8> {
        tar_of(vec![
            tar_entry("a.txt", b'0', "0000644", b"aaa"),
            tar_entry("b.txt", b'0', "0000644", b"bbb"),
        ])
    }

    #[test]
    fn an_archive_is_unpacked_into_a_new_folder_by_a_switch_and_nothing_else_is_left() {
        let archive = tar_of(vec![
            tar_entry("world/", b'5', "0000755", b""),
            tar_entry("world/level.dat", b'0', "0000644", b"level"),
            tar_entry("world/region/r.0.0.mca", b'0', "0000644", &vec![9u8; 5000]),
        ]);
        let (_dir, archive_path, work) = setup(&archive);
        let limits = ArchiveLimits::default();
        let inspection = inspect_file(&archive_path, &limits, &never).unwrap();
        let report = extract_file(
            &archive_path,
            &inspection,
            &place(&work, "restored"),
            &my_spec(),
            &limits,
            &never,
        )
        .unwrap();
        let destination = work.join("restored");
        assert_eq!((report.files, report.dirs), (2, 1));
        assert_eq!(
            std::fs::read(destination.join("world/level.dat")).unwrap(),
            b"level"
        );
        assert_eq!(
            std::fs::read(destination.join("world/region/r.0.0.mca"))
                .unwrap()
                .len(),
            5000
        );
        assert_eq!(
            names_in(&work),
            vec!["restored".to_string()],
            "aucun dossier d'attente"
        );
        let one = read_file_entry(
            &archive_path,
            &inspection,
            "world/level.dat",
            &limits,
            &never,
        )
        .unwrap();
        assert_eq!(one, b"level");
    }

    #[test]
    fn a_destination_below_existing_folders_is_reached_one_component_at_a_time() {
        let (_dir, archive_path, work) = setup(&two_file_archive());
        std::fs::create_dir_all(work.join("games/mc")).unwrap();
        std::fs::set_permissions(
            work.join("games/mc"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        let limits = ArchiveLimits::default();
        let inspection = inspect_file(&archive_path, &limits, &never).unwrap();
        extract_file(
            &archive_path,
            &inspection,
            &place(&work, "games/mc/world"),
            &my_spec(),
            &limits,
            &never,
        )
        .unwrap();
        assert_eq!(
            std::fs::read(work.join("games/mc/world/a.txt")).unwrap(),
            b"aaa"
        );
        assert_eq!(names_in(&work.join("games/mc")), vec!["world".to_string()]);
    }

    #[test]
    fn a_cancelled_or_failed_extraction_leaves_no_destination_and_no_staging() {
        let archive = two_file_archive();
        let (_dir, archive_path, work) = setup(&archive);
        let limits = ArchiveLimits::default();
        let inspection = inspect_file(&archive_path, &limits, &never).unwrap();
        let calls = std::cell::Cell::new(0);
        let cancel = || {
            calls.set(calls.get() + 1);
            calls.get() > 5
        };
        assert_eq!(
            extract_file(
                &archive_path,
                &inspection,
                &place(&work, "out"),
                &my_spec(),
                &limits,
                &cancel
            )
            .unwrap_err(),
            ArchiveRefusal::Cancelled
        );
        assert!(names_in(&work).is_empty());

        // L'archive change entre l'inspection et l'extraction : rien n'est créé.
        let mut changed = archive.clone();
        changed[600] ^= 0x01;
        std::fs::write(&archive_path, changed).unwrap();
        assert_eq!(
            extract_file(
                &archive_path,
                &inspection,
                &place(&work, "out"),
                &my_spec(),
                &limits,
                &never
            )
            .unwrap_err(),
            ArchiveRefusal::ArchiveChanged
        );
        assert!(names_in(&work).is_empty());
    }

    #[test]
    fn an_existing_destination_is_never_replaced() {
        let (_dir, archive_path, work) = setup(&two_file_archive());
        let limits = ArchiveLimits::default();
        let inspection = inspect_file(&archive_path, &limits, &never).unwrap();
        let taken = work.join("taken");
        std::fs::create_dir(&taken).unwrap();
        std::fs::write(taken.join("mine"), b"keep").unwrap();
        std::fs::create_dir(work.join("empty")).unwrap();
        std::fs::write(work.join("file"), b"keep").unwrap();
        symlink(&taken, work.join("link")).unwrap();
        for name in ["taken", "empty", "file", "link"] {
            assert_eq!(
                extract_file(
                    &archive_path,
                    &inspection,
                    &place(&work, name),
                    &my_spec(),
                    &limits,
                    &never
                )
                .unwrap_err(),
                ArchiveRefusal::WriteFailed,
                "{name}"
            );
        }
        assert_eq!(std::fs::read(taken.join("mine")).unwrap(), b"keep");
        assert_eq!(std::fs::read_dir(work.join("empty")).unwrap().count(), 0);
        assert_eq!(std::fs::read(work.join("file")).unwrap(), b"keep");
        assert_eq!(names_in(&work), vec!["empty", "file", "link", "taken"]);
    }

    #[test]
    fn modes_and_owners_come_from_the_caller_never_from_the_archive_nor_the_umask() {
        let archive = tar_of(vec![
            tar_entry("bin/", b'5', "0004777", b""),
            tar_entry("bin/run.sh", b'0', "0006755", b"#!/bin/sh\n"),
            tar_entry("plain.txt", b'0', "0000777", b"x"),
        ]);
        let (_dir, archive_path, work) = setup(&archive);
        let limits = ArchiveLimits::default();
        let inspection = inspect_file(&archive_path, &limits, &never).unwrap();
        extract_file(
            &archive_path,
            &inspection,
            &place(&work, "out"),
            &my_spec(),
            &limits,
            &never,
        )
        .unwrap();
        let destination = work.join("out");
        let (uid, gid) = my_ids();
        // Modes EXACTS : 0664 / 0775 ne sortent d'aucun `umask` usuel (022 ôterait l'écriture du
        // groupe) ni de l'archive (0777, 6755, 4777) : seul `fchmod` les produit.
        for file in ["bin/run.sh", "plain.txt"] {
            let metadata = std::fs::metadata(destination.join(file)).unwrap();
            assert_eq!(metadata.mode() & 0o7777, 0o664, "{file}");
            assert_eq!((metadata.uid(), metadata.gid()), (uid, gid), "{file}");
        }
        for folder in ["", "bin"] {
            let metadata = std::fs::metadata(destination.join(folder)).unwrap();
            assert_eq!(metadata.mode() & 0o7777, 0o775, "dossier {folder:?}");
            assert_eq!(
                (metadata.uid(), metadata.gid()),
                (uid, gid),
                "dossier {folder:?}"
            );
        }
    }

    #[test]
    fn the_owner_asked_by_the_caller_is_set_at_creation_when_the_agent_is_root() {
        if !is_root() {
            eprintln!(
                "ignoré : poser un autre propriétaire exige root (le cas de l'agent en production)"
            );
            return;
        }
        let archive = tar_of(vec![
            tar_entry("d/", b'5', "0000755", b""),
            tar_entry("d/f.txt", b'0', "0000644", b"x"),
        ]);
        let (_dir, archive_path, work) = setup(&archive);
        let limits = ArchiveLimits::default();
        let inspection = inspect_file(&archive_path, &limits, &never).unwrap();
        let spec = spec_with(4242, 4343, 0o640, 0o750);
        extract_file(
            &archive_path,
            &inspection,
            &place(&work, "out"),
            &spec,
            &limits,
            &never,
        )
        .unwrap();
        for path in ["out", "out/d", "out/d/f.txt"] {
            let metadata = std::fs::metadata(work.join(path)).unwrap();
            assert_eq!((metadata.uid(), metadata.gid()), (4242, 4343), "{path}");
        }
        assert_eq!(
            std::fs::metadata(work.join("out/d/f.txt")).unwrap().mode() & 0o7777,
            0o640
        );
    }

    #[test]
    fn a_parent_the_group_or_others_can_write_into_is_refused_and_nothing_is_created() {
        let (_dir, archive_path, work) = setup(&two_file_archive());
        let limits = ArchiveLimits::default();
        let inspection = inspect_file(&archive_path, &limits, &never).unwrap();
        for mode in [0o775, 0o757, 0o777, 0o770, 0o702, 0o720, 0o1777] {
            std::fs::set_permissions(&work, std::fs::Permissions::from_mode(mode)).unwrap();
            let refusal = extract_file(
                &archive_path,
                &inspection,
                &place(&work, "out"),
                &my_spec(),
                &limits,
                &never,
            )
            .unwrap_err();
            assert_eq!(refusal, ArchiveRefusal::UnsafeDestination, "{mode:o}");
            assert!(names_in(&work).is_empty(), "{mode:o}");
        }
        // Sûr de nouveau : l'extraction passe.
        std::fs::set_permissions(&work, std::fs::Permissions::from_mode(0o750)).unwrap();
        extract_file(
            &archive_path,
            &inspection,
            &place(&work, "out"),
            &my_spec(),
            &limits,
            &never,
        )
        .unwrap();
    }

    #[test]
    fn a_parent_owned_by_someone_else_is_refused() {
        if is_root() {
            eprintln!(
                "ignoré : ce test suppose un utilisateur ordinaire (« / » appartient à root)"
            );
            return;
        }
        let (_dir, archive_path, _work) = setup(&two_file_archive());
        let limits = ArchiveLimits::default();
        let inspection = inspect_file(&archive_path, &limits, &never).unwrap();
        // « / » n'est inscriptible que par root : seul son propriétaire fait refuser.
        let refusal = extract_file(
            &archive_path,
            &inspection,
            &Destination::new(Path::new("/"), Path::new("hearth-never-created")).unwrap(),
            &my_spec(),
            &limits,
            &never,
        )
        .unwrap_err();
        assert_eq!(refusal, ArchiveRefusal::UnsafeDestination);
        assert!(!Path::new("/hearth-never-created").exists());
    }

    #[test]
    fn a_link_on_the_way_to_the_destination_is_never_followed() {
        let (dir, archive_path, work) = setup(&two_file_archive());
        let limits = ArchiveLimits::default();
        let inspection = inspect_file(&archive_path, &limits, &never).unwrap();
        let outside = dir.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o755)).unwrap();
        symlink(&outside, work.join("link")).unwrap();
        // Un lien en composant intermédiaire du chemin relatif.
        let refusal = extract_file(
            &archive_path,
            &inspection,
            &place(&work, "link/out"),
            &my_spec(),
            &limits,
            &never,
        )
        .unwrap_err();
        assert_eq!(refusal, ArchiveRefusal::UnsafeDestination);
        // La racine de confiance elle-même est un lien.
        let refusal = extract_file(
            &archive_path,
            &inspection,
            &place(&work.join("link"), "out"),
            &my_spec(),
            &limits,
            &never,
        )
        .unwrap_err();
        assert_eq!(refusal, ArchiveRefusal::UnsafeDestination);
        assert!(names_in(&outside).is_empty(), "rien écrit au bout du lien");
        assert_eq!(names_in(&work), vec!["link".to_string()]);
    }

    #[test]
    fn the_archive_itself_is_never_opened_through_a_link_nor_when_it_is_not_a_regular_file() {
        let (dir, archive_path, work) = setup(&two_file_archive());
        let limits = ArchiveLimits::default();
        let inspection = inspect_file(&archive_path, &limits, &never).unwrap();
        let spec = my_spec();
        // Un lien vers une vraie archive.
        let link = dir.path().join("pack-link.tar");
        symlink(&archive_path, &link).unwrap();
        // Un dossier à la place.
        let folder = dir.path().join("pack-dir.tar");
        std::fs::create_dir(&folder).unwrap();
        // Un tube nommé : l'ouverture ne doit pas bloquer.
        let fifo = dir.path().join("pack-fifo.tar");
        rustix::fs::mknodat(
            rustix::fs::CWD,
            fifo.as_path(),
            rustix::fs::FileType::Fifo,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
            0,
        )
        .unwrap();
        for source in [&link, &folder, &fifo] {
            assert_eq!(
                inspect_file(source, &limits, &never).unwrap_err(),
                ArchiveRefusal::SourceUnreadable,
                "{source:?}"
            );
            assert_eq!(
                extract_file(
                    source,
                    &inspection,
                    &place(&work, "out"),
                    &spec,
                    &limits,
                    &never
                )
                .unwrap_err(),
                ArchiveRefusal::SourceUnreadable,
                "{source:?}"
            );
            assert_eq!(
                read_file_entry(source, &inspection, "a.txt", &limits, &never).unwrap_err(),
                ArchiveRefusal::SourceUnreadable,
                "{source:?}"
            );
        }
        assert!(names_in(&work).is_empty());
    }

    #[test]
    fn a_symlink_already_in_the_destination_is_never_followed() {
        let dir = tmp::tempdir().unwrap();
        let outside = dir.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("secret"), b"original").unwrap();
        let root = dir.path().join("root");
        std::fs::create_dir(&root).unwrap();
        symlink(&outside, root.join("link")).unwrap();
        symlink(outside.join("secret"), root.join("filelink")).unwrap();
        symlink(dir.path().join("nowhere"), root.join("dangling")).unwrap();

        let mut sink = DirectorySink::new(open_dir(&root), my_spec());
        // Traverser un lien de dossier : refusé.
        assert!(matches!(
            sink.create_file(&path_of("link/x")),
            Err(SinkError::Blocked)
        ));
        assert!(matches!(
            sink.create_dir(&path_of("link/sub")),
            Err(SinkError::Blocked)
        ));
        assert!(matches!(
            sink.create_dir(&path_of("dangling/sub")),
            Err(SinkError::Blocked)
        ));
        // Écrire sur un lien de fichier (existant ou pendant) : refusé, la cible n'est pas touchée.
        for name in ["filelink", "dangling", "link"] {
            let result = sink.create_file(&path_of(name));
            assert!(
                matches!(result, Err(SinkError::AlreadyExists | SinkError::Blocked)),
                "{name}"
            );
        }
        assert_eq!(std::fs::read(outside.join("secret")).unwrap(), b"original");
        assert_eq!(names_in(&outside), vec!["secret".to_string()]);
        assert!(!dir.path().join("nowhere").exists());
    }

    #[test]
    fn an_existing_hard_link_is_never_written_through() {
        let dir = tmp::tempdir().unwrap();
        let outside = dir.path().join("precious");
        std::fs::write(&outside, b"original").unwrap();
        let root = dir.path().join("root");
        std::fs::create_dir(&root).unwrap();
        std::fs::hard_link(&outside, root.join("hard")).unwrap();
        let mut sink = DirectorySink::new(open_dir(&root), my_spec());
        assert!(matches!(
            sink.create_file(&path_of("hard")),
            Err(SinkError::AlreadyExists)
        ));
        assert_eq!(std::fs::read(&outside).unwrap(), b"original");
    }

    #[test]
    fn files_are_created_new_in_folders_created_on_the_way_and_flush_makes_them_durable() {
        let dir = tmp::tempdir().unwrap();
        let mut sink = DirectorySink::new(open_dir(dir.path()), my_spec());
        {
            let mut file = sink.create_file(&path_of("a/b/c.txt")).unwrap();
            file.write_all(b"hello").unwrap();
            // `flush` = `fsync` : doit réussir sur un vrai fichier.
            file.flush().unwrap();
        }
        assert_eq!(
            std::fs::read(dir.path().join("a/b/c.txt")).unwrap(),
            b"hello"
        );
        assert!(matches!(
            sink.create_file(&path_of("a/b/c.txt")),
            Err(SinkError::AlreadyExists)
        ));
        for folder in ["a", "a/b"] {
            let metadata = std::fs::metadata(dir.path().join(folder)).unwrap();
            assert_eq!(metadata.permissions().mode() & 0o7777, 0o775, "{folder}");
        }
        let metadata = std::fs::metadata(dir.path().join("a/b/c.txt")).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o7777, 0o664);
    }
}
