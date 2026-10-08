//! Garde de la confirmation des actes (HRT-18 tranches 3 à 7, BR-TRUST-045) : aucun moyen, même en test,
//! d'agir sans confirmation.
//!
//! - **le moyen n'existe plus** : ni la porte des bancs (`accept_unconfirmed_acts_for_tests`), ni le marqueur
//!   `Reauthenticated::unconfirmed`, ni le drapeau `reauth_required`, ni la fonction cargo `test-support` de
//!   l'agent ne figurent dans `src/` ni dans un manifeste du workspace (le test lit tous les membres) ;
//! - **le handler de chaque route d'acte prend `Extension<Reauthenticated>`** (jamais optionnelle), dans sa
//!   signature : sans la confirmation posée par la couche, il ne s'exécute pas ;
//! - **comportement** : le scénario `e2e-update` prouve que le binaire de publication exige (un acte sans
//!   `reauth` y reçoit `426`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

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

fn is_comment(line: &str) -> bool {
    line.trim_start().starts_with("//")
}

#[test]
fn nothing_in_src_can_act_without_the_confirmation() {
    let mut files = Vec::new();
    rust_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    let mut hits = Vec::new();
    for path in files {
        let text = std::fs::read_to_string(&path).unwrap();
        for (index, line) in text.lines().enumerate() {
            if is_comment(line) {
                continue;
            }
            for name in [
                "accept_unconfirmed_acts_for_tests",
                "Reauthenticated::unconfirmed",
                "fn unconfirmed",
                "reauth_required",
                "feature = \"test-support\"",
            ] {
                if line.contains(name) {
                    hits.push(format!("{}:{} {name}", path.display(), index + 1));
                }
            }
        }
    }
    assert!(hits.is_empty(), "moyen d'agir sans confirmation : {hits:?}");
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
fn no_manifest_of_the_workspace_asks_the_agent_for_a_test_door() {
    let members = members();
    assert!(members.len() >= 5, "membres lus : {members:?}");
    let mut asking = Vec::new();
    for member in members {
        let manifest = std::fs::read_to_string(member.join("Cargo.toml")).unwrap();
        for line in manifest.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with('#')
                && trimmed.contains("hearth-agent")
                && trimmed.contains("test-support")
            {
                asking.push(format!("{} : {trimmed}", member.display()));
            }
        }
    }
    assert!(
        asking.is_empty(),
        "fonction `test-support` demandée à l'agent : {asking:?}"
    );
    let agent =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    assert!(
        !agent.contains("test-support"),
        "l'agent n'a plus cette fonction"
    );
}

/// Défense en profondeur (HRT-18 tranche 5) : la couche `reauth` n'est pas la seule garde d'un acte. Le
/// handler de CHAQUE route de la liste fermée des actes (`hearth_proto::admin_act::ROUTES`) prend
/// `Extension<Reauthenticated>` (non optionnelle) dans sa signature : sans la confirmation posée par la couche,
/// il ne s'exécute pas.
#[test]
fn every_act_route_handler_takes_the_reauthenticated_extension_in_its_signature() {
    let http = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/entrypoint/http");
    let table = std::fs::read_to_string(http.join("mod.rs")).unwrap();
    let mut checked = 0;
    // Le retrait d'un poste garde son contrat livré (`0x04`) et sa propre garde : hors de cette liste.
    let acts = || {
        hearth_proto::admin_act::ROUTES
            .iter()
            .filter(|route| route.contract == hearth_proto::admin_act::Contract::Reauth)
    };
    for route in acts() {
        // Le bloc `Endpoint { … }` de cette route (méthode puis chemin).
        let block = table
            .split("Endpoint {")
            .find(|block| {
                block.contains(&format!("path: \"{}\"", route.pattern))
                    && block.contains(&format!("method: Method::{}", route.method))
            })
            .unwrap_or_else(|| panic!("route absente de ENDPOINTS : {route:?}"));
        let handler = block
            .split("route: || ")
            .nth(1)
            .and_then(|rest| rest.split('(').nth(1))
            .and_then(|rest| rest.split(')').next())
            .unwrap_or_else(|| panic!("handler illisible : {route:?}"));
        let (module, name) = handler.split_once("::").unwrap();
        let source = std::fs::read_to_string(http.join(format!("{module}.rs"))).unwrap();
        let signature = source
            .split(&format!("pub async fn {name}("))
            .nth(1)
            .and_then(|rest| rest.split(") ->").next())
            .unwrap_or_else(|| panic!("signature introuvable : {handler}"));
        // Jamais optionnelle : `Option<Extension<Reauthenticated>>` est la forme du défaut corrigé (un handler
        // qui reçoit l'option et ne la regarde pas).
        assert!(
            !signature.contains("Option<Extension<Reauthenticated")
                && !signature
                    .contains("Option<Extension<crate::application::sessions::Reauthenticated"),
            "le handler {handler} prend l'extension en option : {signature}"
        );
        assert!(
            signature.contains("Extension<") && signature.contains("Reauthenticated>"),
            "le handler {handler} ({} {}) ne prend pas `Extension<Reauthenticated>` : {signature}",
            route.method,
            route.pattern
        );
        checked += 1;
    }
    assert_eq!(checked, acts().count());
    assert!(checked >= 9, "la liste des actes est lue en entier");
}
