//! `cargo xtask shellcheck` : les scripts de `deploy/` passent `shellcheck`, lancé en conteneur.

use crate::docker::{self, args, mount_path};

pub fn run() -> Result<(), String> {
    let deploy = docker::repo_root()?.join("deploy");
    let mut command = args(&["run", "--rm", "--mount"]);
    command.push(format!(
        "type=bind,source={},target=/mnt,readonly",
        mount_path(&deploy)
    ));
    command.extend(args(&[
        "koalaman/shellcheck:stable@sha256:bb596a0d169b85ddd81d8b6d3a2ff6d5baf5fca10b97f575ebc647c3dff62b3d",
        "-s",
        "sh",
        "-S",
        "style",
        "-x",
        "/mnt/install.sh",
        "/mnt/e2e/scenario.sh",
    ]));
    docker::run(&command)?;
    println!("shellcheck : aucun avertissement");
    Ok(())
}
