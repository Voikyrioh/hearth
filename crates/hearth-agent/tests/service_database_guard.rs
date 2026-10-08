//! Garde (HRT-18, suites de la review de la PR #42) : le service n'obtient sa base que par
//! `Database::open_for_service`, qui reprend l'effacement physique des anciennes empreintes. Le câblage
//! (`app::start_*`) n'accepte que `ServiceDatabase`, que seule cette porte et `ServiceDatabase::adopt` (bancs
//! d'essai) construisent : ce test échoue si le code de production prend la porte des bancs d'essai.

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

#[test]
fn production_code_never_adopts_a_database_it_opened_without_the_erasure_retry() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    assert!(files.len() > 50, "la garde lit bien le code");
    let mut offenders = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        // La définition elle-même et ses commentaires de doc sont dans `sqlite/mod.rs`.
        let inside_definition = file.ends_with("sqlite/mod.rs");
        for (number, line) in text.lines().enumerate() {
            if line.contains("ServiceDatabase::adopt(") && !inside_definition {
                offenders.push(format!("{}:{}", file.display(), number + 1));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "adopt() en production : {offenders:?}"
    );
}

#[test]
fn the_service_entry_point_opens_its_database_through_the_service_door() {
    let app =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/app.rs")).unwrap();
    let start = app.find("pub async fn start(config").expect("`app::start`");
    let body: String = app[start..].lines().take(6).collect::<Vec<_>>().join("\n");
    assert!(body.contains("Database::open_for_service("), "{body}");
    assert!(!body.contains("Database::open("), "{body}");
}
