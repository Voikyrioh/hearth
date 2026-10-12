//! La suite de contrat du port `ContainerEngine` : UNE suite, trois exécutants.
//!
//! - le faux moteur (`FakeEngine`, `tests/engine_fake.rs`) ;
//! - un faux démon HTTP local qui rejoue des réponses de la forme documentée de l'API
//!   (`tests/engine_fake_daemon.rs`, par `BollardEngine` sur `Target::Http`) ;
//! - un vrai moteur, en CI Linux seulement (`tests/engine_real.rs`, `--ignored`).
//!
//! C'est ce qui empêche le faux de mentir : ce que le faux promet, le vrai le tient, sinon le job
//! `engine` de la CI échoue. La suite ne touche qu'à la LECTURE (liste, inspection, événements) ; les
//! écritures dont elle a besoin pour préparer son décor (créer, démarrer, arrêter, supprimer) sont
//! faites par l'exécutant, jamais par le port.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use hearth_agent::application::ports::ContainerEngine;
use hearth_agent::domain::services::{
    EngineState, Observation, PortProtocol, ProbeOutcome, RunState, classify_probe,
};

/// Ce que l'exécutant fait faire à son moteur pour préparer le décor.
#[derive(Debug, Clone)]
pub struct Spec {
    pub name: String,
    /// Un port publié : (port du serveur, port du conteneur), en TCP.
    pub publish: Option<(u16, u16)>,
    /// `NOM=valeur` : une variable d'environnement SENTINELLE, qui ne doit jamais ressortir.
    pub env: Vec<String>,
    pub run: Run,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Run {
    /// Tourne jusqu'à ce qu'on l'arrête.
    Forever,
    /// Sort tout de suite avec ce code.
    ExitsWith(i64),
}

#[async_trait]
pub trait Harness: Send + Sync {
    fn engine(&self) -> Arc<dyn ContainerEngine>;
    /// Crée le conteneur (sans le démarrer) et rend son identifiant complet.
    async fn create(&self, spec: &Spec) -> String;
    async fn start(&self, id: &str);
    /// Arrête un conteneur qui tourne.
    async fn stop(&self, id: &str);
    async fn remove(&self, id: &str);
}

/// La variable sentinelle : ni son nom ni sa valeur ne doivent ressortir d'une observation.
pub const SENTINEL_NAME: &str = "HEARTH_CONTRACT_SECRET";
pub const SENTINEL_VALUE: &str = "sentinel-7f3a91-do-not-leak";

async fn eventually<F, Fut>(what: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    for _ in 0..100 {
        if check().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("{what} : condition jamais atteinte en 10 s");
}

async fn find(engine: &Arc<dyn ContainerEngine>, name: &str) -> Option<Observation> {
    engine
        .list()
        .await
        .expect("liste")
        .into_iter()
        .find(|observation| observation.name == name)
}

/// Joue la suite. `prefix` rend les noms uniques (un vrai moteur peut avoir d'autres conteneurs).
pub async fn run(harness: &dyn Harness, prefix: &str) {
    let engine = harness.engine();

    // 1. La sonde d'un moteur qui répond : Docker, version d'API prise en charge, prêt.
    let probe = engine.probe().await;
    assert!(probe.installed, "un moteur qui répond est installé");
    let ProbeOutcome::Answered(announced) = &probe.outcome else {
        panic!("le moteur doit répondre : {probe:?}");
    };
    assert!(!announced.name.is_empty() && !announced.version.is_empty());
    assert_eq!(classify_probe(true, Some(&probe)), EngineState::Ready);

    // 2. Le décor : un service qui tourne (port publié, variable sentinelle), un service jamais
    //    démarré, un service sorti avec le code 3.
    let running_name = format!("{prefix}-running");
    let created_name = format!("{prefix}-created");
    let exited_name = format!("{prefix}-exited");
    let running = harness
        .create(&Spec {
            name: running_name.clone(),
            publish: Some((18_080, 80)),
            env: vec![format!("{SENTINEL_NAME}={SENTINEL_VALUE}")],
            run: Run::Forever,
        })
        .await;
    harness.start(&running).await;
    let created = harness
        .create(&Spec {
            name: created_name.clone(),
            publish: None,
            env: vec![],
            run: Run::Forever,
        })
        .await;
    let exited = harness
        .create(&Spec {
            name: exited_name.clone(),
            publish: None,
            env: vec![],
            run: Run::ExitsWith(3),
        })
        .await;
    harness.start(&exited).await;
    eventually("le service sorti est arrêté", || {
        let engine = engine.clone();
        let name = exited_name.clone();
        async move {
            find(&engine, &name)
                .await
                .is_some_and(|observation| observation.run_state == RunState::Exited)
        }
    })
    .await;

    // 3. La liste contient tout, arrêtés compris, avec ce que la liste montre.
    let listed_running = find(&engine, &running_name)
        .await
        .expect("service en marche");
    assert_eq!(listed_running.id, running);
    assert_eq!(listed_running.run_state, RunState::Running);
    assert!(listed_running.started_at.is_some());
    assert!(
        listed_running
            .ports
            .iter()
            .any(|port| port.host_port == 18_080
                && port.container_port == 80
                && port.protocol == PortProtocol::Tcp),
        "port publié : {:?}",
        listed_running.ports
    );
    assert!(listed_running.image.reference.is_some(), "image nommée");
    assert_eq!(listed_running.image.short_id.len(), 12);

    let listed_created = find(&engine, &created_name)
        .await
        .expect("service jamais démarré");
    assert_eq!(listed_created.run_state, RunState::Created);
    assert_eq!(
        listed_created.started_at, None,
        "jamais démarré : pas de date"
    );

    let listed_exited = find(&engine, &exited_name).await.expect("service sorti");
    assert_eq!(listed_exited.run_state, RunState::Exited);
    assert_eq!(listed_exited.exit_code, Some(3));
    assert!(listed_exited.finished_at.is_some());

    // 4. L'inspection d'un conteneur est celle de la liste ; un inconnu n'est pas une erreur.
    assert_eq!(
        engine.inspect(&running).await.expect("inspection"),
        Some(listed_running.clone())
    );
    assert_eq!(
        engine.inspect(&"0".repeat(64)).await.expect("inspection"),
        None,
        "un conteneur inconnu rend None, pas une erreur"
    );

    // 5. Ni la valeur ni le nom de la variable sentinelle ne sortent de la lecture.
    let everything = format!(
        "{:?} {:?}",
        engine.list().await.expect("liste"),
        engine.inspect(&running).await.expect("inspection")
    );
    assert!(
        !everything.contains(SENTINEL_VALUE),
        "valeur de variable divulguée"
    );
    assert!(
        !everything.contains(SENTINEL_NAME),
        "nom de variable divulgué"
    );

    // 6. Les événements : l'arrêt du service en marche est annoncé. Le flux se connecte à sa
    //    première lecture : on recommence (démarrer, arrêter) tant qu'aucun événement n'est vu.
    let mut events = engine.events();
    let mut seen = false;
    for _ in 0..5 {
        harness.stop(&running).await;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        while let Ok(Some(event)) = tokio::time::timeout_at(deadline, events.next()).await {
            let event = event.expect("événement");
            if event.container_id == running {
                seen = true;
                break;
            }
        }
        if seen {
            break;
        }
        harness.start(&running).await;
    }
    assert!(seen, "l'arrêt du service doit produire un événement");

    // 7. Conflit de lecture : un conteneur supprimé n'est plus listé et son inspection rend None.
    harness.remove(&created).await;
    assert_eq!(engine.inspect(&created).await.expect("inspection"), None);
    assert!(find(&engine, &created_name).await.is_none());

    // Nettoyage.
    harness.remove(&running).await;
    harness.remove(&exited).await;
}
