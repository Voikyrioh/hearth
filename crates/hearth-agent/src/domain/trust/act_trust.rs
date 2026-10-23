//! L'état de confiance d'un compte au moment d'un acte (HRT-194, ADR-0032 amendée, BR-TRUST-052).
//!
//! Fonction pure : l'application lit le mode attaque et le compteur de l'identifiant, puis demande le
//! niveau ici. Aucun état d'alerte n'est stocké : il se déduit du compteur de l'identifiant
//! (`identifier_slowdown::is_alert`, BR-TRUST-006). Le client ne fournit jamais cet état.

use hearth_proto::admin_act::TrustLevel;

/// Le niveau de confiance : le mode attaque (actif ou suspendu) l'emporte sur l'alerte de l'identifiant
/// du compte, qui l'emporte sur l'état normal.
pub fn level(attack: bool, own_alert: bool) -> TrustLevel {
    if attack {
        TrustLevel::Attack
    } else if own_alert {
        TrustLevel::Alert
    } else {
        TrustLevel::Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attack_beats_alert_and_alert_beats_normal() {
        assert_eq!(level(false, false), TrustLevel::Normal);
        assert_eq!(level(false, true), TrustLevel::Alert);
        assert_eq!(level(true, false), TrustLevel::Attack);
        assert_eq!(level(true, true), TrustLevel::Attack);
    }
}
