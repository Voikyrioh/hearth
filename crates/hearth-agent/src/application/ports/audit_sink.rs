use async_trait::async_trait;
use tokio::sync::broadcast;

use crate::domain::audit::{Actor, AuditAction, AuditRecord, Outcome, Target};

/// Écrit une entrée du journal **hors transaction** : ce qui n'a pas à être atomique avec une
/// action (refus, échecs). L'entrée est datée par l'horloge de l'agent. Un échec d'écriture ne
/// fait jamais échouer l'action : il est tracé en `error` et l'appelant continue. Pour une entrée
/// atomique avec l'action (gestion des comptes, connexion), le cas d'usage écrit dans son unité
/// de travail (`UnitOfWork::audit`).
#[async_trait]
pub trait AuditSink: Send + Sync {
    async fn record(&self, actor: Actor, action: AuditAction, target: Target, outcome: Outcome);
}

/// Diffusion interne des entrées écrites, à destination du flux temps réel (sujet `audit`,
/// administrateurs seulement : le filtrage par rôle est à la charge de l'abonné). Une entrée est
/// publiée une fois sa transaction validée, jamais avant.
pub trait AuditFeed: Send + Sync {
    fn publish(&self, record: AuditRecord);

    /// **Le rôle n'est pas contrôlé ici** : `AuditService::subscribe` le contrôle à l'abonnement
    /// (le flux temps réel n'a pas d'autre chemin), puis le flux le relit sur le compte toutes les
    /// 5 s (`AuditService::ensure_reader`), car une session reste ouverte pendant qu'un
    /// administrateur peut être rétrogradé ou supprimé : un administrateur rétrogradé peut donc
    /// encore recevoir, au plus 5 s.
    ///
    /// Un abonné reçoit les entrées publiées après son abonnement. Un abonné trop lent perd les
    /// plus anciennes (`RecvError::Lagged`) : il doit alors recharger le journal.
    fn subscribe(&self) -> broadcast::Receiver<AuditRecord>;
}
