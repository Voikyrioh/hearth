use std::sync::Arc;

use serde_json::Value;
use tokio::sync::broadcast;

/// Flux des événements du journal d'activité, pour le sujet `audit` du flux temps réel.
///
/// Point d'extension : le journal (HRT-05) fournit son implémentation, déjà convertie dans la
/// forme que les clients reçoivent. Sans journal branché, `subscribe` rend `None` et le sujet
/// reste muet.
pub trait AuditFeed: Send + Sync {
    fn subscribe(&self) -> Option<broadcast::Receiver<Arc<Value>>>;
}
