//! Erreur unique des commandes exposées à l'interface, typée de bout en bout.

use serde::Serialize;
use specta::Type;

/// Erreur d'une commande. Sérialisée `{ kind, message }` côté TypeScript ;
/// l'interface choisit son texte d'après `kind`.
#[derive(Debug, thiserror::Error, Serialize, Type)]
#[serde(tag = "kind", content = "message", rename_all = "camelCase")]
pub enum AppError {
    /// Lecture ou écriture des réglages locaux impossible.
    #[error("réglages locaux inaccessibles : {0}")]
    Store(String),
    /// Lecture ou écriture de l'entrée de démarrage de Windows impossible.
    #[error("démarrage avec Windows inaccessible : {0}")]
    Autostart(String),
    /// Dossier des journaux impossible à créer ou à ouvrir.
    #[error("dossier des journaux inaccessible : {0}")]
    Logs(String),
}
