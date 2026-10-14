//! Destination des événements de la bibliothèque.

use std::sync::Arc;

use hearth_proto::api::service_ops::OperationView;

use crate::domain::event::Event;
use crate::domain::server::ServerId;

/// Ne bloque jamais : un destinataire lent perd des événements, il ne ralentit pas le lien.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: Event);

    /// L'état d'une opération de service, tel que l'agent l'envoie sur le sujet `services` (message
    /// `operation`, HRT-144, ADR-0045) : à la connexion pour chaque opération en cours que ce compte peut
    /// voir, puis à chaque changement. Annoncé à part de [`Event`] : un destinataire qui ne s'en sert pas
    /// n'a rien à écrire (le défaut ignore), et ajouter une variante à [`Event`] casserait chaque
    /// `match` exhaustif du client. Ne bloque jamais, comme `emit`.
    fn service_operation(&self, _server: &ServerId, _operation: Arc<OperationView>) {}
}
