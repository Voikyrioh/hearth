//! Recompile quand une migration change (`sqlx::migrate!` les embarque à la compilation), et
//! embarque la clé publique de signature des mises à jour (ADR-0008, ADR-0014) :
//! `update-key.pub`, ou le fichier désigné par `HEARTH_UPDATE_PUBKEY_FILE` (la clé jetable des
//! tests de bout en bout ; une construction de publication ne le définit pas).

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=migrations");
    println!("cargo:rerun-if-changed=update-key.pub");
    println!("cargo:rerun-if-env-changed=HEARTH_UPDATE_PUBKEY_FILE");
    println!("cargo:rerun-if-env-changed=HEARTH_AGENT_VERSION");
    println!("cargo:rerun-if-env-changed=HEARTH_UPDATE_ALLOW_LOCAL_ADDRESSES");

    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default());
    let source = match std::env::var_os("HEARTH_UPDATE_PUBKEY_FILE") {
        Some(path) if !path.is_empty() => {
            let path = PathBuf::from(path);
            println!("cargo:rerun-if-changed={}", path.display());
            path
        }
        _ => manifest.join("update-key.pub"),
    };
    let key = std::fs::read_to_string(&source).unwrap_or_else(|error| {
        panic!(
            "clé publique de mise à jour {} illisible : {error}",
            source.display()
        )
    });
    // Ce qui rend cette construction différente d'une construction de publication : dit au
    // démarrage de l'agent et par `hearth-agent build-info` (une variable exportée dans le terminal
    // de qui construit donne un agent ouvert : cela doit se voir).
    let mut notes = Vec::new();
    if std::env::var_os("HEARTH_UPDATE_PUBKEY_FILE").is_some_and(|v| !v.is_empty()) {
        notes.push("clé de mise à jour de test");
    }
    if std::env::var_os("HEARTH_UPDATE_ALLOW_LOCAL_ADDRESSES").is_some() {
        notes.push("téléchargements vers des adresses locales permis");
    }
    if std::env::var_os("HEARTH_AGENT_VERSION").is_some() {
        notes.push("version imposée à la construction");
    }
    println!("cargo:rustc-env=HEARTH_BUILD_NOTES={}", notes.join(", "));
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_default());
    std::fs::write(out.join("update-key.pub"), key).unwrap_or_else(|error| {
        panic!("clé publique non copiée dans OUT_DIR : {error}");
    });
}
