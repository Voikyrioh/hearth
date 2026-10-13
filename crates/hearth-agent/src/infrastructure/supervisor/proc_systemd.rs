//! Le superviseur systemd, par `/proc` et `systemctl show` (HRT-63, ADR-0037). DÉTECTION SEULE.
//!
//! **Propriétaire d'un conteneur.** Un conteneur lancé par une unité (`ExecStart=docker run --name=x
//! ...`, ou `systemd-run docker run ...`) a un processus client du moteur (`docker` ou `podman`)
//! dans le groupe de contrôle de l'unité. On le cherche dans `/proc` : le programme (`exe`, à défaut
//! le premier argument), sa ligne de commande (un `run` qui nomme le conteneur, ou un `start`), puis
//! `cgroup`. Ce n'est qu'une REVENDICATION : le client doit encore être le processus principal de
//! l'unité (`MainPID` de `systemctl show`), sinon un service qui lance des conteneurs au premier plan
//! (tâche planifiée, exécuteur de CI) passerait pour eux. Le client d'un `docker run -d` qui a déjà rendu
//! la main n'existe plus : son conteneur n'a pas de propriétaire connu (limite assumée, ADR-0037).
//!
//! **État d'une unité.** `systemctl show --property=... -- <unité>`, lancé SANS interpréteur, chaque
//! argument un par un, après `--`, avec un délai. Le nom d'unité est un `UnitName` : validé avant
//! d'arriver ici. `systemctl` absent du `PATH`, système sans systemd, délai dépassé ou sortie
//! illisible sont des cas NORMAUX : `UnitLookup::Unavailable`, sans bruit.
//!
//! Limites de lecture : au plus `MAX_PROCESSES` processus parcourus, `SCAN_BUDGET` de temps, 64 Kio de
//! ligne de commande et 16 Kio de groupe de contrôle par processus.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::io::AsyncReadExt;

use crate::application::ports::{ContainerRef, ServiceSupervisor};
use crate::domain::services::unit::SHOW_PROPERTIES;
use crate::domain::services::{
    ClientClaim, UnitLookup, UnitName, attribute, client_refs, is_engine_client, parse_main_pid,
    parse_show, unit_of_cgroup,
};
use crate::infrastructure::install::scrub::scrubbed;

/// Processus parcourus au plus par recherche.
const MAX_PROCESSES: usize = 100_000;
/// Temps maximal d'une recherche : au-delà, ce qui est trouvé est rendu.
const SCAN_BUDGET: Duration = Duration::from_secs(2);
const MAX_CMDLINE_BYTES: u64 = 64 * 1024;
const MAX_CGROUP_BYTES: u64 = 16 * 1024;
/// Sortie de `systemctl show` lue au plus.
const MAX_SHOW_BYTES: u64 = 16 * 1024;
/// Unités vérifiées au plus par recherche (une par `systemctl show`).
const MAX_CLAIMED_UNITS: usize = 32;
/// Délai de `systemctl show`.
const SHOW_TIMEOUT: Duration = Duration::from_secs(5);

pub struct ProcSystemd {
    proc_root: PathBuf,
    systemctl: OsString,
    timeout: Duration,
}

impl ProcSystemd {
    /// Le `/proc` et le `systemctl` (du `PATH`) de cette machine.
    pub fn system() -> Self {
        Self::new(PathBuf::from("/proc"), OsString::from("systemctl"))
    }

    /// Racine de `/proc` et programme `systemctl` explicites (tests : faux arbre, faux `systemctl`).
    pub fn new(proc_root: PathBuf, systemctl: OsString) -> Self {
        Self {
            proc_root,
            systemctl,
            timeout: SHOW_TIMEOUT,
        }
    }

    /// Un autre délai pour `systemctl show` (tests).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Lance `systemctl show` ; `None` si la commande n'existe pas, échoue, dépasse son délai ou ne
    /// rend rien d'exploitable.
    async fn show(&self, unit: &UnitName) -> Option<String> {
        let mut command = tokio::process::Command::from(scrubbed(&self.systemctl));
        command
            .arg("show")
            .arg(format!("--property={SHOW_PROPERTIES}"))
            .arg("--")
            .arg(unit.as_str())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let work = async {
            let mut child = command.spawn().ok()?;
            let mut stdout = child.stdout.take()?.take(MAX_SHOW_BYTES);
            let mut bytes = Vec::new();
            stdout.read_to_end(&mut bytes).await.ok()?;
            let status = child.wait().await.ok()?;
            status
                .success()
                .then(|| String::from_utf8_lossy(&bytes).into_owned())
        };
        tokio::time::timeout(self.timeout, work)
            .await
            .ok()
            .flatten()
    }
}

#[async_trait]
impl ServiceSupervisor for ProcSystemd {
    async fn owners(&self, containers: &[ContainerRef]) -> HashMap<String, UnitName> {
        if containers.is_empty() {
            return HashMap::new();
        }
        let root = self.proc_root.clone();
        let wanted = containers.to_vec();
        // Lectures de fichiers : hors de la tâche asynchrone.
        let claims = tokio::task::spawn_blocking(move || scan(&root, &wanted))
            .await
            .unwrap_or_default();
        // Un client du moteur dans le groupe de contrôle d'un service ne suffit pas : il faut qu'il soit
        // le processus principal de l'unité (`MainPID`). Une unité qui ne répond pas n'est pas crue.
        let mut units: Vec<UnitName> = claims.iter().map(|claim| claim.unit.clone()).collect();
        units.sort();
        units.dedup();
        units.truncate(MAX_CLAIMED_UNITS);
        let mut main_pids: HashMap<UnitName, u32> = HashMap::new();
        for unit in units {
            if let Some(pid) = self
                .show(&unit)
                .await
                .and_then(|text| parse_main_pid(&text))
            {
                main_pids.insert(unit, pid);
            }
        }
        attribute(&claims, &main_pids)
    }

    async fn unit(&self, unit: &UnitName) -> UnitLookup {
        match self.show(unit).await {
            Some(text) => parse_show(&text),
            None => UnitLookup::Unavailable,
        }
    }
}

/// Parcourt `proc_root` : pour chaque client du moteur dont la ligne de commande désigne l'un des
/// conteneurs cherchés, la revendication de l'unité de service de son groupe de contrôle (processus,
/// conteneur, unité). Aucune attribution ici : `domain::services::attribute` décide, après avoir lu le
/// processus principal de chaque unité.
pub fn scan(proc_root: &Path, wanted: &[ContainerRef]) -> Vec<ClientClaim> {
    let Ok(entries) = std::fs::read_dir(proc_root) else {
        return Vec::new();
    };
    let started = Instant::now();
    let mut claims: Vec<ClientClaim> = Vec::new();
    for entry in entries.flatten().take(MAX_PROCESSES) {
        if started.elapsed() > SCAN_BUDGET {
            break;
        }
        let file_name = entry.file_name();
        let Some(pid) = file_name
            .to_str()
            .filter(|name| name.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let dir = entry.path();
        let args = read_cmdline(&dir.join("cmdline"));
        let Some(program) = program_of(&dir, &args) else {
            continue;
        };
        if !is_engine_client(&program) {
            continue;
        }
        let refs = client_refs(&args);
        if refs.is_empty() {
            continue;
        }
        let Some(unit) = read_limited(&dir.join("cgroup"), MAX_CGROUP_BYTES)
            .and_then(|text| unit_of_cgroup(&text))
        else {
            continue;
        };
        for container in wanted {
            if refs
                .iter()
                .any(|reference| designates(reference, container))
            {
                claims.push(ClientClaim {
                    container: container.id.clone(),
                    unit: unit.clone(),
                    pid,
                });
            }
        }
    }
    claims
}

/// Le nom, l'identifiant complet ou un préfixe d'au moins 12 caractères de l'identifiant.
fn designates(reference: &str, container: &ContainerRef) -> bool {
    reference == container.name
        || reference == container.id
        || (reference.len() >= 12 && container.id.starts_with(reference))
}

/// Le programme d'un processus : la cible du lien `exe`, à défaut le premier argument (le lien est
/// illisible sans droits suffisants ; les faux arbres des tests n'ont pas de lien).
fn program_of(dir: &Path, args: &[String]) -> Option<String> {
    match std::fs::read_link(dir.join("exe")) {
        Ok(target) => Some(target.to_string_lossy().into_owned()),
        Err(_) => args.first().cloned(),
    }
}

fn read_limited(path: &Path, limit: u64) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(limit).read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// `/proc/<pid>/cmdline` : arguments séparés par des octets nuls.
fn read_cmdline(path: &Path) -> Vec<String> {
    let Some(text) = read_limited(path, MAX_CMDLINE_BYTES) else {
        return Vec::new();
    };
    let mut args: Vec<String> = text.split('\0').map(str::to_owned).collect();
    if args.last().is_some_and(String::is_empty) {
        args.pop();
    }
    args
}
