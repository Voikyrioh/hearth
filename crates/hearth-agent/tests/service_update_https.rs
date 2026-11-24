//! Modifier un service par recréation, de bout en bout (HRT-87, ADR-0044, ADR-0045, BR-EDIT-*) : vrai agent en HTTPS,
//! vraie base, faux moteur, faux dossiers de données. `PUT /services/{key}/spec` répond `202`, la modification court
//! à l'arrière-plan, l'issue se lit dans l'opération et dans le journal d'activité.
//!
//! Règles : BR-EDIT-005 à BR-EDIT-007, BR-EDIT-011, BR-EDIT-013, BR-EDIT-015, BR-EDIT-018 à BR-EDIT-022,
//! BR-EDIT-024, BR-EDIT-028, BR-EDIT-033, BR-EDIT-034, BR-EDIT-036, BR-EDIT-037.
//! La preuve de clé et le mot de passe sont dans la matrice de `admin_reauth.rs` ; ici : ce que fait la route et le
//! cas d'usage, jusqu'au journal d'activité et à la base.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use hearth_agent::application::ports::{EngineError, EngineInfo};
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::services::{RunState, ServiceKey};
use hearth_proto::api::deploy::DeployAccepted;
use hearth_proto::api::service_ops::{OperationCause, OperationStatus, OperationView};
use hearth_proto::api::service_spec::DriftView;
use hearth_proto::api::service_update::{OperationTail, ServiceSpecView, UpdateAccepted};
use serde_json::{Value, json};
use support::device::Actor;
use support::engine::{fast_timings, parts_supervised};
use support::fake_engine::{Call, FakeEngine, PullScript};
use support::fake_game_console::FakeGameConsole;
use support::fake_host::{FakeFreeSpace, FakeHostPorts};
use support::fake_safe_fs::FakeSafeFs;
use support::fake_supervisor::FakeSupervisor;
use support::https::{self, Agent, Wire};
use support::{Env, env};

const GIB: u64 = 1024 * 1024 * 1024;
/// Une valeur et un nom de variable qui ne doivent apparaître NULLE PART hors du moteur et de la base scellée.
const SECRET_VALUE: &str = "sentinelle-valeur-7f3a";
const SECRET_NAME: &str = "SENTINELLE_NOM_7F3A";
const NEW_VALUE: &str = "sentinelle-neuve-c91d";
/// Une ligne du nouveau service qui n'a pas démarré : du contenu sensible, jamais ailleurs qu'en mémoire.
const LOG_LINE: &str = "sentinelle-ligne-de-journal-5e21";

struct Bench {
    env: Env,
    engine: Arc<FakeEngine>,
    agent: Agent,
    /// Administrateur avec le droit de gérer les services.
    ada: Actor,
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
    let mut parts = parts_supervised(
        engine.clone(),
        Arc::new(FakeSupervisor::new()),
        fast_timings(),
    );
    parts.host_ports = Arc::new(FakeHostPorts::new());
    parts.free_space = Arc::new(FakeFreeSpace::new());
    parts.safe_fs = Some(Arc::new(FakeSafeFs::new()));
    change(&mut parts);
    let agent = https::start_engined(&env, parts).await;
    let ada = agent.actor(&env, "ada", Role::Admin).await;
    let lucas = agent.actor(&env, "lucas", Role::ReadOnly).await;
    let dan = agent.actor(&env, "dan", Role::ReadOnly).await;
    env.grant_services("ada").await;
    env.grant_services("dan").await;
    engine.publish_image("nginx:latest", PullScript::two_layers());
    engine.publish_image("nginx:1.27", PullScript::two_layers());
    Bench {
        env,
        engine,
        agent,
        ada,
        lucas,
        dan,
    }
}

/// Un service déployé : sa clé, son nom et son conteneur.
struct Deployed {
    key: String,
    container: String,
}

impl Bench {
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

    /// Attend la fin de l'opération (un statut qui n'attend plus rien).
    async fn finished(&self, id: &str) -> OperationView {
        let started = Instant::now();
        loop {
            let operation = self.operation(id).await;
            if !matches!(
                operation.status,
                OperationStatus::Accepted
                    | OperationStatus::Running
                    | OperationStatus::Cancelling
                    | OperationStatus::RollingBack
            ) {
                return operation;
            }
            assert!(
                started.elapsed() < Duration::from_secs(15),
                "l'opération ne finit pas : {operation:?}"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Déploie `name` : un port, une variable secrète, un dossier géré. Rend le service une fois arrivé en marche.
    async fn deploy(&self, name: &str, port: u16) -> Deployed {
        let reply = self
            .agent
            .confirmed(
                &self.ada,
                "POST",
                "/deployments",
                json!({
                    "name": name, "image": "nginx",
                    "env": [{ "name": SECRET_NAME, "value": SECRET_VALUE }],
                    "ports": [{ "host_port": port, "container_port": 80 }],
                    "mounts": [{ "target": "/data" }],
                    "resources": { "cpu_millis": 1000, "memory_bytes": 268_435_456_u64 }
                }),
            )
            .await
            .send()
            .await;
        assert_eq!(reply.status, 202, "{}", reply.text);
        let accepted: DeployAccepted = serde_json::from_value(reply.body).expect("accepté");
        let operation = self.finished(&accepted.operation_id).await;
        assert_eq!(
            operation.status,
            OperationStatus::Succeeded,
            "{operation:?}"
        );
        let container = self
            .engine
            .container_ids()
            .into_iter()
            .last()
            .expect("conteneur");
        Deployed {
            key: accepted.service_key,
            container,
        }
    }

    async fn put(&self, who: &Actor, key: &str, body: Value) -> Wire {
        self.agent
            .confirmed(who, "PUT", &format!("/services/{key}/spec"), body)
            .await
            .send()
            .await
    }

    /// La modification demandée par `ada`, acceptée : `202` et l'opération à suivre.
    async fn accepted(&self, key: &str, body: Value) -> String {
        let reply = self.put(&self.ada, key, body).await;
        assert_eq!(reply.status, 202, "{}", reply.text);
        let accepted: UpdateAccepted = serde_json::from_value(reply.body).expect("accepté");
        assert!(accepted.applied);
        accepted.operation_id.expect("opération")
    }

    async fn spec(&self, key: &str) -> ServiceSpecView {
        let reply = self
            .agent
            .request("GET", &format!("/services/{key}/spec"))
            .token(&self.dan.token)
            .send()
            .await;
        assert_eq!(reply.status, 200, "{}", reply.text);
        serde_json::from_value(reply.body).expect("ServiceSpecView")
    }

    async fn resync(&self) {
        self.agent.services().resync().await.expect("liste");
    }

    /// Le conteneur que la liste publie pour ce service : (identifiant, nom, état).
    fn published(&self, key: &str) -> Option<(String, String, RunState)> {
        let key = ServiceKey::parse(key).unwrap();
        let target = self.agent.services().lifecycle_target(&key)?;
        let hearth_agent::application::services::Driver::Engine { container_id } = target.driver
        else {
            return None;
        };
        let state = self
            .engine
            .container_ids()
            .contains(&container_id)
            .then(|| self.engine_state(&container_id))?;
        Some((container_id, target.name, state))
    }

    fn engine_state(&self, id: &str) -> RunState {
        let mut found = RunState::Unknown;
        self.engine
            .update(id, |container| found = container.run_state);
        found
    }

    async fn revisions(&self, key: &str) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM service_specs WHERE service_key = ?")
            .bind(key)
            .fetch_one(self.env.db.pool())
            .await
            .unwrap()
    }

    async fn update_ops(&self, key: &str) -> i64 {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM service_ops WHERE service_key = ? AND kind = 'update'",
        )
        .bind(key)
        .fetch_one(self.env.db.pool())
        .await
        .unwrap()
    }

    /// Les entrées du journal d'activité de modification : résultat, cible, raison, compte.
    async fn journal(&self) -> Vec<(String, Option<String>, Option<String>, Option<String>)> {
        self.agent.flush_journal().await;
        sqlx::query_as(
            "SELECT outcome, target, reason, account FROM audit_events
             WHERE action = 'service.update' ORDER BY id",
        )
        .fetch_all(self.env.db.pool())
        .await
        .unwrap()
    }

    /// Tout le texte que l'agent écrit sur le disque hors de la description scellée : journal d'activité, opérations
    /// et leur journal d'étapes. Aucune valeur secrète ne s'y trouve (BR-EDIT-033, BR-EDIT-034).
    async fn plain_text_of_the_database(&self) -> String {
        self.agent.flush_journal().await;
        let mut text = String::new();
        let audit: Vec<(String, Option<String>, Option<String>)> =
            sqlx::query_as("SELECT action, target, reason FROM audit_events")
                .fetch_all(self.env.db.pool())
                .await
                .unwrap();
        for (action, target, reason) in audit {
            text.push_str(&format!("{action} {target:?} {reason:?}\n"));
        }
        let ops: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT plan_json, progress_json, COALESCE(decision_json, ''), cause FROM service_ops",
        )
        .fetch_all(self.env.db.pool())
        .await
        .unwrap();
        for (plan, progress, decision, cause) in ops {
            text.push_str(&format!("{plan} {progress} {decision} {cause}\n"));
        }
        text
    }
}

fn names_of(operation: &OperationView) -> Vec<String> {
    operation.steps.iter().map(|s| s.step.clone()).collect()
}

#[tokio::test]
async fn reading_the_description_never_shows_a_value_nor_the_command() {
    let b = bench().await;
    let service = b.deploy("web", 18_080).await;
    let reply = b
        .agent
        .request("GET", &format!("/services/{}/spec", service.key))
        .token(&b.dan.token)
        .send()
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert!(
        !reply.text.contains(SECRET_VALUE),
        "aucune valeur n'est rendue : {}",
        reply.text
    );
    let view: ServiceSpecView = serde_json::from_value(reply.body).unwrap();
    assert_eq!(view.rev, 1);
    assert_eq!(view.name, "web");
    assert_eq!(view.image.repository, "nginx");
    assert_eq!(view.env.len(), 1);
    assert_eq!(view.env[0].name.expose(), SECRET_NAME);
    assert_eq!(view.ports.len(), 1);
    assert!(!view.command_set);
    assert_eq!(view.drift, DriftView::None);
    // Sans le droit, la description n'est pas lisible.
    let denied = b
        .agent
        .request("GET", &format!("/services/{}/spec", service.key))
        .token(&b.lucas.token)
        .send()
        .await;
    assert_eq!(denied.status, 403, "{}", denied.text);
    // Une clé inconnue est un service inconnu.
    let unknown = b
        .agent
        .request("GET", "/services/h_01JZ0000000000000000000099/spec")
        .token(&b.dan.token)
        .send()
        .await;
    assert_eq!(unknown.status, 404);
    assert_eq!(unknown.body["error"]["code"], "SERVICE_NOT_FOUND");
}

// ---------------------------------------------------------------------------------------------
// Enregistrer le monde par la console avant l'arrêt de l'ancien conteneur (HRT-292, BR-CONSOLE-012)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn recreating_a_running_game_saves_by_the_console_before_the_old_one_is_stopped() {
    let game = FakeGameConsole::new();
    let wired = game.clone();
    let b = bench_with(move |parts| parts.catalog.console = Some(wired)).await;
    game.watch_engine(b.engine.clone());
    let key =
        support::game::minecraft_console_service(&b.env, &b.engine, &b.agent.services(), 1).await;
    b.engine
        .publish_image("itzg/minecraft-server:java21", PullScript::two_layers());
    game.answer("save-all flush", "Saved the game");
    let operation_id = b
        .accepted(
            key.as_str(),
            json!({ "base_rev": 1, "seen_drift": "modified_outside" }),
        )
        .await;
    let operation = b.finished(&operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    let names = names_of(&operation);
    let save = names.iter().position(|n| n == "console_save").unwrap();
    let stop_old = names.iter().position(|n| n == "stop_old").unwrap();
    assert!(save < stop_old, "{names:?}");
    assert_eq!(game.lines(), ["save-all flush"]);
    assert_eq!(
        game.received()[0].stops_seen,
        0,
        "avant l'arrêt de l'ancien"
    );
}

#[tokio::test]
async fn recreating_a_stopped_game_has_no_console_save_step() {
    let game = FakeGameConsole::new();
    let wired = game.clone();
    let b = bench_with(move |parts| parts.catalog.console = Some(wired)).await;
    let key =
        support::game::minecraft_console_service(&b.env, &b.engine, &b.agent.services(), 1).await;
    b.engine
        .publish_image("itzg/minecraft-server:java21", PullScript::two_layers());
    let container = b.engine.container_ids().into_iter().last().unwrap();
    b.engine
        .update(&container, |o| o.run_state = RunState::Exited);
    b.resync().await;
    let operation_id = b
        .accepted(
            key.as_str(),
            json!({ "base_rev": 1, "seen_drift": "modified_outside" }),
        )
        .await;
    let operation = b.finished(&operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    assert!(
        !names_of(&operation).iter().any(|n| n == "console_save"),
        "{:?}",
        names_of(&operation)
    );
    assert!(game.lines().is_empty());
}

#[tokio::test]
async fn a_running_service_is_recreated_with_its_kept_values_and_the_old_one_goes_last() {
    let b = bench().await;
    let service = b.deploy("web", 18_081).await;
    let operation_id = b
        .accepted(
            &service.key,
            json!({ "base_rev": 1, "tag": "1.27",
                    "env": [{ "name": "AUTRE", "op": "set", "value": NEW_VALUE }] }),
        )
        .await;
    let operation = b.finished(&operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    assert_eq!(
        names_of(&operation),
        [
            "check",
            "pull",
            "stop_old",
            "rename_old",
            "create_new",
            "start_new",
            "watch",
            "remove_old"
        ]
    );
    // L'ancien est mis de côté sous un nom provisoire AVANT que le nouveau prenne son nom, et n'est supprimé qu'après.
    assert_eq!(
        b.engine.renamed(),
        [(service.container.clone(), "web.hearth-prev-1".to_owned())]
    );
    let created = b.engine.created();
    assert_eq!(created.len(), 2, "le déploiement, puis la modification");
    let plan = &created[1];
    assert_eq!(plan.name, "web");
    assert_eq!(plan.image, "nginx:1.27");
    // La valeur conservée vient de la description : le client ne l'a pas renvoyée (BR-EDIT-006).
    assert!(
        plan.env
            .iter()
            .any(|(name, value)| name == SECRET_NAME && value == SECRET_VALUE),
        "{:?}",
        plan.env.len()
    );
    assert!(plan.env.iter().any(|(_, value)| value == NEW_VALUE));
    assert_eq!(
        b.engine.removed_keeping_volumes(),
        std::slice::from_ref(&service.container),
        "l'ancien conteneur part, sans volume ni force"
    );
    let calls = b.engine.calls();
    let old_stop = calls
        .iter()
        .position(|call| matches!(call, Call::Stop(id, _) if *id == service.container))
        .expect("l'ancien est arrêté");
    let new_start = calls
        .iter()
        .rposition(|call| matches!(call, Call::Start(id) if *id != service.container))
        .expect("le nouveau est lancé");
    assert!(old_stop < new_start);
    // La liste montre un seul conteneur sous le nom du service, en marche.
    b.resync().await;
    let (container, name, state) = b.published(&service.key).expect("publié");
    assert_ne!(container, service.container);
    assert_eq!((name.as_str(), state), ("web", RunState::Running));
    // La description est la révision 2, l'ancienne est purgée.
    let view = b.spec(&service.key).await;
    assert_eq!((view.rev, view.image.tag.as_str()), (2, "1.27"));
    assert_eq!(view.env.len(), 2);
    assert_eq!(b.revisions(&service.key).await, 1);
    assert_eq!(view.drift, DriftView::None);
    // Une seule entrée au journal d'activité : réussie, sans nom ni valeur de variable.
    let journal = b.journal().await;
    assert_eq!(journal.len(), 1, "{journal:?}");
    assert_eq!(journal[0].0, "ok");
    let target = journal[0].1.clone().expect("cible");
    assert!(
        target.contains("web") && target.contains("version 1.27"),
        "{target}"
    );
    assert!(
        target.contains("variables (1 posée, 0 retirée)"),
        "{target}"
    );
    let plain = b.plain_text_of_the_database().await;
    for secret in [SECRET_VALUE, SECRET_NAME, NEW_VALUE, "AUTRE"] {
        assert!(!plain.contains(secret), "{secret} trouvé dans {plain}");
    }
}

#[tokio::test]
async fn a_stopped_service_is_recreated_and_stays_stopped() {
    let b = bench().await;
    let service = b.deploy("web", 18_082).await;
    b.engine.update(&service.container, |container| {
        container.run_state = RunState::Exited;
        container.exit_code = Some(0);
    });
    b.resync().await;
    let starts_before = b
        .engine
        .calls()
        .iter()
        .filter(|call| matches!(call, Call::Start(_)))
        .count();
    let operation_id = b
        .accepted(
            &service.key,
            json!({ "base_rev": 1,
                    "env": [{ "name": "AUTRE", "op": "set", "value": NEW_VALUE }] }),
        )
        .await;
    let operation = b.finished(&operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    assert_eq!(
        names_of(&operation),
        ["check", "rename_old", "create_new", "remove_old"],
        "ni arrêt ni démarrage : le service était arrêté (BR-EDIT-015)"
    );
    let starts_after = b
        .engine
        .calls()
        .iter()
        .filter(|call| matches!(call, Call::Start(_)))
        .count();
    assert_eq!(starts_after, starts_before, "Hearth ne démarre rien seul");
    b.resync().await;
    let (container, _, state) = b.published(&service.key).expect("publié");
    assert_ne!(container, service.container);
    assert_eq!(state, RunState::Created, "recréé, jamais démarré");
}

#[tokio::test]
async fn a_new_service_that_stops_within_ten_seconds_is_replaced_by_the_old_one() {
    let b = bench().await;
    let service = b.deploy("web", 18_083).await;
    // Le nouveau programme s'arrête aussitôt lancé, code zéro compris (BR-EDIT-019) ; il écrit une ligne avant.
    b.engine.exit_once_on_start("web", 0);
    b.engine.log_on_start("web", LOG_LINE);
    let operation_id = b
        .accepted(
            &service.key,
            json!({ "base_rev": 1, "env": [{ "name": "AUTRE", "op": "set", "value": NEW_VALUE }] }),
        )
        .await;
    let operation = b.finished(&operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::RolledBack,
        "{operation:?}"
    );
    assert_eq!(
        operation.cause,
        Some(OperationCause::StoppedRightAfterStart)
    );
    // Le nouveau est retiré, l'ancien a repris son nom et tourne, la révision candidate est abandonnée.
    let created = b.engine.created();
    assert_eq!(created.len(), 2);
    b.resync().await;
    let (container, name, state) = b.published(&service.key).expect("publié");
    assert_eq!(
        container, service.container,
        "c'est l'ancien qui est revenu"
    );
    assert_eq!((name.as_str(), state), ("web", RunState::Running));
    assert_eq!(b.engine.container_ids().len(), 1, "un seul conteneur reste");
    let view = b.spec(&service.key).await;
    assert_eq!(view.rev, 1, "la saisie ne reste pas dans la description");
    assert_eq!(view.env.len(), 1);
    assert_eq!(b.revisions(&service.key).await, 1);
    // Les 20 dernières lignes du nouveau service sont lisibles, en mémoire seulement (BR-EDIT-020).
    let tail = b
        .agent
        .request("GET", &format!("/operations/services/{operation_id}/tail"))
        .token(&b.dan.token)
        .send()
        .await;
    assert_eq!(tail.status, 200, "{}", tail.text);
    let tail: OperationTail = serde_json::from_value(tail.body).unwrap();
    assert_eq!(tail.lines, [LOG_LINE]);
    // Le journal d'activité dit l'échec et le retour arrière, jamais la ligne ni une valeur.
    let journal = b.journal().await;
    assert_eq!(journal.len(), 1, "{journal:?}");
    assert_ne!(journal[0].0, "ok", "l'échec est consigné comme un échec");
    assert!(
        journal[0]
            .1
            .as_deref()
            .is_some_and(|target| target.contains("retour arrière effectué")),
        "{journal:?}"
    );
    // Sans le droit, ces lignes ne se lisent pas.
    let denied = b
        .agent
        .request("GET", &format!("/operations/services/{operation_id}/tail"))
        .token(&b.lucas.token)
        .send()
        .await;
    assert_eq!(denied.status, 403);
    let plain = b.plain_text_of_the_database().await;
    for secret in [LOG_LINE, SECRET_VALUE, NEW_VALUE] {
        assert!(!plain.contains(secret), "{secret} trouvé dans {plain}");
    }
}

#[tokio::test]
async fn a_creation_refused_by_the_engine_puts_the_old_service_back() {
    let b = bench().await;
    let service = b.deploy("web", 18_084).await;
    b.engine.fail_create(Some(EngineError::PortInUse));
    let operation_id = b
        .accepted(
            &service.key,
            json!({ "base_rev": 1, "env": [{ "name": "AUTRE", "op": "set", "value": NEW_VALUE }] }),
        )
        .await;
    let operation = b.finished(&operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::RolledBack,
        "{operation:?}"
    );
    assert_eq!(operation.cause, Some(OperationCause::PortInUse));
    b.engine.fail_create(None);
    b.resync().await;
    let (container, name, state) = b.published(&service.key).expect("publié");
    assert_eq!(container, service.container);
    assert_eq!((name.as_str(), state), ("web", RunState::Running));
    assert_eq!(b.spec(&service.key).await.rev, 1);
    assert!(b.engine.removed_keeping_volumes().is_empty());
}

#[tokio::test]
async fn when_the_way_back_fails_too_the_old_service_is_kept_and_nothing_restarts_alone() {
    let b = bench().await;
    let service = b.deploy("web", 18_085).await;
    b.engine.exit_once_on_start("web", 0);
    // Rendre son nom à l'ancien est refusé : le retour arrière échoue (BR-EDIT-022).
    b.engine.fail_rename_to("web", Some(EngineError::Refused));
    let operation_id = b
        .accepted(
            &service.key,
            json!({ "base_rev": 1, "env": [{ "name": "AUTRE", "op": "set", "value": NEW_VALUE }] }),
        )
        .await;
    let operation = b.finished(&operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Interrupted,
        "{operation:?}"
    );
    assert_eq!(operation.cause, Some(OperationCause::StepFailed));
    // L'ancien n'est pas supprimé : il reste, de côté, arrêté, avec ses données.
    assert!(b.engine.removed_keeping_volumes().is_empty());
    assert!(b.engine.container_ids().contains(&service.container));
    assert_eq!(b.engine_state(&service.container), RunState::Exited);
    assert_eq!(
        b.engine.renamed().len(),
        1,
        "seul le renommage de mise de côté a eu lieu"
    );
}

#[tokio::test]
async fn a_failed_pull_touches_nothing() {
    let b = bench().await;
    let service = b.deploy("web", 18_086).await;
    let calls_before = b.engine.calls();
    // Cette version n'existe pas dans le registre.
    let operation_id = b
        .accepted(&service.key, json!({ "base_rev": 1, "tag": "9.9" }))
        .await;
    let operation = b.finished(&operation_id).await;
    assert_eq!(operation.status, OperationStatus::Failed, "{operation:?}");
    assert_eq!(
        b.engine.calls(),
        calls_before,
        "pas un arrêt, pas un renommage : le service n'a pas été modifié (BR-EDIT-021)"
    );
    assert!(b.engine.renamed().is_empty());
    assert_eq!(b.engine.created().len(), 1);
    assert_eq!(b.spec(&service.key).await.rev, 1);
    let (container, _, state) = b.published(&service.key).expect("publié");
    assert_eq!((container, state), (service.container, RunState::Running));
}

#[tokio::test]
async fn a_stale_revision_is_refused_before_anything_is_stopped() {
    let b = bench().await;
    let service = b.deploy("web", 18_087).await;
    let calls_before = b.engine.calls();
    let reply = b
        .put(
            &b.ada,
            &service.key,
            json!({ "base_rev": 7, "env": [{ "name": "A", "op": "set", "value": "x" }] }),
        )
        .await;
    assert_eq!(reply.status, 409, "{}", reply.text);
    assert_eq!(reply.body["error"]["code"], "SPEC_CONFLICT");
    assert_eq!(reply.body["error"]["details"]["current_rev"], 1);
    // Jamais le nom d'un compte, jamais une valeur.
    assert!(!reply.text.contains("ada") && !reply.text.contains("x\""));
    assert_eq!(b.engine.calls(), calls_before);
    assert_eq!(b.update_ops(&service.key).await, 0);
}

#[tokio::test]
async fn a_service_changed_outside_hearth_is_refused_without_an_author_unless_it_was_already_seen()
{
    let b = bench().await;
    let service = b.deploy("web", 18_088).await;
    // Quelqu'un règle le processeur à la main : le conteneur n'est plus la description.
    b.engine.update(&service.container, |container| {
        container.cpu_millis = Some(2000)
    });
    b.resync().await;
    assert_eq!(b.spec(&service.key).await.drift, DriftView::ModifiedOutside);
    let refused = b
        .put(
            &b.ada,
            &service.key,
            json!({ "base_rev": 1, "env": [{ "name": "A", "op": "set", "value": "x" }] }),
        )
        .await;
    assert_eq!(refused.status, 409, "{}", refused.text);
    assert_eq!(refused.body["error"]["code"], "SPEC_CONFLICT");
    assert_eq!(refused.body["error"]["details"]["outside"], true);
    assert!(!refused.text.contains("ada"), "{}", refused.text);
    // Le formulaire montrait déjà l'écart : la modification est permise.
    let operation_id = b
        .accepted(
            &service.key,
            json!({ "base_rev": 1, "seen_drift": "modified_outside",
                    "env": [{ "name": "A", "op": "set", "value": "x" }] }),
        )
        .await;
    let operation = b.finished(&operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    b.resync().await;
    assert_eq!(b.spec(&service.key).await.drift, DriftView::None);
}

#[tokio::test]
async fn nothing_to_apply_creates_no_operation_and_no_entry() {
    let b = bench().await;
    let service = b.deploy("web", 18_089).await;
    let reply = b
        .put(
            &b.ada,
            &service.key,
            json!({ "base_rev": 1, "env": [
                { "name": SECRET_NAME, "op": "keep" },
                { "name": SECRET_NAME, "op": "set", "value": SECRET_VALUE }
            ] }),
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let accepted: UpdateAccepted = serde_json::from_value(reply.body).unwrap();
    assert!(!accepted.applied && accepted.operation_id.is_none());
    assert_eq!(b.update_ops(&service.key).await, 0);
    assert!(b.journal().await.is_empty());
    assert_eq!(b.spec(&service.key).await.rev, 1);
}

#[tokio::test]
async fn the_players_address_changes_without_recreating_anything() {
    let b = bench().await;
    let service = b.deploy("web", 18_090).await;
    let operation_id = b
        .accepted(
            &service.key,
            json!({ "base_rev": 1, "public_address": "jeu.example.org:7777" }),
        )
        .await;
    let operation = b.finished(&operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    assert_eq!(names_of(&operation), ["check"]);
    assert_eq!(b.engine.created().len(), 1, "aucun conteneur n'est recréé");
    assert!(b.engine.renamed().is_empty());
    let view = b.spec(&service.key).await;
    assert_eq!(view.rev, 2);
    assert_eq!(view.public_address.as_deref(), Some("jeu.example.org:7777"));
    assert_eq!(b.revisions(&service.key).await, 1);
    let (container, _, state) = b.published(&service.key).expect("publié");
    assert_eq!((container, state), (service.container, RunState::Running));
}

#[tokio::test]
async fn a_service_whose_container_vanished_is_recreated_stopped_and_never_alone() {
    let b = bench().await;
    let service = b.deploy("web", 18_091).await;
    // Le conteneur est supprimé à la main : la description reste, le service n'est plus nulle part ailleurs.
    b.engine.remove(&service.container);
    b.resync().await;
    assert_eq!(
        b.spec(&service.key).await.drift,
        DriftView::ContainerMissing,
        "BR-EDIT-036"
    );
    assert!(b.published(&service.key).is_none());
    let starts_before = b
        .engine
        .calls()
        .iter()
        .filter(|call| matches!(call, Call::Start(_)))
        .count();
    // « Recréer le service » : une modification sans changement.
    let operation_id = b
        .accepted(
            &service.key,
            json!({ "base_rev": 1, "seen_drift": "container_missing" }),
        )
        .await;
    let operation = b.finished(&operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    assert_eq!(names_of(&operation), ["check", "create_new"]);
    let starts_after = b
        .engine
        .calls()
        .iter()
        .filter(|call| matches!(call, Call::Start(_)))
        .count();
    assert_eq!(starts_after, starts_before, "jamais démarré seul");
    b.resync().await;
    let (_, name, state) = b.published(&service.key).expect("publié de nouveau");
    assert_eq!((name.as_str(), state), ("web", RunState::Created));
    let view = b.spec(&service.key).await;
    assert_eq!((view.rev, view.drift), (1, DriftView::None));
    assert_eq!(
        b.revisions(&service.key).await,
        1,
        "aucune révision de plus"
    );
}

#[tokio::test]
async fn the_closed_plan_and_the_validator_refuse_what_a_deployment_refuses() {
    let b = bench().await;
    let service = b.deploy("web", 18_092).await;
    let calls_before = b.engine.calls();
    let privileged = b
        .put(
            &b.ada,
            &service.key,
            json!({ "base_rev": 1, "privileged": true }),
        )
        .await;
    assert_eq!(privileged.status, 422, "{}", privileged.text);
    assert_eq!(privileged.body["error"]["code"], "PLAN_SETTING_REFUSED");
    let bad_port = b
        .put(
            &b.ada,
            &service.key,
            json!({ "base_rev": 1, "ports": [{ "host_port": 0, "container_port": 80 }] }),
        )
        .await;
    assert_eq!(bad_port.status, 422, "{}", bad_port.text);
    assert_eq!(bad_port.body["error"]["code"], "VALIDATION_ERROR");
    let folder = b
        .put(
            &b.ada,
            &service.key,
            json!({ "base_rev": 1, "mounts": [
                { "target": "/data", "place": "host", "host_path": "/srv/x" }
            ] }),
        )
        .await;
    assert_eq!(folder.status, 422, "{}", folder.text);
    assert_eq!(folder.body["error"]["code"], "FOLDER_REFUSED");
    // Une valeur sur plusieurs lignes : refusée, sans la citer.
    let multiline = b
        .put(
            &b.ada,
            &service.key,
            json!({ "base_rev": 1, "env": [{ "name": "A", "op": "set", "value": format!("{NEW_VALUE}\nB=2") }] }),
        )
        .await;
    assert_eq!(multiline.status, 422, "{}", multiline.text);
    assert!(!multiline.text.contains(NEW_VALUE));
    // Un port déjà tenu par un autre service : refusé comme au déploiement.
    let other = b.deploy("autre", 18_093).await;
    let taken = b
        .put(
            &b.ada,
            &service.key,
            json!({ "base_rev": 1, "ports": [{ "host_port": 18_093, "container_port": 80 }] }),
        )
        .await;
    assert_eq!(taken.status, 409, "{}", taken.text);
    assert_eq!(taken.body["error"]["code"], "PORT_IN_USE");
    let _ = other;
    assert!(
        b.engine
            .calls()
            .iter()
            .skip(calls_before.len())
            .all(|call| !matches!(call, Call::Stop(..))),
        "aucun arrêt : tout est refusé avant"
    );
    assert_eq!(b.update_ops(&service.key).await, 0);
}

#[tokio::test]
async fn one_action_at_a_time_per_service() {
    let b = bench().await;
    let service = b.deploy("web", 18_094).await;
    // La récupération d'une version lente tient le service.
    b.engine.publish_image(
        "nginx:1.28",
        PullScript {
            delay: Duration::from_millis(300),
            ..PullScript::two_layers()
        },
    );
    let first = b
        .accepted(&service.key, json!({ "base_rev": 1, "tag": "1.28" }))
        .await;
    let second = b
        .put(
            &b.ada,
            &service.key,
            json!({ "base_rev": 1, "env": [{ "name": "A", "op": "set", "value": "x" }] }),
        )
        .await;
    assert_eq!(second.status, 409, "{}", second.text);
    assert_eq!(second.body["error"]["code"], "SERVICE_BUSY");
    assert_eq!(second.body["error"]["details"]["kind"], "update");
    let operation = b.finished(&first).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// Un redémarrage de l'agent au milieu d'une modification : « Revenir » et « Garder » (BR-EDIT-031)
// ---------------------------------------------------------------------------------------------

/// Une modification coupée par le redémarrage de l'agent, pendant la surveillance du nouveau (10 secondes), l'ancien étant de côté : un second
/// agent démarre sur la même base et le même moteur, l'opération y est « à vérifier ».
async fn interrupted_by_a_restart(b: &Bench, name: &str, port: u16) -> (Deployed, Agent, String) {
    let service = b.deploy(name, port).await;
    let operation_id = b
        .accepted(
            &service.key,
            json!({ "base_rev": 1, "env": [{ "name": "AUTRE", "op": "set", "value": NEW_VALUE }] }),
        )
        .await;
    let started = Instant::now();
    loop {
        let operation = b.operation(&operation_id).await;
        if operation.step.as_deref() == Some("watch") {
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(15), "{operation:?}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let mut parts = parts_supervised(
        b.engine.clone(),
        Arc::new(FakeSupervisor::new()),
        fast_timings(),
    );
    parts.host_ports = Arc::new(FakeHostPorts::new());
    parts.free_space = Arc::new(FakeFreeSpace::new());
    parts.safe_fs = Some(Arc::new(FakeSafeFs::new()));
    let second = https::start_engined(&b.env, parts).await;
    let reply = second
        .request("GET", &format!("/operations/services/{operation_id}"))
        .token(&b.dan.token)
        .send()
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let operation: OperationView = serde_json::from_value(reply.body).expect("OperationView");
    assert_eq!(
        operation.status,
        OperationStatus::NeedsAttention,
        "{operation:?}"
    );
    (service, second, operation_id)
}

async fn decide(b: &Bench, second: &Agent, operation_id: &str, choice: &str) {
    let reply = second
        .confirmed(
            &b.ada,
            "POST",
            &format!("/operations/services/{operation_id}/decision"),
            json!({ "choice": choice }),
        )
        .await
        .send()
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
}

#[tokio::test]
async fn reverting_an_interrupted_update_puts_the_old_service_back_under_its_name() {
    let b = bench().await;
    let (service, second, operation_id) = interrupted_by_a_restart(&b, "web", 18_100).await;
    decide(&b, &second, &operation_id, "revert").await;
    let started = Instant::now();
    loop {
        let reply = second
            .request("GET", &format!("/operations/services/{operation_id}"))
            .token(&b.dan.token)
            .send()
            .await;
        let operation: OperationView = serde_json::from_value(reply.body).expect("OperationView");
        if operation.status == OperationStatus::RolledBack {
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(15), "{operation:?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(b.engine.container_ids().contains(&service.container));
    assert_eq!(b.engine_state(&service.container), RunState::Running);
    assert!(b.engine.removed_keeping_volumes().is_empty());
    assert_eq!(b.revisions(&service.key).await, 1, "candidate abandoned");
    second.shutdown().await;
    b.agent.shutdown().await;
}

#[tokio::test]
async fn keeping_an_interrupted_update_removes_the_parked_container_only() {
    let b = bench().await;
    let (service, second, operation_id) = interrupted_by_a_restart(&b, "web", 18_101).await;
    decide(&b, &second, &operation_id, "keep").await;
    let started = Instant::now();
    while !b
        .engine
        .removed_keeping_volumes()
        .contains(&service.container)
    {
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "the parked container is never removed"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let started = Instant::now();
    while b.revisions(&service.key).await != 1 {
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "revisions kept"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    second.shutdown().await;
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_vanished_container_whose_recreation_fails_leaves_the_description_as_it_was() {
    let b = bench().await;
    let service = b.deploy("web", 18_102).await;
    b.engine.remove(&service.container);
    b.resync().await;
    b.engine.fail_create(Some(EngineError::PortInUse));
    let operation_id = b
        .accepted(
            &service.key,
            json!({ "base_rev": 1, "seen_drift": "container_missing", "tag": "1.27" }),
        )
        .await;
    let operation = b.finished(&operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::RolledBack,
        "{operation:?}"
    );
    assert_eq!(b.spec(&service.key).await.rev, 1);
    assert_eq!(b.revisions(&service.key).await, 1);
}

#[tokio::test]
async fn the_tail_of_an_operation_is_refused_to_an_account_that_does_not_see_it() {
    let b = bench().await;
    let reply = b
        .agent
        .request("GET", "/operations/services/inconnue/tail")
        .token(&b.dan.token)
        .send()
        .await;
    assert_eq!(reply.status, 404, "{}", reply.text);
}
