use std::path::PathBuf;

use thiserror::Error;

/// Le service qui fait tourner l'agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceSpec {
    /// Le binaire à lancer.
    pub binary: PathBuf,
    /// Le fichier de configuration passé à `serve`.
    pub config: PathBuf,
    /// Le dossier de données (le seul où l'agent écrit, avec son binaire pour les mises à jour).
    pub data_dir: PathBuf,
}

/// Comment le service est géré.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceKind {
    /// Une unité systemd écrite par l'agent.
    Systemd,
    /// Installation gérée par le système (NixOS par exemple) : l'agent n'écrit aucune unité.
    Unmanaged,
}

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("écriture de l'unité {path} impossible : {source}")]
    Unit {
        path: String,
        source: std::io::Error,
    },
    /// `systemctl` n'a pas pu être lancé ou a refusé la commande.
    #[error("`systemctl {command}` a échoué : {detail}")]
    Command { command: String, detail: String },
}

/// Gestion du service de l'agent : l'unité, le démarrage automatique, l'arrêt. Deux adaptateurs :
/// `systemd`, et `none` (installation gérée : rien n'est écrit, rien n'est lancé).
pub trait ServiceManager: Send + Sync {
    fn kind(&self) -> ServiceKind;

    /// L'unité existe-t-elle ?
    fn is_installed(&self) -> Result<bool, ServiceError>;

    /// Le service tourne-t-il ?
    fn is_active(&self) -> Result<bool, ServiceError>;

    /// Le service démarre-t-il avec le système ?
    fn is_enabled(&self) -> Result<bool, ServiceError>;

    /// Le fait démarrer avec le système, sans le lancer.
    fn enable(&self) -> Result<(), ServiceError>;

    /// Écrit l'unité, la fait connaître au système, l'active au démarrage et la démarre
    /// (idempotent : sans effet sur un service qui tourne déjà avec la même unité).
    fn install(&self, spec: &ServiceSpec) -> Result<(), ServiceError>;

    /// Le texte de l'unité telle qu'elle est écrite, `None` s'il n'y en a pas : sert à la rétablir
    /// telle quelle après un échec.
    fn unit_text(&self) -> Result<Option<String>, ServiceError>;

    /// Réécrit l'unité avec ce texte et en informe le système, sans la démarrer ni l'activer.
    fn restore_unit(&self, text: &str) -> Result<(), ServiceError>;

    /// Redémarre le service.
    fn restart(&self) -> Result<(), ServiceError>;

    /// Arrête le service (sans erreur s'il ne tourne pas).
    fn stop(&self) -> Result<(), ServiceError>;

    /// Retire le service du démarrage automatique (sans erreur s'il n'y est pas).
    fn disable(&self) -> Result<(), ServiceError>;

    /// Supprime l'unité et en informe le système (sans erreur si elle n'existe pas).
    fn remove(&self) -> Result<(), ServiceError>;
}
