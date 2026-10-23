//! Déployer une image PAR LA LIAISON contre un VRAI agent (TLS 1.3, SQLite, vrai flux WebSocket, faux moteur) :
//! le défi, la preuve de la clé de ce poste et le mot de passe sont ceux de l'acte réel. L'acte que la liaison
//! reconstruit depuis le corps (`ServiceDeploy`, `0x0F`) est celui que l'agent reconstruit : la signature passe
//! (HRT-69, BR-DEPLOY-030). Aucune attente de durée : des attentes sur des faits observables.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use hearth_agent::application::ports::EngineInfo;
use hearth_link::domain::secret::Secret;
use hearth_link::{ActionOutcome, InputField, LinkError};
use hearth_proto::api::deploy::{
    DeployAccepted, DeployCheckRequest, DeployEnv, DeployPort, DeployProtocol, DeployResources,
    DeploySpec, NameStatus,
};
use hearth_proto::api::service_ops::OperationStatus;
use hearth_proto::error::{ErrorBody, ErrorCode};
use support::agent::fake_engine::FakeEngine;
use support::{Options, PASSWORD, World};

const GIB: u64 = 1024 * 1024 * 1024;

fn options() -> Options {
    let engine = Arc::new(FakeEngine::new());
    engine.set_info(Some(EngineInfo {
        cpus: 8,
        memory_bytes: 16 * GIB,
        storage_dir: Some("/var/lib/docker".into()),
    }));
    Options {
        device_key: true,
        engine: Some(engine),
        ..Options::default()
    }
}

fn spec(name: &str) -> DeploySpec {
    DeploySpec {
        name: name.into(),
        image: "nginx".into(),
        tag: Some("1.27".into()),
        env: vec![DeployEnv {
            name: "TOKEN".into(),
            value: "SENTINEL-VALUE".into(),
        }],
        ports: vec![DeployPort {
            host_port: 18_080,
            container_port: 80,
            protocol: DeployProtocol::Tcp,
        }],
        mounts: vec![],
        command: None,
        resources: DeployResources {
            cpu_millis: Some(500),
            memory_bytes: Some(GIB),
        },
        accept: Default::default(),
        extra: Default::default(),
    }
}

fn completed(outcome: ActionOutcome) -> (u16, serde_json::Value) {
    let ActionOutcome::Completed { status, body, .. } = outcome else {
        panic!("réponse attendue, reçu {outcome:?}");
    };
    (status, body)
}

#[tokio::test]
async fn a_deployment_sent_by_the_link_is_signed_like_the_agent_reads_it_and_never_without_the_password()
 {
    let world = World::connected(options()).await;
    world.agent.grant_services("marie").await;

    // Jamais couvert par l'élévation : sans mot de passe, rien ne part (BR-DEPLOY-030).
    let refused = world
        .manager
        .deploy_service(&world.id, &spec("web"), None)
        .await;
    assert!(matches!(
        refused,
        Err(LinkError::InvalidInput(InputField::Credentials))
    ));

    let (status, body) = completed(
        world
            .manager
            .deploy_service(&world.id, &spec("web"), Some(&Secret::from(PASSWORD)))
            .await
            .unwrap(),
    );
    assert_eq!(status, 202, "{body}");
    let accepted: DeployAccepted = serde_json::from_value(body).unwrap();
    assert!(accepted.service_key.starts_with("h_"));

    // L'opération se relit par son identifiant : un déploiement.
    let operation = world
        .manager
        .service_operation(&world.id, &accepted.operation_id)
        .await
        .unwrap();
    assert_eq!(operation.key, accepted.service_key);
    assert_ne!(operation.status, OperationStatus::Unknown);
}

#[tokio::test]
async fn a_refusal_before_any_gesture_keeps_its_code_and_repeats_no_secret() {
    let world = World::connected(options()).await;
    world.agent.grant_services("marie").await;
    // Sans limites : refus synchrone typé (BR-DEPLOY-038).
    let mut unbounded = spec("noisy");
    unbounded.resources = DeployResources::default();
    let (status, body) = completed(
        world
            .manager
            .deploy_service(&world.id, &unbounded, Some(&Secret::from(PASSWORD)))
            .await
            .unwrap(),
    );
    assert_eq!(status, 422);
    let error: ErrorBody = serde_json::from_value(body.clone()).unwrap();
    assert_eq!(error.error.code, ErrorCode::LimitsRequired);
    assert!(!body.to_string().contains("SENTINEL"));
}

#[tokio::test]
async fn the_check_is_a_plain_read_without_secret_and_the_capacity_bounds_the_machine() {
    let world = World::connected(options()).await;
    world.agent.grant_services("marie").await;
    let check = world
        .manager
        .deploy_check(
            &world.id,
            &DeployCheckRequest {
                name: "web".into(),
                image: "nginx".into(),
                tag: None,
                ports: vec![DeployPort {
                    host_port: 18_081,
                    container_port: 80,
                    protocol: DeployProtocol::Tcp,
                }],
                mounts: vec![],
            },
        )
        .await
        .unwrap();
    assert_eq!(check.name, NameStatus::Ok);
    assert_eq!(check.ports.len(), 1);

    let capacity = world.manager.engine_capacity(&world.id).await.unwrap();
    assert_eq!(capacity.host.cpus, 8);
    assert_eq!(capacity.host.memory_bytes, 16 * GIB);
}

#[tokio::test]
async fn without_the_right_the_check_is_refused_with_its_code() {
    let world = World::connected(options()).await;
    // « marie » n'a pas le droit de gérer les services.
    let refused = world
        .manager
        .deploy_check(
            &world.id,
            &DeployCheckRequest {
                name: "web".into(),
                image: "nginx".into(),
                tag: None,
                ports: vec![],
                mounts: vec![],
            },
        )
        .await;
    assert!(
        matches!(&refused, Err(LinkError::MissingRight(right)) if right == "services"),
        "{refused:?}"
    );
}
