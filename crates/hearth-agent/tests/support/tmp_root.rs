//! Où sont les dossiers temporaires des tests : UNE seule source, lue par l'aide des tests
//! (`tmp.rs`) et par le garde-fou (`xtask/src/test_tmp.rs`), pour qu'ils ne cherchent jamais à deux
//! endroits différents (HRT-18, T43).

use std::path::{Path, PathBuf};

/// Nom du dossier racine, sous le dossier `target` de cargo.
pub const ROOT_NAME: &str = "hearth-test-tmp";

/// Le dossier `target` de cargo : le premier ancêtre de `exe` (un exécutable construit par cargo,
/// donc sous `target/<profil>/…`) qui porte `CACHEDIR.TAG` ou `.rustc_info.json`.
pub fn target_dir_of(exe: &Path) -> Option<PathBuf> {
    exe.ancestors()
        .skip(1)
        .find(|dir| dir.join("CACHEDIR.TAG").is_file() || dir.join(".rustc_info.json").is_file())
        .map(Path::to_path_buf)
}

/// Les racines où un test a pu créer ses dossiers, vues depuis l'exécutable `exe` : celle du `target`
/// (ou, à défaut de `target` repérable, le repli `%TEMP%` de l'aide), puis la mémoire partagée Linux.
pub fn roots_for(exe: &Path) -> Vec<PathBuf> {
    let mut roots = vec![
        target_dir_of(exe)
            .unwrap_or_else(std::env::temp_dir)
            .join(ROOT_NAME),
    ];
    // Le repli de l'aide, surveillé aussi quand `target` est repérable.
    roots.push(std::env::temp_dir().join(ROOT_NAME));
    roots.push(Path::new("/dev/shm").join(ROOT_NAME));
    roots.dedup();
    roots
}
