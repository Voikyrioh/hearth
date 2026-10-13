//! Adaptateurs du port `ServiceSupervisor` (HRT-63, ADR-0037), en DÉTECTION SEULE.
//!
//! - `proc_systemd` : le propriétaire d'un conteneur se lit dans `/proc` (processus client du moteur,
//!   groupe de contrôle), l'état d'une unité par `systemctl show` ;
//! - `none` : aucun superviseur (système sans systemd) ;
//! - `fake` : le faux superviseur des tests, qui vit dans `tests/support/fake_supervisor.rs`
//!   (partagé avec les tests d'intégration) et n'est compilé que sous `cfg(test)`.
//!
//! Aucun adaptateur n'agit sur une unité : ni démarrer, ni arrêter, ni redémarrer, ni `disable`, ni
//! `mask` (HRT-64 ajoutera les trois premiers ; les deux autres, jamais).

#[cfg(test)]
#[path = "../../../tests/support/fake_supervisor.rs"]
pub mod fake;
pub mod none;
pub mod proc_systemd;

pub use self::none::NoSupervisor;
pub use self::proc_systemd::ProcSystemd;
