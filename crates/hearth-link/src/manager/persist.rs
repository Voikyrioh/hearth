//! Écriture sur disque hors de la boucle d'un serveur : la boucle ne fait jamais d'E/S disque
//! (une synchronisation de fichier peut durer des secondes).
//!
//! Une tâche écrit pour le serveur. Ce qu'on lui confie est **borné et « dernier état gagne »**
//! par type de fichier : une vue, un enregistrement ou une liste d'opérations plus récents
//! remplacent ceux qui attendent encore, au lieu de s'empiler. Disque pendu : la mémoire reste
//! constante (une vue, un enregistrement, une liste).
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

#[derive(Default)]
struct Waiting {
    view: Option<LastKnown>,
    record: Option<ServerRecord>,
    operations: Option<Vec<PendingOp>>,
    /// Ceux qui attendent la fin de l'écriture des opérations (résultat : écrit ou non).
    acks: Vec<oneshot::Sender<bool>>,
    /// Ceux qui attendent que tout soit écrit.
    flushes: Vec<oneshot::Sender<()>>,
}

struct Shared {
    waiting: Mutex<Waiting>,
    wake: Notify,
    closed: AtomicBool,
}

pub(crate) struct Persister {
    shared: Arc<Shared>,
}

impl Waiting {
    fn is_empty(&self) -> bool {
        self.view.is_none()
            && self.record.is_none()
            && self.operations.is_none()
            && self.acks.is_empty()
            && self.flushes.is_empty()
    }
}

impl Drop for Persister {
    fn drop(&mut self) {
        self.shared.closed.store(true, Ordering::SeqCst);
        self.shared.wake.notify_one();
    }
}

impl Persister {
    pub(crate) fn spawn(deps: Arc<Deps>, id: ServerId) -> Self {
        Self::spawn_with(Arc::new(DepsWriter { deps, id }))
    }

    pub(crate) fn spawn_with(writer: Arc<dyn Writer>) -> Self {
        let shared = Arc::new(Shared {
            waiting: Mutex::new(Waiting::default()),
            wake: Notify::new(),
            closed: AtomicBool::new(false),
        });
        let state = shared.clone();
        let writer = writer.clone();
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
                write(writer.as_ref(), work).await;
            }
        });
        Self { shared }
    }

    fn with(&self, change: impl FnOnce(&mut Waiting)) {
        change(
            &mut self
                .shared
                .waiting
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        self.shared.wake.notify_one();
    }

    pub(crate) fn save_view(&self, view: LastKnown) {
        self.with(|waiting| waiting.view = Some(view));
    }

    pub(crate) fn save_record(&self, record: ServerRecord) {
        self.with(|waiting| waiting.record = Some(record));
    }

    /// Sans attendre : la liste la plus récente remplace celle qui attend.
    pub(crate) fn save_operations(&self, operations: Vec<PendingOp>) {
        self.with(|waiting| waiting.operations = Some(operations));
    }

    /// Comme `save_operations`, avec un accusé : `true` quand cette liste (ou une plus récente) est
    /// écrite, `false` si l'écriture a échoué.
    pub(crate) fn save_operations_acked(
        &self,
        operations: Vec<PendingOp>,
    ) -> oneshot::Receiver<bool> {
        let (ack, written) = oneshot::channel();
        self.with(|waiting| {
            waiting.operations = Some(operations);
            waiting.acks.push(ack);
        });
        written
    }

    /// Attend (au plus `limit`) que tout ce qui est déposé soit écrit.
    pub(crate) async fn flush(&self, limit: Duration) {
        let (done, written) = oneshot::channel();
        self.with(|waiting| waiting.flushes.push(done));
        let _ = tokio::time::timeout(limit, written).await;
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

async fn write(writer: &dyn Writer, work: Waiting) {
    // Les opérations d'abord : des actions attendent leur accusé pour partir.
    let written = match &work.operations {
        Some(operations) => writer.operations(operations).await,
        None => true,
    };
    for ack in work.acks {
        let _ = ack.send(written);
    }
    if let Some(record) = &work.record {
        writer.record(record).await;
    }
    if let Some(view) = &work.view {
        writer.view(view).await;
    }
    for flush in work.flushes {
        let _ = flush.send(());
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
            let waiting = persister.shared.waiting.lock().unwrap();
            // Une liste, une vue : la file ne grossit pas avec le nombre de travaux.
            assert!(waiting.operations.is_some() && waiting.view.is_some());
            assert!(waiting.acks.is_empty() && waiting.flushes.is_empty());
        }
        writer.open.store(true, Ordering::SeqCst);
        writer.release.notify_waiters();
        writer.release.notify_one();
        persister.flush(std::time::Duration::from_secs(5)).await;
        // La liste en cours, puis la plus récente seulement ; une seule vue.
        assert_eq!(*writer.writes.lock().unwrap(), vec![1, 3]);
        assert_eq!(writer.views.load(Ordering::SeqCst), 1);
    }
}
