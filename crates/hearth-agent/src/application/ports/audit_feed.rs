use std::sync::Arc;

use serde_json::Value;
use tokio::sync::broadcast;

/// Flux des événements du journal d'activité, pour le sujet `audit` du flux temps réel.
///
/// **Forme provisoire.** L'événement est ici un `serde_json::Value` déjà converti dans sa forme du
/// fil : un détail de protocole qui remonte dans un port applicatif, accepté tant que le journal
/// (HRT-05) n'a pas défini son événement de domaine. À la fusion avec HRT-05, le port rendra cet
/// événement du domaine et `entrypoint/ws` le convertira vers le fil, comme pour les mesures.
///
/// Point d'extension : le journal (HRT-05) fournit son implémentation, déjà convertie dans la
/// forme que les clients reçoivent. Sans journal branché, `subscribe` rend `None` et le sujet
/// reste muet.
pub trait AuditFeed: Send + Sync {
    fn subscribe(&self) -> Option<broadcast::Receiver<Arc<Value>>>;
}
