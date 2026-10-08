//! Garde du banc de la liaison (HRT-18 tranche 5, BR-TRUST-045) : le banc EXIGE la confirmation des actes
//! par défaut, comme l'agent en production. Seuls les scénarios qui envoient volontairement une route
//! d'acte brute à un VRAI agent (`execute_raw`, pour éprouver le transport coupé) le baissent, par
//! `Options::accepting_bare_acts()`. La liste des fichiers autorisés, avec leur nombre d'appels, est
//! fermée : un nouvel usager (ou un usager de plus) fait échouer cette garde, à décider en revue.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

/// Fichier de test -> nombre d'appels à `accepting_bare_acts()`.
const ALLOWED: &[(&str, usize)] = &[
    // 9 scénarios de résilience (action coupée, interrompue, abandonnée, redémarrage...).
    ("fault_proxy.rs", 9),
    // 5 scénarios de fin de session, tous par `world()`.
    ("session_end_actions.rs", 1),
    // Un administrateur autre que le poste teste lance la mise à jour par une session brute.
    ("agent_update.rs", 1),
];

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name != "support") {
                rust_files(&path, out);
            }
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

fn count(path: &Path) -> usize {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .map(|line| {
            line.matches("accepting_bare_acts(").count()
                + line.matches("accept_bare_acts(").count()
                + line.matches("bare_acts:").count()
                + line.matches("accept_unconfirmed_acts_for_tests").count()
        })
        .sum()
}

#[test]
fn only_the_listed_files_lower_the_bench_and_each_as_many_times_as_listed() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&manifest.join("tests"), &mut files);
    rust_files(
        &manifest.join("../../apps/desktop/src-tauri/tests"),
        &mut files,
    );
    let mut found = Vec::new();
    for path in files {
        if path
            .file_name()
            .is_some_and(|name| name == "bench_guard.rs")
        {
            continue;
        }
        let uses = count(&path);
        if uses > 0 {
            found.push((
                path.file_name().unwrap().to_string_lossy().into_owned(),
                uses,
            ));
        }
    }
    found.sort();
    let mut expected: Vec<(String, usize)> = ALLOWED
        .iter()
        .map(|(name, uses)| ((*name).to_owned(), *uses))
        .collect();
    expected.sort();
    assert_eq!(found, expected, "usagers de la porte brute du banc");
}

#[test]
fn the_bench_requires_by_default() {
    let support =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/mod.rs"))
            .unwrap();
    assert!(
        support.contains("bare_acts: false,"),
        "le défaut du banc est d'exiger"
    );
    let agent = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/agent.rs"),
    )
    .unwrap();
    assert!(agent.contains("bare_acts: false,"));
}
