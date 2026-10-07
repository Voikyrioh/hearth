//! `cargo xtask test-tmp-check` : la suite de tests n'a laissé aucun dossier temporaire derrière elle
//! (HRT-18, T43).
//!
//! Les tests créent leurs dossiers sous `<target>/hearth-test-tmp/` (et, pour la mort simulée sous
//! Linux, `/dev/shm/hearth-test-tmp/`) avec l'aide `crates/hearth-agent/tests/support/tmp.rs`, et les
//! suppriment. Après une suite complète, ces racines doivent être vides : sinon un test a gardé un
//! fichier ouvert (base SQLite) ou oublié de nettoyer, et la commande échoue en nommant les dossiers.
//! `--clean` les supprime (à lancer à la main sur un poste ; en CI la machine est neuve).

use std::path::{Path, PathBuf};

/// Même nom que `ROOT_NAME` dans `crates/hearth-agent/tests/support/tmp.rs`.
const ROOT_NAME: &str = "hearth-test-tmp";

/// Les racines surveillées.
fn roots() -> Vec<PathBuf> {
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| PathBuf::from("target"), PathBuf::from);
    vec![
        target.join(ROOT_NAME),
        Path::new("/dev/shm").join(ROOT_NAME),
    ]
}

/// Sous-dossiers laissés dans `root` (noms triés).
fn leftovers(root: &Path) -> Result<Vec<String>, String> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        // Jamais créée : aucun test n'a tourné là, donc rien n'a fui.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("{} illisible : {error}", root.display())),
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    Ok(names)
}

pub fn run(args: &[String]) -> Result<(), String> {
    let clean = args.iter().any(|arg| arg == "--clean");
    let mut report = Vec::new();
    for root in roots() {
        let left = leftovers(&root)?;
        if left.is_empty() {
            continue;
        }
        if clean {
            std::fs::remove_dir_all(&root)
                .map_err(|error| format!("{} : {error}", root.display()))?;
            println!(
                "test-tmp-check : {} dossiers supprimés dans {}",
                left.len(),
                root.display()
            );
            continue;
        }
        let shown: Vec<&str> = left.iter().take(10).map(String::as_str).collect();
        report.push(format!(
            "{} dossier(s) dans {} (dix premiers : {})",
            left.len(),
            root.display(),
            shown.join(", ")
        ));
    }
    if report.is_empty() {
        println!("test-tmp-check : aucun dossier laissé par les tests");
        return Ok(());
    }
    Err(format!(
        "la suite a laissé {} ; un test garde un fichier ouvert ou ne nettoie pas ; \
         `cargo xtask test-tmp-check --clean` les supprime",
        report.join(" ; ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_root_is_empty() {
        let dir = std::env::temp_dir().join("hearth-xtask-no-such-root");
        assert_eq!(leftovers(&dir), Ok(Vec::new()));
    }
}
