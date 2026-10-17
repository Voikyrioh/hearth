//! Adaptateurs du port `ServiceSupervisor` (HRT-63, ADR-0037) : détection, puis démarrer, arrêter,
//! redémarrer une unité (HRT-64).
//!
//! - `proc_systemd` : le propriétaire d'un conteneur se lit dans `/proc` (processus client du moteur,
//!   groupe de contrôle), l'état d'une unité par `systemctl show`, les ordres par `systemctl start|stop|
//!   restart -- <unité>` ;
//! - `none` : aucun superviseur (système sans systemd) : tout ordre est « indisponible » ;
//! - `fake` : le faux superviseur des tests, qui vit dans `tests/support/fake_supervisor.rs`
//!   (partagé avec les tests d'intégration) et n'est compilé que sous `cfg(test)`.
//!
//! Aucun adaptateur ne fait autre chose que démarrer, arrêter ou redémarrer une unité : jamais `disable`,
//! `mask`, `daemon-reload`, ni l'écriture d'une unité.

#[cfg(test)]
#[path = "../../../tests/support/fake_supervisor.rs"]
pub mod fake;
pub mod none;
pub mod proc_systemd;

pub use self::none::NoSupervisor;
pub use self::proc_systemd::ProcSystemd;
