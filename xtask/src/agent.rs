//! `cargo xtask agent` : le binaire statique de l'agent, construit en conteneur Alpine.
//!
//! Le dépôt est monté en lecture seule ; Cargo et le dossier de cibles vivent dans des volumes
//! nommés (ou, si `HEARTH_XTASK_CACHE` est défini, dans ce dossier de l'hôte : c'est ce que
//! met en cache la CI). Le binaire est copié dans `target/dist/`, vérifié statique dans le
//! conteneur (`file`, `readelf`), puis sa taille et son SHA-256 sont affichés.

use std::path::{Path, PathBuf};

use crate::docker::{self, args, mount_path};

pub const IMAGE: &str = "rust:1.95-alpine";
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
cp "$bin" /dist/hearth-agent
chmod 0755 /dist/hearth-agent
echo "--- file"
file /dist/hearth-agent
if ! file /dist/hearth-agent | grep -Eq "statically linked|static-pie linked"; then
  echo "le binaire n'est pas lie statiquement" >&2
  exit 3
fi
echo "--- ldd (un static-pie y apparait comme son propre chargeur)"
ldd /dist/hearth-agent 2>&1 || true
echo "--- readelf"
if readelf -l /dist/hearth-agent | grep -q INTERP; then
  echo "le binaire demande un interpreteur dynamique (PT_INTERP)" >&2
  exit 4
fi
if readelf -d /dist/hearth-agent 2>/dev/null | grep -q NEEDED; then
  echo "le binaire a des bibliotheques partagees requises (NEEDED)" >&2
  exit 4
fi
echo "statique : ni interpreteur, ni bibliotheque partagee requise"
echo "--- sha256"
sha256sum /dist/hearth-agent
echo "--- taille"
wc -c < /dist/hearth-agent
"#;

pub fn run() -> Result<PathBuf, String> {
    let root = docker::repo_root()?;
    let dist = root.join("target").join("dist");
    std::fs::create_dir_all(&dist)
        .map_err(|error| format!("création de {} impossible : {error}", dist.display()))?;

    let mut command = args(&["run", "--rm"]);
    command.extend(mounts(&root, &dist)?);
    command.extend(args(&[
        "-e",
        "CARGO_HOME=/cargo",
        "-e",
        "CARGO_TARGET_DIR=/target",
        "-e",
        "SQLX_OFFLINE=true",
        "-e",
        &format!("HEARTH_TARGET={TARGET}"),
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

    let path = dist.join("hearth-agent");
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

fn mounts(root: &Path, dist: &Path) -> Result<Vec<String>, String> {
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
                let dir = base.join(name);
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
            for (volume, target) in [
                ("hearth-xtask-cargo", "/cargo"),
                ("hearth-xtask-target", "/target"),
            ] {
                out.push("--mount".to_owned());
                out.push(format!("type=volume,source={volume},target={target}"));
            }
        }
    }
    Ok(out)
}
