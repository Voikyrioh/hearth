//! La reprise au démarrage regarde le VRAI moteur (HRT-144, ADR-0045, BR-OPS-008, BR-EDIT-031) : l'observateur
//! du moteur compte les conteneurs du service d'une opération non terminée (présent, absent, en double,
//! illisible) et la reprise classe sans rien rejouer ni écrire dans le moteur. Un faux moteur derrière le
//! port ; le vrai journal SQLite.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "support/tmp.rs"]
mod tmp;

#[path = "support/fake_engine.rs"]
mod fake_engine;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use fake_engine::{FakeEngine, container};
use hearth_agent::application::ports::{Clock, EngineError, IdGen, OpJournal, ServiceObserver};
use hearth_agent::application::service_ops::{AcceptRequest, OpLocks, ServiceOps};
use hearth_agent::domain::ops::{Cause, OpKind, OpPlan, OpStatus, PlannedStep, StepName, UndoInfo};
use hearth_agent::domain::services::{RunState, ServiceKey};
use hearth_agent::infrastructure::created_objects::FsCreatedObjects;
use hearth_agent::infrastructure::ops_observer::{EngineObserver, UnobservedState};
use hearth_agent::infrastructure::sqlite::{Database, SqliteOpJournal};
use time::{Duration, OffsetDateTime};

const HEARTH_KEY: &str = "h_01J9ZY0G3Q8M2K6W4T7V5N1B9D";

struct TestClock;

impl Clock for TestClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH + Duration::days(20_000)
    }
}

struct Ids(AtomicU64);

impl IdGen for Ids {
    fn new_id(&self) -> String {
        format!("OP{:08}", self.0.fetch_add(1, Ordering::SeqCst))
    }
}

fn name(text: &str) -> StepName {
    StepName::parse(text).unwrap()
}

/// Mise à jour par recréation : un geste de vérification, deux gestes destructifs.
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

struct Bench {
    _dir: tmp::TestDir,
    db: Database,
    engine: Arc<FakeEngine>,
    ids: Arc<Ids>,
}

async fn bench() -> Bench {
    let dir = tmp::tempdir().unwrap();
    let db = Database::open(dir.path()).await.unwrap();
    Bench {
        _dir: dir,
        db,
        engine: Arc::new(FakeEngine::new()),
        ids: Arc::new(Ids(AtomicU64::new(1))),
    }
}

impl Bench {
    /// L'agent « démarre » : de nouveaux verrous, le même journal, cet observateur.
    fn ops(&self) -> ServiceOps {
        ServiceOps::new(
            Arc::new(SqliteOpJournal::new(self.db.pool().clone())) as Arc<dyn OpJournal>,
            Arc::new(OpLocks::new()),
            Arc::new(TestClock),
            self.ids.clone(),
            Arc::new(FsCreatedObjects::new([])),
        )
    }

    /// Une opération dont les deux gestes destructifs sont faits, sur `key`.
    async fn finished_gestures(&self, key: &str) -> String {
        let ops = self.ops();
        let id = ops
            .accept(AcceptRequest {
                op_key: "k".into(),
                service: ServiceKey::parse(key).unwrap(),
                service_name: "jeu".into(),
                kind: OpKind::Update,
                account: "marie".into(),
                plan: plan(),
                forced: false,
                reserve_name: None,
                heavy_io: false,
            })
            .await
            .unwrap();
        ops.start(&id).await.unwrap();
        for step in ["stop_old", "create_new"] {
            ops.intent(&id, step, &UndoInfo::default()).await.unwrap();
            ops.done(&id, step, &UndoInfo::default()).await.unwrap();
        }
        id
    }

    async fn restart_with(
        &self,
        observer: &dyn ServiceObserver,
        id: &str,
    ) -> (OpStatus, Option<Cause>) {
        let ops = self.ops();
        ops.recover_at_startup(observer).await.unwrap();
        let record = ops.snapshot(id).await.unwrap().unwrap().record;
        (record.status, record.cause)
    }
}

fn labelled(name: &str, key: &str) -> hearth_agent::domain::services::Observation {
    let mut observation = container(name, RunState::Running);
    observation.hearth_key = Some(ServiceKey::parse(key).unwrap());
    observation
}

#[tokio::test]
async fn one_container_of_the_service_and_a_complete_plan_is_interrupted_and_runs_with_the_new_one()
{
    let b = bench().await;
    b.engine.put_silently(labelled("jeu", HEARTH_KEY));
    let id = b.finished_gestures(HEARTH_KEY).await;
    let observer = EngineObserver::new(b.engine.clone());
    let (status, cause) = b.restart_with(&observer, &id).await;
    assert_eq!(status, OpStatus::Interrupted);
    assert_eq!(cause, Some(Cause::RunsWithNew));
}

#[tokio::test]
async fn two_containers_of_the_same_service_need_attention_and_keep_the_lock() {
    let b = bench().await;
    b.engine.put_silently(labelled("jeu", HEARTH_KEY));
    b.engine.put_silently(labelled("jeu-ancien", HEARTH_KEY));
    let id = b.finished_gestures(HEARTH_KEY).await;
    let observer = EngineObserver::new(b.engine.clone());
    let (status, cause) = b.restart_with(&observer, &id).await;
    assert_eq!(status, OpStatus::NeedsAttention);
    assert_eq!(cause, Some(Cause::TwoStates));
}

#[tokio::test]
async fn a_container_that_belongs_to_another_service_does_not_count() {
    let b = bench().await;
    b.engine
        .put_silently(labelled("autre", "h_01J9ZY0G3Q8M2K6W4T7V5N1B9E"));
    let id = b.finished_gestures(HEARTH_KEY).await;
    let observer = EngineObserver::new(b.engine.clone());
    // Aucun conteneur de ce service alors que le plan est allé au bout : ne correspond pas, à vérifier.
    let (status, cause) = b.restart_with(&observer, &id).await;
    assert_eq!(status, OpStatus::NeedsAttention);
    assert_eq!(cause, Some(Cause::StateUnknown));
}

#[tokio::test]
async fn a_container_service_is_counted_by_its_identifier() {
    let b = bench().await;
    let present = container("present", RunState::Running);
    let key = format!("c_{}", present.id);
    b.engine.put_silently(present);
    let id = b.finished_gestures(&key).await;
    let observer = EngineObserver::new(b.engine.clone());
    let (status, cause) = b.restart_with(&observer, &id).await;
    assert_eq!(
        (status, cause),
        (OpStatus::Interrupted, Some(Cause::RunsWithNew))
    );
}

#[tokio::test]
async fn an_engine_that_does_not_answer_is_never_taken_for_a_good_state() {
    let b = bench().await;
    b.engine.put_silently(labelled("jeu", HEARTH_KEY));
    b.engine.fail_list(Some(EngineError::Unreachable));
    let id = b.finished_gestures(HEARTH_KEY).await;
    let observer = EngineObserver::new(b.engine.clone());
    let (status, cause) = b.restart_with(&observer, &id).await;
    assert_eq!(status, OpStatus::NeedsAttention);
    assert_eq!(cause, Some(Cause::StateUnknown));
}

#[tokio::test]
async fn the_observer_only_reads_the_engine_and_a_half_executed_plan_is_never_presumed_coherent() {
    let b = bench().await;
    b.engine.put_silently(labelled("jeu", HEARTH_KEY));
    let ops = b.ops();
    let id = ops
        .accept(AcceptRequest {
            op_key: "k".into(),
            service: ServiceKey::parse(HEARTH_KEY).unwrap(),
            service_name: "jeu".into(),
            kind: OpKind::Update,
            account: "marie".into(),
            plan: plan(),
            forced: false,
            reserve_name: None,
            heavy_io: false,
        })
        .await
        .unwrap();
    ops.start(&id).await.unwrap();
    ops.intent(&id, "stop_old", &UndoInfo::default())
        .await
        .unwrap();
    ops.done(&id, "stop_old", &UndoInfo::default())
        .await
        .unwrap();
    let before = b.engine.container_ids();
    let observer = EngineObserver::new(b.engine.clone());
    let (status, cause) = b.restart_with(&observer, &id).await;
    // Un geste fait, un autre pas commencé : le moteur montre un conteneur mais le plan n'est pas allé au bout.
    assert_eq!(status, OpStatus::NeedsAttention);
    assert_eq!(cause, Some(Cause::StateUnknown));
    // Le moteur n'a pas bougé (le port n'a d'ailleurs aucune méthode qui écrit).
    assert_eq!(b.engine.container_ids(), before);
}

#[tokio::test]
async fn without_an_engine_observer_the_state_stays_unreadable_and_to_verify() {
    let b = bench().await;
    b.engine.put_silently(labelled("jeu", HEARTH_KEY));
    let id = b.finished_gestures(HEARTH_KEY).await;
    let (status, cause) = b.restart_with(&UnobservedState, &id).await;
    assert_eq!(
        (status, cause),
        (OpStatus::NeedsAttention, Some(Cause::StateUnknown))
    );
}

#[tokio::test]
async fn busy_and_last_action_are_rebuilt_from_the_journal_at_startup() {
    let b = bench().await;
    // Une opération terminée sur un service, une opération à vérifier sur un autre.
    let done = b.finished_gestures(HEARTH_KEY).await;
    b.ops()
        .finish(&done, OpStatus::Succeeded, None)
        .await
        .unwrap();
    let other = "h_01J9ZY0G3Q8M2K6W4T7V5N1B9E";
    let pending = b.finished_gestures(other).await;
    // Un nouvel agent : la projection repart du journal.
    let ops = b.ops();
    ops.recover_at_startup(&UnobservedState).await.unwrap();
    let first = ServiceKey::parse(HEARTH_KEY).unwrap();
    let second = ServiceKey::parse(other).unwrap();
    let last = ops.with_index(|index| {
        index
            .last_action(&first)
            .map(|op| (op.id.clone(), op.status))
    });
    assert_eq!(last, Some((done, OpStatus::Succeeded)));
    assert!(ops.with_index(|index| index.busy(&first).is_none()));
    // L'opération classée « à vérifier » tient toujours son service : `busy`.
    let busy = ops.with_index(|index| index.busy(&second).map(|op| (op.id.clone(), op.status)));
    assert_eq!(busy, Some((pending, OpStatus::NeedsAttention)));
}
