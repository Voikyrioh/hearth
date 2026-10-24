//! Démarrer, arrêter et redémarrer un service, de bout en bout (HRT-64, ADR-0037, ADR-0045) : vrai agent en
//! HTTPS, vraie base, faux moteur et faux superviseur. Les trois routes répondent `202 { action_id, grace_s }`,
//! l'ordre part à l'arrière-plan, l'issue se lit dans l'opération.
//!
//! Règles : BR-LIFE-001, BR-LIFE-002, BR-LIFE-004 à BR-LIFE-006, BR-LIFE-009 à BR-LIFE-016, BR-LIFE-018 à
//! BR-LIFE-021, BR-LIFE-023 à BR-LIFE-025, BR-SVC-004 à BR-SVC-006.
//! La preuve de clé, le mot de passe et l'élévation de ces actes sont dans la matrice de `admin_reauth.rs` ;
//! ici : ce que font les routes et le cas d'usage, jusqu'au journal d'activité et à la base.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use hearth_agent::application::ports::{Clock, ServiceSupervisor};
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::services::{RunState, ServiceStateKind, UnitActivity, UnitLookup};
use hearth_proto::api::service_ops::{OperationCause, OperationStatus, OperationView};
use hearth_proto::api::services::{ActionAccepted, LastAction, ServicesState};
use serde_json::json;
use support::device::Actor;
use support::engine::{fast_timings, parts_supervised};
use support::fake_engine::{Call, FakeEngine, container};
use support::fake_supervisor::{FakeSupervisor, Order, running_unit, unit_named};
use support::https::{self, Agent, Wire};
use support::{Env, env};

struct Bench {
    env: Env,
    engine: Arc<FakeEngine>,
    supervisor: Arc<FakeSupervisor>,
    agent: Agent,
    /// Administrateur avec le droit de gérer les services.
    ada: Actor,
    /// Administrateur SANS le droit.
    bob: Actor,
    /// Lecture seule sans le droit.
    lucas: Actor,
    /// Lecture seule AVEC le droit.
    dan: Actor,
}

async fn bench() -> Bench {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    // Les exécutions que le faux lance commencent une seconde APRÈS l'ordre (l'horloge de l'agent est figée).
    engine.set_now(Some(env.clock.now() + time::Duration::seconds(1)));
    let supervisor = Arc::new(FakeSupervisor::new());
    let parts = parts_supervised(engine.clone(), supervisor.clone(), fast_timings());
    let agent = https::start_engined(&env, parts).await;
    let ada = agent.actor(&env, "ada", Role::Admin).await;
    let bob = agent.actor(&env, "bob", Role::Admin).await;
    let lucas = agent.actor(&env, "lucas", Role::ReadOnly).await;
    let dan = agent.actor(&env, "dan", Role::ReadOnly).await;
    env.grant_services("ada").await;
    env.grant_services("dan").await;
    Bench {
        env,
        engine,
        supervisor,
        agent,
        ada,
        bob,
        lucas,
        dan,
    }
}

/// Un service que le registre publie : le conteneur est adopté, comme le ferait `POST /services/adopt`.
async fn publish(b: &Bench, name: &str, run_state: RunState) -> (String, String) {
    publish_with(b, name, run_state, |_| {}).await
}

async fn publish_with(
    b: &Bench,
    name: &str,
    run_state: RunState,
    change: impl FnOnce(&mut hearth_agent::domain::services::Observation),
) -> (String, String) {
    let mut observation = container(name, run_state);
    // Les dates du faux sont de l'automne ; l'horloge de l'agent est figée plus tôt : le conteneur tourne
    // depuis une heure, et s'il est arrêté, depuis une demi-heure.
    let now = b.env.clock.now();
    observation.created_at = Some(now - time::Duration::hours(2));
    observation.started_at = observation
        .started_at
        .map(|_| now - time::Duration::hours(1));
    observation.finished_at = observation
        .finished_at
        .map(|_| now - time::Duration::minutes(30));
    change(&mut observation);
    let id = observation.id.clone();
    let key = format!("c_{id}");
    b.engine.put_silently(observation);
    b.agent.services().resync().await.expect("liste");
    b.env.adopt(&key, name, "ada").await;
    b.agent.services().reload_adopted().await;
    (key, id)
}

impl Bench {
    async fn act(&self, who: &Actor, verb: &str, key: &str) -> Wire {
        self.agent
            .confirmed(who, "POST", &format!("/services/{key}/{verb}"), json!({}))
            .await
            .send()
            .await
    }

    async fn accepted(&self, verb: &str, key: &str) -> ActionAccepted {
        let reply = self.act(&self.ada, verb, key).await;
        assert_eq!(reply.status, 202, "{}", reply.text);
        serde_json::from_value(reply.body).expect("ActionAccepted")
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

    /// Attend la fin de l'action et rend son opération.
    async fn finished(&self, id: &str) -> OperationView {
        let started = Instant::now();
        loop {
            let operation = self.operation(id).await;
            if !matches!(
                operation.status,
                OperationStatus::Accepted | OperationStatus::Running
            ) {
                return operation;
            }
            assert!(
                started.elapsed() < Duration::from_secs(15),
                "l'action ne finit pas : {operation:?}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// La dernière action du service, telle qu'un compte qui a le droit la lit dans la liste.
    async fn last_action(&self, key: &str) -> LastAction {
        let reply = self
            .agent
            .request("GET", "/services")
            .token(&self.dan.token)
            .send()
            .await;
        let state: ServicesState = serde_json::from_value(reply.body).expect("ServicesState");
        state
            .services
            .into_iter()
            .find(|item| item.key == key)
            .and_then(|item| item.last_action)
            .expect("une dernière action")
    }

    fn state_of(&self, key: &str) -> ServiceStateKind {
        self.agent
            .services()
            .snapshot()
            .services
            .iter()
            .find(|view| view.key.as_str() == key)
            .map(|view| view.derived.state)
            .expect("service publié")
    }

    async fn intents(&self) -> Vec<(String, String, String)> {
        sqlx::query_as("SELECT service_key, container_id, account FROM service_intents ORDER BY 1")
            .fetch_all(self.env.db.pool())
            .await
            .unwrap()
    }

    /// Les entrées du journal d'activité de cycle de vie : action, résultat, cible, raison, compte.
    async fn journal(
        &self,
    ) -> Vec<(
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    )> {
        self.agent.flush_journal().await;
        sqlx::query_as(
            "SELECT action, outcome, target, reason, account FROM audit_events
             WHERE action IN ('service.start', 'service.stop', 'service.restart') ORDER BY id",
        )
        .fetch_all(self.env.db.pool())
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
// Les droits (BR-LIFE-001, BR-LIFE-016, BR-LIFE-025)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn only_an_account_with_the_right_may_drive_a_service_whatever_its_role() {
    let b = bench().await;
    let (key, id) = publish(&b, "terraria", RunState::Running).await;
    for who in [&b.bob, &b.lucas] {
        for verb in ["start", "stop", "restart"] {
            let refused = b.act(who, verb, &key).await;
            assert_eq!(refused.status, 403, "{verb}");
            assert_eq!(
                refused.body["error"]["details"]["missing"],
                hearth_proto::api::accounts::RIGHT_SERVICES
            );
        }
    }
    // Aucun ordre n'est parti pour les refusés.
    assert!(b.engine.calls().is_empty());
    // Un compte en lecture seule qui a le droit est servi (BR-GRANT-003).
    let reply = b.act(&b.dan, "stop", &key).await;
    assert_eq!(reply.status, 202, "{}", reply.text);
    let accepted: ActionAccepted = serde_json::from_value(reply.body).unwrap();
    b.finished(&accepted.action_id).await;
    assert_eq!(
        b.engine.calls(),
        vec![Call::Stop(id, Duration::from_secs(10))]
    );
    // Les refus de droit sont au journal, avec la cause « droits insuffisants » (BR-LIFE-001).
    let journal = b.journal().await;
    let denied: Vec<_> = journal
        .iter()
        .filter(|row| row.1 == "denied")
        .map(|row| (row.0.as_str(), row.3.as_deref().unwrap_or_default()))
        .collect();
    assert_eq!(denied.len(), 6, "{journal:?}");
    assert!(
        denied
            .iter()
            .all(|(_, reason)| *reason == "droits insuffisants")
    );
}

#[tokio::test]
async fn a_right_withdrawn_while_the_action_runs_does_not_stop_it_but_the_next_request_is_refused()
{
    let b = bench().await;
    let (key, id) = publish_with(&b, "terraria", RunState::Running, |c| {
        c.stop_timeout_s = Some(1);
    })
    .await;
    b.engine.set_stop_delay(&id, Duration::from_millis(300));
    let accepted = b.accepted("stop", &key).await;
    b.env.revoke_services("ada").await;
    let operation = b.finished(&accepted.action_id).await;
    assert_eq!(operation.status, OperationStatus::Succeeded);
    let refused = b.act(&b.ada, "start", &key).await;
    assert_eq!(refused.status, 403);
}

// ---------------------------------------------------------------------------------------------
// Arrêter (BR-LIFE-009, BR-LIFE-010)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn stopping_answers_202_with_the_grace_of_the_service_and_writes_the_wanted_stop_first() {
    let b = bench().await;
    let (key, id) = publish(&b, "terraria", RunState::Running).await;
    let accepted = b.accepted("stop", &key).await;
    assert_eq!(accepted.grace_s, 10, "le délai du moteur par défaut");
    assert!(!accepted.action_id.is_empty());
    let operation = b.finished(&accepted.action_id).await;
    assert_eq!(operation.status, OperationStatus::Succeeded);
    assert_eq!(operation.cause, None);
    let last = b.last_action(&key).await;
    assert_eq!(
        (
            last.kind.as_str(),
            last.by.as_str(),
            last.status.as_str(),
            last.forced
        ),
        ("stop", "ada", "succeeded", false)
    );
    assert_eq!(last.id, accepted.action_id);
    // L'ordre part avec le délai du service, une fois.
    assert_eq!(
        b.engine.calls(),
        vec![Call::Stop(id.clone(), Duration::from_secs(10))]
    );
    // L'arrêt voulu est en base (BR-LIFE-010) et le service est « Arrêté ».
    assert_eq!(b.intents().await, vec![(key.clone(), id, "ada".to_owned())]);
    assert_eq!(b.state_of(&key), ServiceStateKind::Stopped);
    // L'issue est relisible : l'opération porte le genre, le compte et son étape.
    assert_eq!(operation.key, key);
    // Le journal d'activité garde le compte, le service, le geste et le résultat (BR-LIFE-018).
    let journal = b.journal().await;
    assert_eq!(journal.len(), 1, "{journal:?}");
    assert_eq!(journal[0].0, "service.stop");
    assert_eq!(journal[0].1, "ok");
    assert_eq!(journal[0].2.as_deref(), Some("service terraria"));
    assert_eq!(journal[0].4.as_deref(), Some("ada"));
}

#[tokio::test]
async fn a_non_zero_exit_after_a_stop_asked_from_hearth_is_a_stop_not_an_error() {
    let b = bench().await;
    let (key, id) = publish_with(&b, "terraria", RunState::Running, |c| {
        c.stop_timeout_s = Some(1);
    })
    .await;
    // Le programme ignore l'ordre : le moteur le tue à l'échéance, code 137.
    b.engine.make_stubborn(&id);
    let accepted = b.accepted("stop", &key).await;
    assert_eq!(accepted.grace_s, 1);
    let operation = b.finished(&accepted.action_id).await;
    // Réussi, mais forcé, et dit (BR-LIFE-009).
    assert_eq!(operation.status, OperationStatus::Succeeded);
    assert_eq!(operation.cause, Some(OperationCause::ForcedStop));
    let last = b.last_action(&key).await;
    assert!(last.forced);
    assert_eq!(last.cause.as_deref(), Some("forced_stop"));
    // 137 sans arrêt demandé serait « En erreur » ; l'arrêt a été demandé : « Arrêté » (BR-SVC-005).
    assert_eq!(b.state_of(&key), ServiceStateKind::Stopped);
    let journal = b.journal().await;
    assert_eq!(journal[0].1, "ok");
    assert_eq!(
        journal[0].2.as_deref(),
        Some("service terraria (arrêt forcé)")
    );
}

#[tokio::test]
async fn the_wanted_stop_survives_an_agent_restart_and_is_forgotten_when_the_service_runs_again() {
    let b = bench().await;
    let (key, id) = publish_with(&b, "terraria", RunState::Running, |c| {
        c.stop_timeout_s = Some(1);
    })
    .await;
    b.engine.make_stubborn(&id);
    let accepted = b.accepted("stop", &key).await;
    b.finished(&accepted.action_id).await;
    assert_eq!(b.intents().await.len(), 1);
    // Démarrer le service : l'arrêt voulu s'efface, en base et en mémoire.
    let accepted = b.accepted("start", &key).await;
    let operation = b.finished(&accepted.action_id).await;
    assert_eq!(operation.status, OperationStatus::Succeeded);
    assert!(b.intents().await.is_empty());
    assert_eq!(b.state_of(&key), ServiceStateKind::Running);
}

// ---------------------------------------------------------------------------------------------
// Démarrer et redémarrer (BR-LIFE-004, BR-LIFE-006, BR-LIFE-012)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn starting_a_stopped_service_shows_it_running_only_when_the_engine_says_so() {
    let b = bench().await;
    let (key, id) = publish(&b, "terraria", RunState::Exited).await;
    assert_eq!(b.state_of(&key), ServiceStateKind::Stopped);
    let accepted = b.accepted("start", &key).await;
    let operation = b.finished(&accepted.action_id).await;
    assert_eq!(operation.status, OperationStatus::Succeeded);
    assert_eq!(b.engine.calls(), vec![Call::Start(id)]);
    assert_eq!(b.state_of(&key), ServiceStateKind::Running);
    let journal = b.journal().await;
    assert_eq!(
        (journal[0].0.as_str(), journal[0].1.as_str()),
        ("service.start", "ok")
    );
}

#[tokio::test]
async fn a_start_that_meets_a_taken_port_leaves_the_service_stopped_with_the_cause() {
    let b = bench().await;
    let (key, id) = publish(&b, "terraria", RunState::Exited).await;
    b.engine.fail_start(
        &id,
        Some(hearth_agent::application::ports::EngineError::PortInUse),
    );
    let accepted = b.accepted("start", &key).await;
    let operation = b.finished(&accepted.action_id).await;
    assert_eq!(operation.status, OperationStatus::Failed);
    assert_eq!(operation.cause, Some(OperationCause::PortInUse));
    assert_eq!(b.state_of(&key), ServiceStateKind::Stopped);
    let journal = b.journal().await;
    assert_eq!(journal[0].1, "failed");
    assert_eq!(journal[0].3.as_deref(), Some("port occupé"));
}

#[tokio::test]
async fn restarting_is_one_order_and_the_restarts_hearth_asked_never_make_a_boucle() {
    let b = bench().await;
    let (key, id) = publish(&b, "terraria", RunState::Running).await;
    for round in 0..4 {
        b.env.clock.advance(time::Duration::seconds(1));
        b.engine
            .set_now(Some(b.env.clock.now() + time::Duration::seconds(1)));
        let accepted = b.accepted("restart", &key).await;
        let operation = b.finished(&accepted.action_id).await;
        assert_eq!(operation.status, OperationStatus::Succeeded, "{round}");
    }
    assert_eq!(
        b.engine.calls(),
        vec![Call::Restart(id, Duration::from_secs(10)); 4]
    );
    // Quatre exécutions en quatre secondes : si elles comptaient, ce serait « Redémarre en boucle ».
    assert_eq!(b.state_of(&key), ServiceStateKind::Running);
    let view = b
        .agent
        .services()
        .snapshot()
        .services
        .into_iter()
        .find(|view| view.key.as_str() == key)
        .unwrap();
    assert_eq!(view.restarts_10m, 0);
}

#[tokio::test]
async fn a_restart_whose_start_meets_a_taken_port_leaves_the_service_stopped_not_in_error() {
    let b = bench().await;
    let (key, id) = publish(&b, "terraria", RunState::Running).await;
    b.engine.fail_start(
        &id,
        Some(hearth_agent::application::ports::EngineError::PortInUse),
    );
    let accepted = b.accepted("restart", &key).await;
    let operation = b.finished(&accepted.action_id).await;
    assert_eq!(operation.status, OperationStatus::Failed);
    assert_eq!(operation.cause, Some(OperationCause::PortInUse));
    // Arrêté, mais pas reparti : « Arrêté » (BR-LIFE-012), la sortie est celle de l'arrêt de Hearth.
    assert_eq!(b.state_of(&key), ServiceStateKind::Stopped);
}

#[tokio::test]
async fn a_restart_is_forced_by_the_time_its_stop_took_never_by_its_start() {
    // BR-LIFE-009, BR-LIFE-012 : le démarrage remet le code de sortie à zéro ; c'est la fin de l'exécution
    // arrêtée (l'heure du moteur) moins l'heure de l'ordre qui dit si l'arrêt a pris tout son délai.
    let b = bench().await;
    let (key, id) = publish_with(&b, "terraria", RunState::Running, |c| {
        c.stop_timeout_s = Some(1);
    })
    .await;
    b.engine.make_stubborn(&id);
    // L'exécution arrêtée finit une seconde après l'ordre : tout le délai.
    let accepted = b.accepted("restart", &key).await;
    let operation = b.finished(&accepted.action_id).await;
    assert_eq!(operation.status, OperationStatus::Succeeded);
    assert_eq!(operation.cause, Some(OperationCause::ForcedStop));
    assert!(b.last_action(&key).await.forced);

    // Un service au délai plus long, arrêté en une seconde : pas forcé, quoi que dure le démarrage.
    let (slow, _) = publish_with(&b, "valheim", RunState::Running, |c| {
        c.stop_timeout_s = Some(30);
    })
    .await;
    let accepted = b.accepted("restart", &slow).await;
    let operation = b.finished(&accepted.action_id).await;
    assert_eq!(operation.status, OperationStatus::Succeeded);
    assert_eq!(operation.cause, None);
    assert!(!b.last_action(&slow).await.forced);
}

#[tokio::test]
async fn a_unit_is_judged_forced_by_the_stop_the_system_measured() {
    // BR-LIFE-009 : pour une unité, la durée de l'arrêt que le système a datée, démarrage exclu.
    let b = bench().await;
    let unit = unit_named("docker-7dtd.service");
    let observation = container("7dtd", RunState::Running);
    b.supervisor.set_owner(&observation.id, &unit);
    b.supervisor.set_status(
        &unit,
        hearth_agent::domain::services::UnitStatus {
            timeout_stop_s: Some(1),
            ..running_unit()
        },
    );
    b.engine.put_silently(observation);
    b.agent.services().resync().await.expect("liste");
    let key = "u_docker-7dtd.service";
    b.env.adopt(key, "7dtd", "ada").await;
    b.agent.services().reload_adopted().await;

    // L'arrêt de l'unité a pris tout son délai : forcé.
    b.supervisor.set_stop_delay(&unit, Duration::from_secs(1));
    let accepted = b.accepted("restart", key).await;
    assert_eq!(accepted.grace_s, 1);
    let operation = b.finished(&accepted.action_id).await;
    assert_eq!(operation.status, OperationStatus::Succeeded);
    assert_eq!(operation.cause, Some(OperationCause::ForcedStop));

    // Un arrêt rapide : pas forcé.
    b.supervisor
        .set_stop_delay(&unit, Duration::from_millis(100));
    let accepted = b.accepted("restart", key).await;
    let operation = b.finished(&accepted.action_id).await;
    assert_eq!(operation.status, OperationStatus::Succeeded);
    assert_eq!(operation.cause, None);
}

// ---------------------------------------------------------------------------------------------
// Les refus synchrones (BR-LIFE-013 à BR-LIFE-015, BR-LIFE-019, BR-LIFE-023, BR-LIFE-024)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_request_the_state_does_not_allow_is_refused_at_once_with_a_typed_code_and_nothing_is_ordered()
 {
    let b = bench().await;
    let (running, _) = publish(&b, "web", RunState::Running).await;
    let (stopped, _) = publish(&b, "db", RunState::Exited).await;
    let (paused, _) = publish(&b, "cache", RunState::Paused).await;
    for (verb, key, expected, state) in [
        ("start", &running, "SERVICE_ALREADY_IN_STATE", "running"),
        ("stop", &stopped, "SERVICE_ALREADY_IN_STATE", "stopped"),
        // Redémarrer un service arrêté n'est jamais converti en démarrage (BR-LIFE-014).
        ("restart", &stopped, "SERVICE_NOT_DRIVABLE", "stopped"),
        ("start", &paused, "SERVICE_NOT_DRIVABLE", "paused"),
        ("stop", &paused, "SERVICE_NOT_DRIVABLE", "paused"),
        ("restart", &paused, "SERVICE_NOT_DRIVABLE", "paused"),
    ] {
        let refused = b.act(&b.ada, verb, key).await;
        assert_eq!(refused.status, 409, "{verb} {state}");
        assert_eq!(code(&refused), expected, "{verb} {state}");
        assert_eq!(refused.body["error"]["details"]["state"], state);
    }
    assert!(b.engine.calls().is_empty(), "aucun ordre n'est parti");
    // Chaque refus est au journal, avec sa cause typée (BR-LIFE-014, BR-LIFE-018).
    let journal = b.journal().await;
    assert_eq!(journal.len(), 6, "{journal:?}");
    // Les refus identiques qui se suivent sont regroupés par le journal (« n autres fois »).
    let reasons: Vec<_> = journal
        .iter()
        .map(|row| {
            row.3
                .as_deref()
                .unwrap_or_default()
                .split(" (")
                .next()
                .unwrap_or_default()
        })
        .collect();
    assert_eq!(
        reasons,
        [
            "déjà dans l'état demandé",
            "déjà dans l'état demandé",
            "état du service incompatible avec l'action",
            "état du service incompatible avec l'action",
            "état du service incompatible avec l'action",
            "état du service incompatible avec l'action",
        ]
    );
}

#[tokio::test]
async fn an_unknown_a_malformed_and_a_hidden_service_are_all_answered_the_same_way() {
    let b = bench().await;
    // Un conteneur NON adopté : ni créé par Hearth ni adopté, donc pas un service (BR-LIFE-024).
    let hidden = container("cache", RunState::Running);
    let hidden_key = format!("c_{}", hidden.id);
    b.engine.put_silently(hidden);
    b.agent.services().resync().await.expect("liste");
    let unknown = b.act(&b.ada, "stop", "c_0123456789abcdef").await;
    assert_eq!(unknown.status, 404);
    assert_eq!(code(&unknown), "SERVICE_NOT_FOUND");
    for key in [hidden_key.as_str(), "pas-une-cle", "c_", "x_abc"] {
        for verb in ["start", "stop", "restart"] {
            let refused = b.act(&b.ada, verb, key).await;
            assert_eq!(refused.status, 404, "{verb} {key}");
            assert_eq!(refused.body, unknown.body, "{verb} {key}");
        }
    }
    assert!(b.engine.calls().is_empty());
}

#[tokio::test]
async fn an_engine_that_is_not_ready_refuses_before_anything_else_and_says_which_state() {
    let b = bench().await;
    let (key, _) = publish(&b, "terraria", RunState::Running).await;
    b.engine.set_outcome(
        hearth_agent::domain::services::ProbeOutcome::PermissionDenied,
        true,
    );
    b.agent.services().recheck().await;
    // L'anti-clignotement retient le changement d'état quelques centaines de millisecondes.
    let started = Instant::now();
    let refused = loop {
        let reply = b.act(&b.ada, "stop", &key).await;
        if reply.status == 503 {
            break reply;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{}",
            reply.text
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(code(&refused), "ENGINE_UNAVAILABLE");
    assert_eq!(refused.body["error"]["details"]["engine"], "access_denied");
    assert!(b.engine.calls().is_empty());
    let journal = b.journal().await;
    assert!(
        journal
            .iter()
            .any(|row| row.3.as_deref() == Some("moteur inaccessible")),
        "{journal:?}"
    );
}

#[tokio::test]
async fn a_second_request_on_a_busy_service_is_refused_with_who_holds_it_and_other_services_are_free()
 {
    let b = bench().await;
    let (key, id) = publish_with(&b, "terraria", RunState::Running, |c| {
        c.stop_timeout_s = Some(1);
    })
    .await;
    let (other, _) = publish(&b, "valheim", RunState::Running).await;
    b.engine.set_stop_delay(&id, Duration::from_millis(500));
    let first = b.accepted("stop", &key).await;
    let busy = b.act(&b.dan, "restart", &key).await;
    assert_eq!(busy.status, 409, "{}", busy.text);
    assert_eq!(code(&busy), "SERVICE_BUSY");
    assert_eq!(busy.body["error"]["details"]["by"], "ada");
    assert_eq!(busy.body["error"]["details"]["kind"], "stop");
    // Un autre service est indépendant (BR-LIFE-013), et il n'y a pas d'action groupée (BR-LIFE-023).
    let free = b.act(&b.dan, "stop", &other).await;
    assert_eq!(free.status, 202, "{}", free.text);
    let operation = b.finished(&first.action_id).await;
    assert_eq!(operation.status, OperationStatus::Succeeded);
    // Le verrou est rendu : la demande suivante n'est plus occupée (mais refusée par l'état).
    let after = b.act(&b.ada, "stop", &key).await;
    assert_eq!(code(&after), "SERVICE_ALREADY_IN_STATE");
}

// ---------------------------------------------------------------------------------------------
// Un ordre qui n'aboutit pas (BR-LIFE-019, BR-LIFE-021)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn an_order_the_engine_refuses_is_a_failure_and_one_it_never_answers_is_an_unknown_result() {
    let b = bench().await;
    let (key, id) = publish_with(&b, "terraria", RunState::Running, |c| {
        c.stop_timeout_s = Some(0);
    })
    .await;
    // Refusé net : échec, le service n'a pas été modifié, l'arrêt voulu n'a pas lieu d'être.
    b.engine.fail_stop(
        &id,
        Some(hearth_agent::application::ports::EngineError::Refused),
    );
    let accepted = b.accepted("stop", &key).await;
    let operation = b.finished(&accepted.action_id).await;
    assert_eq!(operation.status, OperationStatus::Failed);
    assert_eq!(operation.cause, Some(OperationCause::EngineRefused));
    assert!(b.intents().await.is_empty());
    assert_eq!(b.state_of(&key), ServiceStateKind::Running);
    // Jamais de réponse : « interrompu », cause « délai dépassé », jamais rejoué (BR-LIFE-021).
    b.engine.fail_stop(&id, None);
    b.engine.make_hang_on_stop(&id);
    let accepted = b.accepted("stop", &key).await;
    let operation = b.finished(&accepted.action_id).await;
    assert_eq!(operation.status, OperationStatus::Interrupted);
    assert_eq!(operation.cause, Some(OperationCause::Timeout));
    let calls = b.engine.calls();
    assert_eq!(calls.len(), 2, "l'ordre n'est jamais rejoué : {calls:?}");
    let journal = b.journal().await;
    let reasons: Vec<_> = journal
        .iter()
        .map(|row| row.3.as_deref().unwrap_or_default())
        .collect();
    assert_eq!(reasons, ["refusé par le moteur", "délai dépassé"]);
}

#[tokio::test]
async fn an_engine_that_becomes_unreachable_during_the_action_leaves_the_result_unknown_not_failed()
{
    let b = bench().await;
    let (key, id) = publish_with(&b, "terraria", RunState::Running, |c| {
        c.stop_timeout_s = Some(0);
    })
    .await;
    b.engine.fail_stop(
        &id,
        Some(hearth_agent::application::ports::EngineError::Unreachable),
    );
    let accepted = b.accepted("stop", &key).await;
    let operation = b.finished(&accepted.action_id).await;
    assert_eq!(operation.status, OperationStatus::Interrupted);
    assert_eq!(operation.cause, Some(OperationCause::EngineUnavailable));
}

// ---------------------------------------------------------------------------------------------
// Après la fin de l'action : ce que Hearth surveille sans bloquer (BR-LIFE-005, BR-LIFE-011)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_service_that_stops_right_after_its_start_is_noted_on_the_finished_action() {
    let b = bench().await;
    let (key, id) = publish(&b, "terraria", RunState::Exited).await;
    let accepted = b.accepted("start", &key).await;
    let operation = b.finished(&accepted.action_id).await;
    assert_eq!(operation.status, OperationStatus::Succeeded);
    assert_eq!(operation.cause, None);
    // Le programme s'arrête dans les secondes qui suivent (ici : la surveillance dure 300 ms).
    b.engine.update(&id, |c| {
        c.run_state = RunState::Exited;
        c.exit_code = Some(1);
    });
    let started = Instant::now();
    loop {
        let operation = b.operation(&accepted.action_id).await;
        if operation.cause == Some(OperationCause::StoppedRightAfterStart) {
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(5), "{operation:?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // FIX:01M5NYA5W781YW5NFQ7W1JGR98 Le GET ci-dessus lit le journal des opérations : la cause et la ligne du
    // journal d'activité sont écrites dans la même transaction, la ligne est donc là dès que la cause se lit.
    let journal = b.journal().await;
    assert!(
        journal
            .iter()
            .any(|row| row.1 == "failed"
                && row.3.as_deref() == Some("arrêté juste après le démarrage")),
        "{journal:?}"
    );
}

/// L'issue est lisible (GET) avec son entrée du journal, et le verrou est déjà rendu : l'action suivante du
/// même service n'est jamais refusée (409), sans attente (HRT-101).
#[tokio::test]
async fn the_next_action_is_never_refused_once_the_previous_outcome_can_be_read() {
    let b = bench().await;
    let (key, _id) = publish(&b, "terraria", RunState::Running).await;
    for round in 0..20 {
        let stop = b.accepted("stop", &key).await;
        let operation = b.finished(&stop.action_id).await;
        assert_eq!(operation.status, OperationStatus::Succeeded, "{round}");
        let journal = b.journal().await;
        assert_eq!(journal.len(), 2 * round + 1, "{round} {journal:?}");
        let start = b.accepted("start", &key).await;
        let operation = b.finished(&start.action_id).await;
        assert_eq!(operation.status, OperationStatus::Succeeded, "{round}");
        let journal = b.journal().await;
        assert_eq!(journal.len(), 2 * round + 2, "{round} {journal:?}");
    }
}

/// Si l'entrée du journal d'activité ne peut pas s'écrire, l'action est close sans elle : elle ne reste pas
/// `running` avec son verrou alors que le geste a eu lieu (HRT-101).
#[tokio::test]
async fn an_action_whose_journal_entry_cannot_be_written_is_closed_without_it_and_frees_the_service()
 {
    let b = bench().await;
    let (key, _id) = publish(&b, "terraria", RunState::Running).await;
    sqlx::query(
        "CREATE TRIGGER refuse_audit BEFORE INSERT ON audit_events
         BEGIN SELECT RAISE(ABORT, 'journal indisponible'); END",
    )
    .execute(b.env.db.pool())
    .await
    .unwrap();
    let stop = b.accepted("stop", &key).await;
    let operation = b.finished(&stop.action_id).await;
    assert_eq!(operation.status, OperationStatus::Succeeded);
    assert!(b.journal().await.is_empty());
    // Le verrou est rendu : l'action suivante est acceptée.
    let start = b.accepted("start", &key).await;
    let operation = b.finished(&start.action_id).await;
    assert_eq!(operation.status, OperationStatus::Succeeded);
}

#[tokio::test]
async fn a_service_that_comes_back_alone_after_a_stop_is_told_and_one_that_stays_stopped_is_not() {
    let b = bench().await;
    let (key, id) = publish(&b, "terraria", RunState::Running).await;
    let (quiet, _) = publish(&b, "valheim", RunState::Running).await;
    // Une règle du serveur relance terraria 100 ms après chaque arrêt.
    b.engine
        .relaunch_after_stop(&id, Duration::from_millis(100));
    let accepted = b.accepted("stop", &key).await;
    let calm = b.accepted("stop", &quiet).await;
    b.finished(&accepted.action_id).await;
    b.finished(&calm.action_id).await;
    let started = Instant::now();
    loop {
        let operation = b.operation(&accepted.action_id).await;
        if operation.cause == Some(OperationCause::RestartedByHost) {
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(5), "{operation:?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // L'autre est resté arrêté pendant toute la surveillance : aucune cause.
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(b.operation(&calm.action_id).await.cause, None);
}

// ---------------------------------------------------------------------------------------------
// Un service géré par le système du serveur (BR-LIFE-002, BR-LIFE-009, BR-LIFE-024)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_service_run_by_a_unit_is_driven_by_the_supervisor_and_never_by_the_engine() {
    let b = bench().await;
    let unit = unit_named("docker-7dtd.service");
    let mut observation = container("7dtd", RunState::Running);
    let now = b.env.clock.now();
    observation.created_at = Some(now - time::Duration::hours(2));
    observation.started_at = Some(now - time::Duration::hours(1));
    let id = observation.id.clone();
    b.supervisor.set_owner(&id, &unit);
    b.supervisor.set_status(&unit, running_unit());
    b.engine.put_silently(observation);
    b.agent.services().resync().await.expect("liste");
    let key = "u_docker-7dtd.service";
    b.env.adopt(key, "7dtd", "ada").await;
    b.agent.services().reload_adopted().await;
    assert_eq!(b.state_of(key), ServiceStateKind::Running);

    // Le délai d'arrêt est celui de l'unité : 120 secondes (BR-LIFE-009).
    let accepted = b.accepted("stop", key).await;
    assert_eq!(accepted.grace_s, 120);
    let operation = b.finished(&accepted.action_id).await;
    assert_eq!(operation.status, OperationStatus::Succeeded);
    assert_eq!(
        b.supervisor.orders(),
        // Le délai de l'unité plus la marge (1 s dans ces tests, 30 s en production).
        vec![Order::Stop(unit.clone(), Duration::from_secs(121))],
        "attente = délai + marge"
    );
    assert!(b.engine.calls().is_empty(), "le moteur n'est jamais appelé");
    // L'arrêt voulu d'un service supervisé n'a pas d'identifiant de conteneur.
    assert_eq!(
        b.intents().await,
        vec![(key.to_owned(), String::new(), "ada".to_owned())]
    );

    // Le système a arrêté l'unité et supprimé le conteneur : le service est « Arrêté » d'après l'unité, jamais
    // « En erreur », et il reste listé.
    b.engine.remove(&id);
    let started = Instant::now();
    loop {
        b.agent.services().resync().await.expect("liste");
        if b.state_of(key) == ServiceStateKind::Stopped {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{:?}",
            b.state_of(key)
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        b.supervisor.unit(&unit).await,
        UnitLookup::Loaded(hearth_agent::domain::services::UnitStatus {
            activity: UnitActivity::Inactive,
            ..running_unit()
        })
    );
}

#[tokio::test]
async fn a_supervisor_that_refuses_is_a_failure_and_never_falls_back_to_the_engine() {
    let b = bench().await;
    let unit = unit_named("docker-7dtd.service");
    let observation = container("7dtd", RunState::Running);
    b.supervisor.set_owner(&observation.id, &unit);
    b.supervisor.set_status(&unit, running_unit());
    b.engine.put_silently(observation);
    b.agent.services().resync().await.expect("liste");
    let key = "u_docker-7dtd.service";
    b.env.adopt(key, "7dtd", "ada").await;
    b.agent.services().reload_adopted().await;
    b.supervisor.fail_orders(
        &unit,
        Some(hearth_agent::application::ports::SupervisorError::Refused),
    );
    let accepted = b.accepted("restart", key).await;
    let operation = b.finished(&accepted.action_id).await;
    assert_eq!(operation.status, OperationStatus::Failed);
    assert_eq!(operation.cause, Some(OperationCause::EngineRefused));
    assert!(b.engine.calls().is_empty());
}

// ---------------------------------------------------------------------------------------------
// Une coupure de l'agent (BR-LIFE-020)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn an_action_cut_by_an_agent_restart_is_interrupted_frees_the_service_and_is_never_replayed()
{
    use hearth_agent::application::service_ops::InterruptedAction;
    use hearth_agent::domain::ops::OpKind;
    let b = bench().await;
    let (key, id) = publish(&b, "terraria", RunState::Running).await;
    // Le moteur ne répond pas à l'ordre d'arrêt : l'agent s'arrête en pleine action.
    b.engine.make_hang_on_stop(&id);
    let accepted = b.accepted("stop", &key).await;
    let started = Instant::now();
    while b.engine.calls().is_empty() {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "l'ordre ne part pas"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(b.engine.calls().len(), 1, "l'ordre est parti");
    // Le redémarrage : un autre agent sur la même base classe l'opération restée en plein geste.
    let restarted = hearth_agent::app::service_foundations(
        b.env.dir.path(),
        &b.env.db,
        b.env.clock.clone(),
        Arc::new(hearth_agent::infrastructure::ids::UlidGen),
    )
    .await
    .expect("démarrage");
    assert_eq!(
        restarted.interrupted_actions,
        vec![InterruptedAction {
            account: "ada".to_owned(),
            service_name: "terraria".to_owned(),
            kind: OpKind::Stop,
        }]
    );
    let operation = b.operation(&accepted.action_id).await;
    assert_eq!(operation.status, OperationStatus::Interrupted);
    assert_eq!(operation.cause, Some(OperationCause::Interrupted));
    // Le verrou n'existe plus : le service se pilote de nouveau, rien n'a été rejoué.
    let followup = restarted
        .ops
        .accept(hearth_agent::application::service_ops::AcceptRequest {
            op_key: "suite".to_owned(),
            service: hearth_agent::domain::services::ServiceKey::parse(&key).unwrap(),
            service_name: "terraria".to_owned(),
            kind: OpKind::Start,
            account: "ada".to_owned(),
            plan: hearth_agent::domain::ops::Gesture::Start.plan().unwrap(),
            forced: false,
            reserve_name: None,
            heavy_io: false,
        })
        .await;
    assert!(followup.is_ok(), "{followup:?}");
    assert_eq!(
        b.engine.calls().len(),
        1,
        "l'ordre d'arrêt n'est jamais rejoué"
    );
    // Et l'interruption est au journal d'activité, avec sa cause typée (BR-LIFE-018).
    hearth_agent::application::service_lifecycle::record_interrupted(
        b.env.audit_sink.as_ref(),
        &restarted.interrupted_actions,
    )
    .await;
    let journal = b.journal().await;
    let last = journal.last().expect("une entrée");
    assert_eq!(
        (
            last.0.as_str(),
            last.1.as_str(),
            last.3.as_deref(),
            last.4.as_deref()
        ),
        (
            "service.stop",
            "failed",
            Some("interrompu par un redémarrage de l'agent"),
            Some("ada")
        )
    );
}
