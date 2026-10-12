//! `cargo xtask agent` : le binaire statique de l'agent, construit en conteneur Alpine.
//!
//! Le dépôt est monté en lecture seule ; Cargo et le dossier de cibles vivent dans des volumes
//! nommés (ou, si `HEARTH_XTASK_CACHE` est défini, dans ce dossier de l'hôte : c'est ce que
//! met en cache la CI). Le binaire est copié dans `target/dist/`, vérifié statique dans le
//! conteneur (`file`, `readelf`), puis sa taille et son SHA-256 sont affichés.

use std::path::{Path, PathBuf};

use crate::docker::{self, args, mount_path};
use crate::host::{Host, System};
use crate::release_stage::{head_commit, sha256_hex, tree_problems};

pub const IMAGE: &str =
    "rust:1.95-alpine@sha256:606fd313a0f49743ee2a7bd49a0914bab7deedb12791f3a846a34a4711db7ed2";
pub const TARGET: &str = "x86_64-unknown-linux-musl";

/// Exécuté dans le conteneur. Constante : aucune valeur n'y est insérée, tout vient de
/// l'environnement. `ring` et SQLite compilent du C (paquets `build-base`, `musl-dev`).
const SCRIPT: &str = r#"
set -eu
apk add --no-cache build-base musl-dev file >/dev/null
RUSTUP_TOOLCHAIN="$(ls /usr/local/rustup/toolchains | head -n 1)"
export RUSTUP_TOOLCHAIN
cd /src
cargo build --release --locked -p hearth-agent --bin hearth-agent --target "$HEARTH_TARGET"
bin="$CARGO_TARGET_DIR/$HEARTH_TARGET/release/hearth-agent"
cp "$bin" "/dist/$HEARTH_OUT_NAME"
chmod 0755 "/dist/$HEARTH_OUT_NAME"
echo "--- file"
file "/dist/$HEARTH_OUT_NAME"
if ! file "/dist/$HEARTH_OUT_NAME" | grep -Eq "statically linked|static-pie linked"; then
  echo "le binaire n'est pas lie statiquement" >&2
  exit 3
fi
echo "--- ldd (un static-pie y apparait comme son propre chargeur)"
ldd "/dist/$HEARTH_OUT_NAME" 2>&1 || true
echo "--- readelf"
if readelf -l "/dist/$HEARTH_OUT_NAME" | grep -q INTERP; then
  echo "le binaire demande un interpreteur dynamique (PT_INTERP)" >&2
  exit 4
fi
if readelf -d "/dist/$HEARTH_OUT_NAME" 2>/dev/null | grep -q NEEDED; then
  echo "le binaire a des bibliotheques partagees requises (NEEDED)" >&2
  exit 4
fi
echo "statique : ni interpreteur, ni bibliotheque partagee requise"
echo "--- version"
version="$("/dist/$HEARTH_OUT_NAME" --version)"
echo "$version"
if [ "$version" != "hearth-agent $HEARTH_EXPECT_VERSION" ]; then
  echo "la version du binaire n'est pas celle attendue ($HEARTH_EXPECT_VERSION)" >&2
  exit 5
fi
echo "--- build-info"
info="$("/dist/$HEARTH_OUT_NAME" build-info)"
echo "$info"
if [ -n "${HEARTH_EXPECT_KEY:-}" ]; then
  # Construction de publication : la clé du dépôt, et rien qui la distingue d'une publication.
  printf '%s\n' "$info" | grep -qxF "cle: $HEARTH_EXPECT_KEY" || { echo "la clé embarquée n'est pas celle du dépôt" >&2; exit 6; }
  printf '%s\n' "$info" | grep -qx "notes: " || { echo "la construction porte des notes de test" >&2; exit 7; }
fi
echo "--- sha256"
sha256sum "/dist/$HEARTH_OUT_NAME"
echo "--- taille"
wc -c < "/dist/$HEARTH_OUT_NAME"
"#;

/// Une construction : nom du fichier produit dans `target/dist/` et variables passées à `cargo
/// build` dans le conteneur (la clé publique de mise à jour et la version, pour les tests de bout
/// en bout de la mise à jour ; une construction de publication n'en passe aucune).
pub struct Build {
    pub dist: PathBuf,
    pub out_name: String,
    pub env: Vec<(String, String)>,
    /// La version que `--version` doit rendre (celle du crate, ou `HEARTH_AGENT_VERSION`).
    pub expect_version: String,
    /// Première ligne de la clé publique du dépôt, pour une construction de publication.
    pub expect_key: Option<String>,
}

/// Enregistrement de construction, écrit à côté du binaire (`<binaire>.build`) après une construction de
/// PUBLICATION réussie, jamais assemblé ni déposé. `release-stage` le relit : somme du fichier donné,
/// commit égal à `HEAD`, arbre `clean`. Quatre lignes `clé=valeur`. Fichier en clair, falsifiable à la
/// main : une garde contre l'erreur (binaire d'un autre commit, d'un arbre modifié), pas contre la
/// malveillance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildRecord {
    pub commit: String,
    /// `clean`, `dirty` ou `unknown` (git muet, ou `HEAD` / arbre changés pendant la construction).
    pub tree: String,
    pub sha256: String,
    pub version: String,
}

impl BuildRecord {
    pub fn render(&self) -> String {
        format!(
            "commit={}\ntree={}\nsha256={}\nversion={}\n",
            self.commit, self.tree, self.sha256, self.version
        )
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let bad = |why: &str| {
            format!(
                "enregistrement de construction illisible ({why}) : reconstruis l'agent avec cargo xtask agent"
            )
        };
        let mut fields = std::collections::BTreeMap::new();
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            let (key, value) = line.split_once('=').ok_or_else(|| bad("ligne sans ="))?;
            if fields.insert(key.trim(), value.trim()).is_some() {
                return Err(bad("clé en double"));
            }
        }
        let mut take = |key: &str| {
            fields
                .remove(key)
                .map(str::to_owned)
                .ok_or_else(|| bad(&format!("« {key} » manque")))
        };
        let record = Self {
            commit: take("commit")?,
            tree: take("tree")?,
            sha256: take("sha256")?,
            version: take("version")?,
        };
        if !fields.is_empty() {
            return Err(bad("clé inconnue"));
        }
        let hex = |value: &str| !value.is_empty() && value.chars().all(|c| c.is_ascii_hexdigit());
        if !hex(&record.commit) || !hex(&record.sha256) {
            return Err(bad("commit ou somme pas hexadécimal"));
        }
        if !["clean", "dirty", "unknown"].contains(&record.tree.as_str()) {
            return Err(bad("état de l'arbre inconnu"));
        }
        Ok(record)
    }
}

/// Le fichier d'enregistrement d'un binaire : le même nom suivi de `.build`.
pub fn record_path(binary: &Path) -> PathBuf {
    let mut name = binary.as_os_str().to_owned();
    name.push(".build");
    PathBuf::from(name)
}

/// Ce que git dit de l'arbre à un instant : le commit, et ce qui le sépare du disque (suivis modifiés,
/// non suivis des entrées de la construction, masqués). Une erreur n'est jamais lue comme « propre ».
pub struct Survey {
    pub commit: Result<String, String>,
    pub problems: Result<Vec<String>, String>,
}

impl Survey {
    pub fn take(host: &dyn Host, root: &Path) -> Self {
        Self {
            commit: head_commit(host, root),
            problems: tree_problems(host, root),
        }
    }

    /// `clean` seulement si le commit est connu ET rien ne sépare l'arbre de lui.
    pub fn state(&self) -> &'static str {
        match (&self.commit, &self.problems) {
            (Ok(_), Ok(problems)) if problems.is_empty() => "clean",
            (_, Ok(problems)) if !problems.is_empty() => "dirty",
            _ => "unknown",
        }
    }
}

/// L'état à enregistrer d'après les deux relevés (avant et après la construction) : `clean` seulement
/// si les deux sont propres et sur le même commit, ce qui borne aussi la fenêtre pendant la construction.
pub fn recorded_state(before: &Survey, after: &Survey) -> &'static str {
    match (before.state(), after.state()) {
        ("clean", "clean") if before.commit == after.commit => "clean",
        ("dirty", _) | (_, "dirty") => "dirty",
        _ => "unknown",
    }
}

/// Dit quel commit est construit et avertit si l'arbre n'est pas ce commit (HRT-60). Ne refuse jamais :
/// la commande sert aussi au développement et aux tâches e2e. Un git muet est signalé, pas fatal.
pub fn report(survey: &Survey, say: &mut dyn FnMut(&str)) {
    match &survey.commit {
        Ok(commit) => say(&format!("commit construit : {commit}")),
        Err(error) => say(&format!(
            "AVERTISSEMENT : commit en place inconnu ({error})"
        )),
    }
    match &survey.problems {
        Ok(problems) if problems.is_empty() => {}
        Ok(problems) => say(&format!(
            "AVERTISSEMENT : l'arbre n'est pas le commit : {} écart(s) ({}) : ce binaire ne sort pas du commit seul, release-stage le refusera",
            problems.len(),
            problems
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(" ; ")
        )),
        Err(error) => say(&format!(
            "AVERTISSEMENT : état de l'arbre inconnu ({error}) : release-stage refusera ce binaire"
        )),
    }
}

pub fn report_tree(host: &dyn Host, root: &Path, say: &mut dyn FnMut(&str)) -> Survey {
    let survey = Survey::take(host, root);
    report(&survey, say);
    survey
}

/// Écrit l'enregistrement de construction à côté du binaire produit. Toute erreur est rendue au
/// lecteur comme un avertissement par l'appelant : elle ne fait jamais échouer la construction.
pub fn write_build_record(
    binary: &Path,
    before: &Survey,
    after: &Survey,
    version: &str,
) -> Result<String, String> {
    let bytes = std::fs::read(binary).map_err(|error| format!("{} : {error}", binary.display()))?;
    let commit = match (&before.commit, &after.commit) {
        (Ok(commit), _) | (_, Ok(commit)) => commit.clone(),
        _ => return Err("commit en place inconnu, aucun enregistrement écrit".to_owned()),
    };
    let record = BuildRecord {
        commit,
        tree: recorded_state(before, after).to_owned(),
        sha256: sha256_hex(&bytes),
        version: version.to_owned(),
    };
    let path = record_path(binary);
    std::fs::write(&path, record.render())
        .map_err(|error| format!("{} : {error}", path.display()))?;
    Ok(format!(
        "enregistrement de construction : {} (commit {}, arbre {})",
        path.display(),
        record.commit,
        record.tree
    ))
}

pub fn run() -> Result<PathBuf, String> {
    let root = docker::repo_root()?;
    let version = workspace_version(&root)?;
    let binary = root.join("target").join("dist").join("hearth-agent");
    // Une construction qui échoue ne laisse pas l'enregistrement d'une précédente.
    let record = record_path(&binary);
    if let Err(error) = std::fs::remove_file(&record)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        println!(
            "AVERTISSEMENT : {} non supprimé : {error}",
            record.display()
        );
    }
    let host = System::new();
    let before = report_tree(&host, &root, &mut |line| println!("{line}"));
    let path = build(&Build {
        dist: root.join("target").join("dist"),
        out_name: "hearth-agent".to_owned(),
        env: Vec::new(),
        expect_version: version.clone(),
        expect_key: Some(repo_key_line(&root)?),
    })?;
    let after = Survey::take(&host, &root);
    match write_build_record(&path, &before, &after, &version) {
        Ok(line) => println!("{line}"),
        Err(error) => println!(
            "AVERTISSEMENT : enregistrement de construction non écrit ({error}) : release-stage refusera ce binaire"
        ),
    }
    if recorded_state(&before, &after) != "clean" {
        println!(
            "AVERTISSEMENT : l'arbre ou HEAD n'était pas propre et stable pendant la construction : ne signe pas ce binaire"
        );
    }
    Ok(path)
}

pub fn build(build: &Build) -> Result<PathBuf, String> {
    let root = docker::repo_root()?;
    let dist = build.dist.clone();
    std::fs::create_dir_all(&dist)
        .map_err(|error| format!("création de {} impossible : {error}", dist.display()))?;

    let mut command = args(&["run", "--rm"]);
    // Une construction à variables de test a son propre dossier de cibles : le binaire publié ne
    // peut jamais embarquer la clé jetable ni la version d'un test (garanti par le dossier, vérifié
    // par `--version`).
    command.extend(mounts(&root, &dist, !build.env.is_empty())?);
    command.push("-e".to_owned());
    command.push(format!("HEARTH_EXPECT_VERSION={}", build.expect_version));
    if let Some(key) = &build.expect_key {
        command.push("-e".to_owned());
        command.push(format!("HEARTH_EXPECT_KEY={key}"));
    }
    for (name, value) in &build.env {
        command.push("-e".to_owned());
        command.push(format!("{name}={value}"));
    }
    command.extend(args(&[
        "-e",
        "CARGO_HOME=/cargo",
        "-e",
        "CARGO_TARGET_DIR=/target",
        "-e",
        "SQLX_OFFLINE=true",
        "-e",
        &format!("HEARTH_TARGET={TARGET}"),
        "-e",
        &format!("HEARTH_OUT_NAME={}", build.out_name),
        // `crt-static` est déjà le défaut de la cible musl ; on l'écrit pour qu'un changement
        // d'image ne le change pas en silence.
        "-e",
        "RUSTFLAGS=-C target-feature=+crt-static",
        "-w",
        "/src",
        IMAGE,
        "sh",
        "-c",
        SCRIPT,
    ]));
    println!("construction du binaire {TARGET} dans {IMAGE} (la première fois est longue)...");
    docker::run(&command)?;

    let path = dist.join(&build.out_name);
    let size = std::fs::metadata(&path)
        .map_err(|error| format!("{} introuvable : {error}", path.display()))?
        .len();
    println!(
        "binaire : {} ({} octets, {:.1} Mio), statique vérifié",
        path.display(),
        size,
        size as f64 / (1024.0 * 1024.0)
    );
    Ok(path)
}

/// La première ligne de `update-key.pub` (`untrusted comment: minisign public key: …`).
fn repo_key_line(root: &Path) -> Result<String, String> {
    let path = root
        .join("crates")
        .join("hearth-agent")
        .join("update-key.pub");
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("lecture de {} impossible : {error}", path.display()))?;
    text.lines()
        .next()
        .map(str::to_owned)
        .ok_or_else(|| "update-key.pub vide".to_owned())
}

/// La version du workspace (`[workspace.package] version`).
fn workspace_version(root: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(root.join("Cargo.toml"))
        .map_err(|error| format!("lecture de Cargo.toml impossible : {error}"))?;
    text.lines()
        .find_map(|line| {
            line.strip_prefix("version = \"")
                .and_then(|rest| rest.strip_suffix('"'))
                .map(str::to_owned)
        })
        .ok_or_else(|| "version du workspace introuvable".to_owned())
}

fn mounts(root: &Path, dist: &Path, test_build: bool) -> Result<Vec<String>, String> {
    let mut out = args(&["--mount"]);
    out.push(format!(
        "type=bind,source={},target=/src,readonly",
        mount_path(root)
    ));
    out.push("--mount".to_owned());
    out.push(format!(
        "type=bind,source={},target=/dist",
        mount_path(dist)
    ));
    match std::env::var("HEARTH_XTASK_CACHE") {
        Ok(cache) if !cache.is_empty() => {
            let base = PathBuf::from(cache);
            for name in ["cargo", "target"] {
                let dir_name = if test_build && name == "target" {
                    "target-e2e"
                } else {
                    name
                };
                let dir = base.join(dir_name);
                std::fs::create_dir_all(&dir).map_err(|error| {
                    format!("création de {} impossible : {error}", dir.display())
                })?;
                let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
                out.push("--mount".to_owned());
                out.push(format!(
                    "type=bind,source={},target=/{name}",
                    mount_path(&dir)
                ));
            }
        }
        _ => {
            let target_volume = if test_build {
                "hearth-xtask-target-e2e"
            } else {
                "hearth-xtask-target"
            };
            for (volume, target) in [("hearth-xtask-cargo", "/cargo"), (target_volume, "/target")] {
                out.push("--mount".to_owned());
                out.push(format!("type=volume,source={volume},target={target}"));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::release_stage::fixtures::TreeFake;

    fn said(fake: &TreeFake) -> Vec<String> {
        let mut lines = Vec::new();
        report_tree(fake, Path::new("depot"), &mut |line| {
            lines.push(line.to_owned())
        });
        lines
    }

    #[test]
    fn a_clean_tree_prints_the_commit_and_no_warning() {
        let lines = said(&TreeFake::new("", "abc1234def"));
        assert_eq!(lines, ["commit construit : abc1234def"]);
    }

    #[test]
    fn a_modified_tree_warns_but_never_fails() {
        let lines = said(&TreeFake::new(" M Cargo.toml", "abc1234def"));
        assert_eq!(lines.len(), 2);
        assert!(lines[1].starts_with("AVERTISSEMENT") && lines[1].contains("Cargo.toml"));
    }

    #[test]
    fn an_untracked_build_input_warns() {
        let mut fake = TreeFake::new("", "abc1234def");
        fake.untracked = "crates/hearth-agent/migrations/0005_x.sql".to_owned();
        let lines = said(&fake);
        assert!(lines[1].contains("0005_x.sql"), "{lines:?}");
    }

    #[test]
    fn a_git_that_cannot_answer_warns_and_does_not_stop_the_build() {
        let mut fake = TreeFake::new("", "abc1234def");
        fake.status_fails = true;
        let lines = said(&fake);
        assert!(
            lines.iter().any(|l| l.contains("état de l'arbre inconnu")),
            "{lines:?}"
        );
    }

    #[test]
    fn a_git_that_cannot_even_be_launched_warns_twice_and_never_fails() {
        let mut fake = TreeFake::new("", "abc1234def");
        fake.cannot_launch = true;
        let lines = said(&fake);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines.iter().all(|l| l.starts_with("AVERTISSEMENT")));
        let survey = Survey::take(&fake, Path::new("depot"));
        assert_eq!(survey.state(), "unknown");
    }

    #[test]
    fn the_record_is_rendered_and_parsed_back_and_refuses_anything_else() {
        let record = BuildRecord {
            commit: "abc1234def".to_owned(),
            tree: "clean".to_owned(),
            sha256: "00ff".to_owned(),
            version: "0.1.0".to_owned(),
        };
        assert_eq!(BuildRecord::parse(&record.render()).unwrap(), record);
        for bad in [
            "",
            "commit=abc\ntree=clean\nsha256=00\n",
            "commit=abc\ntree=clean\nsha256=00\nversion=1\nautre=1\n",
            "commit=abc\ncommit=abc\ntree=clean\nsha256=00\nversion=1\n",
            "commit=zz\ntree=clean\nsha256=00\nversion=1\n",
            "commit=abc\ntree=propre\nsha256=00\nversion=1\n",
            "n'importe quoi",
        ] {
            assert!(BuildRecord::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_record_sits_next_to_the_binary() {
        assert_eq!(
            record_path(Path::new("target/dist/hearth-agent")),
            PathBuf::from("target/dist/hearth-agent.build")
        );
    }

    #[test]
    fn the_recorded_state_is_clean_only_when_both_surveys_are_clean_on_the_same_commit() {
        let take =
            |status: &str, head: &str| Survey::take(&TreeFake::new(status, head), Path::new("d"));
        let clean = || take("", "abc1234def");
        assert_eq!(recorded_state(&clean(), &clean()), "clean");
        assert_eq!(recorded_state(&clean(), &take("", "fff0000")), "unknown");
        assert_eq!(
            recorded_state(&take(" M a", "abc1234def"), &clean()),
            "dirty"
        );
        assert_eq!(
            recorded_state(&clean(), &take(" M a", "abc1234def")),
            "dirty"
        );
        let mut mute = TreeFake::new("", "abc1234def");
        mute.cannot_launch = true;
        let mute = Survey::take(&mute, Path::new("d"));
        assert_eq!(recorded_state(&clean(), &mute), "unknown");
    }

    #[test]
    fn the_record_written_after_a_build_matches_the_file_and_a_failure_to_write_is_an_error() {
        let dir = crate::release_stage::fixtures::tmp_dir("agent_record");
        std::fs::create_dir_all(&dir).unwrap();
        let binary = dir.join("hearth-agent");
        std::fs::write(&binary, b"agent").unwrap();
        let clean = Survey::take(&TreeFake::new("", "abc1234def"), Path::new("d"));
        let line = write_build_record(&binary, &clean, &clean, "0.1.0").unwrap();
        let text = std::fs::read_to_string(record_path(&binary)).unwrap();
        let missing = write_build_record(&dir.join("absent"), &clean, &clean, "0.1.0");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            line.contains("abc1234def") && line.contains("clean"),
            "{line}"
        );
        let record = BuildRecord::parse(&text).unwrap();
        assert_eq!(record.sha256, sha256_hex(b"agent"));
        assert_eq!(record.tree, "clean");
        assert!(missing.is_err());
    }
}
