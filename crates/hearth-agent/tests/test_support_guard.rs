//! Garde de compilation de la porte des bancs d'essai (HRT-18 tranche 3, BR-TRUST-045) : un binaire de
//! production ne peut pas contenir `accept_unconfirmed_acts_for_tests`.
//!
//! - la méthode est derrière la fonction cargo `test-support` (jamais par défaut, demandée par les seuls
//!   dev-dependencies) ;
//! - aucun fichier de `src/` ne nomme la méthode hors de sa définition gardée, d'un commentaire ou d'une
//!   chaîne de documentation : un appel dans le code de production fait échouer ce test ;
//! - le binaire de publication est construit SANS cette fonction (`cargo xtask agent` ne demande aucune
//!   fonction) et le scénario de bout en bout `e2e-update` prouve qu'il exige : un acte sans `reauth` y
//!   reçoit `426`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

const NAME: &str = "accept_unconfirmed_acts_for_tests";
const GATE: &str = "#[cfg(feature = \"test-support\")]";

fn walk(dir: &Path, hits: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            walk(&path, hits);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let text = std::fs::read_to_string(&path).unwrap();
            let lines: Vec<&str> = text.lines().collect();
            for (index, line) in lines.iter().enumerate() {
                if !line.contains(NAME) || line.trim_start().starts_with("//") {
                    continue;
                }
                let gated = index > 0 && lines[index - 1].trim() == GATE;
                let definition = line.contains("pub fn ");
                if !(gated && definition) {
                    hits.push(format!("{}:{}", path.display(), index + 1));
                }
            }
        }
    }
}

#[test]
fn the_test_only_door_is_named_nowhere_in_src_outside_its_gated_definition_and_comments() {
    let mut hits = Vec::new();
    walk(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut hits,
    );
    assert!(
        hits.is_empty(),
        "`{NAME}` ne se nomme dans src/ que dans sa définition derrière `{GATE}` : {hits:?}"
    );
}

#[test]
fn the_test_support_feature_is_never_on_by_default_and_only_dev_dependencies_ask_for_it() {
    let manifest =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    let features = manifest
        .split("[features]")
        .nth(1)
        .and_then(|rest| rest.split("\n[").next())
        .unwrap();
    assert!(features.contains("test-support = []"), "{features}");
    assert!(!features.contains("default"), "{features}");
    // Dans la section [dependencies] (hors dev-dependencies), aucune demande de la fonction.
    let dependencies = manifest
        .split("[dependencies]")
        .nth(1)
        .and_then(|rest| rest.split("\n[").next())
        .unwrap();
    assert!(!dependencies.contains("test-support"), "{dependencies}");
}
