//! Garde de la porte des bancs d'essai (HRT-18 tranches 3 et 4, BR-TRUST-045).
//!
//! `SessionService::accept_unconfirmed_acts_for_tests` fait accepter un acte sans `reauth`. Ce qui est
//! prouvé, et par quoi :
//!
//! - **compilation** : la méthode est derrière la fonction cargo `test-support`, jamais par défaut ; la
//!   deuxième garde lit les manifestes de TOUS les membres du workspace et échoue si l'un d'eux demande la
//!   fonction ailleurs que dans ses `[dev-dependencies]`. Un binaire construit par `cargo build` ne la
//!   contient donc pas (les dev-dependencies n'y sont pas construites) ;
//! - **texte** : aucune ligne de `src/` ne nomme la méthode hors de sa définition gardée ou d'un
//!   commentaire, et le DRAPEAU `reauth_required` n'est écrit qu'à un seul endroit, cette méthode gardée
//!   (le constructeur l'initialise à vrai) : une écriture ajoutée ailleurs fait échouer la garde, sans passer
//!   par le nom ;
//! - **comportement** : le scénario `e2e-update` prouve que le binaire de publication exige (un acte sans
//!   `reauth` y reçoit `426`) ; il ne prouve pas, à lui seul, l'absence du symbole : c'est la compilation
//!   qui la donne.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

const NAME: &str = "accept_unconfirmed_acts_for_tests";
const GATE: &str = "#[cfg(feature = \"test-support\")]";

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

fn sources() -> Vec<(PathBuf, Vec<String>)> {
    let mut files = Vec::new();
    rust_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    files
        .into_iter()
        .map(|path| {
            let lines = std::fs::read_to_string(&path)
                .unwrap()
                .lines()
                .map(str::to_owned)
                .collect();
            (path, lines)
        })
        .collect()
}

fn is_comment(line: &str) -> bool {
    line.trim_start().starts_with("//")
}

#[test]
fn the_test_only_door_is_named_nowhere_in_src_outside_its_gated_definition_and_comments() {
    let mut hits = Vec::new();
    for (path, lines) in sources() {
        for (index, line) in lines.iter().enumerate() {
            if !line.contains(NAME) || is_comment(line) {
                continue;
            }
            let gated = index > 0 && lines[index - 1].trim() == GATE;
            if !(gated && line.contains("pub fn ")) {
                hits.push(format!("{}:{}", path.display(), index + 1));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "`{NAME}` ne se nomme dans src/ que dans sa définition derrière `{GATE}` : {hits:?}"
    );
}

#[test]
fn the_flag_is_written_in_exactly_one_place_and_it_is_the_gated_door() {
    let mut writes = Vec::new();
    for (path, lines) in sources() {
        for (index, line) in lines.iter().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            let writes_flag = code.contains("reauth_required.")
                && ["store(", "swap(", "fetch_", "compare_exchange", "get_mut("]
                    .iter()
                    .any(|op| code.contains(op))
                || code.contains("reauth_required =");
            if writes_flag {
                let gated = lines[index.saturating_sub(4)..index]
                    .iter()
                    .any(|above| above.trim() == GATE);
                writes.push((format!("{}:{}", path.display(), index + 1), gated));
            }
        }
    }
    assert_eq!(
        writes.len(),
        1,
        "une seule écriture du drapeau : {writes:?}"
    );
    assert!(writes[0].1, "elle est derrière `{GATE}` : {writes:?}");
}

/// Les membres du workspace, lus de la racine.
fn members() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    let list = manifest
        .split("members = [")
        .nth(1)
        .and_then(|rest| rest.split(']').next())
        .unwrap();
    list.split(',')
        .map(|member| member.trim().trim_matches('"'))
        .filter(|member| !member.is_empty())
        .map(|member| root.join(member))
        .collect()
}

#[test]
fn only_dev_dependencies_of_the_workspace_members_ask_for_the_test_support_feature() {
    let mut asking = Vec::new();
    let members = members();
    assert!(members.len() >= 5, "membres lus : {members:?}");
    for member in members {
        let manifest = std::fs::read_to_string(member.join("Cargo.toml")).unwrap();
        let mut section = String::new();
        for line in manifest.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') {
                section = trimmed.to_owned();
            } else if trimmed.contains("test-support") && !trimmed.starts_with('#') {
                let dev = section.contains("dev-dependencies");
                let defines = section == "[features]" && trimmed.starts_with("test-support");
                if !dev && !defines {
                    asking.push(format!("{} {section} : {trimmed}", member.display()));
                }
            }
        }
    }
    assert!(
        asking.is_empty(),
        "`test-support` demandée hors des dev-dependencies : {asking:?}"
    );
    // Et elle n'est jamais par défaut.
    let agent =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    let features = agent
        .split("[features]")
        .nth(1)
        .and_then(|rest| rest.split("\n[").next())
        .unwrap();
    assert!(features.contains("test-support = []"), "{features}");
    assert!(!features.contains("default"), "{features}");
    // Le manifeste racine ne l'impose pas non plus aux dépendances du workspace.
    let root =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml"))
            .unwrap();
    assert!(!root.contains("test-support"), "racine du workspace");
}
