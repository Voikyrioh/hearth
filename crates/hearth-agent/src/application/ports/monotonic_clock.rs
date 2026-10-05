use time::Duration;

/// Horloge monotone : le temps écoulé depuis le démarrage de l'agent, qui ne recule jamais (à la
/// différence de l'horloge murale, que l'on règle ou que NTP corrige). Sert à cadencer, fenêtrer
/// et ordonner les mesures.
pub trait MonotonicClock: Send + Sync {
    fn elapsed(&self) -> Duration;
}
