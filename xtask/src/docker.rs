//! Lancement de `docker` : toujours avec une liste d'arguments.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Racine du dépôt (le dossier qui contient `xtask`).
pub fn repo_root() -> Result<PathBuf, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "racine du dépôt introuvable".to_owned())
}

/// Chemin d'un montage : `docker --mount` n'aime pas le préfixe `\\?\` de Windows.
pub fn mount_path(path: &Path) -> String {
    let text = path.to_string_lossy().into_owned();
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
}

/// Lance `docker` avec ces arguments, sortie reprise telle quelle ; erreur si le code de sortie
/// n'est pas nul.
pub fn run(args: &[String]) -> Result<(), String> {
    let status = Command::new("docker")
        .args(args)
        .stdin(Stdio::null())
        .status()
        .map_err(|error| {
            format!(
                "lancement de docker impossible : {error} (Docker est-il installé et démarré ?)"
            )
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "docker {} a échoué ({status})",
            args.first().map_or("", String::as_str)
        ))
    }
}

pub fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}
