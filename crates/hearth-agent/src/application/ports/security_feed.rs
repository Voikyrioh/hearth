use tokio::sync::broadcast;

/// Diffusion interne des changements de l'état de sécurité (début ou fin d'une alerte, HRT-24) à
/// destination du flux temps réel. **Le message ne porte rien** : un tic dit « quelque chose a
/// changé », et chaque connexion relit SON état (`SecurityService::state_for`) selon SON compte et
/// son rôle. Un abonné ne peut donc rien apprendre de plus que ce que son rôle lui montre.
///
/// Un abonné trop lent perd des tics (`RecvError::Lagged`) : il relit son état, rien n'est perdu.
pub trait SecurityFeed: Send + Sync {
    fn publish(&self);

    fn subscribe(&self) -> broadcast::Receiver<()>;
}
