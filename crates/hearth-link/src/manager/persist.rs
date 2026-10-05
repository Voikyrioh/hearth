//! Écriture sur disque hors de la boucle d'un serveur : la boucle ne fait jamais d'E/S disque
//! (une synchronisation de fichier peut durer des secondes). Elle dépose des travaux dans une
//! file ; une tâche les exécute dans l'ordre. Une file par serveur : les écritures d'un même
//! fichier ne s'entrelacent jamais.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};

use super::Deps;
use crate::domain::pending_ops::PendingOp;
use crate::domain::server::{LastKnown, ServerId, ServerRecord};

pub(crate) enum Job {
    View(LastKnown),
    Record(ServerRecord),
    Operations(Vec<PendingOp>),
    /// Rend la main quand tout ce qui précède est écrit.
    Flush(oneshot::Sender<()>),
}

pub(crate) struct Persister {
    jobs: mpsc::UnboundedSender<Job>,
}

impl Persister {
    pub(crate) fn spawn(deps: Arc<Deps>, id: ServerId) -> Self {
        let (jobs, mut queue) = mpsc::unbounded_channel::<Job>();
        tokio::spawn(async move {
            while let Some(job) = queue.recv().await {
                match job {
                    Job::View(view) => {
                        if let Err(error) = deps.snapshots.save(&id, &view).await {
                            tracing::warn!(server = %id, %error, "dernière vue non sauvegardée");
                        }
                    }
                    Job::Record(record) => {
                        if let Err(error) = deps.servers.save(&record).await {
                            tracing::warn!(server = %id, %error, "carnet non mis à jour");
                        }
                    }
                    Job::Operations(operations) => {
                        if let Err(error) = deps.operations.save(&id, &operations).await {
                            tracing::warn!(server = %id, %error, "opérations non sauvegardées");
                        }
                    }
                    Job::Flush(done) => {
                        let _ = done.send(());
                    }
                }
            }
        });
        Self { jobs }
    }

    /// Ne bloque jamais. Si la tâche d'écriture a disparu, le travail est perdu en silence.
    pub(crate) fn send(&self, job: Job) {
        let _ = self.jobs.send(job);
    }

    /// Attend (au plus `limit`) que tout ce qui est déposé soit écrit.
    pub(crate) async fn flush(&self, limit: Duration) {
        let (done, written) = oneshot::channel();
        if self.jobs.send(Job::Flush(done)).is_ok() {
            let _ = tokio::time::timeout(limit, written).await;
        }
    }
}
