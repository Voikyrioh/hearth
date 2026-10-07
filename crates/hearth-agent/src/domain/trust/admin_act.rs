//! Règles pures des actes d'administration (HRT-28) : ce que l'élévation du mot de passe couvre, combien
//! de temps elle dure, comment se lit le réglage de fréquence (BR-TRUST-042, 043, 047). Le temps est un
//! paramètre (millisecondes d'horloge monotone) : aucune horloge murale, aucune E/S.

use hearth_proto::admin_act::AdminAct;
use hearth_proto::api::accounts::RoleName;
use hearth_proto::api::reauth::ReauthMode;

/// Durée de l'élévation : 5 minutes fixes depuis la saisie du mot de passe (Q19). **Non glissante** : un
/// acte ne la prolonge pas, seule une nouvelle saisie la relance.
pub const ELEVATION_MS: u64 = 5 * 60 * 1000;

/// Valeur de la colonne `accounts.reauth_window_s` pour « une saisie pour 5 minutes » (défaut).
pub const WINDOW_SECONDS: i64 = 300;

/// L'élévation ouverte à `opened_ms` vaut-elle encore à `now_ms` ? Un instant qui recule (impossible sur
/// une horloge monotone) la ferme : fermé, jamais ouvert.
pub fn elevation_holds(opened_ms: u64, now_ms: u64) -> bool {
    now_ms >= opened_ms && now_ms - opened_ms < ELEVATION_MS
}

/// Secondes restantes de l'élévation, arrondies à la seconde supérieure ; 0 si elle est fermée.
pub fn elevation_remaining_s(opened_ms: u64, now_ms: u64) -> u64 {
    if !elevation_holds(opened_ms, now_ms) {
        return 0;
    }
    (ELEVATION_MS - (now_ms - opened_ms)).div_ceil(1000)
}

/// L'élévation peut-elle remplacer le mot de passe pour cet acte ? **Jamais** pour : changer un mot de
/// passe (le sien ou celui d'un autre), activer ou désactiver le mode attaque, lancer la mise à jour de
/// l'agent, changer le réglage, donner le rôle Administrateur (changement de rôle ou création). Elle
/// couvre ce qui retire ou limite un accès : créer un compte en lecture seule, passer un compte en
/// lecture seule, supprimer un compte, fermer les sessions d'un compte (le dernier administrateur reste
/// protégé par BR-ACCT-007). Le retrait d'un poste (contrat `0x04`) n'est pas un `AdminAct` : il exige
/// toujours le mot de passe.
///
/// La correspondance est exhaustive : un acte de plus oblige à choisir.
pub fn covered_by_elevation(act: &AdminAct<'_>) -> bool {
    match act {
        AdminAct::AccountCreate { role, .. } | AdminAct::AccountRole { role, .. } => {
            *role != RoleName::Admin
        }
        AdminAct::AccountDelete { .. } | AdminAct::SessionsRevoke { .. } => true,
        AdminAct::AccountPassword { .. }
        | AdminAct::AgentUpdate { .. }
        | AdminAct::AttackMode { .. }
        | AdminAct::AccountPasswordOwn
        | AdminAct::ReauthSetting { .. } => false,
    }
}

/// Le réglage d'un compte lu dans la colonne. Toute valeur inconnue est lue comme `each` (le plus strict).
pub fn mode_from_seconds(seconds: i64) -> ReauthMode {
    if seconds == WINDOW_SECONDS {
        ReauthMode::Window
    } else {
        ReauthMode::Each
    }
}

/// La valeur de la colonne pour un réglage.
pub fn seconds_of(mode: ReauthMode) -> i64 {
    match mode {
        ReauthMode::Window => WINDOW_SECONDS,
        ReauthMode::Each => 0,
    }
}

#[cfg(test)]
mod tests {
    use hearth_proto::admin_act::ActKind;

    use super::*;

    #[test]
    fn the_elevation_lasts_five_minutes_exactly_and_never_slides() {
        assert!(elevation_holds(1_000, 1_000));
        assert!(elevation_holds(1_000, 1_000 + ELEVATION_MS - 1));
        assert!(!elevation_holds(1_000, 1_000 + ELEVATION_MS));
        assert!(!elevation_holds(1_000, 1_000 + ELEVATION_MS + 1));
        // Un instant qui recule ferme.
        assert!(!elevation_holds(5_000, 4_999));
        assert_eq!(elevation_remaining_s(0, 0), 300);
        assert_eq!(elevation_remaining_s(0, 1), 300);
        assert_eq!(elevation_remaining_s(0, 299_001), 1);
        assert_eq!(elevation_remaining_s(0, ELEVATION_MS), 0);
        assert_eq!(elevation_remaining_s(10, 5), 0);
    }

    /// Un exemple de chaque acte, avec les deux rôles quand le rôle compte.
    fn every_act() -> Vec<(AdminAct<'static>, bool)> {
        vec![
            (
                AdminAct::AccountCreate {
                    username: "paul",
                    role: RoleName::Readonly,
                },
                true,
            ),
            (
                AdminAct::AccountCreate {
                    username: "paul",
                    role: RoleName::Admin,
                },
                false,
            ),
            (
                AdminAct::AccountRole {
                    target: "x",
                    role: RoleName::Readonly,
                },
                true,
            ),
            (
                AdminAct::AccountRole {
                    target: "x",
                    role: RoleName::Admin,
                },
                false,
            ),
            (AdminAct::AccountDelete { target: "x" }, true),
            (AdminAct::SessionsRevoke { target: "x" }, true),
            (AdminAct::AccountPassword { target: "x" }, false),
            (
                AdminAct::AgentUpdate {
                    version: "0.2.0",
                    sha256: "ab",
                },
                false,
            ),
            (AdminAct::AttackMode { enable: true }, false),
            (AdminAct::AttackMode { enable: false }, false),
            (AdminAct::AccountPasswordOwn, false),
            (
                AdminAct::ReauthSetting {
                    mode: ReauthMode::Each,
                },
                false,
            ),
        ]
    }

    #[test]
    fn the_elevation_covers_only_what_removes_or_limits_an_access() {
        let acts = every_act();
        for (act, covered) in &acts {
            assert_eq!(covered_by_elevation(act), *covered, "{act:?}");
        }
        // Chaque genre d'acte est examiné au moins une fois.
        for kind in ActKind::ALL {
            assert!(
                acts.iter().any(|(act, _)| act.kind() == kind),
                "{kind:?} sans exemple"
            );
        }
    }

    #[test]
    fn the_setting_is_read_strictly_and_round_trips() {
        assert_eq!(mode_from_seconds(300), ReauthMode::Window);
        assert_eq!(mode_from_seconds(0), ReauthMode::Each);
        assert_eq!(mode_from_seconds(-1), ReauthMode::Each);
        assert_eq!(mode_from_seconds(600), ReauthMode::Each);
        for mode in [ReauthMode::Window, ReauthMode::Each] {
            assert_eq!(mode_from_seconds(seconds_of(mode)), mode);
        }
    }
}
