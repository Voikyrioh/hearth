//! `cargo xtask catalog-check [--release]` : contrôle des fiches de serveur de jeu embarquées (HRT-113,
//! conception technique 12.3 et 12.4).
//!
//! - chaque fiche de `crates/hearth-agent/catalog/<id>/` passe le chargeur RÉEL de l'agent (le même code, inclus
//!   par `#[path]` : neuf invariants, format fermé, bornes) ;
//! - `ids.lock` : un identifiant de réglage publié ne disparaît pas par oubli (il passe en `[[retired]]`), ne
//!   change pas de clé sans `rename` d'un `[[migration]]`, n'est pas réutilisé après son retrait ;
//! - `--release` (option, branchée sur aucune publication : la CI lance `catalog-check` sans elle) : refuse toute
//!   fiche `preview`. Toute fiche est embarquée, `preview` comprise ; une fiche `preview` n'est PROPOSÉE que par une
//!   construction d'essai (`domain::catalog::embedded::keep_offered`, HRT-101). Il refuse aussi toute fiche dont la mémoire recommandée manque ou
//!   n'a qu'une mesure au repos (HRT-122, BR-RES-038).
//!
//! Hors périmètre de ce ticket (HRT-114) : `golden.toml`, `probe.toml`, `evidence.toml`, le relevé sur l'image
//! et le banc d'essai.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Le chargeur de fiches, tel quel : autonome (`super::` seulement).
#[path = "../../crates/hearth-agent/src/domain/catalog/loading/mod.rs"]
#[allow(dead_code, unused_imports)]
mod loading;

use loading::ids_lock;
use loading::loader::{Origin, load};
use loading::memory;
use loading::schema::fiche::Status;

const CATALOG_DIR: &str = "crates/hearth-agent/catalog";

/// Ce que le contrôle a trouvé pour UNE fiche : une phrase par reproche, vide si tout est en ordre.
pub fn check_texts(
    dir: &str,
    fiche: &[u8],
    settings: &[u8],
    lock: Option<&[u8]>,
    release: bool,
) -> Vec<String> {
    let loaded = match load(Origin::Embedded, fiche, settings) {
        Ok(loaded) => loaded,
        Err(refusal) => return vec![refusal.message_pour(dir)],
    };
    let mut problems = Vec::new();
    if loaded.id() != dir {
        problems.push(format!(
            "fiche `{dir}` : son identifiant `{}` doit être le nom de son dossier",
            loaded.id()
        ));
    }
    if release && loaded.file.status != Status::Published {
        problems.push(format!("fiche `{dir}` : elle est `preview` (catalog-check --release refuse toute fiche `preview`)"));
    }
    // BR-RES-038 : pas de publication sans mémoire recommandée mesurée ou publiée par l'éditeur. Une fiche
    // `preview` reste en préparation ; ce reproche dit ce qui lui manque pour en sortir.
    if release && let Some(gap) = memory::recommended_gap(&loaded.file) {
        problems.push(format!(
            "fiche `{dir}` : {} (BR-RES-038 : mesure sur un vrai serveur, ou chiffre de l'éditeur avec son adresse)",
            gap.explication()
        ));
    }
    match lock {
        None => problems.push(format!("fiche `{dir}` : ids.lock manquant (le registre des identifiants de réglage est obligatoire)")),
        Some(bytes) => match ids_lock::parse(bytes) {
            Err(refusal) => problems.push(refusal.message_pour(dir)),
            Ok(lock) => {
                for violation in ids_lock::check(&lock, &loaded) {
                    problems.push(format!("fiche `{dir}` : ids.lock : {violation}"));
                }
            }
        },
    }
    problems
}

/// `ids.lock` ne décroît jamais : chaque identifiant du registre de la branche principale reste dans le registre
/// de la branche courante, et un identifiant retiré le reste. Supprimer un réglage ET sa ligne du registre ne
/// passe donc plus.
pub fn check_against_base(
    dir: &str,
    base: &ids_lock::IdsLock,
    now: &ids_lock::IdsLock,
) -> Vec<String> {
    let mut problems = Vec::new();
    for old in &base.ids {
        match now.ids.iter().find(|n| n.id == old.id) {
            None => problems.push(format!(
                "fiche `{dir}` : ids.lock : l'identifiant `{}` figure dans le registre de la branche principale et a disparu du vôtre (un registre ne décroît jamais ; un réglage retiré reste en `[[retired]]`)",
                old.id
            )),
            Some(new)
                if old.state == ids_lock::LockState::Retired
                    && new.state != ids_lock::LockState::Retired =>
            {
                problems.push(format!(
                    "fiche `{dir}` : ids.lock : l'identifiant retiré `{}` est redevenu actif",
                    old.id
                ));
            }
            Some(_) => {}
        }
    }
    problems
}

/// Le registre de la branche principale (`origin/main`, sinon `main`), lu par git sans réseau ; `None` si git ou
/// la référence manque (le contrôle est alors sauté, et dit).
fn base_lock(dir: &str) -> Option<Vec<u8>> {
    for reference in ["origin/main", "main"] {
        let output = std::process::Command::new("git")
            .args(["show", &format!("{reference}:{CATALOG_DIR}/{dir}/ids.lock")])
            .output()
            .ok()?;
        if output.status.success() {
            return Some(output.stdout);
        }
    }
    None
}

/// Les fichiers d'une fiche, lus sur le disque.
struct FicheFiles {
    fiche: Vec<u8>,
    settings: Vec<u8>,
    lock: Option<Vec<u8>>,
}

fn read_fiche(path: &Path) -> Result<FicheFiles, String> {
    let read = |name: &str| {
        std::fs::read(path.join(name))
            .map_err(|e| format!("{} illisible : {e}", path.join(name).display()))
    };
    let lock = std::fs::read(path.join("ids.lock")).ok();
    Ok(FicheFiles {
        fiche: read("fiche.toml")?,
        settings: read("settings.toml")?,
        lock,
    })
}

pub fn run(args: &[String]) -> ExitCode {
    let release = args.iter().any(|a| a == "--release");
    if let Some(unknown) = args.iter().find(|a| a.as_str() != "--release") {
        eprintln!("catalog-check : option inconnue {unknown} (seule --release existe)");
        return ExitCode::FAILURE;
    }
    let root = PathBuf::from(CATALOG_DIR);
    let mut dirs: Vec<String> = match std::fs::read_dir(&root) {
        Ok(entries) => entries
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect(),
        Err(error) => {
            eprintln!("catalog-check : {CATALOG_DIR} illisible : {error}");
            return ExitCode::FAILURE;
        }
    };
    dirs.sort();
    let mut problems = Vec::new();
    let mut locked = 0usize;
    for dir in &dirs {
        match read_fiche(&root.join(dir)) {
            Err(message) => problems.push(format!("fiche `{dir}` : {message}")),
            Ok(FicheFiles {
                fiche,
                settings,
                lock,
            }) => {
                if let Some(bytes) = &lock
                    && let Ok(parsed) = ids_lock::parse(bytes)
                {
                    locked += parsed.ids.len();
                }
                if let (Some(now_bytes), Some(base_bytes)) = (&lock, base_lock(dir)) {
                    if let (Ok(now), Ok(base)) =
                        (ids_lock::parse(now_bytes), ids_lock::parse(&base_bytes))
                    {
                        problems.extend(check_against_base(dir, &base, &now));
                    }
                } else if lock.is_some() {
                    println!(
                        "catalog-check : fiche `{dir}` : pas de registre sur la branche principale (ou git muet), comparaison sautée"
                    );
                }
                problems.extend(check_texts(
                    dir,
                    &fiche,
                    &settings,
                    lock.as_deref(),
                    release,
                ));
            }
        }
    }
    if problems.is_empty() {
        println!(
            "catalog-check : {} fiche(s), {locked} identifiant(s) verrouillé(s), aucun reproche{}",
            dirs.len(),
            if release {
                " (profil de publication)"
            } else {
                ""
            }
        );
        return ExitCode::SUCCESS;
    }
    for problem in &problems {
        eprintln!("{problem}");
    }
    eprintln!("catalog-check : {} reproche(s)", problems.len());
    ExitCode::FAILURE
}

#[cfg(test)]
mod tests {
    use super::*;

    const FICHE: &[u8] = include_bytes!("../../crates/hearth-agent/catalog/terraria/fiche.toml");
    const SETTINGS: &str = include_str!("../../crates/hearth-agent/catalog/terraria/settings.toml");
    const LOCK: &str = include_str!("../../crates/hearth-agent/catalog/terraria/ids.lock");

    fn check(settings: &str, lock: &str, release: bool) -> Vec<String> {
        check_texts(
            "terraria",
            FICHE,
            settings.as_bytes(),
            Some(lock.as_bytes()),
            release,
        )
    }

    #[test]
    fn a_lock_line_removed_with_its_setting_is_caught_against_the_base() {
        let base = ids_lock::parse(LOCK.as_bytes()).expect("registre");
        let without: String = LOCK
            .lines()
            .filter(|l| !l.contains("id = \"motd\""))
            .map(|l| {
                format!(
                    "{l}
"
                )
            })
            .collect();
        let now = ids_lock::parse(without.as_bytes()).expect("registre sans motd");
        let problems = check_against_base("terraria", &base, &now);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("`motd`"), "{problems:?}");
        assert!(check_against_base("terraria", &base, &base).is_empty());
        let reactivated = LOCK.replace(
            "{ id = \"seed\", key = \"seed\", channel = \"config\", state = \"active\", since = 1 }",
            "{ id = \"seed\", key = \"seed\", channel = \"config\", state = \"retired\", since = 1, retired_in = 1 }",
        );
        let retired = ids_lock::parse(reactivated.as_bytes()).expect("registre");
        assert_eq!(check_against_base("terraria", &retired, &base).len(), 1);
    }

    #[test]
    fn the_real_terraria_fiche_and_its_lock_pass() {
        assert_eq!(check(SETTINGS, LOCK, false), Vec::<String>::new());
    }

    #[test]
    fn the_release_profile_refuses_a_preview_fiche() {
        let problems = check(SETTINGS, LOCK, true);
        assert!(problems[0].contains("preview"), "{problems:?}");
    }

    #[test]
    fn the_release_profile_names_the_missing_recommended_memory() {
        // Terraria n'a pas encore de mémoire mesurée (HRT-149) : elle reste en préparation, et le dit.
        let problems = check(SETTINGS, LOCK, true);
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(
            problems[1].contains("mémoire recommandée manque"),
            "{problems:?}"
        );
        assert!(problems[1].contains("BR-RES-038"), "{problems:?}");
        // Hors `--release`, rien de ce genre n'est reproché.
        assert!(check(SETTINGS, LOCK, false).is_empty());
    }

    #[test]
    fn removing_a_setting_without_declaring_it_retired_fails() {
        let start = SETTINGS.find("[[setting]]\nid = \"motd\"").expect("motd");
        let end = SETTINGS[start + 5..]
            .find("[[setting]]")
            .map(|i| start + 5 + i)
            .expect("suivant");
        let without = format!("{}{}", &SETTINGS[..start], &SETTINGS[end..]);
        let problems = check(&without, LOCK, false);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(
            problems[0].contains("motd") && problems[0].contains("[[retired]]"),
            "{problems:?}"
        );
    }

    #[test]
    fn a_changed_key_without_a_rename_and_a_reused_retired_id_fail() {
        let renamed = SETTINGS.replacen("key = \"motd\"", "key = \"message\"", 1);
        let problems = check(&renamed, LOCK, false);
        assert!(
            problems
                .iter()
                .any(|p| p.contains("`motd`") && p.contains("rename")),
            "{problems:?}"
        );
        let retired_lock = LOCK.replace(
            "{ id = \"seed\", key = \"seed\", channel = \"config\", state = \"active\", since = 1 }",
            "{ id = \"seed\", key = \"seed\", channel = \"config\", state = \"retired\", since = 1, retired_in = 1 }",
        );
        let problems = check(SETTINGS, &retired_lock, false);
        assert!(
            problems
                .iter()
                .any(|p| p.contains("`seed`") && p.contains("réutilisé")),
            "{problems:?}"
        );
    }

    #[test]
    fn a_missing_lock_and_a_fiche_the_loader_refuses_are_reported_by_name() {
        let problems = check_texts("terraria", FICHE, SETTINGS.as_bytes(), None, false);
        assert!(problems.iter().any(|p| p.contains("ids.lock manquant")));
        let broken = String::from_utf8(FICHE.to_vec()).expect("utf8").replacen(
            "tag = \"vanilla-1.4.5.8\"",
            "tag = \"latest\"",
            1,
        );
        let problems = check_texts(
            "terraria",
            broken.as_bytes(),
            SETTINGS.as_bytes(),
            Some(LOCK.as_bytes()),
            false,
        );
        assert_eq!(problems.len(), 1);
        assert!(
            problems[0].contains("terraria") && problems[0].contains("I1"),
            "{problems:?}"
        );
        let wrong_dir = check_texts(
            "autre",
            FICHE,
            SETTINGS.as_bytes(),
            Some(LOCK.as_bytes()),
            false,
        );
        assert!(wrong_dir.iter().any(|p| p.contains("nom de son dossier")));
    }
}
