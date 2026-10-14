//! Les opérations de service branchées sur des routes et sur le flux, de bout en bout (HRT-144, ADR-0045) :
//! vrai agent en HTTPS, vraie base, vrai WebSocket, faux moteur. Aucun geste de service n'existe encore :
//! le test ACCEPTE une opération dans le journal comme le ferait le lot d'un geste (HRT-64, HRT-68), puis
//! l'éprouve par les routes `GET`, `DELETE` et `POST …/decision` et par le sujet `services`.
//!
//! Règles : BR-LIFE-013, BR-LIFE-020, BR-EDIT-031, BR-JEU-006, BR-OPS-001 à BR-OPS-008.
//! Le contrôle de la preuve de clé, du mot de passe et de l'élévation de tous les actes est dans
//! `admin_reauth.rs` ; ici : ce que font l'annulation et la décision, qui voit quoi, la trace, les bornes.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use hearth_agent::application::ports::Progress;
use hearth_agent::application::service_ops::{AcceptRequest, ServiceOps};
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::ops::{
    Choice, CreatedObject, ObjectKind, ObjectRoot, OpKind, OpPlan, OpStatus, PlannedStep, StepName,
    UndoInfo,
};
use hearth_agent::domain::services::{RunState, ServiceKey};
use hearth_agent::infrastructure::logging::{self, LogFormat};
use hearth_proto::api::service_ops::{OperationStatus, OperationView};
use hearth_proto::stream::{ServicesMessage, Topic};
use serde_json::{Value, json};
use support::device::Actor;
use support::engine::{fast_timings, parts_supervised};
use support::fake_engine::{FakeEngine, container};
use support::fake_supervisor::FakeSupervisor;
use support::https::{self, Agent, Wire};
use support::ws;
use support::{Env, PASSWORD, env};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::writer::BoxMakeWriter;

/// Un service créé par Hearth (la clé `h_` d'un ULID) : visible de tous les comptes.
const HEARTH_KEY: &str = "h_01J9ZY0G3Q8M2K6W4T7V5N1B9D";
const OTHER_HEARTH_KEY: &str = "h_01J9ZY0G3Q8M2K6W4T7V5N1B9E";

struct Bench {
    env: Env,
    engine: Arc<FakeEngine>,
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
    let supervisor = Arc::new(FakeSupervisor::new());
    let parts = parts_supervised(engine.clone(), supervisor, fast_timings());
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
        agent,
        ada,
        bob,
        lucas,
        dan,
    }
}

fn name(text: &str) -> StepName {
    StepName::parse(text).unwrap()
}

/// Un déploiement : la vérification ne détruit rien, trois gestes détruisent, `create_new` est le point de
/// non-retour.
fn plan() -> OpPlan {
    let step = |text: &str, destructive| PlannedStep {
        name: name(text),
        destructive,
    };
    OpPlan::new(
        vec![
            step("check", false),
            step("stop_old", true),
            step("create_new", true),
            step("start_new", true),
        ],
        Some(name("create_new")),
    )
    .unwrap()
}

async fn accept_on(ops: &ServiceOps, key: &str, service_name: &str, account: &str) -> String {
    ops.accept(AcceptRequest {
        op_key: format!("k-{key}"),
        service: ServiceKey::parse(key).unwrap(),
        service_name: service_name.to_owned(),
        kind: OpKind::Deploy,
        account: account.to_owned(),
        plan: plan(),
        forced: false,
        reserve_name: None,
        heavy_io: false,
    })
    .await
    .unwrap()
}

impl Bench {
    async fn running(&self) -> String {
        let ops = self.agent.ops();
        let id = accept_on(&ops, HEARTH_KEY, "terraria", "ada").await;
        ops.start(&id).await.unwrap();
        id
    }

    async fn read(&self, who: &Actor, id: &str) -> Wire {
        self.agent
            .request("GET", &format!("/operations/services/{id}"))
            .token(&who.token)
            .send()
            .await
    }

    async fn cancel(&self, who: &Actor, id: &str) -> Wire {
        self.agent
            .confirmed(
                who,
                "DELETE",
                &format!("/operations/services/{id}"),
                json!({}),
            )
            .await
            .send()
            .await
    }

    async fn decide(&self, who: &Actor, id: &str, choice: &str) -> Wire {
        self.agent
            .confirmed(
                who,
                "POST",
                &format!("/operations/services/{id}/decision"),
                json!({ "choice": choice }),
            )
            .await
            .send()
            .await
    }

    /// Les entrées du journal d'activité qui touchent les services ou les opérations.
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
             WHERE action LIKE 'service.%' OR action LIKE 'operation.%' ORDER BY id",
        )
        .fetch_all(self.env.db.pool())
        .await
        .unwrap()
    }

    async fn subscribed(&self, who: &Actor) -> ws::WsClient {
        let mut client = ws::open(&self.agent).await;
        client.auth(&who.token).await;
        client.subscribe(&[Topic::Services]).await;
        let ServicesMessage::ServicesSnapshot(_) = client.next_services().await else {
            panic!("un instantané des services est attendu en premier");
        };
        client
    }
}

fn view(wire: &Wire) -> OperationView {
    assert_eq!(wire.status, 200, "{}", wire.text);
    serde_json::from_value(wire.body.clone()).expect("OperationView")
}

fn error_code(wire: &Wire) -> String {
    wire.body["error"]["code"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

// ---------------------------------------------------------------------------------------------
// Lire (BR-LIFE-020) et qui voit quoi
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn any_account_reads_an_operation_of_a_hearth_service_with_its_steps_and_progress() {
    let b = bench().await;
    let ops = b.agent.ops();
    let id = b.running().await;
    ops.done(&id, "check", &UndoInfo::default()).await.unwrap();
    ops.set_progress(
        &id,
        &Progress {
            percent: Some(40),
            bytes_done: Some(400),
            bytes_total: Some(1_000),
            ..Progress::default()
        },
    )
    .await
    .unwrap();
    // Même un compte en lecture seule sans le droit lit (le service est celui de Hearth).
    for who in [&b.ada, &b.bob, &b.lucas, &b.dan] {
        let operation = view(&b.read(who, &id).await);
        assert_eq!(operation.id, id);
        assert_eq!(operation.key, HEARTH_KEY);
        assert_eq!(operation.status, OperationStatus::Running);
        assert_eq!(operation.step.as_deref(), Some("check"));
        assert_eq!(operation.percent, Some(40));
        assert_eq!(operation.bytes_done, Some(400));
        assert!(operation.cancellable);
        assert!(operation.decision.is_none() && operation.cause.is_none());
        let steps: Vec<_> = operation
            .steps
            .iter()
            .map(|step| step.step.as_str())
            .collect();
        assert_eq!(steps, ["check"], "seule une étape commencée est montrée");
    }
    let reply = b
        .agent
        .request("GET", &format!("/operations/services/{id}"))
        .send()
        .await;
    assert_eq!(reply.status, 401, "sans session, rien n'est lu");
}

#[tokio::test]
async fn an_unknown_operation_and_a_badly_shaped_identifier_answer_404_with_the_same_code() {
    let b = bench().await;
    let unknown = b.read(&b.ada, "01J9ZY0G3Q8M2K6W4T7V5N1B9D").await;
    assert_eq!(unknown.status, 404);
    assert_eq!(error_code(&unknown), "OPERATION_NOT_FOUND");
    for bad in ["..", "a%2Fb", &"A".repeat(200), "a.b"] {
        let reply = b.read(&b.ada, bad).await;
        assert_eq!(reply.status, 404, "{bad}");
        assert_eq!(reply.body, unknown.body, "{bad}");
    }
}

#[tokio::test]
async fn an_operation_on_an_adopted_service_is_read_by_the_right_only_and_on_a_hidden_one_by_nobody()
 {
    let adopted = container("adopte", RunState::Running);
    let adopted_key = format!("c_{}", adopted.id);
    let hidden = container("cache", RunState::Running);
    let hidden_key = format!("c_{}", hidden.id);
    let b = bench().await;
    b.engine.put_silently(adopted);
    b.engine.put_silently(hidden);
    b.agent.services().resync().await.expect("liste");
    b.env.adopt(&adopted_key, "adopte", "ada").await;
    b.agent.services().reload_adopted().await;
    let ops = b.agent.ops();
    let on_adopted = accept_on(&ops, &adopted_key, "adopte", "ada").await;
    let on_hidden = accept_on(&ops, &hidden_key, "cache", "ada").await;
    let on_hearth = accept_on(&ops, HEARTH_KEY, "terraria", "ada").await;

    assert_eq!(b.read(&b.ada, &on_adopted).await.status, 200);
    assert_eq!(b.read(&b.dan, &on_adopted).await.status, 200);
    let unknown = b.read(&b.ada, "01J9ZY0G3Q8M2K6W4T7V5N1B9Z").await;
    for who in [&b.bob, &b.lucas] {
        let refused = b.read(who, &on_adopted).await;
        // Exactement la réponse d'une opération inconnue : rien ne dit qu'elle existe.
        assert_eq!(refused.status, 404);
        assert_eq!(refused.body, unknown.body);
    }
    // Un conteneur non adopté n'a aucune opération visible, même pour le droit.
    for who in [&b.ada, &b.dan, &b.bob, &b.lucas] {
        let refused = b.read(who, &on_hidden).await;
        assert_eq!(refused.status, 404);
        assert_eq!(refused.body, unknown.body);
    }
    assert_eq!(b.read(&b.lucas, &on_hearth).await.status, 200);
    // Annuler ou décider sur ce qu'on ne voit pas : la même réponse (le droit y est exigé AVANT, par la couche
    // d'accès ; ici dan a le droit mais l'opération est sur un service non adopté).
    let refused = b.cancel(&b.dan, &on_hidden).await;
    assert_eq!(refused.status, 404);
    assert_eq!(refused.body, unknown.body);
    // L'opération n'a pas bougé.
    assert_eq!(
        ops.snapshot(&on_hidden)
            .await
            .unwrap()
            .unwrap()
            .record
            .status,
        OpStatus::Accepted
    );
}

// ---------------------------------------------------------------------------------------------
// Annuler : point de non-retour, idempotence, trace
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_running_operation_is_cancelled_by_an_account_with_the_right_and_the_journal_keeps_the_author()
 {
    let b = bench().await;
    let id = b.running().await;
    let reply = b.cancel(&b.ada, &id).await;
    let operation = view(&reply);
    assert_eq!(operation.status, OperationStatus::Cancelling);
    assert!(!operation.cancellable);
    assert_eq!(
        operation.cause,
        Some(hearth_proto::api::service_ops::OperationCause::CancelledByAccount)
    );
    // Sous l'action du GENRE de l'opération (un déploiement), avec le compte, jamais une valeur ni un chemin.
    let entries = b.journal().await;
    let cancels: Vec<_> = entries
        .iter()
        .filter(|(action, outcome, ..)| action == "service.deploy" && outcome == "ok")
        .collect();
    assert_eq!(cancels.len(), 1, "{entries:?}");
    assert_eq!(cancels[0].4.as_deref(), Some("ada"));
    assert_eq!(
        cancels[0].2.as_deref(),
        Some("service terraria : opération annulée")
    );
}

#[tokio::test]
async fn cancelling_twice_answers_the_same_and_writes_one_journal_entry() {
    let b = bench().await;
    let id = b.running().await;
    let first = view(&b.cancel(&b.ada, &id).await);
    let second = view(&b.cancel(&b.ada, &id).await);
    assert_eq!(first.status, OperationStatus::Cancelling);
    assert_eq!(second.status, OperationStatus::Cancelling);
    let entries = b.journal().await;
    assert_eq!(
        entries
            .iter()
            .filter(|(action, outcome, ..)| action == "service.deploy" && outcome == "ok")
            .count(),
        1,
        "{entries:?}"
    );
}

#[tokio::test]
async fn an_accepted_operation_is_cancelled_at_once_and_frees_its_service() {
    let b = bench().await;
    let ops = b.agent.ops();
    let id = accept_on(&ops, HEARTH_KEY, "terraria", "ada").await;
    let operation = view(&b.cancel(&b.ada, &id).await);
    assert_eq!(operation.status, OperationStatus::Cancelled);
    // Le verrou est rendu : une autre opération peut être acceptée sur ce service.
    let again = accept_on(&ops, HEARTH_KEY, "terraria", "ada").await;
    assert_ne!(again, id);
    // Elle apparaît aussi comme « dernière action » du service dans la liste (projection du journal).
    assert_eq!(
        ops.with_index(|index| index
            .last_action(&ServiceKey::parse(HEARTH_KEY).unwrap())
            .map(|op| op.status)),
        Some(OpStatus::Cancelled)
    );
}

#[tokio::test]
async fn after_the_point_of_no_return_the_answer_is_too_late_and_nothing_changes() {
    let b = bench().await;
    let ops = b.agent.ops();
    let id = b.running().await;
    ops.intent(&id, "create_new", &UndoInfo::default())
        .await
        .unwrap();
    let operation = view(&b.read(&b.ada, &id).await);
    assert!(!operation.cancellable, "le point de non-retour est passé");
    let reply = b.cancel(&b.ada, &id).await;
    assert_eq!(reply.status, 409);
    assert_eq!(error_code(&reply), "TOO_LATE_TO_CANCEL");
    let after = ops.snapshot(&id).await.unwrap().unwrap();
    assert_eq!(after.record.status, OpStatus::Running);
    // Le refus est consigné sous l'action de la route (le genre n'est pas la raison du refus).
    let entries = b.journal().await;
    assert!(
        entries
            .iter()
            .any(|(action, outcome, ..)| action == "operation.cancel" && outcome == "failed"),
        "{entries:?}"
    );
    assert!(
        !entries
            .iter()
            .any(|(action, outcome, ..)| action == "service.deploy" && outcome == "ok"),
        "{entries:?}"
    );
}

#[tokio::test]
async fn an_intent_after_a_cancellation_is_refused_by_the_write_itself() {
    // L'annulation qui passe l'opération en `cancelling` ne laisse pas un exécuteur écrire l'`intent` d'un
    // geste destructif ensuite : la condition de statut est dans l'instruction qui écrit (ADR-0045).
    let b = bench().await;
    let ops = b.agent.ops();
    let id = b.running().await;
    view(&b.cancel(&b.ada, &id).await);
    let error = ops
        .intent(&id, "create_new", &UndoInfo::default())
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            hearth_agent::application::service_ops::OpError::OutOfOrder
                | hearth_agent::application::service_ops::OpError::Stale
        ),
        "{error:?}"
    );
    let rows = ops.snapshot(&id).await.unwrap().unwrap().steps;
    assert!(
        rows.is_empty(),
        "aucune ligne destructive n'a été écrite : {rows:?}"
    );
}

#[tokio::test]
async fn cancelling_needs_the_right_and_the_confirmation_of_the_act() {
    let b = bench().await;
    let id = b.running().await;
    // Administrateur sans le droit : refusé, avec le droit qui manque.
    let refused = b.cancel(&b.bob, &id).await;
    assert_eq!(refused.status, 403);
    assert_eq!(refused.body["error"]["details"]["missing"], "services");
    // Sans confirmation (client trop ancien) : refusé, rien n'a bougé.
    let bare = b
        .agent
        .request("DELETE", &format!("/operations/services/{id}"))
        .token(&b.ada.token)
        .json(&json!({}))
        .send()
        .await;
    assert_eq!(bare.status, 426, "{}", bare.text);
    // Un compte en lecture seule qui a le droit annule (le droit est indépendant du rôle, ADR-0041).
    let ops = b.agent.ops();
    assert_eq!(
        ops.snapshot(&id).await.unwrap().unwrap().record.status,
        OpStatus::Running
    );
    assert_eq!(b.cancel(&b.dan, &id).await.status, 200);
}

#[tokio::test]
async fn the_proof_of_a_cancellation_is_bound_to_the_operation_it_names() {
    let b = bench().await;
    let first = b.running().await;
    let other_service = accept_on(&b.agent.ops(), OTHER_HEARTH_KEY, "autre", "ada").await;
    // Une preuve faite pour l'opération `first`, envoyée pour une autre : refusée.
    let body = b
        .agent
        .confirm(
            &b.ada,
            "DELETE",
            &format!("/operations/services/{first}"),
            json!({}),
        )
        .await;
    let reply = b
        .agent
        .request("DELETE", &format!("/operations/services/{other_service}"))
        .token(&b.ada.token)
        .json(&body)
        .send()
        .await;
    assert_eq!(reply.status, 409, "{}", reply.text);
    assert_eq!(error_code(&reply), "POST_NOT_RECOGNIZED");
    assert_eq!(
        b.agent
            .ops()
            .snapshot(&other_service)
            .await
            .unwrap()
            .unwrap()
            .record
            .status,
        OpStatus::Accepted
    );
}

// ---------------------------------------------------------------------------------------------
// Décider (BR-JEU-006) : les choix offerts, l'expiration
// ---------------------------------------------------------------------------------------------

async fn waiting(b: &Bench) -> String {
    let id = b.running().await;
    b.agent
        .ops()
        .ask_decision(&id, &[Choice::Continue, Choice::Cancel])
        .await
        .unwrap();
    id
}

#[tokio::test]
async fn a_decision_among_the_offered_ones_is_applied_and_journaled() {
    let b = bench().await;
    let id = waiting(&b).await;
    let operation = view(&b.read(&b.lucas, &id).await);
    assert_eq!(operation.status, OperationStatus::AwaitingDecision);
    let decision = operation.decision.expect("décision offerte");
    assert_eq!(decision.choices.len(), 2);
    assert!(decision.expires_at.is_some());
    let decided = view(&b.decide(&b.ada, &id, "continue").await);
    assert_eq!(decided.status, OperationStatus::Running);
    assert!(decided.decision.is_none());
    let entries = b.journal().await;
    assert!(
        entries.iter().any(
            |(action, outcome, target, _, account)| action == "service.deploy"
                && outcome == "ok"
                && target.as_deref() == Some("service terraria : décision « poursuivre »")
                && account.as_deref() == Some("ada")
        ),
        "{entries:?}"
    );
}

#[tokio::test]
async fn a_decision_that_is_not_offered_is_refused_whatever_it_is() {
    let b = bench().await;
    let id = waiting(&b).await;
    for choice in ["revert", "keep", "verified", "dance"] {
        let reply = b.decide(&b.ada, &id, choice).await;
        assert_eq!(reply.status, 409, "{choice} : {}", reply.text);
        assert_eq!(error_code(&reply), "DECISION_NOT_OFFERED", "{choice}");
    }
    assert_eq!(
        b.agent
            .ops()
            .snapshot(&id)
            .await
            .unwrap()
            .unwrap()
            .record
            .status,
        OpStatus::AwaitingDecision
    );
    // Une opération qui n'attend rien n'a aucune décision à prendre.
    let running = accept_on(&b.agent.ops(), OTHER_HEARTH_KEY, "autre", "ada").await;
    b.agent.ops().start(&running).await.unwrap();
    let reply = b.decide(&b.ada, &running, "continue").await;
    assert_eq!(reply.status, 409);
    assert_eq!(reply.body["error"]["details"]["reason"], "not_waiting");
}

#[tokio::test]
async fn an_expired_decision_is_refused_and_the_sweep_cancels_it_with_its_cause() {
    let b = bench().await;
    let id = waiting(&b).await;
    b.env.clock.advance(time::Duration::minutes(15));
    let reply = b.decide(&b.ada, &id, "continue").await;
    assert_eq!(reply.status, 409);
    assert_eq!(reply.body["error"]["details"]["reason"], "expired");
    let swept = b.agent.ops().sweep().await.unwrap();
    assert_eq!(swept.expired, 1);
    let operation = view(&b.read(&b.ada, &id).await);
    assert_eq!(operation.status, OperationStatus::Cancelled);
    assert_eq!(
        operation.cause,
        Some(hearth_proto::api::service_ops::OperationCause::DecisionExpired)
    );
}

#[tokio::test]
async fn deciding_to_cancel_ends_the_operation_and_an_operation_to_verify_never_expires() {
    let b = bench().await;
    let id = waiting(&b).await;
    let decided = view(&b.decide(&b.dan, &id, "cancel").await);
    assert_eq!(decided.status, OperationStatus::Cancelled);
    // Une opération à vérifier : ses choix n'ont pas d'échéance.
    let ops = b.agent.ops();
    let to_check = accept_on(&ops, OTHER_HEARTH_KEY, "autre", "ada").await;
    ops.start(&to_check).await.unwrap();
    ops.intent(&to_check, "stop_old", &UndoInfo::default())
        .await
        .unwrap();
    let observer = hearth_agent::infrastructure::ops_observer::UnobservedState;
    ops.recover_at_startup(&observer).await.unwrap();
    let operation = view(&b.read(&b.ada, &to_check).await);
    assert_eq!(operation.status, OperationStatus::NeedsAttention);
    assert!(operation.decision.expect("choix").expires_at.is_none());
    b.env.clock.advance(time::Duration::hours(2));
    assert_eq!(ops.sweep().await.unwrap().expired, 0);
    assert_eq!(
        view(&b.decide(&b.ada, &to_check, "verified").await).status,
        OperationStatus::Failed
    );
}

// ---------------------------------------------------------------------------------------------
// Le flux : un message par changement, au plus un par 500 ms et par opération, filtré par compte
// ---------------------------------------------------------------------------------------------

async fn next_operation(client: &mut ws::WsClient, wait: Duration) -> Option<OperationView> {
    loop {
        match client.try_next_services(wait).await? {
            ServicesMessage::Operation(view) => return Some(view),
            _ => continue,
        }
    }
}

#[tokio::test]
async fn the_stream_announces_open_operations_at_once_then_every_change_at_most_every_500_ms() {
    let b = bench().await;
    let ops = b.agent.ops();
    let id = b.running().await;
    let mut client = b.subscribed(&b.lucas).await;
    // L'opération en cours est annoncée à l'abonnement.
    let first = next_operation(&mut client, Duration::from_secs(5))
        .await
        .expect("opération en cours");
    assert_eq!(first.id, id);
    assert_eq!(first.status, OperationStatus::Running);
    // Vingt étapes en rafale : le dernier état part à l'échéance, les intermédiaires se fondent.
    let first_seen = std::time::Instant::now();
    let mut stamps = vec![first_seen];
    for _ in 0..20 {
        ops.intent(&id, "check", &UndoInfo::default())
            .await
            .unwrap();
        ops.done(&id, "check", &UndoInfo::default()).await.unwrap();
    }
    ops.finish(&id, OpStatus::Succeeded, None).await.unwrap();
    let burst = first_seen.elapsed();
    let mut received = Vec::new();
    while let Some(next) = next_operation(&mut client, Duration::from_millis(900)).await {
        stamps.push(std::time::Instant::now());
        received.push(next);
    }
    // Quarante changements en rafale : bien moins de messages (la rafale elle-même peut durer sur une machine
    // chargée, d'où une borne sur le temps écoulé et non un nombre fixe), et deux messages d'une même
    // opération ne se suivent jamais de plus près que la cadence (avec la marge de la gigue de réception).
    assert!(
        received.len() <= 2 + usize::try_from(burst.as_millis() / 500).unwrap(),
        "au plus un message par 500 ms et par opération : {} en {:?}",
        received.len(),
        burst
    );
    for pair in stamps.windows(2).skip(1) {
        assert!(
            pair[1] - pair[0] >= Duration::from_millis(350),
            "deux messages d'une même opération à {:?} d'écart",
            pair[1] - pair[0]
        );
    }
    let last = received.last().expect("le dernier état n'est jamais perdu");
    assert_eq!(last.status, OperationStatus::Succeeded);
    assert!(!last.cancellable);
}

#[tokio::test]
async fn the_stream_shows_an_account_only_the_operations_of_the_services_it_sees() {
    let adopted = container("adopte", RunState::Running);
    let adopted_key = format!("c_{}", adopted.id);
    let b = bench().await;
    b.engine.put_silently(adopted);
    b.agent.services().resync().await.expect("liste");
    b.env.adopt(&adopted_key, "adopte", "ada").await;
    b.agent.services().reload_adopted().await;
    let ops = b.agent.ops();
    let on_adopted = accept_on(&ops, &adopted_key, "adopte", "ada").await;
    let on_hearth = accept_on(&ops, HEARTH_KEY, "terraria", "ada").await;
    let mut with_right = b.subscribed(&b.dan).await;
    let mut without = b.subscribed(&b.lucas).await;
    let mut seen_with = Vec::new();
    while let Some(next) = next_operation(&mut with_right, Duration::from_millis(900)).await {
        seen_with.push(next.id);
    }
    let mut seen_without = Vec::new();
    while let Some(next) = next_operation(&mut without, Duration::from_millis(900)).await {
        seen_without.push(next.id);
    }
    seen_with.sort();
    let mut expected = vec![on_adopted.clone(), on_hearth.clone()];
    expected.sort();
    assert_eq!(seen_with, expected);
    assert_eq!(seen_without, vec![on_hearth]);
    // Un changement ensuite : même filtre.
    ops.start(&on_adopted).await.unwrap();
    assert!(
        next_operation(&mut with_right, Duration::from_secs(2))
            .await
            .is_some()
    );
    assert!(
        next_operation(&mut without, Duration::from_millis(900))
            .await
            .is_none(),
        "un compte sans le droit ne reçoit rien d'une opération sur un service adopté"
    );
}

#[tokio::test]
async fn the_list_of_services_projects_busy_and_last_action_from_the_journal() {
    let live = container("vivant", RunState::Running);
    let live_key = format!("c_{}", live.id);
    let b = bench().await;
    b.engine.put_silently(live);
    b.agent.services().resync().await.expect("liste");
    b.env.adopt(&live_key, "vivant", "ada").await;
    b.agent.services().reload_adopted().await;
    let ops = b.agent.ops();
    let id = accept_on(&ops, &live_key, "vivant", "ada").await;
    ops.start(&id).await.unwrap();
    let services = b
        .agent
        .request("GET", "/services")
        .token(&b.ada.token)
        .send()
        .await;
    let item = &services.body["services"][0];
    assert_eq!(item["busy"]["kind"], "deploy");
    assert_eq!(item["busy"]["by"], "ada");
    assert!(item.get("last_action").is_none() || item["last_action"].is_null());
    ops.finish(&id, OpStatus::Succeeded, None).await.unwrap();
    let services = b
        .agent
        .request("GET", "/services")
        .token(&b.ada.token)
        .send()
        .await;
    let item = &services.body["services"][0];
    assert!(item.get("busy").is_none() || item["busy"].is_null());
    assert_eq!(item["last_action"]["id"], id);
    assert_eq!(item["last_action"]["status"], "succeeded");
}

// ---------------------------------------------------------------------------------------------
// Bornes
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_number_of_kept_operations_is_bounded_and_an_open_one_is_never_dropped() {
    let b = bench().await;
    let ops = b.agent.ops();
    let open = b.running().await;
    // Plus que le plafond d'opérations terminées, chacune sur son propre service.
    let cap = hearth_proto::api::service_ops::MAX_RETAINED_OPERATIONS;
    // Les clés `h_` exigent 26 caractères d'un alphabet fermé : on réutilise deux services, une opération à
    // la fois (le verrou est rendu à chaque fin).
    for number in 0..cap + 25 {
        let key = if number % 2 == 0 {
            OTHER_HEARTH_KEY
        } else {
            "h_01J9ZY0G3Q8M2K6W4T7V5N1B9F"
        };
        let id = accept_on(&ops, key, "jeu", "ada").await;
        ops.cancel(&id).await.unwrap();
        b.env.clock.advance(time::Duration::milliseconds(1));
    }
    let swept = ops.sweep().await.unwrap();
    assert!(swept.pruned >= 25, "{swept:?}");
    let kept: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM service_ops WHERE finished_at IS NOT NULL")
            .fetch_one(b.env.db.pool())
            .await
            .unwrap();
    assert_eq!(kept, i64::from(cap));
    assert_eq!(
        view(&b.read(&b.ada, &open).await).status,
        OperationStatus::Running
    );
}

// ---------------------------------------------------------------------------------------------
// Rien de sensible ne sort : ni dans une réponse, ni dans le flux, ni dans le journal, ni dans les traces
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
                EnvFilter::new("trace"),
            )
            .expect("première installation du journal de ce binaire de test");
            buffer
        })
        .clone()
}

#[tokio::test]
async fn no_path_no_secret_and_no_object_name_reaches_a_response_the_stream_the_journal_or_a_trace()
{
    let traces = captured_traces();
    let b = bench().await;
    let ops = b.agent.ops();
    // L'opération nomme un objet qu'elle a créé : son chemin relatif porte une sentinelle, sa racine est le
    // dossier de données de l'agent. Ni l'un ni l'autre ne doit sortir.
    let sentinel = "SENTINEL-REL-4c1d9e";
    let data_dir = b.env.dir.path().to_string_lossy().to_string();
    let id = b.running().await;
    let undo = UndoInfo {
        created: vec![CreatedObject {
            root: ObjectRoot::Backups,
            rel: format!("{sentinel}/partial.tar.gz"),
            dev: 7,
            ino: 42,
            kind: ObjectKind::PartialArchive,
        }],
    };
    ops.intent(&id, "stop_old", &undo).await.unwrap();
    let mut client = b.subscribed(&b.lucas).await;
    let mut everything = String::new();
    while let Some(message) = client.try_next_services(Duration::from_millis(900)).await {
        everything.push_str(&serde_json::to_string(&message).unwrap());
    }
    ops.done(&id, "stop_old", &undo).await.unwrap();
    everything.push_str(&b.read(&b.lucas, &id).await.text);
    everything.push_str(&b.read(&b.ada, &id).await.text);
    // Une annulation refusée, une décision refusée, une opération inconnue : leurs messages d'erreur aussi.
    everything.push_str(&b.cancel(&b.ada, &id).await.text);
    everything.push_str(&b.decide(&b.ada, &id, "keep").await.text);
    everything.push_str(&b.read(&b.ada, "INCONNUE").await.text);
    let waiting = accept_on(&ops, OTHER_HEARTH_KEY, "autre", "ada").await;
    ops.start(&waiting).await.unwrap();
    ops.intent(&waiting, "stop_old", &undo).await.unwrap();
    everything.push_str(&b.cancel(&b.ada, &waiting).await.text);
    let journal = format!("{:?}", b.journal().await);
    let traced = String::from_utf8_lossy(&traces.lock().unwrap()).to_string();
    // Témoins : sans eux, un tampon vide rendrait le test aveugle.
    assert!(
        everything.contains(&id),
        "les réponses portent bien l'opération"
    );
    assert!(journal.contains("service.deploy") || journal.contains("operation.cancel"));
    assert!(traced.contains("hearth"), "les traces sont capturées");
    // Recherche brute, aucun effacement avant : ni la sentinelle, ni le dossier de données, ni le mot de passe.
    for (what, text) in [
        ("réponses et flux", &everything),
        ("journal d'activité", &journal),
        ("traces", &traced),
    ] {
        assert!(!text.contains(sentinel), "{what} : chemin d'objet");
        assert!(!text.contains(PASSWORD), "{what} : mot de passe");
        assert!(!text.contains("partial.tar.gz"), "{what} : nom d'objet");
    }
    // Le dossier de données de l'agent (chemin absolu du serveur) : ni dans une réponse, ni dans le flux, ni
    // dans le journal d'activité. (Les traces de démarrage le disent ailleurs, hors de ce périmètre.)
    for (what, text) in [
        ("réponses et flux", &everything),
        ("journal d'activité", &journal),
    ] {
        assert!(!text.contains(&data_dir), "{what} : dossier de données");
    }
    // Et la trace de l'annulation ne contient que ce qui est prévu : l'identifiant de l'opération.
    assert!(
        !everything.to_lowercase().contains("undo"),
        "le journal d'étapes ne sort pas"
    );
    let value: Value = serde_json::from_str(&b.read(&b.ada, &id).await.text).unwrap();
    assert!(value.get("undo").is_none() && value.get("plan").is_none());
}
