//! Le journal des opérations de service (HRT-83, ADR-0045), de bout en bout contre la VRAIE base SQLite,
//! SANS moteur : accepter, verrouiller, avancer étape par étape, annuler, décider, expirer, et surtout
//! REPRENDRE ou SIGNALER au démarrage de l'agent, sans jamais rejouer ni défaire un geste.
//!
//! Un « redémarrage de l'agent » est un nouveau `ServiceOps` (nouveaux verrous, vides) sur la même base.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "support/tmp.rs"]
mod tmp;

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use hearth_agent::application::ports::{
    Clock, CreatedObjects, IdGen, JournalError, OpJournal, OpRecord, Progress, ServiceObserver,
};
use hearth_agent::application::service_ops::{
    AcceptRequest, CancelOutcome, OpError, OpLocks, ServiceOps,
};
use hearth_agent::domain::ops::{
    Cause, Choice, ChoiceRefused, ConfigSide, CreatedObject, Holder, ObjectKind, ObjectRoot,
    Observed, OpKind, OpPlan, OpStatus, PlannedStep, ReadKind, Refusal, StepName, StepState,
    UndoInfo,
};
use hearth_agent::domain::services::key::ServiceKey;
use hearth_agent::infrastructure::created_objects::FsCreatedObjects;
use hearth_agent::infrastructure::sqlite::{Database, SqliteOpJournal};
use hearth_proto::error::ErrorCode;
use time::{Duration, OffsetDateTime};

struct TestClock(Mutex<OffsetDateTime>);

impl TestClock {
    fn new() -> Arc<Self> {
        Arc::new(Self(Mutex::new(
            OffsetDateTime::UNIX_EPOCH + Duration::days(20_000),
        )))
    }

    fn advance(&self, by: Duration) {
        *self.0.lock().unwrap() += by;
    }
}

impl Clock for TestClock {
    fn now(&self) -> OffsetDateTime {
        *self.0.lock().unwrap()
    }
}

struct Ids(AtomicU64);

impl IdGen for Ids {
    fn new_id(&self) -> String {
        format!("OP{:08}", self.0.fetch_add(1, Ordering::SeqCst))
    }
}

/// L'état que le « moteur » dit, et combien de fois on le lui a demandé. Il n'a AUCUN autre usage : si la
/// reprise voulait rejouer un geste, il n'y a rien à rejouer — et le journal ne doit pas bouger.
struct Observer {
    answer: Mutex<Observed>,
    asked: AtomicU64,
}

impl Observer {
    fn saying(answer: Observed) -> Self {
        Self {
            answer: Mutex::new(answer),
            asked: AtomicU64::new(0),
        }
    }
}

#[async_trait]
impl ServiceObserver for Observer {
    async fn observe(&self, _op: &OpRecord) -> Observed {
        self.asked.fetch_add(1, Ordering::SeqCst);
        *self.answer.lock().unwrap()
    }
}

struct Env {
    dir: tmp::TestDir,
    db: Database,
    clock: Arc<TestClock>,
    ids: Arc<Ids>,
    journal: Arc<SqliteOpJournal>,
    ops: ServiceOps,
}

fn ops_over(
    db: &Database,
    clock: &Arc<TestClock>,
    ids: &Arc<Ids>,
    objects: Arc<dyn CreatedObjects>,
) -> (Arc<SqliteOpJournal>, ServiceOps) {
    let journal = Arc::new(SqliteOpJournal::new(db.pool().clone()));
    let ops = ServiceOps::new(
        journal.clone() as Arc<dyn OpJournal>,
        Arc::new(OpLocks::new()),
        clock.clone(),
        ids.clone(),
        objects,
    );
    (journal, ops)
}

impl Env {
    async fn new() -> Self {
        let dir = tmp::tempdir().unwrap();
        let db = Database::open(dir.path()).await.unwrap();
        let clock = TestClock::new();
        let ids = Arc::new(Ids(AtomicU64::new(1)));
        let objects = Arc::new(FsCreatedObjects::new([
            (ObjectRoot::Backups, dir.path().join("backups")),
            (ObjectRoot::Staging, dir.path().join("staging")),
        ]));
        let (journal, ops) = ops_over(&db, &clock, &ids, objects);
        Self {
            dir,
            db,
            clock,
            ids,
            journal,
            ops,
        }
    }

    /// L'agent redémarre : verrous vides, même base, même disque.
    fn restarted(&self) -> ServiceOps {
        let objects = Arc::new(FsCreatedObjects::new([
            (ObjectRoot::Backups, self.dir.path().join("backups")),
            (ObjectRoot::Staging, self.dir.path().join("staging")),
        ]));
        ops_over(&self.db, &self.clock, &self.ids, objects).1
    }
}

fn service(n: u32) -> ServiceKey {
    ServiceKey::parse(&format!("h_{n:026}")).unwrap()
}

fn name(text: &str) -> StepName {
    StepName::parse(text).unwrap()
}

/// Modifier par recréation : une vérification, puis quatre gestes destructifs ; le point de non-retour est
/// la création du nouveau conteneur.
fn update_plan() -> OpPlan {
    let step = |text: &str, destructive| PlannedStep {
        name: name(text),
        destructive,
    };
    OpPlan::new(
        vec![
            step("check", false),
            step("stop_old", true),
            step("rename_old", true),
            step("create_new", true),
            step("start_new", true),
        ],
        Some(name("create_new")),
    )
    .unwrap()
}

fn request(service: ServiceKey, kind: OpKind) -> AcceptRequest {
    AcceptRequest {
        op_key: "client-key-1".into(),
        service,
        service_name: "Terraria".into(),
        kind,
        account: "acct1".into(),
        plan: update_plan(),
        forced: false,
        reserve_name: None,
        heavy_io: false,
    }
}

async fn running(env: &Env, n: u32, kind: OpKind) -> String {
    let id = env.ops.accept(request(service(n), kind)).await.unwrap();
    env.ops.start(&id).await.unwrap();
    id
}

async fn status(env: &Env, id: &str) -> OpRecord {
    env.journal.get(id).await.unwrap().unwrap()
}

fn none() -> UndoInfo {
    UndoInfo::default()
}

// ---------------------------------------------------------------------------------------------
// Accepter, verrouiller, avancer
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn an_accepted_operation_takes_the_service_and_gives_it_back_when_it_ends() {
    let env = Env::new().await;
    let id = env
        .ops
        .accept(request(service(1), OpKind::Update))
        .await
        .unwrap();
    assert_eq!(status(&env, &id).await.status, OpStatus::Accepted);
    // Le service est pris dès l'acceptation, par n'importe quel genre.
    for kind in [OpKind::Stop, OpKind::Delete, OpKind::Backup] {
        let refused = env.ops.accept(request(service(1), kind)).await.unwrap_err();
        assert!(
            matches!(
                refused,
                OpError::Locked(Refusal::ServiceBusy {
                    holder: Holder::Operation(OpKind::Update)
                })
            ),
            "{refused:?}"
        );
    }
    // Un autre service n'est pas gêné.
    let other = env
        .ops
        .accept(request(service(2), OpKind::Update))
        .await
        .unwrap();

    env.ops.start(&id).await.unwrap();
    env.ops.intent(&id, "stop_old", &none()).await.unwrap();
    env.ops.done(&id, "stop_old", &none()).await.unwrap();
    env.ops
        .finish(&id, OpStatus::Succeeded, None)
        .await
        .unwrap();
    let record = status(&env, &id).await;
    assert_eq!(record.status, OpStatus::Succeeded);
    assert!(record.finished_at.is_some());
    // Le service est rendu.
    env.ops
        .accept(request(service(1), OpKind::Start))
        .await
        .unwrap();
    let _ = other;
}

#[tokio::test]
async fn the_journal_keeps_every_step_in_order_with_its_intent_before_its_done() {
    let env = Env::new().await;
    let id = running(&env, 1, OpKind::Update).await;
    env.ops.done(&id, "check", &none()).await.unwrap();
    env.ops.intent(&id, "stop_old", &none()).await.unwrap();
    env.ops.done(&id, "stop_old", &none()).await.unwrap();
    env.ops.intent(&id, "rename_old", &none()).await.unwrap();
    env.ops.failed(&id, "rename_old").await.unwrap();
    let rows = env.journal.steps(&id).await.unwrap();
    let flat: Vec<(u32, &str, StepState)> = rows
        .iter()
        .map(|row| (row.seq, row.step.as_str(), row.state))
        .collect();
    assert_eq!(
        flat,
        [
            (1, "check", StepState::Done),
            (2, "stop_old", StepState::Intent),
            (3, "stop_old", StepState::Done),
            (4, "rename_old", StepState::Intent),
            (5, "rename_old", StepState::Failed),
        ]
    );
    assert_eq!(status(&env, &id).await.step, Some(name("rename_old")));
}

#[tokio::test]
async fn writes_out_of_order_or_outside_the_plan_are_refused() {
    let env = Env::new().await;
    let id = running(&env, 1, OpKind::Update).await;
    // `done` d'un geste destructif sans `intent`.
    assert!(matches!(
        env.ops.done(&id, "stop_old", &none()).await,
        Err(OpError::OutOfOrder)
    ));
    // Une étape qui n'est pas au plan, ou dont le nom n'est pas un code.
    assert!(matches!(
        env.ops.intent(&id, "mystery", &none()).await,
        Err(OpError::UnknownStep(_))
    ));
    assert!(matches!(
        env.ops.intent(&id, "Bad Name", &none()).await,
        Err(OpError::UnknownStep(_))
    ));
    // Un objet créé qui sort de sa racine.
    let escaping = UndoInfo {
        created: vec![CreatedObject {
            root: ObjectRoot::Backups,
            rel: "../outside".into(),
            dev: 1,
            ino: 1,
            kind: ObjectKind::PartialArchive,
        }],
    };
    assert!(matches!(
        env.ops.intent(&id, "create_new", &escaping).await,
        Err(OpError::OutOfOrder)
    ));
    // `done` de ce qui était `failed`.
    env.ops.intent(&id, "stop_old", &none()).await.unwrap();
    env.ops.failed(&id, "stop_old").await.unwrap();
    assert!(matches!(
        env.ops.done(&id, "stop_old", &none()).await,
        Err(OpError::OutOfOrder)
    ));
    // Opération inconnue.
    assert!(matches!(
        env.ops.intent("NOPE", "stop_old", &none()).await,
        Err(OpError::Unknown)
    ));
    // Une opération terminée n'écrit plus.
    env.ops
        .finish(&id, OpStatus::Failed, Some(Cause::StepFailed))
        .await
        .unwrap();
    assert!(matches!(
        env.ops.intent(&id, "start_new", &none()).await,
        Err(OpError::OutOfOrder)
    ));
    assert!(matches!(
        env.ops.failed(&id, "start_new").await,
        Err(OpError::OutOfOrder)
    ));
    // Un statut non terminal ne se pose pas par `finish`, une transition interdite non plus.
    let other = running(&env, 2, OpKind::Update).await;
    assert!(matches!(
        env.ops.finish(&other, OpStatus::Running, None).await,
        Err(OpError::IllegalTransition)
    ));
    assert!(matches!(
        env.ops.finish(&other, OpStatus::RolledBack, None).await,
        Err(OpError::IllegalTransition)
    ));
}

#[tokio::test]
async fn the_unique_index_is_the_belt_when_the_memory_lock_has_a_defect() {
    let env = Env::new().await;
    let first = env
        .ops
        .accept(request(service(1), OpKind::Update))
        .await
        .unwrap();
    // Un second « agent » (verrous en mémoire vides) sur la même base : la base refuse, et ses verrous
    // sont rendus.
    let second = env.restarted();
    let refused = second
        .accept(request(service(1), OpKind::Stop))
        .await
        .unwrap_err();
    assert!(matches!(refused, OpError::ServiceBusy), "{refused:?}");
    second
        .locks()
        .with(|table| assert_eq!(table.held_services(), 0));
    assert_eq!(status(&env, &first).await.status, OpStatus::Accepted);
}

#[tokio::test]
async fn a_name_being_deleted_is_reserved_and_a_file_write_is_refused_under_an_exclusive_lock() {
    let env = Env::new().await;
    let mut delete = request(service(1), OpKind::Delete);
    delete.reserve_name = Some("terraria".into());
    let id = env.ops.accept(delete).await.unwrap();
    // Un déploiement du même nom est refusé, avec le code de la suppression.
    let mut deploy = request(service(2), OpKind::Deploy);
    deploy.reserve_name = Some("terraria".into());
    let refused = env.ops.accept(deploy).await.unwrap_err();
    let OpError::Locked(refusal) = refused else {
        panic!("un verrou était attendu");
    };
    assert_eq!(refusal, Refusal::NameReserved { by: OpKind::Delete });
    assert_eq!(refusal.error_code(), ErrorCode::NameBeingDeleted);
    // Le refus du nom n'a laissé aucun verrou de service derrière lui.
    env.ops.locks().with(|table| {
        assert_eq!(table.held_services(), 1);
        // BR-FILES-024 : une écriture de fichier est refusée sous le verrou exclusif ; une lecture passe.
        assert!(table.acquire_shared_write(service(1).as_str()).is_err());
        assert_eq!(
            table.check_read(service(1).as_str(), ReadKind::Plain),
            Ok(())
        );
    });
    // La fin de la suppression libère le nom ET le service.
    env.ops.start(&id).await.unwrap();
    env.ops
        .finish(&id, OpStatus::Succeeded, None)
        .await
        .unwrap();
    let mut deploy = request(service(2), OpKind::Deploy);
    deploy.reserve_name = Some("terraria".into());
    env.ops.accept(deploy).await.unwrap();
    env.ops
        .locks()
        .with(|table| assert!(table.acquire_shared_write(service(1).as_str()).is_ok()));
}

#[tokio::test]
async fn heavy_io_has_one_seat_for_the_whole_server() {
    let env = Env::new().await;
    let mut backup = request(service(1), OpKind::Backup);
    backup.heavy_io = true;
    let id = env.ops.accept(backup).await.unwrap();
    let mut restore = request(service(2), OpKind::Restore);
    restore.heavy_io = true;
    let refused = env.ops.accept(restore).await.unwrap_err();
    let OpError::Locked(refusal) = refused else {
        panic!("un verrou était attendu");
    };
    assert_eq!(refusal.error_code(), ErrorCode::HeavyIoBusy);
    env.ops
        .locks()
        .with(|table| assert_eq!(table.held_services(), 1));
    env.ops.start(&id).await.unwrap();
    env.ops
        .finish(&id, OpStatus::Succeeded, None)
        .await
        .unwrap();
    let mut restore = request(service(2), OpKind::Restore);
    restore.heavy_io = true;
    env.ops.accept(restore).await.unwrap();
}

// ---------------------------------------------------------------------------------------------
// L'intent est sur le disque AVANT le geste
// ---------------------------------------------------------------------------------------------

fn copy_database(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    // Un dossier de données porte les droits 0700 (l'agent refuse un dossier ouvert qui a du contenu).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(to, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let file = entry.file_name().to_string_lossy().into_owned();
        if entry.path().is_file() && file.starts_with("hearth.db") {
            std::fs::copy(entry.path(), to.join(&file)).unwrap();
        }
    }
}

#[tokio::test]
async fn an_intent_is_committed_to_the_files_before_the_call_returns() {
    let env = Env::new().await;
    let id = running(&env, 1, OpKind::Update).await;
    env.ops.intent(&id, "stop_old", &none()).await.unwrap();
    // Ce que verrait un agent qui redémarre juste après un arrêt brutal : les fichiers tels quels (base et
    // journal d'écriture), sans la connexion qui les a écrits.
    let crashed = env.dir.path().join("crashed");
    copy_database(env.dir.path(), &crashed);
    let reopened = Database::open(&crashed).await.unwrap();
    let journal = SqliteOpJournal::new(reopened.pool().clone());
    let rows = journal.steps(&id).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].step.as_str(), rows[0].state),
        ("stop_old", StepState::Intent)
    );
    reopened.pool().close().await;
    // Le régime d'écriture normal est rétabli sur la connexion après le geste.
    let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(synchronous, 1, "NORMAL");
}

// ---------------------------------------------------------------------------------------------
// Au démarrage de l'agent : reprendre ou signaler, jamais rejouer
// ---------------------------------------------------------------------------------------------

async fn rows_of(env: &Env, id: &str) -> Vec<(u32, String, StepState)> {
    env.journal
        .steps(id)
        .await
        .unwrap()
        .into_iter()
        .map(|row| (row.seq, row.step.as_str().to_owned(), row.state))
        .collect()
}

#[tokio::test]
async fn without_an_intent_on_a_destructive_step_the_operation_is_interrupted_and_the_service_is_free()
 {
    let env = Env::new().await;
    let id = running(&env, 1, OpKind::Update).await;
    env.ops.done(&id, "check", &none()).await.unwrap();
    let before = rows_of(&env, &id).await;

    let restarted = env.restarted();
    let observer = Observer::saying(Observed::Unreadable);
    let summary = restarted.recover_at_startup(&observer).await.unwrap();
    assert_eq!((summary.interrupted, summary.needs_attention), (1, 0));
    let record = status(&env, &id).await;
    assert_eq!(
        (record.status, record.cause),
        (OpStatus::Interrupted, Some(Cause::NotExecuted))
    );
    assert!(record.finished_at.is_some());
    assert_eq!(record.decision, None);
    assert_eq!(
        rows_of(&env, &id).await,
        before,
        "rien n'a été écrit au journal : rien n'est rejoué"
    );
    // Le verrou est rendu : le service peut recevoir une autre opération.
    restarted
        .accept(request(service(1), OpKind::Start))
        .await
        .unwrap();
}

#[tokio::test]
async fn an_intent_without_done_needs_attention_keeps_the_lock_and_asks_the_account() {
    let env = Env::new().await;
    let id = running(&env, 1, OpKind::Update).await;
    env.ops.intent(&id, "stop_old", &none()).await.unwrap();
    env.ops.done(&id, "stop_old", &none()).await.unwrap();
    env.ops.intent(&id, "rename_old", &none()).await.unwrap();
    let before = rows_of(&env, &id).await;

    let restarted = env.restarted();
    // Même si le moteur dit « un seul état complet », un geste sans `done` ne se devine pas.
    let observer = Observer::saying(Observed::Coherent(ConfigSide::Before));
    let summary = restarted.recover_at_startup(&observer).await.unwrap();
    assert_eq!((summary.interrupted, summary.needs_attention), (0, 1));
    let record = status(&env, &id).await;
    assert_eq!(record.status, OpStatus::NeedsAttention);
    assert_eq!(record.cause, Some(Cause::IntentWithoutDone));
    assert_eq!(record.finished_at, None, "jamais purgée");
    let offer = record.decision.unwrap();
    assert_eq!(
        offer.choices,
        [Choice::Revert, Choice::Keep, Choice::Verified]
    );
    assert_eq!(
        rows_of(&env, &id).await,
        before,
        "ni rejoué, ni défait, ni terminé"
    );
    // Le verrou du service est REPRIS : aucune autre opération, aucune écriture de fichier.
    let refused = restarted
        .accept(request(service(1), OpKind::Stop))
        .await
        .unwrap_err();
    assert!(
        matches!(refused, OpError::Locked(Refusal::ServiceBusy { .. })),
        "{refused:?}"
    );
    restarted
        .locks()
        .with(|table| assert!(table.acquire_shared_write(service(1).as_str()).is_err()));
}

#[tokio::test]
async fn two_states_or_an_unreadable_engine_need_attention_and_one_complete_state_is_just_interrupted()
 {
    for (observed, expected_status, expected_cause) in [
        (
            Observed::Coherent(ConfigSide::Before),
            OpStatus::Interrupted,
            Cause::RunsWithPrevious,
        ),
        (
            Observed::Coherent(ConfigSide::After),
            OpStatus::Interrupted,
            Cause::RunsWithNew,
        ),
        (
            Observed::Ambiguous,
            OpStatus::NeedsAttention,
            Cause::TwoStates,
        ),
        (
            Observed::Unreadable,
            OpStatus::NeedsAttention,
            Cause::StateUnknown,
        ),
    ] {
        let env = Env::new().await;
        let id = running(&env, 1, OpKind::Update).await;
        env.ops.intent(&id, "stop_old", &none()).await.unwrap();
        env.ops.done(&id, "stop_old", &none()).await.unwrap();
        let restarted = env.restarted();
        let observer = Observer::saying(observed);
        restarted.recover_at_startup(&observer).await.unwrap();
        assert_eq!(observer.asked.load(Ordering::SeqCst), 1);
        let record = status(&env, &id).await;
        assert_eq!(
            (record.status, record.cause),
            (expected_status, Some(expected_cause)),
            "{observed:?}"
        );
        let lock_held = restarted
            .accept(request(service(1), OpKind::Start))
            .await
            .is_err();
        assert_eq!(
            lock_held,
            expected_status == OpStatus::NeedsAttention,
            "{observed:?}"
        );
    }
}

#[tokio::test]
async fn an_operation_to_verify_stays_so_across_restarts_and_waits_for_the_account() {
    let env = Env::new().await;
    let id = running(&env, 1, OpKind::Update).await;
    env.ops.intent(&id, "create_new", &none()).await.unwrap();
    env.restarted()
        .recover_at_startup(&Observer::saying(Observed::Ambiguous))
        .await
        .unwrap();
    let first = status(&env, &id).await;
    env.clock.advance(Duration::days(3));

    // Un second redémarrage ne la reclasse pas, ne la purge pas, ne change ni sa cause ni sa date.
    let again = env.restarted();
    let observer = Observer::saying(Observed::Coherent(ConfigSide::After));
    let summary = again.recover_at_startup(&observer).await.unwrap();
    assert_eq!(
        (
            summary.locks_restored,
            summary.needs_attention,
            summary.interrupted
        ),
        (1, 0, 0)
    );
    assert_eq!(
        observer.asked.load(Ordering::SeqCst),
        0,
        "le moteur n'est pas reconsulté"
    );
    let second = status(&env, &id).await;
    assert_eq!(
        (second.status, second.cause, second.updated_at),
        (first.status, first.cause, first.updated_at)
    );
    assert!(
        again
            .accept(request(service(1), OpKind::Stop))
            .await
            .is_err()
    );
    let swept = again.sweep().await.unwrap();
    assert_eq!(
        swept.purged, 0,
        "une opération à vérifier n'est jamais purgée"
    );
    assert_eq!(status(&env, &id).await.status, OpStatus::NeedsAttention);

    // Le compte tranche : « j'ai vérifié ». Le service est libre.
    assert_eq!(
        again.decide(&id, Choice::Verified).await.unwrap(),
        OpStatus::Failed
    );
    let done = status(&env, &id).await;
    assert_eq!(
        (done.status, done.cause),
        (OpStatus::Failed, Some(Cause::DecidedByAccount))
    );
    again
        .accept(request(service(1), OpKind::Start))
        .await
        .unwrap();
    // Un choix qui n'a pas été offert est refusé.
    let other = running(&env, 5, OpKind::Delete).await;
    env.ops.intent(&other, "stop_old", &none()).await.unwrap();
    let restarted = env.restarted();
    restarted
        .recover_at_startup(&Observer::saying(Observed::Ambiguous))
        .await
        .unwrap();
    assert!(matches!(
        restarted.decide(&other, Choice::Revert).await,
        Err(OpError::Choice(ChoiceRefused::NotOffered))
    ));
}

#[tokio::test]
async fn a_destructive_gesture_is_never_replayed_by_the_startup() {
    let env = Env::new().await;
    let ids: Vec<String> = {
        let mut ids = Vec::new();
        for n in 1..=4 {
            let id = running(&env, n, OpKind::Update).await;
            if n >= 2 {
                env.ops.intent(&id, "stop_old", &none()).await.unwrap();
            }
            if n >= 3 {
                env.ops.done(&id, "stop_old", &none()).await.unwrap();
            }
            if n == 4 {
                env.ops.intent(&id, "rename_old", &none()).await.unwrap();
            }
            ids.push(id);
        }
        ids
    };
    let mut before = Vec::new();
    for id in &ids {
        before.push(rows_of(&env, id).await);
    }
    let observer = Observer::saying(Observed::Ambiguous);
    env.restarted().recover_at_startup(&observer).await.unwrap();
    for (id, rows) in ids.iter().zip(&before) {
        assert_eq!(
            &rows_of(&env, id).await,
            rows,
            "le journal de {id} n'a pas bougé"
        );
        let record = status(&env, id).await;
        assert!(matches!(
            record.status,
            OpStatus::Interrupted | OpStatus::NeedsAttention
        ));
    }
}

// ---------------------------------------------------------------------------------------------
// Seuls les objets créés par l'opération sont retirés, revérifiés par leur inode
// ---------------------------------------------------------------------------------------------

#[cfg(unix)]
fn identity(path: &Path) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(path).unwrap();
    (meta.dev(), meta.ino())
}

#[cfg(not(unix))]
fn identity(_path: &Path) -> (u64, u64) {
    (0, 0)
}

#[tokio::test]
async fn only_what_the_operation_created_is_removed_and_only_if_it_is_still_that_object() {
    let env = Env::new().await;
    let backups = env.dir.path().join("backups");
    std::fs::create_dir_all(&backups).unwrap();
    let partial = backups.join("01J.tar.gz.part");
    let replaced = backups.join("02K.tar.gz.part");
    let bystander = backups.join("not-ours.tar.gz");
    for file in [&partial, &replaced, &bystander] {
        std::fs::write(file, b"data").unwrap();
    }
    let object = |file: &Path, rel: &str| CreatedObject {
        root: ObjectRoot::Backups,
        rel: rel.into(),
        dev: identity(file).0,
        ino: identity(file).1,
        kind: ObjectKind::PartialArchive,
    };
    let plan = {
        let step = |text: &str, destructive| PlannedStep {
            name: name(text),
            destructive,
        };
        OpPlan::new(vec![step("archive", true), step("register", true)], None).unwrap()
    };

    // Deux opérations interrompues en écrivant une archive.
    let mut first = request(service(1), OpKind::Backup);
    first.plan = plan.clone();
    let first = env.ops.accept(first).await.unwrap();
    env.ops.start(&first).await.unwrap();
    env.ops
        .intent(
            &first,
            "archive",
            &UndoInfo {
                created: vec![object(&partial, "01J.tar.gz.part")],
            },
        )
        .await
        .unwrap();
    let mut second = request(service(2), OpKind::Backup);
    second.plan = plan;
    let second = env.ops.accept(second).await.unwrap();
    env.ops.start(&second).await.unwrap();
    // L'opération note l'archive, puis quelqu'un la remplace par un autre fichier (autre inode).
    let mut stale = object(&replaced, "02K.tar.gz.part");
    stale.ino = stale.ino.wrapping_add(1);
    env.ops
        .intent(
            &second,
            "archive",
            &UndoInfo {
                created: vec![stale],
            },
        )
        .await
        .unwrap();

    let summary = env
        .restarted()
        .recover_at_startup(&Observer::saying(Observed::Unreadable))
        .await
        .unwrap();
    assert!(
        bystander.exists(),
        "ce que le journal ne nomme pas n'est jamais touché"
    );
    #[cfg(unix)]
    {
        assert!(
            !partial.exists(),
            "l'archive partielle de l'opération est retirée"
        );
        assert_eq!(summary.objects_removed, 1);
        assert!(
            replaced.exists(),
            "un fichier dont l'inode a changé n'est pas retiré"
        );
        assert_eq!(summary.objects_left, 1);
    }
    #[cfg(not(unix))]
    {
        // Sans numéro d'inode, rien n'est retiré : refus de deviner.
        assert!(partial.exists() && replaced.exists());
        assert_eq!((summary.objects_removed, summary.objects_left), (0, 2));
    }
    // Les deux opérations sont à vérifier (un `intent` sans `done`), pas terminées.
    assert_eq!(status(&env, &first).await.status, OpStatus::NeedsAttention);
    assert_eq!(status(&env, &second).await.status, OpStatus::NeedsAttention);
    // Rejouer le démarrage ne casse rien (déjà retiré ou laissé).
    let _ = env
        .restarted()
        .recover_at_startup(&Observer::saying(Observed::Unreadable))
        .await
        .unwrap();
}

// ---------------------------------------------------------------------------------------------
// Annuler, décider, expirer
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn cancel_works_before_the_point_of_no_return_and_answers_too_late_after_it() {
    let env = Env::new().await;
    // Rien commencé : annulée d'un coup, le service est libre.
    let fresh = env
        .ops
        .accept(request(service(1), OpKind::Update))
        .await
        .unwrap();
    assert_eq!(
        env.ops.cancel(&fresh).await.unwrap(),
        CancelOutcome::Cancelled
    );
    let record = status(&env, &fresh).await;
    assert_eq!(
        (record.status, record.cause),
        (OpStatus::Cancelled, Some(Cause::CancelledByAccount))
    );
    env.ops
        .accept(request(service(1), OpKind::Start))
        .await
        .unwrap();

    // En cours, avant la création du nouveau conteneur : elle passe en annulation.
    let id = running(&env, 2, OpKind::Update).await;
    env.ops.intent(&id, "stop_old", &none()).await.unwrap();
    env.ops.done(&id, "stop_old", &none()).await.unwrap();
    assert_eq!(
        env.ops.cancel(&id).await.unwrap(),
        CancelOutcome::Cancelling
    );
    assert_eq!(status(&env, &id).await.status, OpStatus::Cancelling);
    assert_eq!(
        env.ops.cancel(&id).await.unwrap(),
        CancelOutcome::NotCancellable,
        "déjà en annulation"
    );
    env.ops
        .finish(&id, OpStatus::Cancelled, Some(Cause::CancelledByAccount))
        .await
        .unwrap();

    // Après le point de non-retour : trop tard, et la réponse a le code du protocole.
    let late = running(&env, 3, OpKind::Update).await;
    env.ops.intent(&late, "create_new", &none()).await.unwrap();
    assert_eq!(env.ops.cancel(&late).await.unwrap(), CancelOutcome::TooLate);
    assert_eq!(
        status(&env, &late).await.status,
        OpStatus::Running,
        "rien n'a bougé"
    );
    assert_eq!(ErrorCode::TooLateToCancel.http_status(), 409);
    // Une opération terminée ne s'annule pas.
    env.ops.done(&late, "create_new", &none()).await.unwrap();
    env.ops
        .finish(&late, OpStatus::Succeeded, None)
        .await
        .unwrap();
    assert_eq!(
        env.ops.cancel(&late).await.unwrap(),
        CancelOutcome::NotCancellable
    );
}

#[tokio::test]
async fn a_pending_decision_expires_after_fifteen_minutes_towards_cancelled() {
    let env = Env::new().await;
    let id = running(&env, 1, OpKind::Update).await;
    env.ops
        .ask_decision(&id, &[Choice::Continue, Choice::Cancel])
        .await
        .unwrap();
    let waiting = status(&env, &id).await;
    assert_eq!(waiting.status, OpStatus::AwaitingDecision);
    assert_eq!(
        waiting.decision.as_ref().unwrap().choices,
        [Choice::Continue, Choice::Cancel]
    );
    // Le verrou est tenu pendant l'attente.
    assert!(
        env.ops
            .accept(request(service(1), OpKind::Stop))
            .await
            .is_err()
    );

    env.clock
        .advance(Duration::minutes(14) + Duration::seconds(59));
    assert_eq!(env.ops.sweep().await.unwrap().expired, 0);
    assert_eq!(status(&env, &id).await.status, OpStatus::AwaitingDecision);

    env.clock.advance(Duration::seconds(1));
    assert_eq!(env.ops.sweep().await.unwrap().expired, 1);
    let expired = status(&env, &id).await;
    assert_eq!(
        (expired.status, expired.cause),
        (OpStatus::Cancelled, Some(Cause::DecisionExpired))
    );
    assert_eq!(expired.decision, None);
    // Le service est libre, la décision tardive est refusée, un second balayage ne refait rien.
    env.ops
        .accept(request(service(1), OpKind::Start))
        .await
        .unwrap();
    assert!(matches!(
        env.ops.decide(&id, Choice::Continue).await,
        Err(OpError::Choice(ChoiceRefused::NotWaiting))
    ));
    assert_eq!(env.ops.sweep().await.unwrap().expired, 0);
}

#[tokio::test]
async fn a_decision_before_the_expiry_resumes_or_cancels_the_operation() {
    let env = Env::new().await;
    let id = running(&env, 1, OpKind::Update).await;
    env.ops
        .ask_decision(&id, &[Choice::Continue, Choice::Cancel])
        .await
        .unwrap();
    // Un choix hors liste est refusé.
    assert!(matches!(
        env.ops.decide(&id, Choice::Revert).await,
        Err(OpError::Choice(ChoiceRefused::NotOffered))
    ));
    env.clock.advance(Duration::minutes(10));
    assert_eq!(
        env.ops.decide(&id, Choice::Continue).await.unwrap(),
        OpStatus::Running
    );
    let record = status(&env, &id).await;
    assert_eq!((record.status, record.decision), (OpStatus::Running, None));
    // Et le balayage ne touche plus à une opération qui a repris.
    env.clock.advance(Duration::hours(1));
    assert_eq!(env.ops.sweep().await.unwrap().expired, 0);

    let again = running(&env, 2, OpKind::Update).await;
    env.ops
        .ask_decision(&again, &[Choice::Continue, Choice::Cancel])
        .await
        .unwrap();
    assert_eq!(
        env.ops.decide(&again, Choice::Cancel).await.unwrap(),
        OpStatus::Cancelled
    );
    assert_eq!(
        status(&env, &again).await.cause,
        Some(Cause::CancelledByAccount)
    );
    // Pas de décision sans choix offert.
    let third = running(&env, 3, OpKind::Update).await;
    assert!(env.ops.ask_decision(&third, &[]).await.is_err());
}

#[tokio::test]
async fn finished_operations_are_purged_after_a_day_and_never_the_ones_still_open() {
    let env = Env::new().await;
    let done = running(&env, 1, OpKind::Update).await;
    env.ops.intent(&done, "stop_old", &none()).await.unwrap();
    env.ops.done(&done, "stop_old", &none()).await.unwrap();
    env.ops
        .finish(&done, OpStatus::Succeeded, None)
        .await
        .unwrap();
    let open = running(&env, 2, OpKind::Update).await;
    env.ops.intent(&open, "stop_old", &none()).await.unwrap();

    env.clock.advance(Duration::hours(23));
    assert_eq!(env.ops.sweep().await.unwrap().purged, 0);
    env.clock.advance(Duration::hours(2));
    assert_eq!(env.ops.sweep().await.unwrap().purged, 1);
    assert!(env.journal.get(&done).await.unwrap().is_none());
    assert!(
        env.journal.steps(&done).await.unwrap().is_empty(),
        "son journal part avec elle"
    );
    assert_eq!(status(&env, &open).await.status, OpStatus::Running);
    assert_eq!(env.journal.steps(&open).await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_status_change_only_succeeds_from_the_status_the_caller_saw() {
    let env = Env::new().await;
    let id = running(&env, 1, OpKind::Update).await;
    let stale = env
        .journal
        .transition(
            &id,
            OpStatus::Accepted,
            OpStatus::Running,
            None,
            None,
            env.clock.now(),
        )
        .await;
    assert!(matches!(stale, Err(JournalError::Stale)));
    let unknown = env
        .journal
        .transition(
            "NOPE",
            OpStatus::Running,
            OpStatus::Failed,
            None,
            None,
            env.clock.now(),
        )
        .await;
    assert!(matches!(unknown, Err(JournalError::Unknown)));
    // La progression se range et se relit.
    env.ops
        .set_progress(
            &id,
            &Progress {
                percent: Some(40),
                bytes_done: Some(4),
                bytes_total: Some(10),
                items_done: None,
            },
        )
        .await
        .unwrap();
    let progress = status(&env, &id).await.progress;
    assert_eq!(
        (progress.percent, progress.bytes_done, progress.bytes_total),
        (Some(40), Some(4), Some(10))
    );
}

// ---------------------------------------------------------------------------------------------
// Une ligne illisible du journal n'empêche JAMAIS l'agent de démarrer (revue HRT-83, point 1)
// ---------------------------------------------------------------------------------------------

/// Écrit à la main dans la base en désactivant les contraintes (c'est le cas d'un agent plus récent qui a
/// écrit autre chose, d'une restauration partielle ou d'une base éditée).
async fn corrupt(env: &Env, sql: &str) {
    let mut conn = env.db.pool().acquire().await.unwrap();
    sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()))
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query("PRAGMA ignore_check_constraints = OFF")
        .execute(&mut *conn)
        .await
        .unwrap();
}

/// Une colonne d'étape illisible : l'opération est « à vérifier » (cause « état inconnu »), son verrou est
/// repris, rien n'est rejoué ni retiré, et le démarrage réussit.
#[tokio::test]
async fn an_unreadable_step_column_makes_the_operation_to_verify_and_the_agent_still_starts() {
    for (column, value) in [
        ("seq", "-5"),
        ("step", "'étape inconnue!'"),
        ("state", "'weird'"),
        ("undo_json", "'not json'"),
        ("at", "'garbage'"),
    ] {
        let env = Env::new().await;
        let id = running(&env, 1, OpKind::Update).await;
        env.ops.intent(&id, "stop_old", &none()).await.unwrap();
        env.ops.done(&id, "stop_old", &none()).await.unwrap();
        corrupt(
            &env,
            &format!(
                "UPDATE service_op_steps SET {column} = {value} WHERE op_id = '{id}' AND seq = 1"
            ),
        )
        .await;

        let restarted = env.restarted();
        let observer = Observer::saying(Observed::Coherent(ConfigSide::Before));
        let summary = restarted
            .recover_at_startup(&observer)
            .await
            .unwrap_or_else(|error| panic!("{column} : le démarrage a échoué : {error}"));
        assert_eq!(
            (summary.needs_attention, summary.unreadable),
            (1, 1),
            "{column}"
        );
        assert_eq!(observer.asked.load(Ordering::SeqCst), 0, "{column}");
        let record = status(&env, &id).await;
        assert_eq!(record.status, OpStatus::NeedsAttention, "{column}");
        assert_eq!(record.cause, Some(Cause::StateUnknown), "{column}");
        assert_eq!(record.finished_at, None, "{column}");
        let refused = restarted
            .accept(request(service(1), OpKind::Stop))
            .await
            .unwrap_err();
        assert!(
            matches!(refused, OpError::Locked(Refusal::ServiceBusy { .. })),
            "{column} : {refused:?}"
        );
    }
}

/// Une colonne de l'opération elle-même illisible : la ligne est isolée (dite, jamais rejouée), l'agent
/// démarre, et tant que la ligne tient le service (statut et clé lisibles), le service reste refusé par
/// l'index unique de la base.
#[tokio::test]
async fn an_unreadable_operation_column_isolates_the_row_and_the_agent_still_starts() {
    for (column, value, still_holds_the_service) in [
        ("kind", "'weird'", true),
        ("plan_json", "'not json'", true),
        ("step", "'étape inconnue!'", true),
        ("decision_json", "'not json'", true),
        ("progress_json", "'not json'", true),
        ("cause", "'weird'", true),
        ("started_at", "'garbage'", true),
        ("updated_at", "'garbage'", true),
        ("finished_at", "'garbage'", true),
        ("service_key", "'not a key'", false),
        ("status", "'weird'", false),
    ] {
        let env = Env::new().await;
        let id = running(&env, 1, OpKind::Update).await;
        env.ops.intent(&id, "stop_old", &none()).await.unwrap();
        let steps_before = rows_of(&env, &id).await;
        corrupt(
            &env,
            &format!("UPDATE service_ops SET {column} = {value} WHERE id = '{id}'"),
        )
        .await;

        let restarted = env.restarted();
        let observer = Observer::saying(Observed::Coherent(ConfigSide::Before));
        let summary = restarted
            .recover_at_startup(&observer)
            .await
            .unwrap_or_else(|error| panic!("{column} : le démarrage a échoué : {error}"));
        assert_eq!(
            (summary.interrupted, summary.needs_attention),
            (0, 0),
            "{column}"
        );
        assert_eq!(
            rows_of(&env, &id).await,
            steps_before,
            "{column} : rien n'est rejoué"
        );
        if still_holds_the_service {
            let refused = restarted
                .accept(request(service(1), OpKind::Stop))
                .await
                .unwrap_err();
            assert!(
                matches!(refused, OpError::ServiceBusy),
                "{column} : {refused:?}"
            );
        }
    }
}

/// Même un journal entièrement inutilisable (table absente) ne bloque pas le démarrage de l'agent : la
/// racine de composition le dit et continue.
#[tokio::test]
async fn the_composition_root_starts_even_when_the_whole_journal_is_unreadable() {
    use hearth_agent::app::service_foundations;
    let env = Env::new().await;
    let id = running(&env, 1, OpKind::Update).await;
    env.ops.intent(&id, "stop_old", &none()).await.unwrap();
    corrupt(
        &env,
        &format!("UPDATE service_op_steps SET at = 'garbage' WHERE op_id = '{id}'"),
    )
    .await;
    let foundations =
        service_foundations(env.dir.path(), &env.db, env.clock.clone(), env.ids.clone())
            .await
            .expect("l'agent démarre avec une ligne d'étape illisible");
    assert_eq!(status(&env, &id).await.status, OpStatus::NeedsAttention);
    drop(foundations);

    corrupt(&env, "ALTER TABLE service_ops RENAME TO service_ops_gone").await;
    service_foundations(env.dir.path(), &env.db, env.clock.clone(), env.ids.clone())
        .await
        .expect("l'agent démarre même sans lecture possible du journal");
}

/// Le nom réservé par une suppression « à vérifier » (BR-DEL-019) et la place des opérations lourdes
/// survivent au redémarrage ; « revenir » ne dit pas « revenu » : l'opération passe en retour arrière,
/// verrou tenu.
#[tokio::test]
async fn a_name_and_a_heavy_seat_survive_a_restart_and_reverting_only_starts_the_rollback() {
    let env = Env::new().await;
    let delete = env
        .ops
        .accept(request(service(1), OpKind::Delete))
        .await
        .unwrap();
    env.ops.start(&delete).await.unwrap();
    env.ops.intent(&delete, "stop_old", &none()).await.unwrap();
    let backup = env
        .ops
        .accept(request(service(2), OpKind::Backup))
        .await
        .unwrap();
    env.ops.start(&backup).await.unwrap();
    env.ops.intent(&backup, "stop_old", &none()).await.unwrap();
    let restarted = env.restarted();
    restarted
        .recover_at_startup(&Observer::saying(Observed::Ambiguous))
        .await
        .unwrap();
    restarted.locks().with(|table| {
        assert_eq!(table.name_reserved_by("terraria"), Some(OpKind::Delete));
        assert!(
            table
                .acquire_heavy_io(service(9).as_str(), "other")
                .is_err()
        );
    });

    // Une mise à jour à vérifier offre « revenir » : cela démarre un retour arrière, rien n'est dit fait.
    let update = running(&env, 3, OpKind::Update).await;
    env.ops.intent(&update, "stop_old", &none()).await.unwrap();
    let again = env.restarted();
    again
        .recover_at_startup(&Observer::saying(Observed::Ambiguous))
        .await
        .unwrap();
    assert_eq!(
        again.decide(&update, Choice::Revert).await.unwrap(),
        OpStatus::RollingBack
    );
    assert_eq!(status(&env, &update).await.finished_at, None);
    assert!(
        matches!(
            again.accept(request(service(3), OpKind::Stop)).await,
            Err(OpError::Locked(Refusal::ServiceBusy { .. }))
        ),
        "le verrou reste tenu pendant le retour arrière"
    );
}
