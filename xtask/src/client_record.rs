//! Enregistrement de construction de l'installateur du client (HRT-173, BR-RELEASE-007, conception 9.4).
//!
//! Même garantie que `hearth-agent.build` pour l'agent : `release-stage` ne signe ni n'assemble un
//! installateur dont il ne sait pas qu'il sort du commit exact qu'il assemble.
//!
//! - `client-record --begin` : AVANT `npm ci` et `tauri build`, mesure l'état de l'arbre (git) et refuse
//!   tout de suite un arbre qui n'est pas le commit (inutile d'attendre la fin d'une construction de
//!   trente minutes pour l'apprendre). Le commit mesuré est gardé dans `target/client-record.begin`.
//! - `client-record --installer F --version X.Y.Z` : APRÈS la construction, mesure de nouveau et écrit
//!   `F.build` (quatre lignes, format de `BuildRecord`) : somme de l'installateur, commit, arbre `clean`
//!   seulement si les deux relevés sont propres et sur le même commit. Sinon, aucun fichier et une
//!   erreur. Le fichier est en clair, falsifiable à la main : une garde contre l'erreur (installateur
//!   d'un autre commit glissé dans le dossier), pas contre la malveillance.
//!
//! Le fichier `.build` n'est jamais assemblé ni déposé (`release-stage` le relit seulement).

use std::path::{Path, PathBuf};

use crate::agent::{BuildRecord, Survey, record_path, recorded_state, refuse_unless_clean};
use crate::host::{Host, System};
use crate::release_core::{check_version, installer_file_name};
use crate::release_stage::{check_flags, sha256_hex};

/// Relevé de l'arbre fait par `--begin`, sous `target/` (ignoré par git, hors des entrées de la
/// construction de l'agent : l'écrire ne salit pas l'arbre).
pub const BEGIN_FILE: &str = "target/client-record.begin";

/// `cargo xtask client-record --begin | --installer F --version X.Y.Z`.
pub fn run(args: &[String]) -> Result<(), String> {
    check_flags(args, &["--installer", "--version"], &["--begin"])?;
    let root = crate::repo_root::repo_root()?;
    let host = System::new();
    let say = &mut |line: &str| println!("{line}");
    if args.iter().any(|arg| arg == "--begin") {
        if args.len() != 1 {
            return Err("--begin s'utilise seul".to_owned());
        }
        return begin(&host, &root, say);
    }
    let installer = crate::client_release::required(args, "--installer")?;
    let version = crate::client_release::required(args, "--version")?;
    record(&host, &root, Path::new(&installer), &version, say)
}

fn begin_path(root: &Path) -> PathBuf {
    root.join(BEGIN_FILE)
}

/// Relève l'état de l'arbre avant la construction. Refuse un arbre sale ou inconnu.
pub fn begin(host: &dyn Host, root: &Path, say: &mut dyn FnMut(&str)) -> Result<(), String> {
    let survey = Survey::take_client(host, root);
    refuse_unless_clean(&survey, "du client")?;
    let Ok(commit) = &survey.commit else {
        return Err("commit en place inconnu".to_owned());
    };
    let path = begin_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("{} : {error}", parent.display()))?;
    }
    std::fs::write(&path, format!("commit={commit}\n"))
        .map_err(|error| format!("{} : {error}", path.display()))?;
    say(&format!(
        "arbre propre avant la construction, commit {commit}"
    ));
    Ok(())
}

/// Le commit relevé par `--begin`. Absent ou illisible : erreur, jamais « propre par défaut ».
fn read_begin(root: &Path) -> Result<String, String> {
    let path = begin_path(root);
    let text = std::fs::read_to_string(&path).map_err(|error| {
        format!(
            "relevé de l'arbre avant la construction absent ({error}) : lance cargo xtask client-record --begin avant npm ci"
        )
    })?;
    let commit = text
        .lines()
        .find_map(|line| line.strip_prefix("commit="))
        .map(str::trim)
        .unwrap_or_default();
    if commit.is_empty() || !commit.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "{} est illisible : relance cargo xtask client-record --begin",
            path.display()
        ));
    }
    Ok(commit.to_owned())
}

/// Écrit `<installateur>.build` après la construction. Rend l'enregistrement.
pub fn record(
    host: &dyn Host,
    root: &Path,
    installer: &Path,
    version: &str,
    say: &mut dyn FnMut(&str),
) -> Result<(), String> {
    check_version(version)?;
    let name = installer
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or("nom de l'installateur illisible")?;
    if name != installer_file_name(version) {
        return Err(format!(
            "{name} n'est pas l'installateur de la version {version} ({}) : mauvais fichier ?",
            installer_file_name(version)
        ));
    }
    // Un enregistrement d'une construction précédente ne survit jamais à celle-ci.
    let path = record_path(installer);
    if let Err(error) = std::fs::remove_file(&path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        return Err(format!("{} : {error}", path.display()));
    }
    let bytes =
        std::fs::read(installer).map_err(|error| format!("{} : {error}", installer.display()))?;
    // Le relevé de début ne sert qu'UNE fois, réussite ou échec : après un refus, il faut tout reconstruire
    // depuis `--begin`, sinon un installateur construit sur un arbre sale passerait avec un arbre rangé.
    let begun = read_begin(root);
    let _ = std::fs::remove_file(begin_path(root));
    let before = Survey::clean_at(&begun?);
    let after = Survey::take_client(host, root);
    let state = recorded_state(&before, &after);
    if state != "clean" {
        // Dit pourquoi : arbre modifié par la construction, ou git muet, ou HEAD changé.
        refuse_unless_clean(&after, "du client")?;
        return Err(format!(
            "construction du client refusée : HEAD a changé pendant la construction (état {state}). Relance sur un commit stable"
        ));
    }
    let commit = match &after.commit {
        Ok(commit) => commit.clone(),
        Err(error) => return Err(error.clone()),
    };
    let record = BuildRecord {
        commit,
        tree: state.to_owned(),
        sha256: sha256_hex(&bytes),
        version: version.to_owned(),
    };
    std::fs::write(&path, record.render())
        .map_err(|error| format!("{} : {error}", path.display()))?;
    say(&format!(
        "enregistrement de construction : {} (commit {}, arbre clean)",
        path.display(),
        record.commit
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::release_stage::fixtures::{TreeFake, tmp_dir};

    const VERSION: &str = "0.1.0";

    struct Repo {
        root: PathBuf,
        installer: PathBuf,
    }

    impl Repo {
        fn new(name: &str) -> Self {
            let root = tmp_dir(name);
            std::fs::create_dir_all(&root).unwrap();
            let installer = root.join(installer_file_name(VERSION));
            std::fs::write(&installer, b"installateur").unwrap();
            Self { root, installer }
        }
    }

    impl Drop for Repo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn quiet() -> impl FnMut(&str) {
        |_: &str| {}
    }

    fn record_text(repo: &Repo) -> Option<String> {
        std::fs::read_to_string(record_path(&repo.installer)).ok()
    }

    #[test]
    fn a_clean_tree_before_and_after_writes_the_record_with_the_sum_and_the_commit() {
        let repo = Repo::new("client_record_ok");
        let fake = TreeFake::new("", "abc1234def");
        begin(&fake, &repo.root, &mut quiet()).unwrap();
        record(&fake, &repo.root, &repo.installer, VERSION, &mut quiet()).unwrap();
        let parsed = BuildRecord::parse(&record_text(&repo).unwrap()).unwrap();
        assert_eq!(parsed.commit, "abc1234def");
        assert_eq!(parsed.tree, "clean");
        assert_eq!(parsed.sha256, sha256_hex(b"installateur"));
        assert_eq!(parsed.version, VERSION);
        assert!(fake.only_git_ran(), "seul git, en lecture, est lancé");
        assert!(
            !begin_path(&repo.root).exists(),
            "le relevé de début est consommé"
        );
    }

    #[test]
    fn begin_refuses_a_dirty_or_unknown_tree_and_writes_nothing() {
        let repo = Repo::new("client_record_begin_ko");
        let mut cases = Vec::new();
        cases.push(("Cargo.toml", TreeFake::new(" M Cargo.toml", "abc1234def")));
        let mut untracked = TreeFake::new("", "abc1234def");
        untracked.untracked = "crates/hearth-agent/migrations/0005_x.sql".to_owned();
        cases.push(("0005_x.sql", untracked));
        let mut mute = TreeFake::new("", "abc1234def");
        mute.status_fails = true;
        cases.push(("inconnu", mute));
        for (expected, fake) in cases {
            let error = begin(&fake, &repo.root, &mut quiet()).unwrap_err();
            assert!(error.contains(expected), "{expected} : {error}");
            assert!(error.contains("refusée"), "{error}");
            assert!(!begin_path(&repo.root).exists());
        }
    }

    #[test]
    fn a_tree_dirtied_by_the_build_is_refused_and_no_record_is_left() {
        let repo = Repo::new("client_record_dirty_after");
        begin(&TreeFake::new("", "abc1234def"), &repo.root, &mut quiet()).unwrap();
        // Une construction qui laisse un fichier suivi modifié (ou un non suivi sous les entrées).
        let error = record(
            &TreeFake::new(" M apps/desktop/src/bindings.ts", "abc1234def"),
            &repo.root,
            &repo.installer,
            VERSION,
            &mut quiet(),
        )
        .unwrap_err();
        assert!(
            error.contains("bindings.ts") && error.contains("sale"),
            "{error}"
        );
        assert!(record_text(&repo).is_none());
    }

    #[test]
    fn a_head_that_moved_during_the_build_is_refused() {
        let repo = Repo::new("client_record_head_moved");
        begin(&TreeFake::new("", "abc1234def"), &repo.root, &mut quiet()).unwrap();
        let error = record(
            &TreeFake::new("", "fff0000"),
            &repo.root,
            &repo.installer,
            VERSION,
            &mut quiet(),
        )
        .unwrap_err();
        assert!(error.contains("HEAD a changé"), "{error}");
        assert!(record_text(&repo).is_none());
    }

    #[test]
    fn a_git_that_cannot_answer_after_the_build_is_never_read_as_clean() {
        let repo = Repo::new("client_record_mute_after");
        begin(&TreeFake::new("", "abc1234def"), &repo.root, &mut quiet()).unwrap();
        let mut mute = TreeFake::new("", "abc1234def");
        mute.status_fails = true;
        let error = record(&mute, &repo.root, &repo.installer, VERSION, &mut quiet()).unwrap_err();
        assert!(error.contains("inconnu"), "{error}");
        assert!(record_text(&repo).is_none());
    }

    #[test]
    fn without_the_begin_survey_nothing_is_recorded() {
        let repo = Repo::new("client_record_no_begin");
        let error = record(
            &TreeFake::new("", "abc1234def"),
            &repo.root,
            &repo.installer,
            VERSION,
            &mut quiet(),
        )
        .unwrap_err();
        assert!(error.contains("--begin"), "{error}");
        assert!(record_text(&repo).is_none());
        let garbled = begin_path(&repo.root);
        std::fs::create_dir_all(garbled.parent().unwrap()).unwrap();
        std::fs::write(&garbled, "commit=pas-hexa\n").unwrap();
        assert!(
            record(
                &TreeFake::new("", "abc1234def"),
                &repo.root,
                &repo.installer,
                VERSION,
                &mut quiet(),
            )
            .unwrap_err()
            .contains("illisible")
        );
    }

    #[test]
    fn a_file_that_is_not_the_installer_of_the_version_is_refused() {
        let repo = Repo::new("client_record_name");
        let fake = TreeFake::new("", "abc1234def");
        begin(&fake, &repo.root, &mut quiet()).unwrap();
        let other = repo.root.join("Hearth_0.2.0_x64-setup.exe");
        std::fs::write(&other, b"autre").unwrap();
        let error = record(&fake, &repo.root, &other, VERSION, &mut quiet()).unwrap_err();
        assert!(error.contains("mauvais fichier"), "{error}");
        assert!(record(&fake, &repo.root, &repo.installer, "0.1", &mut quiet()).is_err());
        assert!(
            record(
                &fake,
                &repo.root,
                &repo.root.join("absent").join(installer_file_name(VERSION)),
                VERSION,
                &mut quiet()
            )
            .is_err()
        );
    }

    #[test]
    fn a_stale_record_never_survives_a_failed_recording() {
        let repo = Repo::new("client_record_stale");
        std::fs::write(record_path(&repo.installer), "commit=aa\n").unwrap();
        // Pas de relevé de début : échec, et l'ancien enregistrement a disparu.
        assert!(
            record(
                &TreeFake::new("", "abc1234def"),
                &repo.root,
                &repo.installer,
                VERSION,
                &mut quiet()
            )
            .is_err()
        );
        assert!(record_text(&repo).is_none());
    }

    #[test]
    fn the_arguments_are_checked() {
        assert!(run(&["--inconnu".to_owned()]).is_err());
        assert!(
            run(&[
                "--begin".to_owned(),
                "--version".to_owned(),
                "0.1.0".to_owned()
            ])
            .is_err()
        );
        assert!(run(&["--installer".to_owned(), "x".to_owned()]).is_err());
    }

    #[test]
    fn after_a_refusal_the_begin_survey_is_gone_and_a_retry_without_rebuilding_is_refused() {
        let repo = Repo::new("client_record_retry");
        begin(&TreeFake::new("", "abc1234def"), &repo.root, &mut quiet()).unwrap();
        // Construction sur un arbre sale : refus.
        let dirty = TreeFake::new(" M apps/desktop/src/App.vue", "abc1234def");
        assert!(record(&dirty, &repo.root, &repo.installer, VERSION, &mut quiet()).is_err());
        // L'arbre est rangé (git checkout) mais l'installateur n'a pas été reconstruit : refus aussi.
        let tidy = TreeFake::new("", "abc1234def");
        let error = record(&tidy, &repo.root, &repo.installer, VERSION, &mut quiet()).unwrap_err();
        assert!(error.contains("--begin"), "{error}");
        assert!(record_text(&repo).is_none());
    }

    #[test]
    fn the_client_looks_for_untracked_files_under_apps_desktop_but_not_for_ignored_ones() {
        let repo = Repo::new("client_record_inputs");
        let fake = TreeFake::new("", "abc1234def");
        begin(&fake, &repo.root, &mut quiet()).unwrap();
        let calls = fake.calls.borrow();
        let has = |call: &Vec<String>, word: &str| call.iter().any(|a| a == word);
        let untracked: Vec<_> = calls
            .iter()
            .filter(|c| has(c, "--others") && !has(c, "--ignored"))
            .collect();
        let ignored: Vec<_> = calls.iter().filter(|c| has(c, "--ignored")).collect();
        assert!(
            untracked
                .iter()
                .all(|c| has(c, "apps/desktop") && has(c, "crates"))
        );
        assert!(!untracked.is_empty() && !ignored.is_empty());
        assert!(
            ignored
                .iter()
                .all(|c| !has(c, "apps/desktop") && has(c, "crates"))
        );
    }

    #[test]
    fn an_untracked_file_under_apps_desktop_refuses_the_client_build() {
        let repo = Repo::new("client_record_untracked");
        let mut fake = TreeFake::new("", "abc1234def");
        fake.untracked = "apps/desktop/src-tauri/capabilities/extra.json".to_owned();
        let error = begin(&fake, &repo.root, &mut quiet()).unwrap_err();
        assert!(error.contains("extra.json"), "{error}");
    }
}
