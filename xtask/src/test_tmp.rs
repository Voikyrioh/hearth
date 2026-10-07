//! `cargo xtask test-tmp-check [--expect-tests] [--clean]` : la suite de tests n'a laissé aucun
//! dossier temporaire derrière elle (HRT-18, T43).
//!
//! Les tests créent leurs dossiers sous `<target>/hearth-test-tmp/` (et, pour la mort simulée sous
//! Linux, `/dev/shm/hearth-test-tmp/`) avec l'aide `crates/hearth-agent/tests/support/tmp.rs`, et les
//! suppriment. Les racines sont trouvées par le MÊME code que celui de l'aide (`tmp_root.rs`, repris
//! par `#[path]`), depuis l'exécutable de `xtask`, qui vit dans le même `target` que les tests : peu
//! importe le dossier courant ou un dossier de cibles réglé ailleurs. Chaque dossier est préfixé du
//! nom du test qui l'a créé : le rapport dit qui fuit.
//!
//! - sans option : échoue si une racine contient des dossiers ;
//! - `--expect-tests` : échoue aussi si la racine du `target` n'existe pas (l'aide la crée au premier
//!   dossier : absente, aucun test n'a tourné, et un « rien n'est resté » serait un faux vert) ;
//! - `--clean` : supprime les restes (à la main sur un poste ; en CI la machine est neuve).

use std::path::{Path, PathBuf};

#[path = "../../crates/hearth-agent/tests/support/tmp_root.rs"]
mod tmp_root;

/// Les racines surveillées, vues depuis l'exécutable `exe`.
fn roots(exe: &Path) -> Vec<PathBuf> {
    tmp_root::roots_for(exe)
}

/// Sous-dossiers laissés dans `root` (noms triés) ; `None` si la racine n'existe pas.
fn leftovers(root: &Path) -> Result<Option<Vec<String>>, String> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("{} illisible : {error}", root.display())),
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    Ok(Some(names))
}

/// Le verdict sur `roots` : `Ok(message)` ou `Err(raison)`. La première racine est celle du
/// `target` : avec `expect_tests`, elle doit exister.
fn check(roots: &[PathBuf], expect_tests: bool, clean: bool) -> Result<String, String> {
    let mut report = Vec::new();
    for (at, root) in roots.iter().enumerate() {
        let Some(left) = leftovers(root)? else {
            if at == 0 && expect_tests {
                return Err(format!(
                    "{} n'existe pas : aucun test n'a créé de dossier, la vérification ne prouverait rien \
                     (les tests ont-ils tourné avec ce `target` ?)",
                    root.display()
                ));
            }
            continue;
        };
        if left.is_empty() {
            continue;
        }
        if clean {
            for name in &left {
                let path = root.join(name);
                std::fs::remove_dir_all(&path)
                    .map_err(|error| format!("{} : {error}", path.display()))?;
            }
            continue;
        }
        let shown: Vec<&str> = left.iter().take(10).map(String::as_str).collect();
        report.push(format!(
            "{} dossier(s) dans {} (dix premiers, préfixés du test qui les a créés : {})",
            left.len(),
            root.display(),
            shown.join(", ")
        ));
    }
    if report.is_empty() {
        return Ok("aucun dossier laissé par les tests".to_owned());
    }
    Err(format!(
        "la suite a laissé {} ; un test garde un fichier ouvert ou ne nettoie pas ; \
         `cargo xtask test-tmp-check --clean` les supprime",
        report.join(" ; ")
    ))
}

pub fn run(args: &[String]) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|error| format!("exécutable de xtask : {error}"))?;
    let verdict = check(
        &roots(&exe),
        args.iter().any(|arg| arg == "--expect-tests"),
        args.iter().any(|arg| arg == "--clean"),
    )?;
    println!("test-tmp-check : {verdict}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hearth-xtask-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_roots_are_those_the_tests_use() {
        // Un exécutable de test sous un `target` : la même racine que l'aide des tests.
        let target = scratch("target");
        let exe = target.join("debug").join("deps").join("x.exe");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(target.join(".rustc_info.json"), "{}").unwrap();
        assert_eq!(roots(&exe)[0], target.join(tmp_root::ROOT_NAME));
        let _ = std::fs::remove_dir_all(&target);
    }

    #[test]
    fn a_directory_left_on_purpose_fails_the_check_and_names_the_test() {
        let root = scratch("left");
        std::fs::create_dir_all(root.join("the_test_that_leaks.AbC123")).unwrap();
        let error = check(std::slice::from_ref(&root), false, false).unwrap_err();
        assert!(error.contains("the_test_that_leaks.AbC123"), "{error}");
        // `--clean` les retire, puis la vérification passe.
        check(std::slice::from_ref(&root), false, true).unwrap();
        assert_eq!(leftovers(&root).unwrap(), Some(Vec::new()));
        check(std::slice::from_ref(&root), true, false).unwrap();
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_root_passes_only_when_no_test_was_expected() {
        let root = scratch("missing");
        assert!(check(std::slice::from_ref(&root), false, false).is_ok());
        let error = check(std::slice::from_ref(&root), true, false).unwrap_err();
        assert!(error.contains("aucun test n'a créé de dossier"), "{error}");
    }
}
