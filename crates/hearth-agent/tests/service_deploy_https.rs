//! Déployer une image, de bout en bout (HRT-68, ADR-0039, ADR-0041, ADR-0044, ADR-0045) : vrai agent en HTTPS,
//! vraie base, faux moteur, faux dossiers de données, faux hôte. Les routes répondent `202`, le déploiement court
//! à l'arrière-plan, l'issue se lit dans l'opération.
//!
//! Règles : BR-DEPLOY-001 à BR-DEPLOY-009, BR-DEPLOY-014 à BR-DEPLOY-017, BR-DEPLOY-019 à BR-DEPLOY-034,
//! BR-DEPLOY-037, BR-DEPLOY-038. La preuve de clé et le mot de passe sont dans la matrice de
//! `admin_reauth.rs` ; ici : ce que fait la route et le cas d'usage, jusqu'au journal d'activité et à la base.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use hearth_agent::application::ports::{EngineError, EngineInfo};
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::services::RunState;
use hearth_agent::infrastructure::logging::{self, LogFormat};
use hearth_proto::api::deploy::{DeployAccepted, DeployCheck, EngineCapacity};
use hearth_proto::api::service_ops::{OperationCause, OperationStatus, OperationView};
use hearth_proto::api::services::ServicesState;
use serde_json::{Value, json};
use support::device::Actor;
use support::engine::{fast_timings, parts_supervised};
use support::fake_engine::{FakeEngine, PullScript, container};
use support::fake_host::{FakeFreeSpace, FakeHostPorts};
use support::fake_safe_fs::FakeSafeFs;
use support::fake_supervisor::FakeSupervisor;
use support::https::{self, Agent, Wire};
use support::{Env, env};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::writer::BoxMakeWriter;

const GIB: u64 = 1024 * 1024 * 1024;

struct Bench {
    env: Env,
    engine: Arc<FakeEngine>,
    host_ports: Arc<FakeHostPorts>,
    free_space: Arc<FakeFreeSpace>,
    safe_fs: Arc<FakeSafeFs>,
    agent: Agent,
    /// Administrateur avec le droit de gérer les services.
    ada: Actor,
    /// Administrateur SANS le droit.
    bob: Actor,
    /// Lecture seule SANS le droit.
    lucas: Actor,
    /// Lecture seule AVEC le droit.
    dan: Actor,
}

async fn bench() -> Bench {
    bench_with(|_| {}).await
}

async fn bench_with(change: impl FnOnce(&mut hearth_agent::app::EngineParts)) -> Bench {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    engine.set_info(Some(EngineInfo {
        cpus: 8,
        memory_bytes: 16 * GIB,
        storage_dir: Some("/var/lib/docker".into()),
    }));
    let host_ports = Arc::new(FakeHostPorts::new());
    let free_space = Arc::new(FakeFreeSpace::new());
    let safe_fs = Arc::new(FakeSafeFs::new());
    let mut parts = parts_supervised(
        engine.clone(),
        Arc::new(FakeSupervisor::new()),
        fast_timings(),
    );
    parts.host_ports = host_ports.clone();
    parts.free_space = free_space.clone();
    parts.safe_fs = Some(safe_fs.clone());
    change(&mut parts);
    let agent = https::start_engined(&env, parts).await;
    let ada = agent.actor(&env, "ada", Role::Admin).await;
    let bob = agent.actor(&env, "bob", Role::Admin).await;
    let lucas = agent.actor(&env, "lucas", Role::ReadOnly).await;
    let dan = agent.actor(&env, "dan", Role::ReadOnly).await;
    env.grant_services("ada").await;
    env.grant_services("dan").await;
    // Une image qui se récupère en deux couches.
    engine.publish_image("nginx:latest", PullScript::two_layers());
    Bench {
        env,
        engine,
        host_ports,
        free_space,
        safe_fs,
        agent,
        ada,
        bob,
        lucas,
        dan,
    }
}

/// Une demande minimale et valable : un nom, une image, les deux limites.
fn spec(name: &str) -> Value {
    json!({
        "name": name,
        "image": "nginx",
        "resources": { "cpu_millis": 1000, "memory_bytes": 268_435_456_u64 }
    })
}

impl Bench {
    async fn deploy_as(&self, who: &Actor, body: Value) -> Wire {
        self.agent
            .confirmed(who, "POST", "/deployments", body)
            .await
            .send()
            .await
    }

    async fn deploy(&self, body: Value) -> Wire {
        self.deploy_as(&self.ada, body).await
    }

    async fn accepted(&self, body: Value) -> DeployAccepted {
        let reply = self.deploy(body).await;
        assert_eq!(reply.status, 202, "{}", reply.text);
        serde_json::from_value(reply.body).expect("DeployAccepted")
    }

    async fn operation(&self, id: &str) -> OperationView {
        let reply = self
            .agent
            .request("GET", &format!("/operations/services/{id}"))
            .token(&self.dan.token)
            .send()
            .await;
        assert_eq!(reply.status, 200, "{}", reply.text);
        serde_json::from_value(reply.body).expect("OperationView")
    }

    /// Attend la fin du déploiement et rend son opération.
    async fn finished(&self, id: &str) -> OperationView {
        let started = Instant::now();
        loop {
            let operation = self.operation(id).await;
            if !matches!(
                operation.status,
                OperationStatus::Accepted | OperationStatus::Running | OperationStatus::Cancelling
            ) {
                return operation;
            }
            assert!(
                started.elapsed() < Duration::from_secs(15),
                "le déploiement ne finit pas : {operation:?}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Attend que l'opération en soit à cette étape.
    async fn at_step(&self, id: &str, step: &str) -> OperationView {
        let started = Instant::now();
        loop {
            let operation = self.operation(id).await;
            if operation.step.as_deref() == Some(step) {
                return operation;
            }
            assert!(
                started.elapsed() < Duration::from_secs(15),
                "l'étape {step} n'arrive pas : {operation:?}"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn services(&self) -> ServicesState {
        let reply = self
            .agent
            .request("GET", "/services")
            .token(&self.lucas.token)
            .send()
            .await;
        serde_json::from_value(reply.body).expect("ServicesState")
    }

    /// Les entrées du journal d'activité de déploiement : résultat, cible, raison, compte.
    async fn journal(&self) -> Vec<(String, Option<String>, Option<String>, Option<String>)> {
        self.agent.flush_journal().await;
        sqlx::query_as(
            "SELECT outcome, target, reason, account FROM audit_events
             WHERE action = 'service.deploy' ORDER BY id",
        )
        .fetch_all(self.env.db.pool())
        .await
        .unwrap()
    }

    async fn specs_count(&self) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM service_specs")
            .fetch_one(self.env.db.pool())
            .await
            .unwrap()
    }
}

fn code(wire: &Wire) -> String {
    wire.body["error"]["code"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

// ---------------------------------------------------------------------------------------------
// Un déploiement qui réussit (BR-DEPLOY-002, 016, 021, 024, 026, 037)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_minimal_deployment_pulls_creates_starts_and_publishes_a_service_created_by_hearth() {
    let b = bench().await;
    let accepted = b.accepted(spec("web")).await;
    assert!(accepted.service_key.starts_with("h_"));
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    assert_eq!(operation.cause, None);
    let steps: Vec<&str> = operation.steps.iter().map(|s| s.step.as_str()).collect();
    assert!(
        steps
            .iter()
            .all(|s| ["check", "pull", "create", "start", "watch"].contains(s)),
        "{steps:?}"
    );
    for wanted in ["check", "pull", "create", "start", "watch"] {
        assert!(steps.contains(&wanted), "{wanted} : {steps:?}");
    }
    // Le conteneur créé porte les étiquettes de Hearth, les limites, `unless-stopped` (BR-DEPLOY-021).
    let created = b.engine.created();
    assert_eq!(created.len(), 1);
    let plan = &created[0];
    assert_eq!(plan.name, "web");
    assert_eq!(plan.image, "nginx:latest");
    assert_eq!((plan.cpu_millis, plan.memory_bytes), (1000, 268_435_456));
    assert_eq!(plan.labels["io.hearth.managed"], "true");
    assert_eq!(
        plan.labels["io.hearth.service-id"],
        accepted.service_key[2..]
    );
    assert!(!plan.labels["io.hearth.install-id"].is_empty());
    assert_eq!(plan.labels["io.hearth.created-by"], "ada");
    assert!(plan.env.is_empty() && plan.ports.is_empty() && plan.binds.is_empty());
    // Le service est publié comme créé par Hearth, en marche (BR-DEPLOY-026).
    let state = b.services().await;
    let item = state
        .services
        .iter()
        .find(|item| item.key == accepted.service_key)
        .expect("service publié");
    assert_eq!(item.name, "web");
    assert_eq!(
        item.state,
        hearth_proto::api::services::ServiceState::Running
    );
    assert_eq!(
        item.origin,
        hearth_proto::api::services::ServiceOrigin::Hearth
    );
    // La description durable existe (révision 1) et le conteneur est consigné.
    assert_eq!(b.specs_count().await, 1);
    let recorded: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM service_containers")
        .fetch_one(b.env.db.pool())
        .await
        .unwrap();
    assert_eq!(recorded, 1);
    // L'entrée du journal d'activité : réussite, le compte, le service, l'image.
    let journal = b.journal().await;
    assert_eq!(journal.len(), 1, "{journal:?}");
    assert_eq!(journal[0].0, "ok");
    assert_eq!(journal[0].3.as_deref(), Some("ada"));
    let target = journal[0].1.clone().unwrap();
    assert!(
        target.contains("web") && target.contains("nginx:latest"),
        "{target}"
    );
    b.agent.shutdown().await;
}

#[tokio::test]
async fn the_pull_reports_its_progress_and_an_image_already_there_is_used_as_is() {
    let b = bench().await;
    let mut slow = PullScript::two_layers();
    slow.delay = Duration::from_millis(60);
    b.engine.publish_image("nginx:latest", slow);
    let accepted = b.accepted(spec("web")).await;
    let started = Instant::now();
    let mut seen = Vec::new();
    loop {
        let operation = b.operation(&accepted.operation_id).await;
        if operation.step.as_deref() == Some("pull") {
            seen.push((
                operation.percent,
                operation.bytes_done,
                operation.cancellable,
            ));
        }
        if !matches!(
            operation.status,
            OperationStatus::Accepted | OperationStatus::Running
        ) {
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(15));
        tokio::time::sleep(Duration::from_millis(15)).await;
    }
    assert!(
        seen.iter()
            .any(|(percent, bytes, _)| percent.is_some() && bytes.is_some()),
        "{seen:?}"
    );
    assert!(
        seen.iter().all(|(_, _, cancellable)| *cancellable),
        "annulable pendant la récupération"
    );
    let percents: Vec<u8> = seen.iter().filter_map(|(p, _, _)| *p).collect();
    assert!(
        percents.windows(2).all(|w| w[0] <= w[1]),
        "ne recule jamais : {percents:?}"
    );

    // Une image déjà présente n'est pas récupérée de nouveau (BR-DEPLOY-018).
    let b2 = bench().await;
    b2.engine.have_image("nginx:latest");
    let accepted = b2.accepted(spec("web")).await;
    let operation = b2.finished(&accepted.operation_id).await;
    assert_eq!(operation.status, OperationStatus::Succeeded);
    assert!(b2.engine.pulls().is_empty());
    assert!(operation.steps.iter().all(|s| s.step != "pull"));
    b.agent.shutdown().await;
    b2.agent.shutdown().await;
}

#[tokio::test]
async fn variables_ports_a_command_and_a_data_folder_reach_the_container_from_the_description() {
    let b = bench().await;
    let mut body = spec("db");
    body["env"] = json!([{ "name": "POSTGRES_PASSWORD", "value": "s3cret" }, { "name": "MODE", "value": "" }]);
    body["ports"] = json!([{ "host_port": 5432, "container_port": 5432 }, { "host_port": 5353, "container_port": 53, "protocol": "udp" }]);
    body["command"] = json!("postgres -c 'shared_buffers=128MB'");
    body["mounts"] = json!([{ "target": "/var/lib/postgresql/data" }, { "target": "/etc/conf", "read_only": true }]);
    let accepted = b.accepted(body).await;
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    let plan = &b.engine.created()[0];
    assert_eq!(
        plan.env,
        vec![
            ("POSTGRES_PASSWORD".to_owned(), "s3cret".to_owned()),
            ("MODE".to_owned(), String::new())
        ]
    );
    assert_eq!(
        plan.command,
        Some(vec![
            "postgres".into(),
            "-c".into(),
            "shared_buffers=128MB".into()
        ])
    );
    assert_eq!(plan.ports.len(), 2);
    // Un dossier géré par service, un sous-dossier par point de montage (BR-DEPLOY-010).
    assert_eq!(plan.binds.len(), 2);
    assert!(
        plan.binds
            .iter()
            .all(|(host, _, _)| host.contains(&format!("/{}/", accepted.service_key)))
    );
    assert!(plan.binds.iter().any(
        |(host, target, ro)| host.ends_with("/var+lib+postgresql+data")
            && target == "/var/lib/postgresql/data"
            && !ro
    ));
    assert!(
        plan.binds
            .iter()
            .any(|(_, target, ro)| target == "/etc/conf" && *ro)
    );
    // Les dossiers existent, créés par l'agent sous la racine du service.
    let key = hearth_agent::domain::services::ServiceKey::parse(&accepted.service_key).unwrap();
    use hearth_agent::application::ports::SafeFs;
    assert!(b.safe_fs.open_service_root(&key).is_ok());
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Les refus connus d'avance : synchrones, typés, rien n'est créé (BR-DEPLOY-001, 003, 007, 032, 035, BR-DEPLOY-038)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn only_an_account_with_the_right_may_deploy_whatever_its_role() {
    let b = bench().await;
    for who in [&b.bob, &b.lucas] {
        let refused = b.deploy_as(who, spec("web")).await;
        assert_eq!(refused.status, 403);
    }
    assert!(b.engine.created().is_empty() && b.engine.pulls().is_empty());
    assert_eq!(b.specs_count().await, 0);
    // Un compte en lecture seule qui a le droit est servi (BR-GRANT-003).
    let reply = b.deploy_as(&b.dan, spec("web")).await;
    assert_eq!(reply.status, 202, "{}", reply.text);
    let accepted: DeployAccepted = serde_json::from_value(reply.body).unwrap();
    b.finished(&accepted.operation_id).await;
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_bad_request_is_refused_on_its_field_and_nothing_is_created() {
    let b = bench().await;
    let cases: Vec<(Value, &str, &str)> = vec![
        (
            json!({ "name": "web app", "image": "nginx", "resources": spec("x")["resources"] }),
            "name",
            "bad_format",
        ),
        (
            json!({ "name": "web", "image": "", "resources": spec("x")["resources"] }),
            "image",
            "empty",
        ),
        (
            json!({ "name": "web", "image": "Nginx", "resources": spec("x")["resources"] }),
            "image",
            "bad_format",
        ),
        (
            json!({ "name": "web", "image": "nginx@sha256:abc", "resources": spec("x")["resources"] }),
            "image",
            "digest_unsupported",
        ),
        (
            json!({ "name": "web", "image": "nginx", "tag": "bad tag", "resources": spec("x")["resources"] }),
            "tag",
            "bad_format",
        ),
        (
            {
                let mut b = spec("web");
                b["env"] = json!([{ "name": "1A", "value": "x" }]);
                b
            },
            "env",
            "bad_name",
        ),
        (
            {
                let mut b = spec("web");
                b["ports"] = json!([{ "host_port": 0, "container_port": 80 }]);
                b
            },
            "ports",
            "range",
        ),
        (
            {
                let mut b = spec("web");
                b["mounts"] = json!([{ "target": "relatif" }]);
                b
            },
            "mounts",
            "not_absolute",
        ),
        (
            {
                let mut b = spec("web");
                b["command"] = json!("run \"ouvert");
                b
            },
            "command",
            "bad_format",
        ),
        (
            {
                let mut b = spec("web");
                b["resources"] = json!({ "cpu_millis": 99_000, "memory_bytes": 268_435_456_u64 });
                b
            },
            "resources",
            "cpu_bounds",
        ),
    ];
    for (body, field, reason) in cases {
        let refused = b.deploy(body.clone()).await;
        assert_eq!(refused.status, 422, "{field} : {}", refused.text);
        assert_eq!(code(&refused), "VALIDATION_ERROR", "{field}");
        assert_eq!(refused.body["error"]["details"]["field"], field);
        assert_eq!(
            refused.body["error"]["details"]["reason"], reason,
            "{field}"
        );
    }
    assert!(b.engine.created().is_empty() && b.engine.pulls().is_empty());
    assert_eq!(b.specs_count().await, 0);
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_service_is_never_created_without_both_limits() {
    let b = bench().await;
    for resources in [
        json!({}),
        json!({ "cpu_millis": 1000 }),
        json!({ "memory_bytes": 268_435_456_u64 }),
    ] {
        let mut body = spec("web");
        body["resources"] = resources;
        let refused = b.deploy(body).await;
        assert_eq!(refused.status, 422);
        assert_eq!(code(&refused), "LIMITS_REQUIRED");
    }
    let mut body = spec("web");
    body.as_object_mut().unwrap().remove("resources");
    let refused = b.deploy(body).await;
    assert_eq!(code(&refused), "LIMITS_REQUIRED");
    assert!(b.engine.created().is_empty());
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_setting_outside_the_closed_plan_is_refused_without_naming_it() {
    let b = bench().await;
    for field in [
        "privileged",
        "network_mode",
        "devices",
        "cap_add",
        "pid",
        "binds",
    ] {
        let mut body = spec("web");
        body[field] = json!(true);
        let refused = b.deploy(body).await;
        assert_eq!(refused.status, 422, "{field}");
        assert_eq!(code(&refused), "PLAN_SETTING_REFUSED", "{field}");
        assert_eq!(
            refused.body["error"]["message"],
            "Ce réglage n'est pas permis pour un service créé par Hearth."
        );
        assert!(
            !refused.text.contains(field),
            "le réglage est nommé : {field}"
        );
    }
    assert!(b.engine.created().is_empty());
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_folder_of_the_server_is_refused_never_mounted() {
    let b = bench().await;
    let mut body = spec("web");
    body["mounts"] = json!([{ "target": "/data", "place": "host", "host_path": "/srv/data", "confirmed": true }]);
    let refused = b.deploy(body).await;
    assert_eq!(refused.status, 422);
    assert_eq!(code(&refused), "FOLDER_REFUSED");
    assert_eq!(refused.body["error"]["details"]["field"], "mounts");
    assert!(!refused.text.contains("/srv/data"), "le chemin est nommé");
    assert!(b.engine.created().is_empty());
    assert_eq!(b.specs_count().await, 0);
    b.agent.shutdown().await;
}

#[tokio::test]
async fn an_engine_that_is_not_ready_refuses_before_anything() {
    let b = bench().await;
    b.engine.set_outcome(
        hearth_agent::domain::services::ProbeOutcome::SocketMissing,
        true,
    );
    b.agent.services().recheck().await;
    // L'état publié suit l'anti-clignotement ; la sonde observée suffit pour refuser.
    let started = Instant::now();
    let refused = loop {
        let reply = b.deploy(spec("web")).await;
        if reply.status == 503 || started.elapsed() > Duration::from_secs(5) {
            break reply;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(refused.status, 503, "{}", refused.text);
    assert_eq!(code(&refused), "ENGINE_UNAVAILABLE");
    assert!(b.engine.created().is_empty());
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Le nom (BR-DEPLOY-004, BR-ADOPT-022)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_name_taken_by_anyone_is_refused_without_saying_by_whom() {
    let b = bench().await;
    // Un conteneur hors Hearth, non adopté, de ce nom (à la casse près).
    b.engine.put_silently(container("Cache", RunState::Running));
    b.agent.services().resync().await.expect("liste");
    let refused = b.deploy(spec("cache")).await;
    assert_eq!(refused.status, 409);
    assert_eq!(code(&refused), "NAME_TAKEN");
    assert_eq!(refused.body["error"]["message"], "Ce nom est déjà pris.");
    assert!(
        refused.body["error"]["details"]
            .as_object()
            .is_none_or(|details| details.is_empty()),
        "{}",
        refused.text
    );
    // Un service créé par Hearth, puis le même nom redemandé.
    let accepted = b.accepted(spec("web")).await;
    b.finished(&accepted.operation_id).await;
    let refused = b.deploy(spec("WEB")).await;
    assert_eq!(code(&refused), "NAME_TAKEN");
    // Rien d'autre n'a été créé.
    assert_eq!(b.engine.created().len(), 1);
    b.agent.shutdown().await;
}

#[tokio::test]
async fn two_deployments_of_the_same_name_at_once_one_wins() {
    let b = bench().await;
    let mut slow = PullScript::two_layers();
    slow.delay = Duration::from_millis(100);
    b.engine.publish_image("nginx:latest", slow);
    let (first, second) = tokio::join!(b.deploy(spec("web")), b.deploy(spec("web")));
    let statuses = [first.status, second.status];
    assert_eq!(
        statuses.iter().filter(|s| **s == 202).count(),
        1,
        "{statuses:?}"
    );
    let loser = if first.status == 202 { &second } else { &first };
    assert_eq!(code(loser), "NAME_TAKEN");
    let winner = if first.status == 202 { &first } else { &second };
    let accepted: DeployAccepted = serde_json::from_value(winner.body.clone()).unwrap();
    b.finished(&accepted.operation_id).await;
    assert_eq!(b.engine.created().len(), 1);
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Les ports (BR-DEPLOY-007 à 009, BR-ADOPT-022)
// ---------------------------------------------------------------------------------------------

fn with_port(host: u16) -> Value {
    let mut body = spec("web");
    body["ports"] = json!([{ "host_port": host, "container_port": 80 }]);
    body
}

#[tokio::test]
async fn a_port_held_by_a_container_a_program_or_the_agent_is_refused_with_a_suggestion() {
    let b = bench().await;
    // Un conteneur hors Hearth qui tourne : jamais nommé, plus petit port libre au-dessus proposé.
    let mut hidden = container("autre", RunState::Running);
    hidden.ports = vec![hearth_agent::domain::services::PortBinding {
        host_port: 8080,
        container_port: 80,
        protocol: hearth_agent::domain::services::PortProtocol::Tcp,
        scope: hearth_agent::domain::services::PortScope::All,
    }];
    b.engine.put_silently(hidden);
    b.agent.services().resync().await.expect("liste");
    let refused = b.deploy(with_port(8080)).await;
    assert_eq!(refused.status, 409);
    assert_eq!(code(&refused), "PORT_IN_USE");
    let details = &refused.body["error"]["details"];
    assert_eq!(details["port"], 8080);
    assert_eq!(details["suggested"], 8081);
    assert!(details["holder"].is_null());
    assert_eq!(
        refused.body["error"]["message"],
        "Port tenu par un conteneur créé hors de Hearth."
    );
    assert!(!refused.text.contains("autre"));
    // Un programme du serveur, nommé.
    b.host_ports
        .listen(2222, hearth_proto::api::deploy::DeployProtocol::Tcp, "sshd");
    let refused = b.deploy(with_port(2222)).await;
    assert_eq!(code(&refused), "PORT_IN_USE");
    assert_eq!(refused.body["error"]["details"]["holder"], "sshd");
    assert!(b.engine.created().is_empty());
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_port_reserved_by_a_stopped_service_needs_the_box_to_be_ticked() {
    let b = bench().await;
    let mut stopped = container("ancien", RunState::Exited);
    stopped.ports = vec![hearth_agent::domain::services::PortBinding {
        host_port: 9000,
        container_port: 80,
        protocol: hearth_agent::domain::services::PortProtocol::Tcp,
        scope: hearth_agent::domain::services::PortScope::All,
    }];
    b.engine.put_silently(stopped);
    b.agent.services().resync().await.expect("liste");
    let refused = b.deploy(with_port(9000)).await;
    assert_eq!(code(&refused), "PORT_IN_USE");
    assert_eq!(
        refused.body["error"]["details"]["status"],
        "reserved_by_stopped"
    );
    let mut body = with_port(9000);
    body["accept"] = json!({ "reserved_ports": true });
    let accepted = b.accepted(body).await;
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(operation.status, OperationStatus::Succeeded);
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// La vérification du formulaire (BR-DEPLOY-032, BR-DEPLOY-033)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_check_reports_each_field_writes_nothing_and_is_not_journaled() {
    let b = bench().await;
    b.engine.put_silently(container("pris", RunState::Running));
    b.agent.services().resync().await.expect("liste");
    b.host_ports
        .listen(22, hearth_proto::api::deploy::DeployProtocol::Tcp, "sshd");
    b.engine.have_image("busybox:1.36");
    let reply = b
        .agent
        .request("POST", "/deployments/check")
        .token(&b.ada.token)
        .json(&json!({
            "name": "pris",
            "image": "busybox",
            "tag": "1.36",
            "ports": [
                { "host_port": 22, "container_port": 22 },
                { "host_port": 8080, "container_port": 80 },
                { "host_port": 8080, "container_port": 81 }
            ],
            "mounts": [
                { "target": "/data" },
                { "target": "/x", "place": "host", "host_path": "/srv/x" },
                { "target": "relatif" }
            ]
        }))
        .send()
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let check: DeployCheck = serde_json::from_value(reply.body).unwrap();
    assert_eq!(check.name, hearth_proto::api::deploy::NameStatus::Taken);
    assert!(check.image_present);
    assert!(check.invalid_fields.contains(&"mounts".to_owned()));
    let ports: Vec<_> = check
        .ports
        .iter()
        .map(|p| (p.host_port, p.status, p.privileged))
        .collect();
    use hearth_proto::api::deploy::PortStatus::{HeldByProgram, Invalid};
    assert_eq!(
        ports,
        vec![
            (22, HeldByProgram, true),
            (8080, Invalid, false),
            (8080, Invalid, false)
        ]
    );
    assert_eq!(check.ports[0].holder.as_deref(), Some("sshd"));
    assert!(check.ports[0].suggested.is_some());
    use hearth_proto::api::deploy::MountStatus::{NotAvailable, Ok as MountOk};
    let mounts: Vec<_> = check.mounts.iter().map(|m| m.status).collect();
    assert_eq!(mounts[0], MountOk);
    assert_eq!(mounts[1], NotAvailable);
    assert_eq!(mounts[2], hearth_proto::api::deploy::MountStatus::Invalid);
    assert!(check.disk.is_some_and(|disk| !disk.low));
    // Rien n'est écrit, rien n'est journalisé.
    assert_eq!(b.specs_count().await, 0);
    assert!(b.engine.pulls().is_empty() && b.engine.created().is_empty());
    assert!(b.journal().await.is_empty());
    // Réservé au droit de gérer les services.
    let refused = b
        .agent
        .request("POST", "/deployments/check")
        .token(&b.bob.token)
        .json(&json!({ "name": "x", "image": "nginx" }))
        .send()
        .await;
    assert_eq!(refused.status, 403);
    b.agent.shutdown().await;
}

#[tokio::test]
async fn the_capacity_gives_the_bounds_of_the_machine_and_what_is_already_committed() {
    let b = bench().await;
    let mut taking = container("lourd", RunState::Running);
    taking.memory_limit_bytes = Some(2 * GIB);
    taking.cpu_millis = Some(1500);
    b.engine.put_silently(taking);
    b.agent.services().resync().await.expect("liste");
    // Ouverte à tout compte authentifié.
    let reply = b
        .agent
        .request("GET", "/engine/capacity")
        .token(&b.lucas.token)
        .send()
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let capacity: EngineCapacity = serde_json::from_value(reply.body.clone()).unwrap();
    assert_eq!(capacity.host.cpus, 8);
    assert_eq!(capacity.host.memory_bytes, 16 * GIB);
    assert_eq!(capacity.host.reserve_bytes, GIB);
    assert_eq!(capacity.committed.memory.unmanaged_running, 2 * GIB);
    assert_eq!(capacity.committed.cpu_millis.unmanaged_running, 1500);
    assert_eq!(capacity.committed.memory.managed_running, 0);
    // La forme que le client lit (HRT-146).
    assert_eq!(reply.body["host"]["reserve_bytes"], GIB);
    assert_eq!(
        reply.body["committed"]["memory"]["unmanaged_running"],
        2 * GIB
    );
    assert!(!reply.text.contains("lourd"));
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Deux déploiements au plus (BR-DEPLOY-023)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_third_deployment_is_refused_until_one_of_the_two_ends() {
    let b = bench().await;
    let mut slow = PullScript::two_layers();
    slow.delay = Duration::from_millis(150);
    b.engine.publish_image("nginx:latest", slow);
    let one = b.accepted(spec("un")).await;
    let two = b.accepted(spec("deux")).await;
    let refused = b.deploy(spec("trois")).await;
    assert_eq!(refused.status, 409);
    assert_eq!(code(&refused), "DEPLOY_LIMIT");
    assert_eq!(
        refused.body["error"]["message"],
        "Deux déploiements sont déjà en cours sur ce serveur. Attends la fin de l'un d'eux."
    );
    b.finished(&one.operation_id).await;
    b.finished(&two.operation_id).await;
    // Les places sont rendues.
    let three = b.accepted(spec("trois")).await;
    b.finished(&three.operation_id).await;
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Annuler (BR-DEPLOY-016)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn cancelling_during_the_pull_stops_it_and_creates_nothing() {
    let b = bench().await;
    let mut slow = PullScript::two_layers();
    slow.delay = Duration::from_millis(200);
    b.engine.publish_image("nginx:latest", slow);
    let accepted = b.accepted(spec("web")).await;
    let pulling = b.at_step(&accepted.operation_id, "pull").await;
    assert!(pulling.cancellable);
    let reply = b
        .agent
        .confirmed(
            &b.ada,
            "DELETE",
            &format!("/operations/services/{}", accepted.operation_id),
            json!({}),
        )
        .await
        .send()
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Cancelled,
        "{operation:?}"
    );
    assert_eq!(operation.cause, Some(OperationCause::CancelledByAccount));
    assert_eq!(b.engine.pulls_abandoned(), 1, "le flux est abandonné");
    assert!(b.engine.created().is_empty());
    // Ni service, ni description, ni ligne « Déploiement en cours » : le nom est libre.
    assert!(b.services().await.services.is_empty());
    assert_eq!(b.specs_count().await, 0);
    let again = b.accepted(spec("web")).await;
    b.engine
        .publish_image("nginx:latest", PullScript::two_layers());
    b.finished(&again.operation_id).await;
    let journal = b.journal().await;
    assert!(
        journal
            .iter()
            .any(|row| row.0 == "failed" && row.2.as_deref() == Some("annulé par l'utilisateur")),
        "{journal:?}"
    );
    b.agent.shutdown().await;
}

#[tokio::test]
async fn once_the_container_is_being_created_it_is_too_late_to_cancel() {
    // La surveillance dure assez pour qu'une demande confirmée arrive pendant elle.
    let b = bench_with(|parts| parts.deploy.watch = Duration::from_secs(4)).await;
    b.engine.have_image("nginx:latest");
    let accepted = b.accepted(spec("web")).await;
    b.at_step(&accepted.operation_id, "watch").await;
    let reply = b
        .agent
        .confirmed(
            &b.ada,
            "DELETE",
            &format!("/operations/services/{}", accepted.operation_id),
            json!({}),
        )
        .await
        .send()
        .await;
    assert_eq!(reply.status, 409, "{}", reply.text);
    assert_eq!(code(&reply), "TOO_LATE_TO_CANCEL");
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(operation.status, OperationStatus::Succeeded);
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Les échecs de l'image sont dits, aucun service n'est créé (BR-DEPLOY-017, 019, 025)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn every_pull_failure_is_told_clearly_and_leaves_nothing_behind() {
    let b = bench().await;
    let table = [
        (
            EngineError::ImageNotFound,
            OperationCause::ImageNotFound,
            "image introuvable",
        ),
        (
            EngineError::TagNotFound,
            OperationCause::TagNotFound,
            "version introuvable",
        ),
        (
            EngineError::ImagePrivateOrMissing,
            OperationCause::ImagePrivateOrMissing,
            "image privée ou introuvable",
        ),
        (
            EngineError::RegistryUnreachable,
            OperationCause::RegistryUnreachable,
            "registre injoignable",
        ),
        (
            EngineError::RegistryRateLimited,
            OperationCause::RegistryRateLimited,
            "limite du registre",
        ),
        (
            EngineError::PlatformMismatch,
            OperationCause::PlatformMismatch,
            "architecture incompatible",
        ),
        (
            EngineError::DiskFull,
            OperationCause::DiskFull,
            "disque plein",
        ),
    ];
    for (error, cause, text) in table {
        b.engine
            .publish_image("nginx:latest", PullScript::failing(error));
        let accepted = b.accepted(spec("web")).await;
        let operation = b.finished(&accepted.operation_id).await;
        assert_eq!(operation.status, OperationStatus::Failed, "{error:?}");
        assert_eq!(operation.cause, Some(cause), "{error:?}");
        // Rien ne reste : ni service, ni description, ni conteneur ; le nom est libre pour la suite.
        assert!(b.services().await.services.is_empty(), "{error:?}");
        assert_eq!(b.specs_count().await, 0, "{error:?}");
        let journal = b.journal().await;
        let last = journal.last().unwrap();
        assert_eq!(
            (last.0.as_str(), last.2.as_deref()),
            ("failed", Some(text)),
            "{error:?}"
        );
    }
    assert!(b.engine.created().is_empty());
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_container_refused_at_start_is_removed_and_nothing_stays() {
    let b = bench().await;
    b.engine.have_image("nginx:latest");
    // Le port est pris au dernier moment : après les vérifications, avant le démarrage.
    let accepted = {
        let mut slow = PullScript::two_layers();
        slow.delay = Duration::from_millis(100);
        b.engine.forget_image("nginx:latest");
        b.engine.publish_image("nginx:latest", slow);
        b.accepted(with_port(8088)).await
    };
    let mut rival = container("rival", RunState::Running);
    rival.ports = vec![hearth_agent::domain::services::PortBinding {
        host_port: 8088,
        container_port: 80,
        protocol: hearth_agent::domain::services::PortProtocol::Tcp,
        scope: hearth_agent::domain::services::PortScope::All,
    }];
    b.engine.put_silently(rival);
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(operation.status, OperationStatus::Failed, "{operation:?}");
    assert_eq!(operation.cause, Some(OperationCause::PortInUse));
    // Le conteneur créé puis refusé est supprimé (aucun orphelin) ; la description et les consignations partent.
    assert_eq!(b.engine.created().len(), 1);
    assert_eq!(b.engine.removed().len(), 1);
    assert!(!b.engine.container_ids().contains(&b.engine.removed()[0]));
    assert_eq!(b.specs_count().await, 0);
    let recorded: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM service_containers")
        .fetch_one(b.env.db.pool())
        .await
        .unwrap();
    assert_eq!(recorded, 0);
    assert!(b.services().await.services.is_empty());
    let journal = b.journal().await;
    assert_eq!(journal.last().unwrap().2.as_deref(), Some("port occupé"));
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_name_taken_at_the_last_moment_by_the_engine_fails_with_its_cause() {
    let b = bench().await;
    b.engine.have_image("nginx:latest");
    b.engine.fail_create(Some(EngineError::NameTaken));
    let accepted = b.accepted(spec("web")).await;
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(operation.status, OperationStatus::Failed);
    assert_eq!(operation.cause, Some(OperationCause::NameTaken));
    assert_eq!(b.specs_count().await, 0);
    b.agent.shutdown().await;
}

#[tokio::test]
async fn an_unreachable_engine_after_the_order_leaves_an_unknown_result_and_removes_nothing() {
    let b = bench().await;
    b.engine.have_image("nginx:latest");
    b.engine.fail_create(Some(EngineError::Unreachable));
    let accepted = b.accepted(spec("web")).await;
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Interrupted,
        "{operation:?}"
    );
    assert_eq!(operation.cause, Some(OperationCause::EngineUnavailable));
    // On ne sait pas si le conteneur existe : la description reste (BR-DEPLOY-028).
    assert_eq!(b.specs_count().await, 1);
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// La surveillance de 10 secondes (BR-DEPLOY-020)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_program_that_stops_right_after_its_start_is_announced_without_failing_the_deployment() {
    let b = bench().await;
    b.engine.have_image("nginx:latest");
    b.engine.exit_on_start("web", 3);
    let accepted = b.accepted(spec("web")).await;
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    assert_eq!(
        operation.cause,
        Some(OperationCause::StoppedRightAfterStart)
    );
    // Le service est conservé, dans la liste, pour être corrigé.
    let state = b.services().await;
    assert!(
        state
            .services
            .iter()
            .any(|item| item.key == accepted.service_key)
    );
    let journal = b.journal().await;
    let last = journal.last().unwrap();
    assert_eq!(last.0, "ok");
    assert!(
        last.1
            .as_deref()
            .unwrap()
            .contains("arrêté juste après le démarrage")
    );
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Le déploiement en cours est visible de tous (BR-DEPLOY-024)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_deployment_in_progress_is_listed_for_everyone_without_its_variables_then_becomes_the_service()
 {
    let b = bench().await;
    let mut slow = PullScript::two_layers();
    slow.delay = Duration::from_millis(150);
    b.engine.publish_image("nginx:latest", slow);
    let mut body = spec("web");
    body["env"] = json!([{ "name": "PASSWORD", "value": "SENTINEL-LIST-0a1b" }]);
    let accepted = b.accepted(body).await;
    b.at_step(&accepted.operation_id, "pull").await;
    // Un compte en lecture seule SANS le droit le voit.
    let reply = b
        .agent
        .request("GET", "/services")
        .token(&b.lucas.token)
        .send()
        .await;
    let state: ServicesState = serde_json::from_value(reply.body).unwrap();
    let item = state
        .services
        .iter()
        .find(|item| item.key == accepted.service_key)
        .expect("ligne");
    assert_eq!(
        item.state,
        hearth_proto::api::services::ServiceState::Deploying
    );
    assert_eq!(item.name, "web");
    assert!(!reply.text.contains("SENTINEL-LIST") && !reply.text.contains("PASSWORD"));
    assert!(item.busy.is_some());
    b.finished(&accepted.operation_id).await;
    let after = b.services().await;
    let item = after
        .services
        .iter()
        .find(|item| item.key == accepted.service_key)
        .unwrap();
    assert_eq!(
        item.state,
        hearth_proto::api::services::ServiceState::Running
    );
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Les secrets (BR-DEPLOY-006, BR-DEPLOY-025, BR-EDIT-034) : le test sentinelle
// ---------------------------------------------------------------------------------------------

fn captured_traces() -> Arc<Mutex<Vec<u8>>> {
    #[derive(Clone)]
    struct Sink(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Sink {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Sink {
        type Writer = Sink;
        fn make_writer(&'a self) -> Sink {
            self.clone()
        }
    }
    static TRACES: OnceLock<Arc<Mutex<Vec<u8>>>> = OnceLock::new();
    TRACES
        .get_or_init(|| {
            let buffer = Arc::new(Mutex::new(Vec::new()));
            logging::try_init_with(
                BoxMakeWriter::new(Sink(buffer.clone())),
                LogFormat::Json,
                false,
                EnvFilter::new("trace,bollard=trace,hyper_util=trace"),
            )
            .expect("première installation du journal de ce binaire de test");
            buffer
        })
        .clone()
}

fn all_bytes(dir: &std::path::Path) -> Vec<u8> {
    let mut bytes = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            bytes.extend(all_bytes(&path));
        } else if let Ok(content) = std::fs::read(&path) {
            bytes.extend(content);
        }
    }
    bytes
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

/// Une valeur de variable, un nom de variable et la commande ne se retrouvent ni en base (fichier, journal
/// d'écriture compris), ni dans les traces, ni au journal d'activité, ni dans ce que les routes rendent : après
/// une réussite comme après un échec. Seul le moteur les reçoit.
#[tokio::test]
async fn no_variable_value_name_or_command_leaves_the_engine_after_a_success_or_a_failure() {
    const VALUE: &str = "SENTINEL-VALUE-5c1d90e7";
    const NAME: &str = "SENTINEL_NAME_TOKEN_77aa";
    const COMMAND: &str = "--password=SENTINEL-CMD-3f2b11";
    let traces = captured_traces();
    log::debug!(target: "hearth_witness", "witness-deploy-line");
    let b = bench().await;
    let body = |name: &str| {
        let mut body = spec(name);
        body["env"] = json!([{ "name": NAME, "value": VALUE }]);
        body["command"] = json!(format!("serve {COMMAND}"));
        body
    };
    let mut shown = String::new();
    // Une réussite…
    let ok = b.accepted(body("sain")).await;
    shown.push_str(&serde_json::to_string(&b.finished(&ok.operation_id).await).unwrap());
    // …un échec de l'image…
    b.engine.publish_image(
        "nginx:latest",
        PullScript::failing(EngineError::RegistryUnreachable),
    );
    let failed = b.accepted(body("rate")).await;
    shown.push_str(&serde_json::to_string(&b.finished(&failed.operation_id).await).unwrap());
    // …un refus de validation.
    let mut refused = body("refuse");
    refused["image"] = json!("MAJUSCULES");
    let reply = b.deploy(refused).await;
    shown.push_str(&reply.text);
    // …un refus du plan fermé.
    let mut closed = body("ferme");
    closed["privileged"] = json!(true);
    shown.push_str(&b.deploy(closed).await.text);
    // Le moteur, lui, a bien reçu la valeur, le nom et la commande (le test n'est pas aveugle).
    let created = b.engine.created();
    assert!(
        created
            .iter()
            .any(|plan| plan.env.iter().any(|(n, v)| n == NAME && v == VALUE))
    );
    assert!(created.iter().any(|plan| {
        plan.command
            .as_ref()
            .is_some_and(|c| c.iter().any(|a| a.contains("SENTINEL-CMD")))
    }));
    // Ce que les routes rendent.
    for path in ["/services", "/engine/capacity"] {
        shown.push_str(
            &b.agent
                .request("GET", path)
                .token(&b.dan.token)
                .send()
                .await
                .text,
        );
    }
    shown.push_str(
        &b.agent
            .request("GET", &format!("/services/{}/env", ok.service_key))
            .token(&b.dan.token)
            .send()
            .await
            .text
            .replace(VALUE, "")
            .replace(NAME, ""),
    );
    let journal = format!("{:?}", b.journal().await);
    let audit = b
        .agent
        .request("GET", "/audit")
        .token(&b.ada.token)
        .send()
        .await
        .text;
    let data_dir = b.env.dir.path().to_owned();
    b.agent.shutdown().await;
    let captured = String::from_utf8_lossy(&traces.lock().unwrap()).into_owned();
    assert!(
        captured.contains("witness-deploy-line"),
        "le pont `log` doit être actif"
    );
    assert!(
        captured.contains("déploiement terminé"),
        "la trace de fin existe : le test n'est pas aveugle"
    );
    let files = all_bytes(&data_dir);
    // Ce que les routes rendent : la valeur n'y est jamais ; le NOM des variables et la commande d'un service
    // créé par Hearth sont, eux, lisibles de tous les comptes (liste des services, BR-DEPLOY-014, BR-SVC-009).
    assert!(
        !shown.contains(VALUE),
        "la valeur dans ce que les routes rendent"
    );
    for forbidden in [VALUE, NAME, COMMAND, "SENTINEL-CMD"] {
        assert!(!captured.contains(forbidden), "{forbidden} dans les traces");
        assert!(
            !contains(&files, forbidden),
            "{forbidden} dans les fichiers de l'agent"
        );
        assert!(
            !journal.contains(forbidden),
            "{forbidden} au journal d'activité"
        );
        assert!(
            !audit.contains(forbidden),
            "{forbidden} dans le journal rendu"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// L'espace disque (BR-DEPLOY-018)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn under_ten_gigabytes_free_the_box_must_be_ticked_unless_the_image_is_already_there() {
    let b = bench().await;
    b.free_space.set(5 * GIB);
    let refused = b.deploy(spec("web")).await;
    assert_eq!(refused.status, 507, "{}", refused.text);
    assert_eq!(code(&refused), "NO_SPACE");
    assert_eq!(refused.body["error"]["details"]["reason"], "low_disk");
    assert_eq!(refused.body["error"]["details"]["free_bytes"], 5 * GIB);
    assert!(b.engine.created().is_empty() && b.engine.pulls().is_empty());
    // La vérification le dit aussi.
    let check = b
        .agent
        .request("POST", "/deployments/check")
        .token(&b.ada.token)
        .json(&json!({ "name": "web", "image": "nginx" }))
        .send()
        .await;
    let check: DeployCheck = serde_json::from_value(check.body).unwrap();
    assert!(check.disk.is_some_and(|disk| disk.low));
    // La case cochée : le déploiement part.
    let mut body = spec("web");
    body["accept"] = json!({ "low_disk": true });
    let accepted = b.accepted(body).await;
    b.finished(&accepted.operation_id).await;
    // Une image déjà présente n'a pas besoin de place.
    b.engine.have_image("nginx:latest");
    let accepted = b.accepted(spec("autre")).await;
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(operation.status, OperationStatus::Succeeded);
    // Un disque illisible ne bloque rien : l'agent ne prétend pas savoir.
    b.free_space.fail(true);
    let accepted = b.accepted(spec("encore")).await;
    b.finished(&accepted.operation_id).await;
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Le droit retiré (BR-DEPLOY-030) et le délai de 30 minutes (BR-DEPLOY-034)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_right_withdrawn_while_the_deployment_runs_does_not_stop_it_but_the_next_request_is_refused()
 {
    let b = bench().await;
    let mut slow = PullScript::two_layers();
    slow.delay = Duration::from_millis(80);
    b.engine.publish_image("nginx:latest", slow);
    let accepted = b.accepted(spec("web")).await;
    b.env.revoke_services("ada").await;
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(operation.status, OperationStatus::Succeeded);
    let refused = b.deploy(spec("autre")).await;
    assert_eq!(refused.status, 403);
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_deadline_that_cuts_the_creation_order_leaves_an_unknown_result_and_removes_nothing() {
    // Le moteur crée le conteneur mais ne répond qu'après le délai : l'agent ne sait pas s'il existe.
    let b = bench_with(|parts| parts.deploy.deadline = Duration::from_millis(400)).await;
    b.engine.have_image("nginx:latest");
    b.engine.delay_create_reply(Duration::from_secs(5));
    let accepted = b.accepted(spec("web")).await;
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Interrupted,
        "{operation:?}"
    );
    assert_eq!(operation.cause, Some(OperationCause::Timeout));
    assert_eq!(b.engine.created().len(), 1);
    assert!(
        b.engine.removed().is_empty(),
        "rien n'est retiré sous un résultat inconnu"
    );
    assert_eq!(
        b.specs_count().await,
        1,
        "la description reste : le compte relit l'état réel"
    );
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_deployment_that_lasts_past_its_deadline_is_stopped_and_leaves_nothing() {
    // Les 30 minutes deviennent 400 ms ; la récupération en met plus.
    let b = bench_with(|parts| parts.deploy.deadline = Duration::from_millis(400)).await;
    let mut slow = PullScript::two_layers();
    slow.delay = Duration::from_secs(5);
    b.engine.publish_image("nginx:latest", slow);
    let accepted = b.accepted(spec("web")).await;
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(operation.status, OperationStatus::Failed, "{operation:?}");
    assert_eq!(operation.cause, Some(OperationCause::Timeout));
    assert!(b.engine.created().is_empty());
    assert_eq!(b.specs_count().await, 0);
    assert!(b.services().await.services.is_empty());
    let journal = b.journal().await;
    assert_eq!(journal.last().unwrap().2.as_deref(), Some("délai dépassé"));
    b.agent.shutdown().await;
}
