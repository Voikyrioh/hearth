//! `cargo xtask e2e-double-failure` : la double panne de la mise à jour de l'agent (HRT-27,
//! BR-UPDATE-030 à 034) sur le systemd de la machine jetable de `e2e-install`.
//!
//! Construit le binaire statique (`cargo xtask agent`), puis exécute `deploy/e2e/double-failure-systemd.sh`
//! dans la machine (Debian, systemd réellement démarré) avec ce binaire : le script installe l'agent,
//! joue les sections 1 (double panne), 2 et 2 bis (arrêt voulu), 3 (reprises bornées) et 4 (agent
//! redémarré pendant l'attente de relance) puis le désinstalle. Chaque section affiche `ok - …` dans
//! le journal du job ; la première qui échoue arrête tout avec la fin du journal de systemd.
//! Le script a besoin de `python3`, `curl`, `openssl` et `journalctl` : l'image de la machine
//! (`deploy/e2e/Dockerfile`) les porte.

use crate::agent;
use crate::docker;
use crate::e2e;

pub fn run() -> Result<(), String> {
    agent::run()?;
    let root = docker::repo_root()?;
    e2e::machine(
        &root.join("target").join("dist"),
        "/deploy/e2e/double-failure-systemd.sh",
        &["/dist/hearth-agent"],
        "journalctl -u hearth-agent -u hearth-agent-update --no-pager 2>/dev/null | tail -n 60 || true",
    )?;
    println!("double panne de la mise à jour de l'agent : vert");
    Ok(())
}
