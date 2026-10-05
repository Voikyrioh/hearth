//! `cargo xtask agent` : le binaire statique de l'agent, construit en conteneur Alpine.
//!
//! Le dépôt est monté en lecture seule ; Cargo et le dossier de cibles vivent dans des volumes
//! nommés (ou, si `HEARTH_XTASK_CACHE` est défini, dans ce dossier de l'hôte : c'est ce que
//! met en cache la CI). Le binaire est copié dans `target/dist/`, vérifié statique dans le
//! conteneur (`file`, `readelf`), puis sa taille et son SHA-256 sont affichés.

use std::path::{Path, PathBuf};

use crate::docker::{self, args, mount_path};

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

pub fn run() -> Result<PathBuf, String> {
    let root = docker::repo_root()?;
    build(&Build {
        dist: root.join("target").join("dist"),
        out_name: "hearth-agent".to_owned(),
        env: Vec::new(),
        expect_version: workspace_version(&root)?,
        expect_key: Some(repo_key_line(&root)?),
    })
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
