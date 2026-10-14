//! Diffusion des événements : un canal borné, un destinataire lent perd les plus anciens.

use std::sync::Arc;

use tokio::sync::broadcast;

use crate::domain::event::Event;
use crate::ports::EventSink;

/// Flux d'événements d'un abonné (`LinkManager::subscribe`).
pub struct EventStream {
    receiver: broadcast::Receiver<Event>,
}

impl EventStream {
    /// Prochain événement ; `None` quand la bibliothèque est arrêtée. Un abonné en retard reçoit
    /// `Event::Lagged { skipped }` : des événements ont été perdus, dont peut-être le dernier
    /// changement d'état. Il doit alors relire `LinkManager::states()`.
    pub async fn recv(&mut self) -> Option<Event> {
        match self.receiver.recv().await {
            Ok(event) => Some(event),
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!(skipped, "abonné aux événements du lien en retard");
                Some(Event::Lagged { skipped })
            }
            Err(broadcast::error::RecvError::Closed) => None,
        }
    }

    /// Événement déjà disponible, sans attendre (même règle de retard que `recv`).
    pub fn try_recv(&mut self) -> Option<Event> {
        match self.receiver.try_recv() {
            Ok(event) => Some(event),
            Err(broadcast::error::TryRecvError::Lagged(skipped)) => Some(Event::Lagged { skipped }),
            Err(_) => None,
        }
    }
}

/// Destination interne : le canal des abonnés, plus une destination supplémentaire facultative.
pub(crate) struct Fanout {
    sender: broadcast::Sender<Event>,
    extra: Option<Arc<dyn EventSink>>,
}

impl Fanout {
    pub(crate) fn new(capacity: usize, extra: Option<Arc<dyn EventSink>>) -> Self {
        let (sender, _) = broadcast::channel(capacity.max(16));
        Self { sender, extra }
    }

    pub(crate) fn subscribe(&self) -> EventStream {
        EventStream {
            receiver: self.sender.subscribe(),
        }
    }
}

impl EventSink for Fanout {
    fn emit(&self, event: Event) {
        if let Some(extra) = &self.extra {
            extra.emit(event.clone());
        }
        // Aucun abonné : l'événement est simplement perdu.
        let _ = self.sender.send(event);
    }

    /// Le suivi d'une opération de service (HRT-144) ne passe pas par le canal des abonnés (`Event` n'a pas de
    /// variante pour lui) : il va seulement à la destination supplémentaire, qui l'implémente ou l'ignore.
    fn service_operation(
        &self,
        server: &crate::domain::server::ServerId,
        operation: Arc<hearth_proto::api::service_ops::OperationView>,
    ) {
        if let Some(extra) = &self.extra {
            extra.service_operation(server, operation);
        }
    }
}
