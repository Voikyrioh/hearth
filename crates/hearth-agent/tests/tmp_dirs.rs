//! L'aide des dossiers temporaires des tests (`support/tmp.rs`, HRT-18, T43) : sous la racine connue,
//! supprimés à la fin, même quand le test échoue, même quand un fichier est encore ouvert à ce
//! moment (cas des bases SQLite sous Windows).
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "support/tmp.rs"]
mod tmp;

use std::fs::OpenOptions;
use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[test]
fn a_test_directory_lives_under_the_known_root_and_is_removed_with_its_contents() {
    let dir = tmp::tempdir().unwrap();
    let path = dir.path().to_path_buf();
    assert!(path.starts_with(tmp::root()), "{}", path.display());
    std::fs::create_dir_all(path.join("a").join("b")).unwrap();
    std::fs::write(path.join("a").join("b").join("x"), b"x").unwrap();

    drop(dir);

    assert!(!path.exists());
}

#[test]
fn a_failing_test_still_removes_its_directory() {
    let path = std::sync::Mutex::new(None);
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let dir = tmp::tempdir().unwrap();
        *path.lock().unwrap() = Some(dir.path().to_path_buf());
        std::fs::write(dir.path().join("x"), b"x").unwrap();
        panic!("le test échoue");
    }));
    assert!(outcome.is_err());
    let path = path.lock().unwrap().take().unwrap();
    assert!(!path.exists(), "{}", path.display());
}

/// Un fichier ouvert SANS partage de suppression (comme SQLite sous Windows) : la suppression au
/// `Drop` échoue ; le dossier est supprimé quand le fil du test sort, fichier refermé.
#[test]
fn a_directory_with_an_open_file_is_removed_when_the_test_thread_exits() {
    let path = std::thread::spawn(|| {
        let dir = tmp::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let mut options = OpenOptions::new();
        options.create(true).write(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            // FILE_SHARE_READ | FILE_SHARE_WRITE : pas FILE_SHARE_DELETE.
            options.share_mode(1 | 2);
        }
        let mut file = options.open(path.join("hearth.db")).unwrap();
        file.write_all(b"x").unwrap();
        // Le dossier est détruit AVANT le fichier, comme un banc d'essai dont la base reste ouverte.
        drop(dir);
        drop(file);
        path
    })
    .join()
    .unwrap();

    assert!(!path.exists(), "{}", path.display());
}

#[test]
fn a_leftover_directory_is_named_after_the_test_that_created_it() {
    let dir = tmp::tempdir().unwrap();
    let name = dir
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(
        name.starts_with("a_leftover_directory_is_named_after_the_test_that_created_it."),
        "{name}"
    );
}
