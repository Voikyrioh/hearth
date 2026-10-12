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
