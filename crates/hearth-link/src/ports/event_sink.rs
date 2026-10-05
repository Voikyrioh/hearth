//! Destination des événements de la bibliothèque.

use crate::domain::event::Event;

/// Ne bloque jamais : un destinataire lent perd des événements, il ne ralentit pas le lien.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: Event);
}
