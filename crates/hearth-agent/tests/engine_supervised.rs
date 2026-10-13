//! Un service lancé par une unité systemd, contre un VRAI moteur et un VRAI systemd (étape `supervised`
//! du job `engine` de la CI, HRT-63, ADR-0037). Ignoré par défaut ET activé par l'environnement : le
//! poste de développement n'a ni moteur ni systemd et n'en démarre jamais.
//!
//! ```text
//! sudo HEARTH_TEST_SUPERVISED=1 ./engine_supervised-<hash> --ignored --test-threads=1 --nocapture
//! ```
//!
//! Il faut être root (`systemd-run` crée une unité du système) sur un système avec systemd et un moteur
//! Docker. Le test lance `systemd-run --unit=hearth-e2e --property=Restart=always docker run --rm
//! --name hearth-e2e ...`, puis :
//!
//! 1. vérifie que le propriétaire du conteneur est détecté (`/proc`, groupe de contrôle) et que l'unité
//!    est lue (`systemctl show`) ;
//! 2. vérifie, par le registre complet, que le service porte la clé `u_hearth-e2e.service` ;
//! 3. CONSIGNE ce que fait un arrêt par l'API du moteur (`docker stop`, même appel que
//!    `POST /containers/{id}/stop`) sur un conteneur lancé par une unité : le conteneur disparaît-il,
//!    l'unité le relance-t-elle, avec quel nouvel identifiant, après combien de temps, et le registre
//!    garde-t-il le même service ? C'est la question ouverte de la section 0 de la conception
//!    technique : la réponse est écrite dans la sortie du test et dans `$GITHUB_STEP_SUMMARY`, elle ne
//!    fait pas échouer le test ;
//! 4. arrête l'unité par le superviseur (`systemctl stop`). Une unité TRANSITOIRE (`systemd-run`) est
//!    déchargée par systemd dès qu'elle s'arrête proprement : le service n'existe plus pour le système,
//!    Hearth le retire de la liste. Pour l'unité d'un fichier (comme `docker-<nom>.service` sur NixOS),
//!    qui reste chargée et arrêtée, il reste listé « arrêté », d'après l'unité, alors que son
//!    conteneur (`--rm`) n'existe plus : c'est le cas de forge, vérifié par une seconde unité écrite
//!    dans `/run/systemd/system`.
//!
//! Ce test n'a JAMAIS été exécuté sur le poste de développement : il ne l'est qu'en CI. Il compile
//! partout (aucune API propre à Linux) mais ne s'exécute que sous Linux.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use hearth_agent::application::ports::{
    ContainerEngine, ContainerRef, ServiceRepo, ServiceSupervisor, StoreError, SupervisedRecord,
};
use hearth_agent::application::services::{EngineTimings, ServicesRegistry};
use hearth_agent::domain::services::{ServiceStateKind, UnitActivity, UnitLookup, UnitName};
use hearth_agent::infrastructure::clock::{SystemClock, SystemMonotonic};
use hearth_agent::infrastructure::engine::BollardEngine;
use hearth_agent::infrastructure::supervisor::ProcSystemd;
use hearth_agent::infrastructure::system::SystemHostEnvironment;

const IMAGE: &str = "public.ecr.aws/docker/library/busybox:1.36";
const UNIT: &str = "hearth-e2e.service";
const CONTAINER: &str = "hearth-e2e";
const KEY: &str = "u_hearth-e2e.service";
/// La seconde unité, écrite dans un fichier : elle reste chargée une fois arrêtée.
const FILE_UNIT: &str = "hearth-e2e-file.service";
const FILE_CONTAINER: &str = "hearth-e2e-file";
const FILE_KEY: &str = "u_hearth-e2e-file.service";
const FILE_UNIT_PATH: &str = "/run/systemd/system/hearth-e2e-file.service";

fn enabled() -> bool {
    std::env::var("HEARTH_TEST_SUPERVISED").as_deref() == Ok("1")
}

fn socket() -> PathBuf {
    std::env::var_os("HEARTH_TEST_ENGINE_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/run/docker.sock"))
}

/// Lance une commande du décor (jamais le port) et rend sa sortie, ou `None` si elle échoue.
fn run(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn must(program: &str, args: &[&str]) -> String {
    let output = Command::new(program)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("{program} introuvable : {error}"));
    assert!(
        output.status.success(),
        "{program} {args:?} : {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// Le décor est défait quoi qu'il arrive : unité arrêtée et remise à zéro, conteneur supprimé.
struct Cleanup;

impl Drop for Cleanup {
    fn drop(&mut self) {
        for unit in [UNIT, FILE_UNIT] {
            let _ = run("systemctl", &["stop", unit]);
            let _ = run("systemctl", &["reset-failed", unit]);
        }
        let _ = std::fs::remove_file(FILE_UNIT_PATH);
        let _ = run("systemctl", &["daemon-reload"]);
        let _ = run("docker", &["rm", "-f", CONTAINER, FILE_CONTAINER]);
    }
}

/// Le dépôt qui ne retient rien : ce test ne touche à aucune base.
struct NoRepo;

#[async_trait]
impl ServiceRepo for NoRepo {
    async fn load_supervised(&self) -> Result<Vec<SupervisedRecord>, StoreError> {
        Ok(Vec::new())
    }

    async fn save_supervised(&self, _record: &SupervisedRecord) -> Result<(), StoreError> {
        Ok(())
    }

    async fn forget_supervised(&self, _unit: &UnitName) -> Result<(), StoreError> {
        Ok(())
    }
}

fn is_root() -> bool {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find_map(|line| line.strip_prefix("Uid:"))
                .and_then(|uids| uids.split_whitespace().next().map(str::to_owned))
        })
        .as_deref()
        == Some("0")
}

/// L'identifiant du conteneur de ce nom, vide s'il n'existe pas (entre deux démarrages de l'unité).
fn container_id() -> String {
    run(
        "docker",
        &[
            "ps",
            "-aq",
            "--no-trunc",
            "--filter",
            &format!("name=^{CONTAINER}$"),
        ],
    )
    .unwrap_or_default()
}

fn unit_line() -> String {
    run(
        "systemctl",
        &[
            "show",
            "--property=ActiveState,SubState,NRestarts",
            "--",
            UNIT,
        ],
    )
    .unwrap_or_default()
    .replace('\n', " ")
}

async fn eventually<F: FnMut() -> bool>(what: &str, limit: Duration, mut check: F) {
    let started = Instant::now();
    while !check() {
        assert!(started.elapsed() < limit, "délai dépassé : {what}");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

fn registry_view(registry: &ServicesRegistry) -> Option<(ServiceStateKind, bool)> {
    view_of(registry, KEY)
}

fn view_of(registry: &ServicesRegistry, key: &str) -> Option<(ServiceStateKind, bool)> {
    registry
        .snapshot()
        .services
        .into_iter()
        .find(|view| view.key.as_str() == key)
        .map(|view| {
            (
                view.derived.state,
                view.supervision
                    .is_some_and(|supervision| supervision.container_present),
            )
        })
}

/// Écrit la consignation dans la sortie du test et, en CI, dans le résumé de l'étape.
fn record(lines: &[String]) {
    let text = lines.join("\n");
    println!("{text}");
    if let Some(path) = std::env::var_os("GITHUB_STEP_SUMMARY")
        && let Ok(mut file) = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
    {
        let _ = writeln!(file, "### Service supervisé : arrêt par l'API du moteur\n");
        for line in lines {
            let _ = writeln!(file, "- {line}");
        }
    }
}

#[tokio::test]
#[ignore = "vrai moteur et vrai systemd, root : HEARTH_TEST_SUPERVISED=1 (étape `supervised` du job `engine`)"]
async fn a_container_run_by_a_unit_is_detected_and_the_engine_stop_is_recorded() {
    if !enabled() {
        eprintln!("HEARTH_TEST_SUPERVISED=1 n'est pas posé : test non exécuté");
        return;
    }
    assert!(
        is_root(),
        "l'étape `supervised` s'exécute en root (systemd-run crée une unité du système)"
    );
    assert!(
        std::path::Path::new("/run/systemd/system").is_dir(),
        "l'étape `supervised` demande un système avec systemd"
    );
    // Les restes d'une exécution interrompue, puis le décor défait quoi qu'il arrive.
    drop(Cleanup);
    let _cleanup = Cleanup;
    must("docker", &["pull", "--quiet", IMAGE]);

    // Le décor : un conteneur `--rm` lancé par une unité qui se relance toujours, comme les unités
    // `docker-<nom>.service` de `virtualisation.oci-containers` sur NixOS.
    must(
        "systemd-run",
        &[
            &format!("--unit={}", UNIT.trim_end_matches(".service")),
            "--property=Restart=always",
            "--property=RestartSec=3",
            "docker",
            "run",
            "--rm",
            "--name",
            CONTAINER,
            IMAGE,
            "sleep",
            "600",
        ],
    );
    eventually(
        "le conteneur de l'unité tourne",
        Duration::from_secs(60),
        || {
            run(
                "docker",
                &["inspect", "--format", "{{.State.Running}}", CONTAINER],
            )
            .as_deref()
                == Some("true")
        },
    )
    .await;
    let first_id = container_id();
    assert_eq!(first_id.len(), 64, "identifiant complet : {first_id}");

    // 1. Le propriétaire est détecté, l'unité est lue.
    let supervisor = ProcSystemd::system();
    let owners = supervisor
        .owners(&[ContainerRef {
            id: first_id.clone(),
            name: CONTAINER.to_owned(),
        }])
        .await;
    assert_eq!(
        owners.get(&first_id).map(UnitName::as_str),
        Some(UNIT),
        "propriétaire du conteneur : {owners:?}"
    );
    let unit = UnitName::parse(UNIT).unwrap();
    let UnitLookup::Loaded(status) = supervisor.unit(&unit).await else {
        panic!("l'unité doit être chargée");
    };
    assert_eq!(status.activity, UnitActivity::Active);
    assert!(status.restarts, "Restart=always");
    assert!(status.timeout_stop_s.is_some(), "délai d'arrêt déclaré");

    // 2. Le registre complet : la clé du service est celle de l'unité.
    let registry = ServicesRegistry::with_supervision(
        Arc::new(BollardEngine::unix(socket())) as Arc<dyn ContainerEngine>,
        Arc::new(ProcSystemd::system()),
        Arc::new(NoRepo),
        &SystemHostEnvironment,
        Arc::new(SystemClock),
        Arc::new(SystemMonotonic::new()),
        true,
        EngineTimings::production(),
    );
    let task = tokio::spawn(registry.clone().run());
    eventually(
        "le service de l'unité est listé en marche",
        Duration::from_secs(60),
        || registry_view(&registry) == Some((ServiceStateKind::Running, true)),
    )
    .await;

    // 3. L'arrêt par l'API du moteur : consigné, pas jugé.
    let before_unit = unit_line();
    let stopped_at = Instant::now();
    let stop_output = Command::new("docker")
        .args(["stop", "--time", "2", CONTAINER])
        .output()
        .expect("docker stop");
    let mut seen_states: Vec<String> = Vec::new();
    let mut unit_transitions: Vec<String> = vec![before_unit.clone()];
    let mut container_vanished_after: Option<Duration> = None;
    let mut new_container: Option<(String, Duration)> = None;
    let mut key_always_listed = true;
    while stopped_at.elapsed() < Duration::from_secs(30) {
        let id = container_id();
        if id.is_empty() && container_vanished_after.is_none() {
            container_vanished_after = Some(stopped_at.elapsed());
        }
        if !id.is_empty()
            && id != first_id
            && run(
                "docker",
                &["inspect", "--format", "{{.State.Running}}", CONTAINER],
            )
            .as_deref()
                == Some("true")
        {
            new_container = Some((id, stopped_at.elapsed()));
        }
        let line = unit_line();
        if unit_transitions.last() != Some(&line) {
            unit_transitions.push(line);
        }
        match registry_view(&registry) {
            Some((state, _)) => {
                let text = format!("{state:?}");
                if seen_states.last() != Some(&text) {
                    seen_states.push(text);
                }
            }
            None => key_always_listed = false,
        }
        if new_container.is_some()
            && registry_view(&registry).is_some_and(|(s, c)| s == ServiceStateKind::Running && c)
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    record(&[
        format!(
            "`docker stop` (appel de l'API du moteur) sur le conteneur de l'unité : code {:?}, {}",
            stop_output.status.code(),
            String::from_utf8_lossy(&stop_output.stderr).trim()
        ),
        format!(
            "conteneur supprimé par le moteur (`--rm`) : {}",
            match container_vanished_after {
                Some(after) => format!("oui, vu après {} ms", after.as_millis()),
                None => "non vu absent (relance plus rapide que l'échantillonnage)".to_owned(),
            }
        ),
        format!(
            "l'unité relance le service : {}",
            match &new_container {
                Some((id, after)) => format!(
                    "oui, nouveau conteneur {} après {} ms (identifiant différent : {})",
                    &id[..12.min(id.len())],
                    after.as_millis(),
                    id != &first_id
                ),
                None => "non, aucun nouveau conteneur en 30 s".to_owned(),
            }
        ),
        format!("états de l'unité vus : {unit_transitions:?}"),
        format!("états du service vus par le registre : {seen_states:?}"),
        format!("la clé {KEY} est restée listée pendant tout l'arrêt : {key_always_listed}"),
    ]);
    assert!(
        key_always_listed,
        "le service doit rester listé sous la même clé quand son conteneur est recréé"
    );

    // 4a. L'arrêt par le superviseur d'une unité transitoire : systemd la décharge, le service
    //     n'existe plus pour le système et sort de la liste.
    must("systemctl", &["stop", UNIT]);
    eventually(
        "le service d'une unité transitoire arrêtée sort de la liste",
        Duration::from_secs(60),
        || registry_view(&registry).is_none(),
    )
    .await;
    assert!(container_id().is_empty(), "`--rm` : plus de conteneur");

    // 4b. L'unité d'un fichier, comme celles de NixOS : arrêtée, elle reste chargée, et son service reste
    //     listé « arrêté » alors que son conteneur n'existe plus.
    std::fs::write(
        FILE_UNIT_PATH,
        format!(
            "[Service]
Restart=always
RestartSec=3
ExecStart=/usr/bin/docker run --rm --name {FILE_CONTAINER} {IMAGE} sleep 600
"
        ),
    )
    .expect("unité écrite dans /run/systemd/system");
    must("systemctl", &["daemon-reload"]);
    must("systemctl", &["start", FILE_UNIT]);
    eventually(
        "le service de l'unité d'un fichier est listé en marche",
        Duration::from_secs(60),
        || view_of(&registry, FILE_KEY) == Some((ServiceStateKind::Running, true)),
    )
    .await;
    must("systemctl", &["stop", FILE_UNIT]);
    eventually(
        "le service de l'unité d'un fichier reste listé, arrêté, sans conteneur",
        Duration::from_secs(60),
        || view_of(&registry, FILE_KEY) == Some((ServiceStateKind::Stopped, false)),
    )
    .await;
    let file_unit = UnitName::parse(FILE_UNIT).unwrap();
    let UnitLookup::Loaded(after) = supervisor.unit(&file_unit).await else {
        panic!("l'unité d'un fichier, arrêtée, reste chargée");
    };
    assert_eq!(after.activity, UnitActivity::Inactive);
    record(&[
        format!(
            "`systemctl stop` d'une unité transitoire : {KEY} sort de la liste (systemd la décharge)"
        ),
        format!(
            "`systemctl stop` d'une unité de fichier : {FILE_KEY} reste listé, arrêté, sans conteneur, d'après l'unité"
        ),
    ]);
    task.abort();
}
