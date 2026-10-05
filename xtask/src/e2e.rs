//! `cargo xtask e2e-install` : l'installation de l'agent sur une vraie machine jetable.
//!
//! Construit le binaire statique (`cargo xtask agent`), démarre un conteneur Debian **avec
//! systemd réellement démarré** (comme un vrai serveur), y monte le binaire et `deploy/`, et y
//! exécute `deploy/e2e/scenario.sh` : installation par `install.sh`, `/hello`, connexion,
//! empreinte, réinstallation, désinstallation avec conservation puis purge, retour en arrière,
//! installation gérée. Le conteneur est supprimé quoi qu'il arrive. `machine` est aussi la machine
//! de `cargo xtask e2e-update` (mise à jour de l'agent à distance).

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::agent;
use crate::docker::{self, args, mount_path};

const IMAGE: &str = "hearth-e2e-systemd";

/// Supprime le conteneur à la sortie, y compris sur erreur.
struct Container(String);

impl Drop for Container {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["rm", "-f", &self.0])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

pub fn run() -> Result<(), String> {
    agent::run()?;
    let root = docker::repo_root()?;
    machine(
        &root.join("target").join("dist"),
        "/deploy/e2e/scenario.sh",
        "journalctl -u hearth-agent --no-pager 2>/dev/null | tail -n 40 || true",
    )?;
    println!("bout en bout : vert");
    Ok(())
}

/// Démarre la machine jetable (`dist` monté sur `/dist`, `deploy/` sur `/deploy`, tous deux en
/// lecture seule), y exécute `scenario` (un script de `/deploy`) ; si le scénario échoue, affiche
/// `diagnostic` (une commande constante exécutée dans le conteneur). Le conteneur est supprimé.
pub fn machine(dist: &Path, scenario: &str, diagnostic: &str) -> Result<(), String> {
    let root = docker::repo_root()?;
    let deploy = root.join("deploy");

    println!("image {IMAGE} (Debian + systemd)...");
    docker::run(&[
        "build".to_owned(),
        "-q".to_owned(),
        "-t".to_owned(),
        IMAGE.to_owned(),
        mount_path(&deploy.join("e2e")),
    ])?;

    let name = format!("hearth-e2e-{}", std::process::id());
    let _guard = Container(name.clone());
    let mut command = args(&[
        "run",
        "-d",
        "--name",
        &name,
        "--privileged",
        "--cgroupns=host",
        "--tmpfs",
        "/run",
        "--tmpfs",
        "/run/lock",
        "--tmpfs",
        "/tmp:exec,mode=1777",
        "--mount",
        "type=bind,source=/sys/fs/cgroup,target=/sys/fs/cgroup",
    ]);
    command.extend([
        "--mount".to_owned(),
        format!(
            "type=bind,source={},target=/dist,readonly",
            mount_path(dist)
        ),
        "--mount".to_owned(),
        format!(
            "type=bind,source={},target=/deploy,readonly",
            mount_path(&deploy)
        ),
        IMAGE.to_owned(),
    ]);
    docker::run(&command)?;
    wait_for_systemd(&name)?;

    println!("scénario...");
    let result = docker::run(&args(&["exec", &name, "sh", scenario]));
    if result.is_err() {
        // Ce que dit le service, pour comprendre un échec.
        let _ = docker::run(&args(&["exec", &name, "sh", "-c", diagnostic]));
    }
    result
}

/// Attend que systemd ait fini de démarrer (`running`, ou `degraded` : des unités inutiles dans
/// un conteneur peuvent échouer sans gêner).
fn wait_for_systemd(name: &str) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let output = Command::new("docker")
            .args(["exec", name, "systemctl", "is-system-running"])
            .stdin(Stdio::null())
            .output()
            .map_err(|error| format!("lancement de docker impossible : {error}"))?;
        let state = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if state == "running" || state == "degraded" {
            println!("systemd démarré ({state})");
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "systemd n'a pas démarré dans le conteneur (état : {state:?}). Docker doit autoriser --privileged et --cgroupns=host."
            ));
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}
