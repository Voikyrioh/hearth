//! `cargo xtask release-github [--stage-dir D] --notes-file F` (HRT-174, BR-RELEASE-010) : l'étiquette
//! `vX.Y.Z` et la release GitHub, APRÈS le manifeste. Idempotent : chaque lancement lit ce qui existe
//! déjà et ne crée que ce qui manque (conception technique 7.2) ; une étiquette sur un AUTRE commit
//! arrête tout, sans rien modifier.
//!
//! | étiquette | release | action |
//! |---|---|---|
//! | absente | absente | `gh release create vX.Y.Z --target <commit> …` |
//! | sur le commit | absente | `gh release create vX.Y.Z --verify-tag …` |
//! | sur le commit | présente, `SHA256SUMS` joint | rien (relance après un succès partiel) |
//! | sur le commit | présente, `SHA256SUMS` absent | `gh release upload vX.Y.Z SHA256SUMS` |
//! | sur un autre commit | — | ARRÊT, à examiner à la main |
//! | absente | présente | ARRÊT, à examiner à la main |
//! | lecture en échec | — | ARRÊT |
//!
//! Toute lecture se fait par code HTTP exact (`release_preflight::fetch_release`) ou par code de sortie
//! de `git ls-remote` : une absence ne se déduit jamais d'un texte d'erreur. La création est TOUJOURS
//! relue ensuite (la release existe, `SHA256SUMS` est joint) puis le manifeste public est relu : il doit
//! annoncer la version. Le jeton (`GH_TOKEN`) est lu par `gh` et `curl` dans l'environnement ou l'entrée
//! standard, jamais passé en argument ni affiché. Le commit est celui de `COMMIT` (écrit par
//! `release-stage`), contrôlé contre `HEARTH_SHA`.

use std::path::Path;

use serde_json::Value;

use crate::client_release::{option, root};
use crate::host::{Call, Host, System};
use crate::release_core::{DOWNLOAD_PREFIX, check_version};
use crate::release_preflight::{
    Manifest, check_notes, fetch_release, git_call, read_public_manifest, short,
};
use crate::release_stage::{STAGE_DIR, SUMS_NAME, read_stage_commit};

const REPOSITORY: &str = "Voikyrioh/hearth";

/// Spec fonctionnelle section 4 : tout échec APRÈS le manifeste (lecture ou création de la release).
fn release_failed(detail: &str) -> String {
    format!(
        "Version déposée et annoncée, mais la release GitHub n'a pas pu être créée : relancez l'étape en échec.\n{detail}"
    )
}

/// Spec fonctionnelle section 4 : la fin.
fn published(version: &str) -> String {
    format!("Version {version} publiée : manifeste à jour, release v{version} créée.")
}

/// Ce que `release-github` reçoit de l'environnement.
#[derive(Debug, Clone, Default)]
pub struct Inputs {
    /// Le commit lancé (`HEARTH_SHA`).
    pub sha: String,
    /// Jeton de lecture de l'API (`GH_TOKEN`).
    pub token: String,
}

/// Ce que le dossier assemblé dit de la version à publier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Staged {
    pub version: String,
    pub commit: String,
    pub sums_path: std::path::PathBuf,
}

/// Version (manifeste assemblé) et commit (`COMMIT`) du dossier assemblé ; le commit doit être celui du
/// lancement (BR-RELEASE-007 : l'étiquette tombe sur le commit construit, pas sur un autre).
pub fn read_staged(stage_dir: &Path, sha: &str) -> Result<Staged, String> {
    let commit = read_stage_commit(stage_dir)?;
    if sha.trim().is_empty() {
        return Err("HEARTH_SHA est absent : le commit lancé est inconnu".to_owned());
    }
    if !commit.eq_ignore_ascii_case(sha.trim()) {
        return Err(format!(
            "Le dossier assemblé vient du commit {commit}, pas du commit lancé {}. Rien n'a été créé sur GitHub.",
            sha.trim()
        ));
    }
    let path = stage_dir.join("releases/manifest.json");
    let text =
        std::fs::read_to_string(&path).map_err(|error| format!("{} : {error}", path.display()))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| format!("{} illisible : {error}", path.display()))?;
    let version = value["version"]
        .as_str()
        .ok_or_else(|| format!("{} n'annonce aucune version", path.display()))?
        .to_owned();
    check_version(&version)?;
    let sums_path = stage_dir.join(format!("releases/{version}/{SUMS_NAME}"));
    if !sums_path.is_file() {
        return Err(format!(
            "{} est absent : relancez release-stage",
            sums_path.display()
        ));
    }
    Ok(Staged {
        version,
        commit,
        sums_path,
    })
}

/// Le commit sur lequel pointe l'étiquette `vX.Y.Z` d'`origin` (`None` : étiquette absente). Lecture par
/// code de sortie de `git ls-remote` (0 présente, 2 absente, autre = erreur) ; une étiquette annotée
/// rend aussi sa ligne dépouillée (`^{}`), qui est le commit.
fn tag_commit(host: &dyn Host, root: &Path, version: &str) -> Result<Option<String>, String> {
    let name = format!("refs/tags/v{version}");
    let peeled = format!("{name}^{{}}");
    let output = git_call(
        host,
        root,
        &[
            "ls-remote",
            "--exit-code",
            "--tags",
            "origin",
            &name,
            &peeled,
        ],
    )?;
    match output.code {
        Some(2) => return Ok(None),
        Some(0) => {}
        other => {
            return Err(format!(
                "L'étiquette v{version} n'a pas pu être cherchée (git ls-remote : code {other:?} {}).",
                short(&output.stderr)
            ));
        }
    }
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let mut plain = None;
    let mut dereferenced = None;
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let (Some(sha), Some(reference)) = (parts.next(), parts.next()) else {
            continue;
        };
        if reference == peeled {
            dereferenced = Some(sha.to_owned());
        } else if reference == name {
            plain = Some(sha.to_owned());
        }
    }
    dereferenced.or(plain).map(Some).ok_or_else(|| {
        format!(
            "L'étiquette v{version} n'a pas pu être cherchée (réponse de git ls-remote illisible)."
        )
    })
}

fn has_sums(release: &Value) -> bool {
    release["assets"].as_array().is_some_and(|assets| {
        assets
            .iter()
            .any(|asset| asset["name"].as_str() == Some(SUMS_NAME))
    })
}

fn gh(host: &dyn Host, args: &[String]) -> Result<(), String> {
    let output = host.run(&Call::new("gh", args))?;
    if output.success {
        Ok(())
    } else {
        Err(format!(
            "gh {} : code {:?} {}",
            args.iter().take(2).cloned().collect::<Vec<_>>().join(" "),
            output.code,
            short(&output.stderr)
        ))
    }
}

/// Ce qu'il y a à faire d'après l'étiquette et la release lues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    /// Étiquette et release absentes : `--target <commit>`.
    CreateWithTarget,
    /// Étiquette déjà sur le commit, release absente : `--verify-tag`.
    CreateOnTag,
    /// Release présente sans `SHA256SUMS`.
    UploadSums,
    /// Tout est là.
    Nothing,
}

/// La table de la conception 7.2 (pure). `release` : `None` absente, sinon `Some(avec SHA256SUMS)`.
fn decide(
    version: &str,
    commit: &str,
    tag: Option<&str>,
    release: Option<bool>,
) -> Result<Action, String> {
    match (tag, release) {
        (Some(found), _) if !found.eq_ignore_ascii_case(commit) => Err(format!(
            "L'étiquette v{version} pointe sur le commit {found} et non sur {commit} : à examiner à la main. Rien n'a été modifié."
        )),
        (None, None) => Ok(Action::CreateWithTarget),
        (None, Some(_)) => Err(format!(
            "La release GitHub v{version} existe sans étiquette lisible sur origin : à examiner à la main. Rien n'a été modifié."
        )),
        (Some(_), None) => Ok(Action::CreateOnTag),
        (Some(_), Some(false)) => Ok(Action::UploadSums),
        (Some(_), Some(true)) => Ok(Action::Nothing),
    }
}

/// Étiquette et release, ce qui manque seulement, puis relecture. `say` reçoit chaque ligne de compte rendu.
pub fn ensure_release(
    host: &dyn Host,
    root: &Path,
    staged: &Staged,
    notes_file: &str,
    token: &str,
    say: &mut dyn FnMut(String),
) -> Result<(), String> {
    let Staged {
        version,
        commit,
        sums_path,
    } = staged;
    let notes =
        std::fs::read_to_string(notes_file).map_err(|error| format!("{notes_file} : {error}"))?;
    check_notes(&notes)?;
    let tag = format!("v{version}");

    // Lectures : toute erreur ici arrête, après le manifeste (donc le message de la spec).
    let tag_at = tag_commit(host, root, version).map_err(|error| release_failed(&error))?;
    let release = fetch_release(host, version, token).map_err(|error| {
        release_failed(&format!(
            "La release GitHub {tag} n'a pas pu être cherchée ({error})."
        ))
    })?;
    let release_state = release.as_ref().map(has_sums);
    let action = decide(version, commit, tag_at.as_deref(), release_state)?;

    let sums = sums_path.to_string_lossy().into_owned();
    let title = format!("Hearth {version}");
    match action {
        Action::CreateWithTarget | Action::CreateOnTag => {
            let mut args = vec!["release".to_owned(), "create".to_owned(), tag.clone()];
            if action == Action::CreateWithTarget {
                args.extend(["--target".to_owned(), commit.clone()]);
            } else {
                args.push("--verify-tag".to_owned());
            }
            args.extend([
                "--title".to_owned(),
                title,
                "--notes-file".to_owned(),
                notes_file.to_owned(),
                sums,
            ]);
            gh(host, &args).map_err(|error| release_failed(&error))?;
            say(format!("release {tag} créée sur le commit {commit}"));
        }
        Action::UploadSums => {
            gh(
                host,
                &["release".to_owned(), "upload".to_owned(), tag.clone(), sums],
            )
            .map_err(|error| release_failed(&error))?;
            say(format!("{SUMS_NAME} joint à la release {tag} existante"));
        }
        Action::Nothing => say(format!(
            "release {tag} déjà présente avec {SUMS_NAME} : rien à créer"
        )),
    }

    // Relecture obligatoire : la création n'est crue que relue.
    match fetch_release(host, version, token) {
        Ok(Some(release)) if has_sums(&release) => {}
        Ok(Some(_)) => {
            return Err(release_failed(&format!(
                "La release {tag} existe mais {SUMS_NAME} n'y est pas joint."
            )));
        }
        Ok(None) => {
            return Err(release_failed(&format!(
                "La release {tag} reste introuvable après sa création."
            )));
        }
        Err(error) => return Err(release_failed(&error)),
    }
    // Le manifeste public annonce la version publiée.
    match read_public_manifest(host) {
        Manifest::Version(found) if found == *version => {}
        Manifest::Version(found) => {
            return Err(format!(
                "Le manifeste public annonce {found} et non {version} : à examiner à la main."
            ));
        }
        Manifest::Absent => {
            return Err(format!(
                "Le manifeste public n'annonce aucune version alors que {version} est déposée : à examiner à la main."
            ));
        }
        Manifest::Unreadable(cause) => {
            return Err(format!(
                "Le manifeste public n'a pas pu être relu ({cause}) : à examiner à la main."
            ));
        }
    }
    say(published(version));
    Ok(())
}

/// Résumé de succès (spec section 3) : numéro, commit, fichiers déposés avec leur somme, adresse du
/// manifeste, lien de la release.
fn success_summary(staged: &Staged) -> String {
    let sums = std::fs::read_to_string(&staged.sums_path).unwrap_or_default();
    let files: String = sums
        .lines()
        .filter_map(|line| line.split_once("  "))
        .map(|(sum, name)| format!("- `{name}` : `{sum}`\n"))
        .collect();
    format!(
        "### {}\n\n- Commit : `{}`\n- Manifeste : {DOWNLOAD_PREFIX}latest\n- Release : https://github.com/{REPOSITORY}/releases/tag/v{}\n\nFichiers déposés :\n{files}",
        published(&staged.version),
        staged.commit,
        staged.version,
    )
}

/// `cargo xtask release-github [--stage-dir D] --notes-file F` (env `HEARTH_SHA`, `GH_TOKEN`).
pub fn run(args: &[String]) -> Result<(), String> {
    let root = root()?;
    let stage_dir = option(args, "--stage-dir")
        .map_or_else(|| root.join(STAGE_DIR), |dir| Path::new(&dir).to_path_buf());
    let notes_file = option(args, "--notes-file").ok_or("--notes-file manquant")?;
    let inputs = Inputs {
        sha: std::env::var("HEARTH_SHA").unwrap_or_default(),
        token: std::env::var("GH_TOKEN").unwrap_or_default(),
    };
    let staged = read_staged(&stage_dir, &inputs.sha)?;
    ensure_release(
        &System::new(),
        &root,
        &staged,
        &notes_file,
        &inputs.token,
        &mut |line| println!("{line}"),
    )?;
    crate::ci::summary(&success_summary(&staged));
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::time::Duration;

    use super::*;
    use crate::host::CmdOutput;
    use crate::release_stage::fixtures::tmp_dir;

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
    const OTHER: &str = "fedcba9876543210fedcba9876543210fedcba98";
    const VERSION: &str = "0.1.0";

    /// GitHub, `origin` et le manifeste public en mémoire ; `gh` modifie l'état.
    struct Fake {
        /// Étiquette : `ls-remote` rend (code, sortie).
        tag: RefCell<Option<String>>,
        tag_lookup_fails: bool,
        /// Release : `None` absente, sinon les noms de ses fichiers joints.
        release: RefCell<Option<Vec<String>>>,
        release_lookup_status: u16,
        /// Version annoncée par le manifeste public.
        announced: RefCell<Option<String>>,
        /// `gh` échoue (message d'erreur).
        gh_fails: bool,
        /// `gh release create` réussit mais ne crée rien (relecture introuvable).
        gh_forgets: bool,
        calls: RefCell<Vec<(String, Vec<String>)>>,
    }

    impl Fake {
        fn new() -> Self {
            Self {
                tag: RefCell::new(None),
                tag_lookup_fails: false,
                release: RefCell::new(None),
                release_lookup_status: 200,
                announced: RefCell::new(Some(VERSION.to_owned())),
                gh_fails: false,
                gh_forgets: false,
                calls: RefCell::new(Vec::new()),
            }
        }

        fn gh_calls(&self) -> Vec<Vec<String>> {
            self.calls
                .borrow()
                .iter()
                .filter(|(program, _)| program == "gh")
                .map(|(_, args)| args.clone())
                .collect()
        }
    }

    fn reply(code: u16, body: &str) -> Result<CmdOutput, String> {
        Ok(CmdOutput {
            success: true,
            code: Some(0),
            stdout: format!("{body}\n{code}").into_bytes(),
            stderr: String::new(),
        })
    }

    fn done(code: i32, stdout: &str, stderr: &str) -> Result<CmdOutput, String> {
        Ok(CmdOutput {
            success: code == 0,
            code: Some(code),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.to_owned(),
        })
    }

    impl Host for Fake {
        fn run(&self, call: &Call) -> Result<CmdOutput, String> {
            self.calls
                .borrow_mut()
                .push((call.program.to_owned(), call.args.to_vec()));
            match call.program {
                "git" => {
                    if self.tag_lookup_fails {
                        return done(128, "", "fatal: impossible de lire origin");
                    }
                    match self.tag.borrow().as_deref() {
                        None => done(2, "", ""),
                        Some(sha) => done(0, &format!("{sha}\trefs/tags/v{VERSION}\n"), ""),
                    }
                }
                "curl" => {
                    let url = call.args.last().cloned().unwrap_or_default();
                    if url.contains("api.github.com") {
                        if self.release_lookup_status != 200 {
                            return reply(self.release_lookup_status, "{}");
                        }
                        return match self.release.borrow().as_ref() {
                            None => reply(404, "{\"message\":\"Not Found\"}"),
                            Some(names) => {
                                let assets: Vec<Value> = names
                                    .iter()
                                    .map(|n| serde_json::json!({ "name": n }))
                                    .collect();
                                reply(
                                    200,
                                    &serde_json::json!({
                                        "tag_name": format!("v{VERSION}"),
                                        "assets": assets,
                                    })
                                    .to_string(),
                                )
                            }
                        };
                    }
                    match self.announced.borrow().as_deref() {
                        None => reply(404, ""),
                        Some(version) => {
                            reply(200, &serde_json::json!({ "version": version }).to_string())
                        }
                    }
                }
                "gh" => {
                    if self.gh_fails {
                        return done(1, "", "HTTP 502: Bad Gateway");
                    }
                    let args = call.args;
                    match (args[0].as_str(), args[1].as_str()) {
                        ("release", "create") if !self.gh_forgets => {
                            *self.tag.borrow_mut() = Some(COMMIT.to_owned());
                            *self.release.borrow_mut() = Some(vec![SUMS_NAME.to_owned()]);
                        }
                        ("release", "upload") => {
                            self.release
                                .borrow_mut()
                                .as_mut()
                                .unwrap()
                                .push(SUMS_NAME.to_owned());
                        }
                        _ => {}
                    }
                    done(0, "", "")
                }
                other => Err(format!("appel inattendu : {other}")),
            }
        }

        fn elapsed(&self) -> Duration {
            Duration::ZERO
        }

        fn sleep(&self, _: Duration) {}
    }

    /// Un dossier assemblé minimal : COMMIT, manifeste, SHA256SUMS ; et un fichier de notes.
    fn staged_dir(name: &str) -> (std::path::PathBuf, Staged, String) {
        let dir = tmp_dir(name);
        let stage = dir.join("stage");
        std::fs::create_dir_all(stage.join(format!("releases/{VERSION}"))).unwrap();
        std::fs::write(stage.join("COMMIT"), format!("{COMMIT}\n")).unwrap();
        std::fs::write(
            stage.join("releases/manifest.json"),
            format!("{{\"version\":\"{VERSION}\"}}"),
        )
        .unwrap();
        std::fs::write(
            stage.join(format!("releases/{VERSION}/{SUMS_NAME}")),
            format!("{}  Hearth_{VERSION}_x64-setup.exe\n", "a".repeat(64)),
        )
        .unwrap();
        let notes = dir.join("notes.txt");
        std::fs::write(&notes, "Corrections.").unwrap();
        let staged = read_staged(&stage, COMMIT).unwrap();
        (dir, staged, notes.to_string_lossy().into_owned())
    }

    fn run_with(fake: &Fake, name: &str) -> (Result<(), String>, Vec<String>) {
        let (dir, staged, notes) = staged_dir(name);
        let mut lines = Vec::new();
        let result = ensure_release(fake, Path::new("."), &staged, &notes, "", &mut |l| {
            lines.push(l)
        });
        let _ = std::fs::remove_dir_all(dir);
        (result, lines)
    }

    const SUCCESS: &str = "Version 0.1.0 publiée : manifeste à jour, release v0.1.0 créée.";

    #[test]
    fn nothing_there_creates_the_tag_and_the_release_on_the_commit_then_rereads() {
        let fake = Fake::new();
        let (result, lines) = run_with(&fake, "rg_create");
        result.unwrap();
        let gh = fake.gh_calls();
        assert_eq!(gh.len(), 1);
        let args = &gh[0];
        assert_eq!(&args[..3], ["release", "create", "v0.1.0"]);
        assert!(args.windows(2).any(|w| w == ["--target", COMMIT]));
        assert!(args.windows(2).any(|w| w == ["--title", "Hearth 0.1.0"]));
        assert!(args.iter().any(|a| a.ends_with(SUMS_NAME)));
        assert_eq!(lines.last().unwrap(), SUCCESS);
        // Lecture avant, relecture après : deux appels de l'API.
        let api = fake
            .calls
            .borrow()
            .iter()
            .filter(|(p, a)| p == "curl" && a.last().is_some_and(|u| u.contains("api.github.com")))
            .count();
        assert_eq!(api, 2);
    }

    #[test]
    fn a_complete_release_is_recognised_and_nothing_is_created() {
        let fake = Fake::new();
        *fake.tag.borrow_mut() = Some(COMMIT.to_owned());
        *fake.release.borrow_mut() = Some(vec![SUMS_NAME.to_owned()]);
        let (result, lines) = run_with(&fake, "rg_complete");
        result.unwrap();
        assert!(fake.gh_calls().is_empty());
        assert_eq!(lines.last().unwrap(), SUCCESS);
    }

    #[test]
    fn a_release_without_its_sums_gets_them_uploaded() {
        let fake = Fake::new();
        *fake.tag.borrow_mut() = Some(COMMIT.to_owned());
        *fake.release.borrow_mut() = Some(Vec::new());
        let (result, _) = run_with(&fake, "rg_upload");
        result.unwrap();
        let gh = fake.gh_calls();
        assert_eq!(gh.len(), 1);
        assert_eq!(&gh[0][..3], ["release", "upload", "v0.1.0"]);
    }

    #[test]
    fn a_tag_on_the_commit_without_release_is_verified_not_moved() {
        let fake = Fake::new();
        *fake.tag.borrow_mut() = Some(COMMIT.to_owned());
        let (result, _) = run_with(&fake, "rg_tag_only");
        result.unwrap();
        let args = &fake.gh_calls()[0];
        assert!(args.contains(&"--verify-tag".to_owned()));
        assert!(!args.contains(&"--target".to_owned()));
    }

    #[test]
    fn a_tag_on_another_commit_stops_and_touches_nothing() {
        let fake = Fake::new();
        *fake.tag.borrow_mut() = Some(OTHER.to_owned());
        let (result, _) = run_with(&fake, "rg_other");
        let error = result.unwrap_err();
        assert!(
            error.contains("v0.1.0")
                && error.contains(OTHER)
                && error.contains("à examiner à la main"),
            "{error}"
        );
        assert!(fake.gh_calls().is_empty());
    }

    #[test]
    fn a_release_without_a_readable_tag_stops() {
        let fake = Fake::new();
        *fake.release.borrow_mut() = Some(vec![SUMS_NAME.to_owned()]);
        let (result, _) = run_with(&fake, "rg_no_tag");
        assert!(result.unwrap_err().contains("à examiner à la main"));
        assert!(fake.gh_calls().is_empty());
    }

    #[test]
    fn a_failed_reading_stops_with_the_message_of_the_spec() {
        let spec = "Version déposée et annoncée, mais la release GitHub n'a pas pu être créée : relancez l'étape en échec.";
        let mut fake = Fake::new();
        fake.tag_lookup_fails = true;
        let error = run_with(&fake, "rg_read_tag").0.unwrap_err();
        assert_eq!(error.lines().next().unwrap(), spec);
        assert!(error.contains("128"));
        assert!(fake.gh_calls().is_empty());

        let mut fake = Fake::new();
        fake.release_lookup_status = 500;
        let error = run_with(&fake, "rg_read_release").0.unwrap_err();
        assert_eq!(error.lines().next().unwrap(), spec);
        assert!(error.contains("500"));
        assert!(fake.gh_calls().is_empty());
    }

    #[test]
    fn a_failing_creation_says_to_rerun_the_failed_step() {
        let spec = "Version déposée et annoncée, mais la release GitHub n'a pas pu être créée : relancez l'étape en échec.";
        let mut fake = Fake::new();
        fake.gh_fails = true;
        let error = run_with(&fake, "rg_gh_fails").0.unwrap_err();
        assert_eq!(error.lines().next().unwrap(), spec);
        assert!(error.contains("502"));
    }

    #[test]
    fn a_creation_that_cannot_be_read_back_is_not_believed() {
        let mut fake = Fake::new();
        fake.gh_forgets = true;
        let error = run_with(&fake, "rg_forgets").0.unwrap_err();
        assert!(error.contains("introuvable après sa création"), "{error}");
    }

    #[test]
    fn the_public_manifest_must_announce_the_version() {
        let fake = Fake::new();
        *fake.announced.borrow_mut() = Some("0.0.9".to_owned());
        let error = run_with(&fake, "rg_manifest_old").0.unwrap_err();
        assert!(error.contains("annonce 0.0.9 et non 0.1.0"), "{error}");
        let fake = Fake::new();
        *fake.announced.borrow_mut() = None;
        assert!(run_with(&fake, "rg_manifest_none").0.is_err());
    }

    #[test]
    fn a_second_run_after_a_full_success_creates_nothing_more() {
        let fake = Fake::new();
        run_with(&fake, "rg_twice_a").0.unwrap();
        let before = fake.gh_calls().len();
        run_with(&fake, "rg_twice_b").0.unwrap();
        assert_eq!(fake.gh_calls().len(), before);
    }

    #[test]
    fn the_staged_commit_must_be_the_launched_one() {
        let (dir, _, _) = staged_dir("rg_staged");
        let stage = dir.join("stage");
        let error = read_staged(&stage, OTHER).unwrap_err();
        assert!(error.contains(COMMIT) && error.contains(OTHER), "{error}");
        assert!(read_staged(&stage, "").unwrap_err().contains("HEARTH_SHA"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_annotated_tag_is_read_through_its_peeled_line() {
        struct Annotated;
        impl Host for Annotated {
            fn run(&self, _: &Call) -> Result<CmdOutput, String> {
                done(
                    0,
                    &format!(
                        "{OTHER}\trefs/tags/v{VERSION}\n{COMMIT}\trefs/tags/v{VERSION}^{{}}\n"
                    ),
                    "",
                )
            }
            fn elapsed(&self) -> Duration {
                Duration::ZERO
            }
            fn sleep(&self, _: Duration) {}
        }
        assert_eq!(
            tag_commit(&Annotated, Path::new("."), VERSION).unwrap(),
            Some(COMMIT.to_owned())
        );
    }

    #[test]
    fn empty_notes_are_refused_before_anything_is_read() {
        let (dir, staged, notes) = staged_dir("rg_notes");
        std::fs::write(&notes, "  \n").unwrap();
        let fake = Fake::new();
        let error =
            ensure_release(&fake, Path::new("."), &staged, &notes, "", &mut |_| {}).unwrap_err();
        let _ = std::fs::remove_dir_all(dir);
        assert_eq!(error, "Les notes de version sont requises.");
        assert!(fake.calls.borrow().is_empty());
    }

    #[test]
    fn the_success_summary_lists_the_commit_the_files_and_the_links() {
        let (dir, staged, _) = staged_dir("rg_summary");
        let text = success_summary(&staged);
        let _ = std::fs::remove_dir_all(dir);
        assert!(text.contains(SUCCESS));
        assert!(text.contains(COMMIT));
        assert!(text.contains(&"a".repeat(64)) && text.contains("Hearth_0.1.0_x64-setup.exe"));
        assert!(text.contains("https://hearth.voikyrioh.fr/api/releases/latest"));
        assert!(text.contains("https://github.com/Voikyrioh/hearth/releases/tag/v0.1.0"));
    }
}
