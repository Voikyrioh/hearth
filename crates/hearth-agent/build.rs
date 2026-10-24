//! Recompile quand une migration change (`sqlx::migrate!` les embarque à la compilation), et
//! embarque la clé publique de signature des mises à jour (ADR-0008, ADR-0014) :
//! `update-key.pub`, ou le fichier désigné par `HEARTH_UPDATE_PUBKEY_FILE` (la clé jetable des
//! tests de bout en bout ; une construction de publication ne le définit pas).
//!
//! Embarque aussi les fiches de serveur de jeu (`catalog/<id>/fiche.toml` et `settings.toml`, HRT-113,
//! ADR-0050) : chacune passe par le chargeur RÉEL (le même code que l'agent, inclus par `#[path]`), et une fiche
//! que le chargeur refuse fait échouer la compilation, avec un message qui nomme la fiche, le fichier, l'invariant
//! et le motif. La table des fiches embarquées et leur empreinte sont écrites dans `OUT_DIR`.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Le chargeur de fiches, tel quel : autonome (`super::` seulement), donc incluable ici.
#[path = "src/domain/catalog/loading/mod.rs"]
#[allow(dead_code, unused_imports)]
mod loading;

fn main() {
    println!("cargo:rerun-if-changed=migrations");
    println!("cargo:rerun-if-changed=update-key.pub");
    // Les fiches : le dossier entier (une fiche ajoutée, retirée ou modifiée relance ce script ; les autres
    // compilations ne le relancent pas). Ses fichiers de construction (ids.lock, golden.toml...) le relancent
    // aussi, ce qui est rare et sans coût pour une compilation ordinaire.
    println!("cargo:rerun-if-changed=catalog");
    println!("cargo:rerun-if-env-changed=HEARTH_UPDATE_PUBKEY_FILE");
    println!("cargo:rerun-if-env-changed=HEARTH_AGENT_VERSION");
    println!("cargo:rerun-if-env-changed=HEARTH_UPDATE_ALLOW_LOCAL_ADDRESSES");
    println!("cargo:rerun-if-env-changed=HEARTH_PREVIEW_FICHES");

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
    if std::env::var_os("HEARTH_PREVIEW_FICHES").is_some() {
        notes.push("fiches d'essai incluses");
    }
    if std::env::var_os("HEARTH_AGENT_VERSION").is_some() {
        notes.push("version imposée à la construction");
    }
    println!("cargo:rustc-env=HEARTH_BUILD_NOTES={}", notes.join(", "));
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_default());
    std::fs::write(out.join("update-key.pub"), key).unwrap_or_else(|error| {
        panic!("clé publique non copiée dans OUT_DIR : {error}");
    });
    embed_fiches(&manifest, &out);
}

/// Message d'échec de compilation : sans trace de pile, avec le nom de la fiche.
fn fail(message: &str) -> ! {
    eprintln!("error: {message}");
    std::process::exit(1);
}

fn embed_fiches(manifest: &Path, out: &Path) {
    let catalog = manifest.join("catalog");
    let mut dirs: Vec<String> = match std::fs::read_dir(&catalog) {
        Ok(entries) => entries
            .flatten()
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect(),
        Err(_) => Vec::new(),
    };
    dirs.sort();
    let mut table = String::from("pub const EMBEDDED: &[EmbeddedFiche] = &[\n");
    for dir in &dirs {
        let fiche_path = catalog.join(dir).join("fiche.toml");
        let settings_path = catalog.join(dir).join("settings.toml");
        let read = |path: &Path| {
            std::fs::read(path).unwrap_or_else(|error| {
                fail(&format!(
                    "fiche `{dir}` : {} illisible : {error}",
                    path.display()
                ))
            })
        };
        let fiche_bytes = read(&fiche_path);
        let settings_bytes = read(&settings_path);
        let fiche = loading::loader::load(
            loading::loader::Origin::Embedded,
            &fiche_bytes,
            &settings_bytes,
        )
        .unwrap_or_else(|refusal| fail(&refusal.message_pour(dir)));
        if fiche.id() != dir {
            fail(&format!(
                "fiche `{dir}` refusée : son identifiant `{}` doit être le nom de son dossier",
                fiche.id()
            ));
        }
        let digest: Vec<String> = fiche.digest.iter().map(|b| format!("{b:#04x}")).collect();
        let _ = writeln!(
            table,
            "    EmbeddedFiche {{ dir: {dir:?}, fiche_toml: include_bytes!({fiche:?}), settings_toml: include_bytes!({settings:?}), digest: [{digest}] }},",
            fiche = fiche_path.to_string_lossy(),
            settings = settings_path.to_string_lossy(),
            digest = digest.join(", "),
        );
    }
    table.push_str("];\n");
    std::fs::write(out.join("embedded_fiches.rs"), table).unwrap_or_else(|error| {
        fail(&format!(
            "table des fiches non écrite dans OUT_DIR : {error}"
        ));
    });
}
