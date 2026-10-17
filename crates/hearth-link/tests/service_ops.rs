//! Les opérations de service contre un VRAI agent (TLS 1.3, SQLite, vrai flux WebSocket), vues par la
//! liaison (HRT-144, ADR-0045) : lire une opération, la suivre en direct par le message `operation`, la
//! décider, l'annuler, avec le refus de l'agent typé par sa raison. Aucun geste de service n'existe encore :
//! le test accepte l'opération dans le journal de l'agent, comme le ferait le lot d'un geste. Aucune
//! attente de durée, seulement des faits observables.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::{Arc, Mutex};

use hearth_agent::application::service_ops::AcceptRequest;
use hearth_agent::domain::ops::{
    Choice, OpKind, OpPlan, OpStatus, PlannedStep, StepName, UndoInfo,
};
use hearth_agent::domain::services::ServiceKey;
use hearth_link::domain::event::Event;
use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use hearth_link::domain::service_ops::{DecisionReason, OperationRefusal};
use hearth_link::domain::state::LinkState;
use hearth_link::ports::EventSink;
use hearth_link::{LinkError, OperationActResult};
use hearth_proto::api::service_ops::{OperationChoice, OperationStatus, OperationView};
use hearth_proto::error::ErrorCode;
use support::agent::fake_engine::FakeEngine;
use support::{Options, PASSWORD, WAIT, World};

const FIRST: &str = "h_01J9ZY0G3Q8M2K6W4T7V5N1B9D";
const SECOND: &str = "h_01J9ZY0G3Q8M2K6W4T7V5N1B9E";
const THIRD: &str = "h_01J9ZY0G3Q8M2K6W4T7V5N1B9F";

/// Les messages `operation` que la liaison a annoncés.
#[derive(Default)]
struct Collector(Mutex<Vec<Arc<OperationView>>>);

impl EventSink for Collector {
    fn emit(&self, _event: Event) {}

    fn service_operation(&self, _server: &ServerId, operation: Arc<OperationView>) {
        self.0.lock().unwrap().push(operation);
    }
}

impl Collector {
    fn count(&self, id: &str, status: OperationStatus) -> usize {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|operation| operation.id == id && operation.status == status)
            .count()
    }

    async fn wait(&self, what: &str, predicate: impl Fn(&OperationView) -> bool) -> OperationView {
        let started = std::time::Instant::now();
        loop {
            if let Some(found) = self
                .0
                .lock()
                .unwrap()
                .iter()
                .find(|operation| predicate(operation))
            {
                return (**found).clone();
            }
            assert!(started.elapsed() < WAIT, "délai dépassé : {what}");
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
}

fn name(text: &str) -> StepName {
    StepName::parse(text).unwrap()
}

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
        ],
        Some(name("create_new")),
    )
    .unwrap()
}

async fn world() -> (World, Arc<Collector>) {
    let collector = Arc::new(Collector::default());
    let world = World::connected(Options {
        device_key: true,
        engine: Some(Arc::new(FakeEngine::new())),
        extra_sink: Some(collector.clone() as Arc<dyn EventSink>),
        ..Options::default()
    })
    .await;
    world.agent.grant_services("marie").await;
    (world, collector)
}

async fn running(world: &World, key: &str) -> String {
    let ops = world.agent.ops();
    let id = ops
        .accept(AcceptRequest {
            op_key: format!("k-{key}"),
            service: ServiceKey::parse(key).unwrap(),
            service_name: "terraria".to_owned(),
            kind: OpKind::Deploy,
            account: "marie".to_owned(),
            plan: plan(),
            forced: false,
            reserve_name: None,
            heavy_io: false,
        })
        .await
        .unwrap();
    ops.start(&id).await.unwrap();
    id
}

fn password() -> Secret {
    Secret::from(PASSWORD)
}

#[tokio::test]
async fn an_operation_is_read_and_followed_live_by_the_link() {
    let (world, collector) = world().await;
    let id = running(&world, FIRST).await;
    // Le suivi en direct : l'agent envoie le message `operation`, la liaison l'annonce.
    let live = collector
        .wait("l'opération en direct", |operation| {
            operation.id == id && operation.status == OperationStatus::Running
        })
        .await;
    assert_eq!(live.key, FIRST);
    // La lecture : le même état par la route.
    let read = world
        .manager
        .service_operation(&world.id, &id)
        .await
        .unwrap();
    assert_eq!(read.id, id);
    assert_eq!(read.status, OperationStatus::Running);
    assert!(read.cancellable);
    // Une opération inconnue : refus typé de l'agent, avec son code.
    let unknown = world
        .manager
        .service_operation(&world.id, "01J9ZY0G3Q8M2K6W4T7V5N1B9Z")
        .await
        .unwrap_err();
    assert_eq!(
        unknown,
        LinkError::Rejected(Some(ErrorCode::OperationNotFound))
    );
    // Un identifiant qui n'en est pas un ne part jamais.
    assert_eq!(
        world
            .manager
            .service_operation(&world.id, "../accounts")
            .await
            .unwrap_err(),
        LinkError::UnreadableAct
    );
}

#[tokio::test]
async fn cancelling_is_a_confirmed_act_followed_by_the_stream_and_idempotent() {
    let (world, collector) = world().await;
    let id = running(&world, FIRST).await;
    let done = world
        .manager
        .cancel_operation(&world.id, &id, Some(&password()))
        .await
        .unwrap();
    let OperationActResult::Done(view) = done else {
        panic!("annulation attendue : {done:?}");
    };
    assert_eq!(view.status, OperationStatus::Cancelling);
    collector
        .wait("l'annulation en direct", |operation| {
            operation.id == id && operation.status == OperationStatus::Cancelling
        })
        .await;
    // Une deuxième fois : la même issue.
    let again = world
        .manager
        .cancel_operation(&world.id, &id, Some(&password()))
        .await
        .unwrap();
    assert!(
        matches!(again, OperationActResult::Done(view) if view.status == OperationStatus::Cancelling)
    );
}

#[tokio::test]
async fn after_the_point_of_no_return_the_refusal_says_too_late() {
    let (world, _collector) = world().await;
    let id = running(&world, SECOND).await;
    world
        .agent
        .ops()
        .intent(&id, "create_new", &UndoInfo::default())
        .await
        .unwrap();
    let refused = world
        .manager
        .cancel_operation(&world.id, &id, Some(&password()))
        .await
        .unwrap();
    assert_eq!(
        refused,
        OperationActResult::Refused(OperationRefusal::TooLate)
    );
}

#[tokio::test]
async fn a_decision_is_signed_with_its_choice_and_a_refused_one_says_why() {
    let (world, _collector) = world().await;
    let id = running(&world, THIRD).await;
    world
        .agent
        .ops()
        .ask_decision(&id, &[Choice::Continue, Choice::Cancel])
        .await
        .unwrap();
    // Un choix que l'opération n'offre pas : refus typé avec sa raison.
    let refused = world
        .manager
        .decide_operation(&world.id, &id, OperationChoice::Revert, Some(&password()))
        .await
        .unwrap();
    assert_eq!(
        refused,
        OperationActResult::Refused(OperationRefusal::DecisionNotOffered(
            DecisionReason::NotOffered
        ))
    );
    let done = world
        .manager
        .decide_operation(&world.id, &id, OperationChoice::Continue, Some(&password()))
        .await
        .unwrap();
    assert!(
        matches!(done, OperationActResult::Done(view) if view.status == OperationStatus::Running)
    );
    // Un choix inconnu de ce client ne part pas.
    assert_eq!(
        world
            .manager
            .decide_operation(&world.id, &id, OperationChoice::Unknown, Some(&password()))
            .await
            .unwrap_err(),
        LinkError::UnreadableAct
    );
}

#[tokio::test]
async fn without_the_right_the_refusal_names_the_missing_right() {
    let (world, _collector) = world().await;
    let id = running(&world, FIRST).await;
    world.agent.revoke_services("marie").await;
    let refused = world
        .manager
        .cancel_operation(&world.id, &id, Some(&password()))
        .await
        .unwrap();
    assert_eq!(
        refused,
        OperationActResult::Refused(OperationRefusal::MissingRight("services".into()))
    );
    // Lire reste permis à tout compte pour un service de Hearth.
    assert!(
        world
            .manager
            .service_operation(&world.id, &id)
            .await
            .is_ok()
    );
}

/// BR-RESIL-010, BR-SVC-029 : après une coupure, l'opération qui court encore est reconstruite sans nouveau
/// message ; celle qui a fini pendant la coupure annonce son issue UNE fois.
#[tokio::test]
async fn after_a_cut_a_running_operation_is_rebuilt_silently_and_a_finished_one_announces_once() {
    let (world, collector) = world().await;
    let still_running = running(&world, FIRST).await;
    let finishing = running(&world, SECOND).await;
    collector
        .wait("l'opération qui court", |o| {
            o.id == still_running && o.status == OperationStatus::Running
        })
        .await;
    collector
        .wait("l'opération qui finira", |o| {
            o.id == finishing && o.status == OperationStatus::Running
        })
        .await;
    let before = collector.0.lock().unwrap().len();

    let mark = world.recorder.mark();
    world.proxy.refuse();
    world.proxy.cut();
    world
        .recorder
        .wait_state(mark, LinkState::Reconnecting, WAIT)
        .await;
    world
        .agent
        .ops()
        .finish(&finishing, OpStatus::Succeeded, None)
        .await
        .unwrap();
    let mark = world.recorder.mark();
    world.proxy.heal();
    world
        .recorder
        .wait_state(mark, LinkState::Connected, WAIT)
        .await;

    // L'issue de celle qui a fini pendant la coupure : relue, annoncée.
    collector
        .wait("l'issue relue", |o| {
            o.id == finishing && o.status == OperationStatus::Succeeded
        })
        .await;
    // Laisse passer la relecture de toutes les opérations non redites, puis compte.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    assert_eq!(collector.count(&finishing, OperationStatus::Succeeded), 1);
    // Celle qui court encore : l'agent l'a redite à l'abonnement, identique, rien de nouveau.
    let after = collector.0.lock().unwrap().len();
    assert_eq!(after, before + 1, "seule l'issue est un message de plus");
}
