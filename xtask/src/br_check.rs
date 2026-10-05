//! `cargo xtask br-check` : toute référence `BR-DOMAINE-NNN` écrite dans le code, les docs
//! d'architecture, d'ADR et d'API doit avoir sa fiche `docs/business-rules/BR-DOMAINE-NNN-*.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Dossiers et fichiers où chercher des références.
const SCANNED: [&str; 6] = [
    "crates",
    "apps/desktop/src",
    "apps/desktop/src-tauri/src",
    "docs/adr",
    "docs/open-api",
    "ARCHITECTURE.md",
];

pub fn run() -> ExitCode {
    let root = PathBuf::from(".");
    let fiches = fiches(&root.join("docs/business-rules"));
    let mut missing: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for entry in SCANNED {
        visit(&root.join(entry), &mut |path, text| {
            for id in references(text) {
                if !fiches.contains(&id) {
                    missing
                        .entry(id)
                        .or_default()
                        .insert(path.display().to_string());
                }
            }
        });
    }
    if missing.is_empty() {
        println!(
            "br-check : {} fiches, aucune référence orpheline",
            fiches.len()
        );
        return ExitCode::SUCCESS;
    }
    for (id, files) in &missing {
        eprintln!(
            "{id} : pas de fiche (cité dans {})",
            files.iter().cloned().collect::<Vec<_>>().join(", ")
        );
    }
    ExitCode::FAILURE
}

/// Identifiants des fiches présentes.
fn fiches(dir: &Path) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(id) = references(&name).into_iter().next() {
                found.insert(id);
            }
        }
    }
    found
}

/// Les `BR-DOMAINE-NNN` d'un texte (domaine en majuscules, trois chiffres).
pub fn references(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(offset) = text[from..].find("BR-") {
        let start = from + offset;
        let mut i = start + 3;
        while i < bytes.len() && bytes[i].is_ascii_uppercase() {
            i += 1;
        }
        let domain_len = i - (start + 3);
        if domain_len > 0 && bytes.get(i) == Some(&b'-') {
            let digits = bytes[i + 1..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .count();
            if digits >= 3 {
                found.push(text[start..i + 4].to_owned());
            }
        }
        from = start + 3;
    }
    found
}

fn visit(path: &Path, on_file: &mut dyn FnMut(&Path, &str)) {
    if path.is_file() {
        if let Ok(text) = std::fs::read_to_string(path) {
            on_file(path, &text);
        }
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if matches!(name.as_str(), "target" | "node_modules" | "dist" | ".git") {
            continue;
        }
        visit(&entry.path(), on_file);
    }
}

#[cfg(test)]
mod tests {
    use super::references;

    #[test]
    fn references_are_found_with_their_three_digits() {
        assert_eq!(
            references(
                "voir BR-RESIL-012 et BR-CONN-001, pas BR-X-1 ni BR-RESIL-0 ni BRX-CONN-001"
            ),
            ["BR-RESIL-012", "BR-CONN-001"]
        );
        assert!(references("BR-resil-012 BR--012").is_empty());
    }
}
