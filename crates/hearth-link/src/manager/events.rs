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
    /// Prochain événement ; `None` quand la bibliothèque est arrêtée. Si l'abonné a pris du
    /// retard, les événements perdus sont sautés (un état plus récent suivra toujours).
    pub async fn recv(&mut self) -> Option<Event> {
        loop {
            match self.receiver.recv().await {
                Ok(event) => return Some(event),
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "abonné aux événements du lien en retard");
                }
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }

    /// Événement déjà disponible, sans attendre.
    pub fn try_recv(&mut self) -> Option<Event> {
        loop {
            match self.receiver.try_recv() {
                Ok(event) => return Some(event),
                Err(broadcast::error::TryRecvError::Lagged(_)) => {}
                Err(_) => return None,
            }
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
}
