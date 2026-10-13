//! La suite de contrat du port `ContainerEngine` contre un VRAI moteur Docker (job `engine` de la CI,
//! runners `ubuntu-latest`). Ignoré par défaut ET activé par l'environnement : le poste de
//! développement n'a pas de moteur et n'en démarre jamais un pour ce test.
//!
//! ```text
//! HEARTH_TEST_ENGINE=1 cargo test -p hearth-agent --test engine_real -- --ignored
//! ```
//!
//! `HEARTH_TEST_ENGINE_SOCKET` (défaut `/var/run/docker.sock`) désigne le socket. Les conteneurs de
//! test sont préfixés `hearth-contract-` et supprimés avant et après ; l'image est `busybox` (un peu
//! plus d'un mégaoctet), tirée du miroir public de Docker Hub chez AWS (pas de limite anonyme serrée).
//!
//! Ce test n'a JAMAIS été exécuté sur le poste de développement (aucun moteur) : il ne l'est qu'en CI.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use async_trait::async_trait;
use hearth_agent::application::ports::ContainerEngine;
use hearth_agent::infrastructure::engine::BollardEngine;
use support::engine_contract::{self, Harness, Run, Spec};

const IMAGE: &str = "public.ecr.aws/docker/library/busybox:1.36";
const PREFIX: &str = "hearth-contract";

fn enabled() -> bool {
    std::env::var("HEARTH_TEST_ENGINE").as_deref() == Ok("1")
}

fn socket() -> PathBuf {
    std::env::var_os("HEARTH_TEST_ENGINE_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/run/docker.sock"))
}

/// Lance `docker` (le décor du test seulement : le port, lui, ne fait que lire).
fn docker(args: &[&str]) -> String {
    let output = Command::new("docker")
        .args(args)
        .output()
        .expect("la commande docker doit exister dans le job engine");
    assert!(
        output.status.success(),
        "docker {args:?} : {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn remove_leftovers() {
    let filter = format!("name={PREFIX}-");
    let ids = docker(&["ps", "-aq", "--filter", &filter]);
    for id in ids.split_whitespace() {
        docker(&["rm", "-f", id]);
    }
}

struct RealHarness {
    engine: Arc<BollardEngine>,
}

#[async_trait]
impl Harness for RealHarness {
    fn engine(&self) -> Arc<dyn ContainerEngine> {
        self.engine.clone()
    }

    async fn create(&self, spec: &Spec) -> String {
        let mut args: Vec<String> = vec!["create".into(), "--name".into(), spec.name.clone()];
        if let Some((host, container)) = spec.publish {
            args.push("-p".into());
            args.push(format!("{host}:{container}"));
        }
        for variable in &spec.env {
            args.push("-e".into());
            args.push(variable.clone());
        }
        args.push(IMAGE.into());
        args.extend(["sh".into(), "-c".into()]);
        args.push(match spec.run {
            Run::Forever => "sleep 300".into(),
            Run::ExitsWith(code) => format!("exit {code}"),
        });
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        docker(&refs)
    }

    async fn start(&self, id: &str) {
        docker(&["start", id]);
    }

    async fn stop(&self, id: &str) {
        docker(&["stop", "-t", "1", id]);
    }

    async fn remove(&self, id: &str) {
        docker(&["rm", "-f", id]);
    }
}

#[tokio::test]
#[ignore = "vrai moteur : HEARTH_TEST_ENGINE=1 cargo test -p hearth-agent --test engine_real -- --ignored"]
async fn the_bollard_adapter_honours_the_engine_contract_against_a_real_engine() {
    if !enabled() {
        eprintln!("HEARTH_TEST_ENGINE=1 n'est pas posé : test non exécuté");
        return;
    }
    docker(&["pull", "--quiet", IMAGE]);
    remove_leftovers();
    let harness = RealHarness {
        engine: Arc::new(BollardEngine::unix(socket())),
    };
    engine_contract::run(&harness, PREFIX).await;
    remove_leftovers();
}

#[tokio::test]
#[ignore = "vrai moteur : HEARTH_TEST_ENGINE=1 cargo test -p hearth-agent --test engine_real -- --ignored"]
async fn the_real_engine_is_detected_as_ready_and_a_missing_socket_as_absent_or_stopped() {
    if !enabled() {
        eprintln!("HEARTH_TEST_ENGINE=1 n'est pas posé : test non exécuté");
        return;
    }
    use hearth_agent::domain::services::{EngineState, classify_probe};
    let ready = BollardEngine::unix(socket());
    assert_eq!(
        classify_probe(true, Some(&ready.probe().await)),
        EngineState::Ready
    );
    // Un socket qui n'existe pas : jamais une erreur, un résultat de sonde.
    let missing = BollardEngine::unix("/nonexistent/hearth-test/docker.sock");
    let state = classify_probe(true, Some(&missing.probe().await));
    assert!(matches!(state, EngineState::Absent | EngineState::Stopped));
}

#[tokio::test]
#[ignore = "vrai moteur : HEARTH_TEST_ENGINE=1 cargo test -p hearth-agent --test engine_real -- --ignored"]
async fn the_negotiated_api_version_is_the_one_the_real_engine_announces() {
    if !enabled() {
        eprintln!("HEARTH_TEST_ENGINE=1 n'est pas posé : test non exécuté");
        return;
    }
    use hearth_agent::domain::services::{ApiVersion, MIN_API_VERSION, ProbeOutcome};
    let engine = BollardEngine::unix(socket());
    let ProbeOutcome::Answered(announced) = engine.probe().await.outcome else {
        panic!("le moteur du runner doit répondre");
    };
    // La source indépendante : la ligne de commande du moteur.
    let cli_api = docker(&["version", "--format", "{{.Server.APIVersion}}"]);
    let cli_version = docker(&["version", "--format", "{{.Server.Version}}"]);
    assert_eq!(
        Some(announced.api_version),
        ApiVersion::parse(&cli_api),
        "version d'API annoncée par la sonde et par `docker version`"
    );
    assert_eq!(announced.version, cli_version);
    assert!(announced.api_version >= MIN_API_VERSION);
    // Le client parle cette version d'API, plafonnée à celle de `bollard` (jamais plus récente que
    // le moteur : un Docker 20.10 refuse une version trop neuve).
    let ceiling = ApiVersion {
        major: bollard::API_DEFAULT_VERSION.major_version as u32,
        minor: bollard::API_DEFAULT_VERSION.minor_version as u32,
    };
    assert_eq!(
        engine.spoken_api_version(),
        Some(announced.api_version.min(ceiling))
    );
}

#[tokio::test]
#[ignore = "vrai moteur : HEARTH_TEST_ENGINE=1 cargo test -p hearth-agent --test engine_real -- --ignored"]
async fn the_real_engine_describes_pause_mounts_labels_policy_and_random_ports() {
    if !enabled() {
        eprintln!("HEARTH_TEST_ENGINE=1 n'est pas posé : test non exécuté");
        return;
    }
    use hearth_agent::domain::services::{MountType, RestartPolicy, RunState};
    docker(&["pull", "--quiet", IMAGE]);
    remove_leftovers();
    let name = format!("{PREFIX}-describe");
    let volume = format!("{PREFIX}-volume");
    docker(&["volume", "create", &volume]);
    let folder = std::env::temp_dir().to_string_lossy().into_owned();
    let volume_mount = format!("{volume}:/data");
    let folder_mount = format!("{folder}:/host:ro");
    let id = docker(&[
        "run",
        "-d",
        "--name",
        &name,
        "--restart",
        "unless-stopped",
        "--label",
        "com.docker.compose.project=hearth-shop",
        "-v",
        &volume_mount,
        "-v",
        &folder_mount,
        // Un port du conteneur sans port du serveur demandé : le moteur en attribue un au hasard.
        "-p",
        "80",
        IMAGE,
        "sleep",
        "300",
    ]);
    let engine = BollardEngine::unix(socket());
    let running = engine
        .inspect(&id)
        .await
        .expect("inspection")
        .expect("le conteneur existe");
    assert_eq!(running.run_state, RunState::Running);
    assert_eq!(running.restart, RestartPolicy::UnlessStopped);
    assert_eq!(running.compose_project.as_deref(), Some("hearth-shop"));
    let data = running
        .mounts
        .iter()
        .find(|mount| mount.target == "/data")
        .expect("volume");
    assert_eq!(
        (data.kind, data.source.as_str()),
        (MountType::Volume, volume.as_str())
    );
    assert!(!data.read_only);
    let host = running
        .mounts
        .iter()
        .find(|mount| mount.target == "/host")
        .expect("dossier");
    assert_eq!(host.kind, MountType::Folder);
    assert!(host.read_only);
    let random = running
        .ports
        .iter()
        .find(|port| port.container_port == 80)
        .expect("port attribué au hasard");
    assert!(random.host_port > 0, "le vrai port, pas 0 : {random:?}");

    // En pause, puis relancé.
    docker(&["pause", &id]);
    let paused = engine.inspect(&id).await.unwrap().unwrap();
    assert_eq!(paused.run_state, RunState::Paused);
    docker(&["unpause", &id]);
    assert_eq!(
        engine.inspect(&id).await.unwrap().unwrap().run_state,
        RunState::Running
    );
    docker(&["rm", "-f", &id]);
    docker(&["volume", "rm", &volume]);
    remove_leftovers();
}

/// Le compte de test n'est pas root : seul un compte ordinaire se voit refuser un socket sans droit.
#[cfg(unix)]
fn running_as_root() -> bool {
    Command::new("id")
        .arg("-u")
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim() == "0")
        .unwrap_or(true)
}

#[cfg(unix)]
#[tokio::test]
#[ignore = "vrai moteur : HEARTH_TEST_ENGINE=1 cargo test -p hearth-agent --test engine_real -- --ignored"]
async fn denied_and_mute_unix_sockets() {
    use std::os::unix::fs::PermissionsExt;

    if !enabled() {
        eprintln!("HEARTH_TEST_ENGINE=1 n'est pas posé : test non exécuté");
        return;
    }
    use hearth_agent::domain::services::{EngineState, classify_probe};
    use hearth_agent::infrastructure::engine::{InstallMarkers, Target};
    // Un chemin de socket Unix tient en 107 octets : `tempdir_fast` (`/dev/shm/hearth-test-tmp/`, dossier
    // nommé d'après ce test, donc court) et des noms de fichier courts, pas `target/` du runner (trop long).
    let dir = support::tmp::tempdir_fast().expect("dossier");
    // La garde de longueur précède le PREMIER socket (les deux noms ont la même longueur) : sur une
    // machine sans `/dev/shm`, c'est elle qui parle, pas un `bind` en échec au message obscur.
    for name in ["d.sock", "m.sock"] {
        let path = dir.path().join(name);
        assert!(
            path.as_os_str().len() < 100,
            "chemin de socket trop long : {path:?}"
        );
    }

    // Un socket Unix réel dont le fichier n'a plus aucun droit : `EACCES` à la connexion. Le journal
    // d'un test réussi est capturé : la branche jouée ou sautée est écrite en clair, et le job
    // `engine` lance ce test avec `--nocapture` pour la lire.
    let denied_played = !running_as_root();
    if denied_played {
        let path = dir.path().join("d.sock");
        let _listener = tokio::net::UnixListener::bind(&path).expect("socket");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        let engine = BollardEngine::new(
            Target::Unix(path),
            InstallMarkers::at(Vec::<PathBuf>::new()),
        );
        assert_eq!(
            classify_probe(true, Some(&engine.probe().await)),
            EngineState::AccessDenied
        );
    }

    // Un socket qui accepte la connexion mais ne répond jamais : « ne répond pas » au bout du délai.
    let path = dir.path().join("m.sock");
    let listener = tokio::net::UnixListener::bind(&path).expect("socket");
    let _holder = tokio::spawn(async move {
        let mut open = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            open.push(stream);
        }
    });
    let engine = BollardEngine::new(
        Target::Unix(path),
        InstallMarkers::at(Vec::<PathBuf>::new()),
    )
    .with_probe_timeout(std::time::Duration::from_secs(1));
    let started = std::time::Instant::now();
    let probe = engine.probe().await;
    assert_eq!(
        classify_probe(true, Some(&probe)),
        EngineState::Unresponsive
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(4));
    eprintln!("socket muet : JOUÉ");
    eprintln!(
        "socket sans droit : {}",
        if denied_played {
            "JOUÉ (accès refusé constaté)"
        } else {
            "SAUTÉ (compte root : un socket sans droit reste ouvert, cas non vérifiable ici)"
        }
    );
}
