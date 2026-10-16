//! `cargo xtask agent` : le binaire statique de l'agent, construit en conteneur Alpine.
//!
//! Le dépôt est monté en lecture seule ; Cargo et le dossier de cibles vivent dans des volumes
//! nommés (ou, si `HEARTH_XTASK_CACHE` est défini, dans ce dossier de l'hôte : c'est ce que
//! met en cache la CI). Le binaire est copié dans `target/dist/`, vérifié statique dans le
//! conteneur (`file`, `readelf`), puis sa taille et son SHA-256 sont affichés.

use std::path::{Path, PathBuf};

use crate::docker::{self, args, mount_path};
use crate::host::{Call, Host, System};
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
    /// Informatif : `release-stage` ne le compare pas (commit égal et arbre propre impliquent le même
    /// `Cargo.toml`, et le script du conteneur contrôle `--version`).
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
        Self::parse_with(text, "reconstruis l'agent avec cargo xtask agent")
    }

    /// Comme `parse`, avec le geste à conseiller quand le fichier est illisible (celui du client n'est
    /// pas produit par `cargo xtask agent`).
    pub fn parse_with(text: &str, rebuild: &str) -> Result<Self, String> {
        let bad =
            |why: &str| format!("enregistrement de construction illisible ({why}) : {rebuild}");
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
    /// Un relevé « propre » déjà mesuré à ce commit (relu du fichier de `client-record --begin`).
    pub fn clean_at(commit: &str) -> Self {
        Self {
            commit: Ok(commit.to_owned()),
            problems: Ok(Vec::new()),
        }
    }

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

/// Refuse un arbre qui n'est pas le commit (HRT-173, BR-RELEASE-007) : une construction de publication
/// ne part jamais d'un arbre sale, et un état que git n'a pas su dire n'est pas « propre ». `what` dit
/// ce qui est construit (« l'agent », « le client »).
pub fn refuse_unless_clean(survey: &Survey, what: &str) -> Result<(), String> {
    match survey.state() {
        "clean" => Ok(()),
        "dirty" => {
            let problems = survey.problems.as_deref().unwrap_or_default();
            Err(format!(
                "construction de {what} refusée : arbre de travail sale ({} écart(s) : {}). Committe ou supprime ces fichiers (git stash -u pour les non suivis), puis relance. Un binaire signé doit sortir du commit exact qui sera étiqueté",
                problems.len(),
                problems
                    .iter()
                    .take(5)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" ; ")
            ))
        }
        _ => {
            let cause = [survey.commit.as_ref().err(), survey.problems.as_ref().err()]
                .into_iter()
                .flatten()
                .cloned()
                .collect::<Vec<_>>()
                .join(" ; ");
            Err(format!(
                "construction de {what} refusée : l'état de l'arbre de travail est inconnu ({cause}). Git doit répondre pour qu'un arbre soit déclaré propre"
            ))
        }
    }
}

/// Erreurs d'un registre public qui passent d'elles-mêmes : limite de débit, panne passagère. Aucune
/// autre erreur n'est rejouée (même règle que `ci.yml`, job `engine`).
const TRANSIENT_PULL_ERRORS: [&str; 14] = [
    "toomanyrequests",
    "too many requests",
    "rate exceeded",
    "rate limit",
    "bad gateway",
    "service unavailable",
    "gateway timeout",
    "tls handshake timeout",
    "i/o timeout",
    "context deadline exceeded",
    "connection reset",
    "connection refused",
    "unexpected eof",
    "temporary failure in name resolution",
];

fn is_transient_pull_error(output: &str) -> bool {
    let text = output.to_lowercase();
    TRANSIENT_PULL_ERRORS
        .iter()
        .any(|needle| text.contains(needle))
}

const PULL_ATTEMPTS: u32 = 5;

/// Télécharge l'image de la construction avec reprise : cinq essais, attente 5, 10, 20 puis 40 s,
/// SEULEMENT pour une limite de débit ou une panne passagère du registre ; toute autre erreur arrête
/// tout de suite. Image déjà présente : rien à faire. Derrière `Host` (faux dans les tests, jamais de
/// Docker réel).
pub fn pull_image(host: &dyn Host, image: &str, say: &mut dyn FnMut(&str)) -> Result<(), String> {
    let inspect = args(&["image", "inspect", image]);
    if host
        .run(&Call::new("docker", &inspect))
        .is_ok_and(|output| output.success)
    {
        say("image de construction déjà présente");
        return Ok(());
    }
    let pull = args(&["pull", "--quiet", image]);
    let mut wait = std::time::Duration::from_secs(5);
    let mut last = String::new();
    for attempt in 1..=PULL_ATTEMPTS {
        let output = host.run(&Call::new("docker", &pull))?;
        if output.success {
            say(&format!("image téléchargée (essai {attempt})"));
            return Ok(());
        }
        let text = format!(
            "{} {}",
            String::from_utf8_lossy(&output.stdout).trim(),
            output.stderr
        )
        .trim()
        .to_owned();
        say(&format!("essai {attempt}/{PULL_ATTEMPTS} : {text}"));
        if !is_transient_pull_error(&text) {
            return Err(format!(
                "téléchargement de l'image refusé (ce n'est pas une panne passagère du registre) : {text}"
            ));
        }
        last = text;
        if attempt < PULL_ATTEMPTS {
            host.sleep(wait);
            wait = (wait * 2).min(std::time::Duration::from_secs(60));
        }
    }
    Err(format!(
        "téléchargement de l'image abandonné après {PULL_ATTEMPTS} essais (limite de débit ou panne du registre) : {last}"
    ))
}

/// La construction de PUBLICATION (`agent --release`, HRT-173) : l'arbre doit être le commit AVANT et
/// APRÈS, l'image est téléchargée avec reprise, et l'enregistrement de construction est obligatoire
/// (la construction échoue si elle n'a pas pu l'écrire). Les deux étapes qui touchent le monde (`pull`,
/// `build`) sont injectées : les tests n'ont besoin ni de Docker ni de réseau.
pub fn build_release(
    host: &dyn Host,
    root: &Path,
    binary: &Path,
    version: &str,
    pull: &mut dyn FnMut() -> Result<(), String>,
    build: &mut dyn FnMut() -> Result<PathBuf, String>,
    say: &mut dyn FnMut(&str),
) -> Result<PathBuf, String> {
    remove_record(binary, say);
    let before = Survey::take(host, root);
    refuse_unless_clean(&before, "l'agent")?;
    if let Ok(commit) = &before.commit {
        say(&format!("commit construit : {commit}"));
    }
    pull()?;
    let path = build()?;
    let after = Survey::take(host, root);
    if recorded_state(&before, &after) != "clean" {
        refuse_unless_clean(&after, "l'agent")?;
        return Err("construction de l'agent refusée : HEAD a changé pendant la construction. Relance sur un commit stable".to_owned());
    }
    say(&write_build_record(&path, &before, &after, version)?);
    Ok(path)
}

/// Une construction qui échoue ne laisse pas l'enregistrement d'une précédente.
fn remove_record(binary: &Path, say: &mut dyn FnMut(&str)) {
    let record = record_path(binary);
    if let Err(error) = std::fs::remove_file(&record)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        say(&format!(
            "AVERTISSEMENT : {} non supprimé : {error}",
            record.display()
        ));
    }
}

/// Écrit l'enregistrement de construction à côté du binaire produit. Toute erreur est rendue au
/// lecteur comme un avertissement par l'appelant en développement ; `agent --release` en fait une erreur.
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

/// `cargo xtask agent [--release]`. Sans l'option : construction de développement (avertit si l'arbre
/// n'est pas le commit). Avec `--release` : construction de publication, arbre propre exigé (HRT-173).
pub fn run(args: &[String]) -> Result<PathBuf, String> {
    crate::release_stage::check_flags(args, &[], &["--release"])?;
    let release = args.iter().any(|arg| arg == "--release");
    let root = docker::repo_root()?;
    let version = workspace_version(&root)?;
    let binary = root.join("target").join("dist").join("hearth-agent");
    let host = System::new();
    let spec = Build {
        dist: root.join("target").join("dist"),
        out_name: "hearth-agent".to_owned(),
        env: Vec::new(),
        expect_version: version.clone(),
        expect_key: Some(repo_key_line(&root)?),
    };
    if release {
        return build_release(
            &host,
            &root,
            &binary,
            &version,
            &mut || pull_image(&host, IMAGE, &mut |line| println!("{line}")),
            &mut || build(&spec),
            &mut |line| println!("{line}"),
        );
    }
    remove_record(&binary, &mut |line| println!("{line}"));
    let before = report_tree(&host, &root, &mut |line| println!("{line}"));
    let path = build(&spec)?;
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

    // ----- HRT-173 : construction de publication (`agent --release`) -----

    use crate::host::CmdOutput;
    use std::cell::{Cell, RefCell};
    use std::time::Duration;

    /// Faux Docker : `image inspect` répond `present`, chaque `pull` dépile une réponse. Enregistre les
    /// appels et les attentes ; ne lance rien.
    struct DockerFake {
        present: bool,
        pulls: RefCell<Vec<Result<(), String>>>,
        calls: RefCell<Vec<Vec<String>>>,
        sleeps: RefCell<Vec<Duration>>,
    }

    impl DockerFake {
        fn new(present: bool, pulls: Vec<Result<(), &str>>) -> Self {
            Self {
                present,
                pulls: RefCell::new(
                    pulls
                        .into_iter()
                        .rev()
                        .map(|r| r.map_err(str::to_owned))
                        .collect(),
                ),
                calls: Default::default(),
                sleeps: Default::default(),
            }
        }

        fn pull_count(&self) -> usize {
            self.calls
                .borrow()
                .iter()
                .filter(|call| call.first().is_some_and(|a| a == "pull"))
                .count()
        }
    }

    impl Host for DockerFake {
        fn run(&self, call: &Call) -> Result<CmdOutput, String> {
            assert_eq!(
                call.program, "docker",
                "seul docker, jamais d'autre programme"
            );
            self.calls.borrow_mut().push(call.args.to_vec());
            let outcome = if call.args.first().is_some_and(|a| a == "image") {
                if self.present {
                    Ok(())
                } else {
                    Err("No such image".to_owned())
                }
            } else {
                self.pulls.borrow_mut().pop().expect("pull inattendu")
            };
            Ok(match outcome {
                Ok(()) => CmdOutput {
                    success: true,
                    code: Some(0),
                    stdout: Vec::new(),
                    stderr: String::new(),
                },
                Err(text) => CmdOutput {
                    success: false,
                    code: Some(1),
                    stdout: Vec::new(),
                    stderr: text,
                },
            })
        }
        fn elapsed(&self) -> Duration {
            Duration::ZERO
        }
        fn sleep(&self, duration: Duration) {
            self.sleeps.borrow_mut().push(duration);
        }
    }

    const RATE: &str = "toomanyrequests: Rate exceeded";

    fn secs(list: &[u64]) -> Vec<Duration> {
        list.iter().map(|s| Duration::from_secs(*s)).collect()
    }

    #[test]
    fn an_image_already_present_is_not_downloaded() {
        let fake = DockerFake::new(true, vec![]);
        pull_image(&fake, IMAGE, &mut |_| {}).unwrap();
        assert_eq!(fake.pull_count(), 0);
    }

    #[test]
    fn a_rate_limit_is_retried_with_a_growing_wait_until_it_passes() {
        let fake = DockerFake::new(
            false,
            vec![Err(RATE), Err("503 Service Unavailable"), Ok(())],
        );
        pull_image(&fake, IMAGE, &mut |_| {}).unwrap();
        assert_eq!(fake.pull_count(), 3);
        assert_eq!(*fake.sleeps.borrow(), secs(&[5, 10]));
    }

    #[test]
    fn five_failed_attempts_give_up_after_waits_of_5_10_20_and_40_seconds() {
        let fake = DockerFake::new(false, vec![Err(RATE); 5]);
        let error = pull_image(&fake, IMAGE, &mut |_| {}).unwrap_err();
        assert_eq!(fake.pull_count(), 5);
        assert_eq!(*fake.sleeps.borrow(), secs(&[5, 10, 20, 40]));
        assert!(
            error.contains("abandonné après 5 essais") && error.contains("toomanyrequests"),
            "{error}"
        );
    }

    #[test]
    fn any_other_error_stops_at_once_without_waiting() {
        let fake = DockerFake::new(
            false,
            vec![
                Err("denied: requested access to the resource is denied"),
                Ok(()),
            ],
        );
        let error = pull_image(&fake, IMAGE, &mut |_| {}).unwrap_err();
        assert_eq!(fake.pull_count(), 1);
        assert!(fake.sleeps.borrow().is_empty());
        assert!(error.contains("pas une panne passagère"), "{error}");
    }

    #[test]
    fn the_transient_errors_are_the_ones_of_the_ci_and_nothing_else() {
        for transient in [
            "toomanyrequests: Rate exceeded",
            "Too Many Requests",
            "502 Bad Gateway",
            "TLS handshake timeout",
            "dial tcp: i/o timeout",
            "read: connection reset by peer",
            "unexpected end of file, Unexpected EOF",
            "Temporary failure in name resolution",
        ] {
            assert!(is_transient_pull_error(transient), "{transient}");
        }
        for fatal in [
            "manifest unknown",
            "denied",
            "no space left on device",
            "",
            "not found",
        ] {
            assert!(!is_transient_pull_error(fatal), "{fatal}");
        }
    }

    /// Git qui change de réponse quand la construction a eu lieu (`built` passé à `true`).
    struct Switching<'a> {
        before: TreeFake,
        after: TreeFake,
        built: &'a Cell<bool>,
    }

    impl Host for Switching<'_> {
        fn run(&self, call: &Call) -> Result<CmdOutput, String> {
            if self.built.get() {
                self.after.run(call)
            } else {
                self.before.run(call)
            }
        }
        fn elapsed(&self) -> Duration {
            Duration::ZERO
        }
        fn sleep(&self, _: Duration) {}
    }

    /// Un dépôt jetable avec un ancien enregistrement à côté du binaire.
    struct Release {
        root: PathBuf,
        binary: PathBuf,
    }

    impl Release {
        fn new(name: &str) -> Self {
            let root = crate::release_stage::fixtures::tmp_dir(name);
            std::fs::create_dir_all(root.join("target/dist")).unwrap();
            let binary = root.join("target/dist/hearth-agent");
            std::fs::write(
                record_path(&binary),
                "commit=aa\ntree=clean\nsha256=00\nversion=0\n",
            )
            .unwrap();
            Self { root, binary }
        }
    }

    impl Drop for Release {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn a_release_build_on_a_dirty_tree_is_refused_before_anything_is_pulled_or_built() {
        let release = Release::new("agent_release_dirty");
        let (mut pulled, mut built) = (false, false);
        let host = TreeFake::new(" M Cargo.toml", "abc1234def");
        let error = build_release(
            &host,
            &release.root,
            &release.binary,
            "0.1.0",
            &mut || {
                pulled = true;
                Ok(())
            },
            &mut || {
                built = true;
                Ok(PathBuf::new())
            },
            &mut |_| {},
        )
        .unwrap_err();
        assert!(
            error.contains("refusée") && error.contains("sale") && error.contains("Cargo.toml"),
            "{error}"
        );
        assert!(!pulled && !built);
        assert!(
            !record_path(&release.binary).exists(),
            "l'ancien enregistrement a disparu"
        );
    }

    #[test]
    fn an_unknown_tree_state_is_refused_never_taken_for_clean() {
        let release = Release::new("agent_release_unknown");
        let mut mute = TreeFake::new("", "abc1234def");
        mute.status_fails = true;
        let error = build_release(
            &mute,
            &release.root,
            &release.binary,
            "0.1.0",
            &mut || panic!("rien n'est téléchargé"),
            &mut || panic!("rien n'est construit"),
            &mut |_| {},
        )
        .unwrap_err();
        assert!(error.contains("inconnu"), "{error}");
    }

    fn fake_build(binary: &Path) -> impl FnMut() -> Result<PathBuf, String> + '_ {
        move || {
            std::fs::write(binary, b"agent").map_err(|e| e.to_string())?;
            Ok(binary.to_path_buf())
        }
    }

    #[test]
    fn a_release_build_on_a_clean_tree_pulls_builds_and_records_the_commit() {
        let release = Release::new("agent_release_ok");
        let order = RefCell::new(Vec::new());
        let mut lines = Vec::new();
        let host = TreeFake::new("", "abc1234def");
        let mut build = fake_build(&release.binary);
        let path = build_release(
            &host,
            &release.root,
            &release.binary,
            "0.1.0",
            &mut || {
                order.borrow_mut().push("pull");
                Ok(())
            },
            &mut || {
                order.borrow_mut().push("build");
                build()
            },
            &mut |line| lines.push(line.to_owned()),
        )
        .unwrap();
        let record =
            BuildRecord::parse(&std::fs::read_to_string(record_path(&path)).unwrap()).unwrap();
        assert_eq!(*order.borrow(), ["pull", "build"]);
        assert_eq!(record.commit, "abc1234def");
        assert_eq!(record.tree, "clean");
        assert_eq!(record.sha256, sha256_hex(b"agent"));
        assert_eq!(record.version, "0.1.0");
        assert!(
            lines
                .iter()
                .any(|l| l.contains("commit construit : abc1234def"))
        );
        assert!(host.only_git_ran());
    }

    #[test]
    fn a_tree_dirtied_or_a_head_moved_during_the_build_is_refused_and_leaves_no_record() {
        for (label, after, expected) in [
            (
                "sale",
                TreeFake::new(" M Cargo.lock", "abc1234def"),
                "Cargo.lock",
            ),
            ("head", TreeFake::new("", "fff0000"), "HEAD a changé"),
        ] {
            let release = Release::new(&format!("agent_release_after_{label}"));
            let built = Cell::new(false);
            let host = Switching {
                before: TreeFake::new("", "abc1234def"),
                after,
                built: &built,
            };
            let mut build = fake_build(&release.binary);
            let error = build_release(
                &host,
                &release.root,
                &release.binary,
                "0.1.0",
                &mut || Ok(()),
                &mut || {
                    built.set(true);
                    build()
                },
                &mut |_| {},
            )
            .unwrap_err();
            assert!(error.contains(expected), "{label} : {error}");
            assert!(!record_path(&release.binary).exists(), "{label}");
        }
    }

    #[test]
    fn a_record_that_cannot_be_written_fails_the_release_build() {
        let release = Release::new("agent_release_no_record");
        let host = TreeFake::new("", "abc1234def");
        let error = build_release(
            &host,
            &release.root,
            &release.binary,
            "0.1.0",
            &mut || Ok(()),
            &mut || Ok(release.root.join("target/dist/absent")),
            &mut |_| {},
        )
        .unwrap_err();
        assert!(error.contains("absent"), "{error}");
        assert!(!record_path(&release.binary).exists());
    }

    #[test]
    fn a_failing_pull_or_build_stops_the_release_build() {
        let release = Release::new("agent_release_fail");
        let host = TreeFake::new("", "abc1234def");
        let pull = build_release(
            &host,
            &release.root,
            &release.binary,
            "0.1.0",
            &mut || Err("registre muet".to_owned()),
            &mut || panic!("pas de construction sans image"),
            &mut |_| {},
        )
        .unwrap_err();
        let build = build_release(
            &host,
            &release.root,
            &release.binary,
            "0.1.0",
            &mut || Ok(()),
            &mut || Err("docker run a échoué".to_owned()),
            &mut |_| {},
        )
        .unwrap_err();
        assert!(pull.contains("registre muet") && build.contains("docker run"));
        assert!(!record_path(&release.binary).exists());
    }

    #[test]
    fn the_task_refuses_an_unknown_argument_before_touching_anything() {
        assert!(
            run(&["--releas".to_owned()])
                .unwrap_err()
                .contains("argument inconnu")
        );
    }

    #[test]
    fn a_record_of_the_client_is_read_with_its_own_advice() {
        let error = BuildRecord::parse_with("nope", "reconstruis le client").unwrap_err();
        assert!(error.ends_with("reconstruis le client"), "{error}");
    }
}
