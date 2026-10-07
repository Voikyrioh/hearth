//! La règle « 2 critères sur 3 » (HRT-24, ADR-0024, BR-TRUST-001, 002, 006, 034, 035).
//!
//! Trois critères : l'**adresse retenue** pour le compte visé, la **session valide**, la **clé**
//! prouvée. À une connexion par mot de passe aucune session n'est présentée (une session n'apporte
//! rien de plus qu'un mot de passe juste : c'est le même critère) ; reste « adresse », « clé » et
//! « authentification réussie du premier coup » (BR-TRUST-001 b).
//!
//! **Fonction pure qui ne décide que deux choses** : ce poste échappe-t-il au ralentissement par
//! identifiant, et le résultat du mot de passe est-il pris en compte ? Elle ne rend jamais « accordé » :
//! seul `login_policy::conclude` le fait, sur un mot de passe vérifié (BR-TRUST-002). Tout le reste
//! (compteurs, réponses, durées) est le chemin existant, identique pour tous : c'est ce qui ferme les
//! oracles (conception technique, 5.4).
//!
//! Deux états seulement ici (HRT-24) :
//!
//! - **NORMAL** : la règle ne joue pas (BR-TRUST-035). Aucun critère n'est exigé ; le comportement est
//!   celui d'avant la règle. L'identifiant n'est pas ralenti, il n'y a donc rien à fuir.
//! - **ALERTE** (identifiant visé : il est ralenti) : la règle décide qui **échappe au ralentissement**.
//!   Personne n'est bloqué : un poste non reconnu est ralenti, jamais refusé au-delà du plafond du
//!   ralentissement, et un mot de passe juste passe entre deux attentes.
//!
//! Le MODE ATTAQUE (HRT-25) est un troisième état : il s'ajoute à `Mode` et à `judge_login` (une
//! branche de plus, et `trial_used` dans `LoginCriteria`, `trial` dans `LoginStanding`) sans réécrire
//! ce qui suit.

use time::OffsetDateTime;

use crate::domain::identifier_slowdown::{self, Slowdown};

/// L'état de sécurité pour une tentative sur un identifiant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Rien d'anormal sur cet identifiant : la règle ne joue pas.
    Normal,
    /// Une attaque probable vise cet identifiant (BR-TRUST-008).
    Alert,
}

/// L'état de l'identifiant à `now`, déduit de son compteur : jamais stocké à part. ALERTE ⇔ plus de
/// `FREE_FAILURES` échecs venus de postes inconnus **et** le dernier date de moins de `RESET_AFTER`
/// (ou une attente est en cours). C'est le moment exact où l'identifiant commence à être ralenti.
pub fn mode_of(identifier: &Slowdown, now: OffsetDateTime) -> Mode {
    if identifier_slowdown::is_alert(identifier, now) {
        Mode::Alert
    } else {
        Mode::Normal
    }
}

/// Ce que l'agent constate d'un poste avant de regarder le mot de passe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoginCriteria {
    /// L'adresse de la connexion est retenue pour le compte visé (faux si le compte n'existe pas).
    pub address: bool,
    /// Preuve valide d'une clé inscrite pour le compte visé.
    pub key: bool,
    /// « Du premier coup » : le compteur du couple (identifiant, adresse) est à zéro, aucun échec
    /// compté depuis son dernier succès (Q14, point 5). Ne vaut qu'accompagné d'un mot de passe
    /// juste, ce que `conclude` exige ensuite.
    pub first_try: bool,
}

/// Ce que le reste du chemin doit faire du verdict de la règle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoginStanding {
    /// Remplace le `known` provisoire de `login_policy::conclude` : ce poste n'est pas ralenti.
    pub escapes_slowdown: bool,
    /// Faux : le mot de passe est traité comme faux (mode attaque seulement ; toujours vrai ici).
    pub password_counts: bool,
}

/// Le verdict de la règle pour une connexion par mot de passe.
pub fn judge_login(mode: Mode, criteria: LoginCriteria) -> LoginStanding {
    // Critères présentés avant le mot de passe.
    let before = u8::from(criteria.address) + u8::from(criteria.key);
    match mode {
        Mode::Normal => LoginStanding {
            escapes_slowdown: false,
            password_counts: true,
        },
        Mode::Alert => LoginStanding {
            // Deux critères avant le mot de passe, ou un critère plus « du premier coup ».
            escapes_slowdown: before == 2 || (before == 1 && criteria.first_try),
            password_counts: true,
        },
    }
}

#[cfg(test)]
mod tests {
    use time::Duration;

    use super::*;
    use crate::domain::identifier_slowdown::{FREE_FAILURES, RESET_AFTER};

    fn t0() -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH + Duration::days(20_000)
    }

    /// La table de vérité, écrite à la main : (adresse, clé, premier coup, échappe au
    /// ralentissement). Les lignes sont celles du plan de scénario « Reconnaissance d'un poste en
    /// état d'alerte ».
    const ALERT_TABLE: [(bool, bool, bool, bool); 8] = [
        (true, true, true, true),
        (true, true, false, true),
        (true, false, true, true),
        (true, false, false, false),
        (false, true, true, true),
        (false, true, false, false),
        (false, false, true, false),
        (false, false, false, false),
    ];

    fn criteria(address: bool, key: bool, first_try: bool) -> LoginCriteria {
        LoginCriteria {
            address,
            key,
            first_try,
        }
    }

    #[test]
    fn the_whole_truth_table_in_alert_is_the_one_written_by_hand() {
        for (address, key, first_try, escapes) in ALERT_TABLE {
            let standing = judge_login(Mode::Alert, criteria(address, key, first_try));
            assert_eq!(
                standing,
                LoginStanding {
                    escapes_slowdown: escapes,
                    password_counts: true
                },
                "adresse {address}, clé {key}, premier coup {first_try}"
            );
        }
    }

    #[test]
    fn in_the_normal_state_the_rule_changes_nothing_whatever_the_criteria() {
        for address in [false, true] {
            for key in [false, true] {
                for first_try in [false, true] {
                    assert_eq!(
                        judge_login(Mode::Normal, criteria(address, key, first_try)),
                        LoginStanding {
                            escapes_slowdown: false,
                            password_counts: true
                        },
                        "adresse {address}, clé {key}, premier coup {first_try}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_rule_never_discards_the_password_in_the_states_of_this_ticket() {
        for mode in [Mode::Normal, Mode::Alert] {
            for bits in 0..8_u8 {
                let c = criteria(bits & 1 != 0, bits & 2 != 0, bits & 4 != 0);
                assert!(judge_login(mode, c).password_counts, "{mode:?} {c:?}");
            }
        }
    }

    #[test]
    fn the_first_try_alone_or_one_criterion_alone_is_never_enough() {
        assert!(!judge_login(Mode::Alert, criteria(false, false, true)).escapes_slowdown);
        assert!(!judge_login(Mode::Alert, criteria(true, false, false)).escapes_slowdown);
        assert!(!judge_login(Mode::Alert, criteria(false, true, false)).escapes_slowdown);
    }

    fn slowdown(failures: u32, last: OffsetDateTime) -> Slowdown {
        Slowdown {
            failures,
            wait_until: None,
            last_failure_at: Some(last),
            alerted_at: None,
        }
    }

    #[test]
    fn the_mode_is_derived_from_the_counter_and_starts_at_the_eleventh_failure() {
        let now = t0();
        assert_eq!(mode_of(&Slowdown::default(), now), Mode::Normal);
        assert_eq!(
            mode_of(&slowdown(FREE_FAILURES, now), now),
            Mode::Normal,
            "dix échecs : encore gratuits"
        );
        assert_eq!(mode_of(&slowdown(FREE_FAILURES + 1, now), now), Mode::Alert);
    }

    #[test]
    fn the_alert_ends_after_thirty_minutes_without_failure() {
        let last = t0();
        let state = slowdown(FREE_FAILURES + 5, last);
        assert_eq!(
            mode_of(&state, last + RESET_AFTER - Duration::seconds(1)),
            Mode::Alert
        );
        assert_eq!(mode_of(&state, last + RESET_AFTER), Mode::Normal);
    }

    #[test]
    fn a_wall_clock_set_back_or_forward_never_blocks_nor_freezes_the_state() {
        let last = t0();
        let state = slowdown(FREE_FAILURES + 5, last);
        // Horloge très reculée ou avancée (sans attente en cours) : l'écart absolu dépasse le délai,
        // retour à NORMAL, comme le compteur lui-même (`record_failure`).
        assert_eq!(mode_of(&state, last - Duration::days(365)), Mode::Normal);
        assert_eq!(mode_of(&state, last + Duration::days(365)), Mode::Normal);
        // Horloge un peu reculée : toujours en alerte.
        assert_eq!(mode_of(&state, last - Duration::minutes(5)), Mode::Alert);
        // Une attente en cours (même après un grand recul) garde l'alerte : un poste reconnu n'est
        // pas ralenti tant que l'identifiant l'est.
        let waiting = Slowdown {
            wait_until: Some(last - Duration::days(365) + Duration::seconds(30)),
            ..state
        };
        assert_eq!(mode_of(&waiting, last - Duration::days(365)), Mode::Alert);
    }
}
