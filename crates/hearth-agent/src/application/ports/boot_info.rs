use time::Duration;

/// Ce que le noyau dit du démarrage en cours (HRT-25, ADR-0025). **Jamais l'horloge murale** : ni
/// l'identifiant ni le temps écoulé depuis le démarrage ne bougent quand on règle l'heure.
pub trait BootInfo: Send + Sync {
    /// L'identifiant de ce démarrage de la machine (Linux : `/proc/sys/kernel/random/boot_id`, tiré
    /// au hasard à chaque démarrage du noyau, identique pour un redémarrage du service ou une mise à
    /// jour de l'agent). `None` s'il est illisible : l'agent n'ouvre alors jamais de fenêtre.
    fn boot_id(&self) -> Option<String>;

    /// Le temps écoulé depuis le démarrage du noyau (Linux : `/proc/uptime`). Illisible : une durée
    /// infinie, donc jamais « tout juste démarré » (aucune fenêtre ne s'ouvre ni ne se prolonge).
    fn uptime(&self) -> Duration;
}
