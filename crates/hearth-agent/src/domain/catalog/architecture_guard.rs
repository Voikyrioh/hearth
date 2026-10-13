//! Garde d'architecture du domaine des fiches (invariants I4 et I5, conception technique 6) : le dossier n'a
//! aucune E/S, aucun processus, aucun réseau, aucun `async`, aucun tirage aléatoire ; et le port du moteur n'a
//! aucune fonction qui exécute une commande dans un conteneur. Ces tests lisent le TEXTE des sources.

use std::path::{Path, PathBuf};

/// Ce qui ne doit jamais apparaître dans le code de production de `domain/catalog`.
const FORBIDDEN: &[&str] = &[
    "std::fs",
    "std::process",
    "std::net",
    "std::env",
    "std::thread",
    "tokio",
    "reqwest",
    "hyper",
    "bollard",
    "sqlx",
    "axum",
    "rustix",
    "libc",
    "Command::new",
    "TcpStream",
    "UdpSocket",
    "async fn",
    ".await",
    "getrandom",
    "ring::rand",
    "rand::",
    "tempfile",
    "include_str!",
    "File::open",
];

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Le code de production : tout avant le module de tests du fichier (`#[cfg(test)]` suivi de `mod tests`).
fn production_part(text: &str) -> &str {
    text.split("#[cfg(test)]\nmod tests").next().unwrap_or(text)
}

fn is_test_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.starts_with("tests_")
        || matches!(
            name,
            "test_support.rs" | "contract.rs" | "architecture_guard.rs"
        )
}

#[test]
fn i4_i5_the_catalog_domain_has_no_io_no_process_no_network_no_async() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/domain/catalog");
    let mut files = Vec::new();
    sources(&root, &mut files);
    assert!(
        files.len() > 15,
        "le dossier est lu ({} fichiers)",
        files.len()
    );
    let mut offences = Vec::new();
    for path in files.iter().filter(|p| !is_test_file(p)) {
        let text = std::fs::read_to_string(path).expect("lisible");
        let production = production_part(&text);
        for token in FORBIDDEN {
            // `embedded.rs` inclut la table engendrée par `build.rs` ; c'est la seule `include!` permise.
            if production.contains(token) {
                offences.push(format!("{} contient {token}", path.display()));
            }
        }
    }
    assert!(offences.is_empty(), "{offences:#?}");
}

#[test]
fn i4_the_loading_directory_is_self_contained_because_build_rs_and_xtask_include_it() {
    // `build.rs` et `xtask` incluent `loading/` par `#[path]` : aucun `crate::` ni `hearth_agent::` dedans.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/domain/catalog/loading");
    let mut files = Vec::new();
    sources(&root, &mut files);
    assert!(files.len() >= 9);
    let mut offences = Vec::new();
    for path in &files {
        let text = std::fs::read_to_string(path).expect("lisible");
        let production = production_part(&text);
        let code: String = production
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for token in ["crate::", "hearth_agent::", "hearth_proto::"] {
            if code.contains(token) {
                offences.push(format!("{} contient {token}", path.display()));
            }
        }
    }
    assert!(offences.is_empty(), "{offences:#?}");
}

#[test]
fn i4_the_engine_port_has_no_function_that_runs_a_command_in_a_container() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/application/ports/container_engine.rs");
    let text = std::fs::read_to_string(path).expect("le port du moteur est lisible");
    for line in text
        .lines()
        .filter(|l| l.trim_start().starts_with("fn ") || l.trim_start().starts_with("async fn "))
    {
        let name = line
            .split("fn ")
            .nth(1)
            .unwrap_or("")
            .split(['(', '<'])
            .next()
            .unwrap_or("");
        for banned in ["exec", "run_command", "shell", "attach", "command"] {
            assert!(
                !name.contains(banned),
                "le port du moteur ne doit pas exposer `{name}`"
            );
        }
    }
}

#[test]
fn no_regular_expression_or_template_library_is_in_the_agent_dependencies() {
    let manifest =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
            .expect("lisible");
    for banned in [
        "regex",
        "pcre",
        "fancy-regex",
        "handlebars",
        "tera",
        "minijinja",
        "quick-xml",
        "roxmltree",
        "serde_ini",
        "configparser",
    ] {
        assert!(
            !manifest
                .lines()
                .any(|l| l.trim_start().starts_with(&format!("{banned} "))
                    || l.trim_start().starts_with(&format!("{banned}."))),
            "{banned} n'a rien à faire dans l'agent (ni expression régulière, ni gabarit, ni bibliothèque XML ou INI)"
        );
    }
}
