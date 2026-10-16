//! La racine du dépôt, en un seul endroit (HRT-173, conception 9.6), et la garde qui la vérifie.
//!
//! La racine est lue à la COMPILATION (`CARGO_MANIFEST_DIR`). Le flux de publication compile `xtask`
//! dans un job et l'exécute dans d'autres : le binaire ne trouve le dépôt que si l'extraction s'est
//! faite au même chemin. En environnement GitHub, `ci-root-check` (et la tête de chaque sous-commande
//! lancée depuis le binaire transmis) refuse tout écart, avec les deux chemins.

use std::path::{Path, PathBuf};

/// Racine du dépôt (le dossier qui contient `xtask`).
pub fn repo_root() -> Result<PathBuf, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "racine du dépôt introuvable".to_owned())
}

/// La racine compilée est-elle l'espace de travail de l'exécution ? `canon` résout les chemins (liens,
/// majuscules, préfixe `\?\` de Windows) ; un chemin qui ne se résout pas est un refus, jamais un accord.
pub fn check_workspace(
    compiled: &Path,
    workspace: Option<&Path>,
    canon: &dyn Fn(&Path) -> Option<PathBuf>,
) -> Result<(), String> {
    let workspace = workspace.ok_or(
        "GITHUB_WORKSPACE est absent ou vide : la racine du dépôt ne peut pas être vérifiée"
            .to_owned(),
    )?;
    let (Some(left), Some(right)) = (canon(compiled), canon(workspace)) else {
        return Err(format!(
            "la racine compilée dans xtask ({}) ou l'espace de travail ({}) est introuvable",
            compiled.display(),
            workspace.display()
        ));
    };
    if left == right {
        Ok(())
    } else {
        Err(format!(
            "la racine du dépôt compilée dans xtask ({}) n'est pas l'espace de travail de l'exécution ({}) : xtask doit être compilé et exécuté au même chemin. Relance la publication depuis le début",
            compiled.display(),
            workspace.display()
        ))
    }
}

/// `GITHUB_ACTIONS=true` seulement : hors GitHub (procédure manuelle, développement), rien à vérifier.
pub fn guard() -> Result<(), String> {
    if !crate::ci::is_github() {
        return Ok(());
    }
    let workspace = std::env::var_os("GITHUB_WORKSPACE")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    check_workspace(&repo_root()?, workspace.as_deref(), &|path| {
        std::fs::canonicalize(path).ok()
    })
}

/// `cargo xtask ci-root-check`.
pub fn run_check() -> Result<(), String> {
    guard()?;
    println!(
        "{}",
        if crate::ci::is_github() {
            "racine du dépôt : celle de l'espace de travail de l'exécution"
        } else {
            "racine du dépôt : hors GitHub Actions, rien à comparer"
        }
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same(path: &Path) -> Option<PathBuf> {
        Some(path.to_path_buf())
    }

    #[test]
    fn the_same_workspace_passes() {
        let root = Path::new("/home/runner/work/hearth/hearth");
        assert!(check_workspace(root, Some(root), &same).is_ok());
    }

    #[test]
    fn another_workspace_is_refused_with_both_paths() {
        let error = check_workspace(
            Path::new("/home/runner/work/hearth/hearth"),
            Some(Path::new("/srv/ailleurs")),
            &same,
        )
        .unwrap_err();
        assert!(
            error.contains("/home/runner/work/hearth/hearth") && error.contains("/srv/ailleurs"),
            "{error}"
        );
    }

    #[test]
    fn a_missing_workspace_or_an_unresolvable_path_is_a_refusal_never_an_agreement() {
        let root = Path::new("/a");
        assert!(
            check_workspace(root, None, &same)
                .unwrap_err()
                .contains("GITHUB_WORKSPACE")
        );
        let nothing = |_: &Path| None;
        assert!(check_workspace(root, Some(root), &nothing).is_err());
    }

    #[test]
    fn paths_are_compared_after_resolution() {
        let resolve = |path: &Path| {
            Some(PathBuf::from(
                path.to_string_lossy().to_lowercase().replace("/link", ""),
            ))
        };
        assert!(
            check_workspace(
                Path::new("/Work/Hearth"),
                Some(Path::new("/link/work/hearth")),
                &resolve
            )
            .is_ok()
        );
    }

    #[test]
    fn the_root_is_the_folder_that_holds_xtask() {
        let root = repo_root().unwrap();
        assert!(root.join("xtask").join("Cargo.toml").is_file());
    }
}
