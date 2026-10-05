//! Écriture sur disque hors de la boucle d'un serveur : la boucle ne fait jamais d'E/S disque
//! (une synchronisation de fichier peut durer des secondes).
//!
//! Deux files, deux tâches par serveur : les opérations en suspens d'un côté, la dernière vue et
//! le carnet de l'autre. Une action attend l'accusé d'écriture de son suivi : il ne passe jamais
//! derrière l'écriture d'une vue qui traîne. Ce qu'on confie à une file est **borné et « dernier
//! état gagne »** par type de fichier : une vue, un enregistrement ou une liste d'opérations plus
//! récents remplacent ceux qui attendent encore, au lieu de s'empiler. Disque pendu : la mémoire
//! reste constante (une vue, un enregistrement, une liste).
//!
//! Les opérations en suspens ont en plus un accusé : [`Persister::save_operations_acked`] rend la
//! fin de l'écriture atomique, car une action ne part qu'une fois son suivi confirmé sur disque.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::{Notify, oneshot};

use super::Deps;
use crate::domain::pending_ops::PendingOp;
use crate::domain::server::{LastKnown, ServerId, ServerRecord};

/// Ce qu'une file attend d'un type de travail.
#[async_trait]
trait Work: Default + Send + 'static {
    fn is_empty(&self) -> bool;
    async fn write(self, writer: &dyn Writer);
}

/// Travail de la file des opérations : liste en suspens et accusés d'écriture.
#[derive(Default)]
struct OperationsWork {
    operations: Option<Vec<PendingOp>>,
    /// Ceux qui attendent la fin de l'écriture des opérations (résultat : écrit ou non).
    acks: Vec<oneshot::Sender<bool>>,
    flushes: Vec<oneshot::Sender<()>>,
}

/// Travail de la file des fichiers : dernière vue et carnet. Ces écritures peuvent être lentes
/// (disque saturé) : elles ne retardent jamais l'accusé d'une action.
#[derive(Default)]
struct FilesWork {
    view: Option<LastKnown>,
    record: Option<ServerRecord>,
    flushes: Vec<oneshot::Sender<()>>,
}

#[async_trait]
impl Work for OperationsWork {
    fn is_empty(&self) -> bool {
        self.operations.is_none() && self.acks.is_empty() && self.flushes.is_empty()
    }

    async fn write(self, writer: &dyn Writer) {
        let written = match &self.operations {
            Some(operations) => writer.operations(operations).await,
            None => true,
        };
        for ack in self.acks {
            let _ = ack.send(written);
        }
        for flush in self.flushes {
            let _ = flush.send(());
        }
    }
}

#[async_trait]
impl Work for FilesWork {
    fn is_empty(&self) -> bool {
        self.view.is_none() && self.record.is_none() && self.flushes.is_empty()
    }

    async fn write(self, writer: &dyn Writer) {
        if let Some(record) = &self.record {
            writer.record(record).await;
        }
        if let Some(view) = &self.view {
            writer.view(view).await;
        }
        for flush in self.flushes {
            let _ = flush.send(());
        }
    }
}

/// Une file : une tâche, un travail « dernier état gagne ».
struct Lane<W> {
    waiting: Mutex<W>,
    wake: Notify,
    closed: AtomicBool,
}

impl<W: Work> Lane<W> {
    fn spawn(writer: Arc<dyn Writer>) -> Arc<Self> {
        let lane = Arc::new(Self {
            waiting: Mutex::new(W::default()),
            wake: Notify::new(),
            closed: AtomicBool::new(false),
        });
        let state = lane.clone();
        tokio::spawn(async move {
            loop {
                let notified = state.wake.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                let work = std::mem::take(
                    &mut *state.waiting.lock().unwrap_or_else(PoisonError::into_inner),
                );
                if work.is_empty() {
                    if state.closed.load(Ordering::SeqCst) {
                        return;
                    }
                    notified.await;
                    continue;
                }
                work.write(writer.as_ref()).await;
            }
        });
        lane
    }

    fn with(&self, change: impl FnOnce(&mut W)) {
        change(&mut self.waiting.lock().unwrap_or_else(PoisonError::into_inner));
        self.wake.notify_one();
    }

    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.wake.notify_one();
    }
}

pub(crate) struct Persister {
    operations: Arc<Lane<OperationsWork>>,
    files: Arc<Lane<FilesWork>>,
}

impl Drop for Persister {
    fn drop(&mut self) {
        self.operations.close();
        self.files.close();
    }
}

impl Persister {
    pub(crate) fn spawn(deps: Arc<Deps>, id: ServerId) -> Self {
        Self::spawn_with(Arc::new(DepsWriter { deps, id }))
    }

    pub(crate) fn spawn_with(writer: Arc<dyn Writer>) -> Self {
        Self {
            operations: Lane::spawn(writer.clone()),
            files: Lane::spawn(writer),
        }
    }

    pub(crate) fn save_view(&self, view: LastKnown) {
        self.files.with(|waiting| waiting.view = Some(view));
    }

    pub(crate) fn save_record(&self, record: ServerRecord) {
        self.files.with(|waiting| waiting.record = Some(record));
    }

    /// Sans attendre : la liste la plus récente remplace celle qui attend.
    pub(crate) fn save_operations(&self, operations: Vec<PendingOp>) {
        self.operations
            .with(|waiting| waiting.operations = Some(operations));
    }

    /// Comme `save_operations`, avec un accusé : `true` quand cette liste (ou une plus récente) est
    /// écrite, `false` si l'écriture a échoué.
    pub(crate) fn save_operations_acked(
        &self,
        operations: Vec<PendingOp>,
    ) -> oneshot::Receiver<bool> {
        let (ack, written) = oneshot::channel();
        self.operations.with(|waiting| {
            waiting.operations = Some(operations);
            waiting.acks.push(ack);
        });
        written
    }

    /// Attend (au plus `limit`) que tout ce qui est déposé soit écrit.
    pub(crate) async fn flush(&self, limit: Duration) {
        let (done_ops, ops) = oneshot::channel();
        let (done_files, files) = oneshot::channel();
        self.operations
            .with(|waiting| waiting.flushes.push(done_ops));
        self.files.with(|waiting| waiting.flushes.push(done_files));
        let _ = tokio::time::timeout(limit, async {
            let _ = ops.await;
            let _ = files.await;
        })
        .await;
    }
}

/// Où les écritures vont : les ports de stockage en production, un faux dans les tests.
#[async_trait]
pub(crate) trait Writer: Send + Sync {
    async fn operations(&self, operations: &[PendingOp]) -> bool;
    async fn record(&self, record: &ServerRecord);
    async fn view(&self, view: &LastKnown);
}

struct DepsWriter {
    deps: Arc<Deps>,
    id: ServerId,
}

#[async_trait]
impl Writer for DepsWriter {
    async fn operations(&self, operations: &[PendingOp]) -> bool {
        match self.deps.operations.save(&self.id, operations).await {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(server = %self.id, %error, "opérations non sauvegardées");
                false
            }
        }
    }

    async fn record(&self, record: &ServerRecord) {
        if let Err(error) = self.deps.servers.save(record).await {
            tracing::warn!(server = %self.id, %error, "carnet non mis à jour");
        }
    }

    async fn view(&self, view: &LastKnown) {
        if let Err(error) = self.deps.snapshots.save(&self.id, view).await {
            tracing::warn!(server = %self.id, %error, "dernière vue non sauvegardée");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use super::*;
    use crate::domain::pending_ops::{OperationId, PendingOps};
    use crate::domain::time::WallTime;

    /// Écrivain qui reste « pendu » tant qu'on ne le libère pas.
    struct Hung {
        release: Notify,
        open: AtomicBool,
        writes: Mutex<Vec<usize>>,
        views: AtomicUsize,
    }

    #[async_trait]
    impl Writer for Hung {
        async fn operations(&self, operations: &[PendingOp]) -> bool {
            while !self.open.load(Ordering::SeqCst) {
                self.release.notified().await;
            }
            self.writes.lock().unwrap().push(operations.len());
            true
        }
        async fn record(&self, _: &ServerRecord) {}
        async fn view(&self, _: &LastKnown) {
            self.views.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn operations(n: usize) -> Vec<PendingOp> {
        let mut ops = PendingOps::new();
        for k in 0..n {
            ops.register(
                OperationId::parse(&format!("op{k}")).unwrap(),
                "x".into(),
                WallTime::from_millis(1),
            )
            .unwrap();
        }
        ops.snapshot()
    }

    #[tokio::test]
    async fn a_hung_disk_keeps_the_queue_constant_and_only_the_latest_state_is_written() {
        let writer = Arc::new(Hung {
            release: Notify::new(),
            open: AtomicBool::new(false),
            writes: Mutex::new(Vec::new()),
            views: AtomicUsize::new(0),
        });
        let persister = Persister::spawn_with(writer.clone());
        // La première liste part et se pend ; le reste s'accumule dans la file.
        persister.save_operations(operations(1));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        for n in 2..=5_000 {
            persister.save_operations(operations(n % 7));
            persister.save_view(LastKnown {
                at: WallTime::from_millis(n as i64),
                machine: None,
                history: vec![],
            });
        }
        persister.save_operations(operations(3));
        {
            let ops = persister.operations.waiting.lock().unwrap();
            let files = persister.files.waiting.lock().unwrap();
            // Une liste, une vue : la file ne grossit pas avec le nombre de travaux.
            assert!(ops.operations.is_some() && files.view.is_some());
            assert!(ops.acks.is_empty() && ops.flushes.is_empty() && files.flushes.is_empty());
        }
        writer.open.store(true, Ordering::SeqCst);
        writer.release.notify_waiters();
        writer.release.notify_one();
        persister.flush(std::time::Duration::from_secs(5)).await;
        // La liste en cours, puis la plus récente seulement ; une seule vue.
        assert_eq!(*writer.writes.lock().unwrap(), vec![1, 3]);
        assert_eq!(writer.views.load(Ordering::SeqCst), 1);
    }

    /// Écrivain dont la vue reste « pendue » : les opérations, elles, s'écrivent.
    struct SlowView {
        release: Notify,
        open: AtomicBool,
    }

    #[async_trait]
    impl Writer for SlowView {
        async fn operations(&self, _: &[PendingOp]) -> bool {
            true
        }
        async fn record(&self, _: &ServerRecord) {}
        async fn view(&self, _: &LastKnown) {
            while !self.open.load(Ordering::SeqCst) {
                self.release.notified().await;
            }
        }
    }

    #[tokio::test]
    async fn a_hung_view_write_never_delays_the_acknowledgement_of_an_action() {
        let writer = Arc::new(SlowView {
            release: Notify::new(),
            open: AtomicBool::new(false),
        });
        let persister = Persister::spawn_with(writer.clone());
        persister.save_view(LastKnown {
            at: WallTime::from_millis(1),
            machine: None,
            history: vec![],
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let acked = persister.save_operations_acked(operations(1));
        let answer = tokio::time::timeout(std::time::Duration::from_millis(500), acked).await;
        assert!(matches!(answer, Ok(Ok(true))), "{answer:?}");
        writer.open.store(true, Ordering::SeqCst);
        writer.release.notify_waiters();
        writer.release.notify_one();
    }
}
