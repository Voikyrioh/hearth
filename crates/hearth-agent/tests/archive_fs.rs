//! L'écriture de l'extraction (HRT-130, ADR-0055, `BR-ARCH-007`, `BR-ARCH-009`) contre le VRAI système
//! de fichiers : jamais de lien suivi, jamais de droit repris de l'archive, un dossier d'attente puis une
//! bascule, rien de partiel laissé. Les cas de liens exigent Unix : ils ne tournent qu'en CI Linux (le
//! poste de développement Windows ne les compile pas).

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "support/tmp.rs"]
mod tmp;

use std::path::Path;

use hearth_agent::domain::archive::{ArchiveLimits, ArchiveRefusal};
use hearth_agent::infrastructure::archive_fs::{extract_file, inspect_file, read_file_entry};

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

fn setup(archive: &[u8]) -> (tmp::TestDir, std::path::PathBuf, std::path::PathBuf) {
    let dir = tmp::tempdir().unwrap();
    let archive_path = dir.path().join("pack.tar");
    std::fs::write(&archive_path, archive).unwrap();
    let work = dir.path().join("work");
    std::fs::create_dir(&work).unwrap();
    (dir, archive_path, work)
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
    let destination = work.join("restored");
    let report = extract_file(&archive_path, &inspection, &destination, &limits, &never).unwrap();
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
fn a_cancelled_or_failed_extraction_leaves_no_destination_and_no_staging() {
    let archive = tar_of(vec![
        tar_entry("a.txt", b'0', "0000644", b"aaa"),
        tar_entry("b.txt", b'0', "0000644", b"bbb"),
    ]);
    let (_dir, archive_path, work) = setup(&archive);
    let limits = ArchiveLimits::default();
    let inspection = inspect_file(&archive_path, &limits, &never).unwrap();
    let destination = work.join("out");
    let calls = std::cell::Cell::new(0);
    let cancel = || {
        calls.set(calls.get() + 1);
        calls.get() > 5
    };
    assert_eq!(
        extract_file(&archive_path, &inspection, &destination, &limits, &cancel).unwrap_err(),
        ArchiveRefusal::Cancelled
    );
    assert!(names_in(&work).is_empty());

    // L'archive change entre l'inspection et l'extraction : rien n'est créé.
    let mut changed = archive.clone();
    changed[600] ^= 0x01;
    std::fs::write(&archive_path, changed).unwrap();
    assert_eq!(
        extract_file(&archive_path, &inspection, &destination, &limits, &never).unwrap_err(),
        ArchiveRefusal::ArchiveChanged
    );
    assert!(names_in(&work).is_empty());
}

#[test]
fn an_existing_destination_is_never_replaced() {
    let archive = tar_of(vec![tar_entry("a.txt", b'0', "0000644", b"aaa")]);
    let (_dir, archive_path, work) = setup(&archive);
    let limits = ArchiveLimits::default();
    let inspection = inspect_file(&archive_path, &limits, &never).unwrap();
    let destination = work.join("taken");
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(destination.join("mine"), b"keep").unwrap();
    assert_eq!(
        extract_file(&archive_path, &inspection, &destination, &limits, &never).unwrap_err(),
        ArchiveRefusal::WriteFailed
    );
    assert_eq!(std::fs::read(destination.join("mine")).unwrap(), b"keep");
    assert_eq!(names_in(&work), vec!["taken".to_string()]);
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

#[cfg(unix)]
mod unix {
    use super::*;
    use std::io::Write;
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

    use hearth_agent::domain::archive::{ArchivePath, ExtractSink, SinkError};
    use hearth_agent::infrastructure::archive_fs::DirectorySink;

    fn path_of(text: &str) -> ArchivePath {
        // Un `ArchivePath` ne se fabrique que par le jugement d'une archive : on passe par une vraie.
        let archive = tar_of(vec![tar_entry(text, b'0', "0000644", b"x")]);
        let dir = tmp::tempdir().unwrap();
        let file = dir.path().join("p.tar");
        std::fs::write(&file, archive).unwrap();
        let inspection = inspect_file(&file, &ArchiveLimits::default(), &never).unwrap();
        inspection.entries()[0].path.clone()
    }

    #[test]
    fn modes_owners_and_special_bits_of_the_archive_are_never_restored() {
        let archive = tar_of(vec![
            tar_entry("bin/", b'5', "0004777", b""),
            tar_entry("bin/run.sh", b'0', "0006755", b"#!/bin/sh\n"),
            tar_entry("plain.txt", b'0', "0000777", b"x"),
        ]);
        let (_dir, archive_path, work) = setup(&archive);
        let limits = ArchiveLimits::default();
        let inspection = inspect_file(&archive_path, &limits, &never).unwrap();
        let destination = work.join("out");
        extract_file(&archive_path, &inspection, &destination, &limits, &never).unwrap();
        for file in ["bin/run.sh", "plain.txt"] {
            let mode = std::fs::metadata(destination.join(file)).unwrap().mode() & 0o7777;
            assert_eq!(
                mode & 0o7111,
                0,
                "{file}: {mode:o} (exécution, setuid, setgid, sticky)"
            );
            assert_eq!(
                mode & 0o027,
                0,
                "{file}: {mode:o} (ni écriture au groupe ni accès aux autres)"
            );
        }
        let dir_mode = std::fs::metadata(destination.join("bin")).unwrap().mode() & 0o7777;
        assert_eq!(dir_mode & 0o7027, 0, "bin: {dir_mode:o}");
        // Le propriétaire est celui du processus, jamais celui de l'archive.
        let me = std::fs::metadata(&work).unwrap().uid();
        assert_eq!(
            std::fs::metadata(destination.join("plain.txt"))
                .unwrap()
                .uid(),
            me
        );
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

        let mut sink = DirectorySink::open(&root).unwrap();
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
        let mut sink = DirectorySink::open(&root).unwrap();
        assert!(matches!(
            sink.create_file(&path_of("hard")),
            Err(SinkError::AlreadyExists)
        ));
        assert_eq!(std::fs::read(&outside).unwrap(), b"original");
    }

    #[test]
    fn a_root_that_is_itself_a_symlink_is_not_opened() {
        let dir = tmp::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        symlink(&real, dir.path().join("root")).unwrap();
        assert!(DirectorySink::open(&dir.path().join("root")).is_err());
    }

    #[test]
    fn files_are_created_new_in_folders_created_on_the_way() {
        let dir = tmp::tempdir().unwrap();
        let mut sink = DirectorySink::open(dir.path()).unwrap();
        {
            let mut file = sink.create_file(&path_of("a/b/c.txt")).unwrap();
            file.write_all(b"hello").unwrap();
        }
        assert_eq!(
            std::fs::read(dir.path().join("a/b/c.txt")).unwrap(),
            b"hello"
        );
        assert!(matches!(
            sink.create_file(&path_of("a/b/c.txt")),
            Err(SinkError::AlreadyExists)
        ));
        let mode = std::fs::metadata(dir.path().join("a"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o7000, 0);
    }
}
