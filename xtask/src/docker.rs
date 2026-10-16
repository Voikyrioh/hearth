//! Lancement de `docker` : toujours avec une liste d'arguments.

use std::path::Path;
use std::process::{Command, Stdio};

/// Racine du dépôt : définie une seule fois, dans `repo_root.rs`.
pub use crate::repo_root::repo_root;

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
