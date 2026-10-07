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
//! Trois états :
//!
//! - **NORMAL** : la règle ne joue pas (BR-TRUST-035). Aucun critère n'est exigé ; le comportement est
//!   celui d'avant la règle. L'identifiant n'est pas ralenti, il n'y a donc rien à fuir.
//! - **ALERTE** (identifiant visé : il est ralenti) : la règle décide qui **échappe au ralentissement**.
//!   Personne n'est bloqué : un poste non reconnu est ralenti, jamais refusé au-delà du plafond du
//!   ralentissement, et un mot de passe juste passe entre deux attentes.
//!
//! - **MODE ATTAQUE** (HRT-25, ADR-0025, activé par un administrateur, global au serveur) : la règle
//!   décide qui **passe et qui est bloqué**. Deux critères avant le mot de passe (adresse retenue et
//!   clé) : reconnu. **Un seul** : UN essai de mot de passe par critère présenté, par compte et par
//!   activation (`LoginStanding::trial`) ; juste, le poste a deux critères et il est reconnu ; faux,
//!   bloqué jusqu'à la fin du mode. Essai déjà consommé, ou aucun critère : le mot de passe, même
//!   juste, n'est pas pris en compte (`password_counts` faux) et la tentative suit le chemin du mot
//!   de passe faux. Une session présentée seule est refusée sans essai (`judge_session`, Q12).

use time::OffsetDateTime;

use crate::domain::identifier_slowdown::{self, Slowdown};

/// L'état de sécurité pour une tentative sur un identifiant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Rien d'anormal sur cet identifiant : la règle ne joue pas.
    Normal,
    /// Une attaque probable vise cet identifiant (BR-TRUST-008).
    Alert,
    /// Le mode attaque est actif et non suspendu (HRT-25, BR-TRUST-011 à 017).
    Attack,
}

/// Le critère présenté seul qui a droit à son essai unique (BR-TRUST-012).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrialKind {
    /// L'adresse retenue ; le sujet de l'essai est l'adresse exacte.
    Address,
    /// La clé inscrite ; le sujet de l'essai est l'identifiant du poste.
    Key,
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
    /// L'essai unique de ce critère est déjà consommé pour cette activation (mode attaque). Sans objet
    /// hors mode attaque ; lu seulement quand un critère est présenté seul.
    pub trial_used: bool,
}

/// Ce que le reste du chemin doit faire du verdict de la règle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoginStanding {
    /// Remplace le `known` provisoire de `login_policy::conclude` : ce poste n'est pas ralenti.
    pub escapes_slowdown: bool,
    /// Faux : le mot de passe est traité comme faux (mode attaque seulement).
    pub password_counts: bool,
    /// L'essai unique que cette tentative consomme, dans la transaction de la tentative (mode attaque
    /// seulement). `None` : rien à consommer.
    pub trial: Option<TrialKind>,
}

/// Le verdict de la règle pour une connexion par mot de passe.
pub fn judge_login(mode: Mode, criteria: LoginCriteria) -> LoginStanding {
    // Critères présentés avant le mot de passe.
    let before = u8::from(criteria.address) + u8::from(criteria.key);
    match mode {
        Mode::Normal => LoginStanding {
            escapes_slowdown: false,
            password_counts: true,
            trial: None,
        },
        Mode::Alert => LoginStanding {
            // Deux critères avant le mot de passe, ou un critère plus « du premier coup ».
            escapes_slowdown: before == 2 || (before == 1 && criteria.first_try),
            password_counts: true,
            trial: None,
        },
        Mode::Attack => match before {
            // Reconnu : le chemin ordinaire, sans essai limité.
            2 => LoginStanding {
                escapes_slowdown: true,
                password_counts: true,
                trial: None,
            },
            // Un critère et son essai encore libre : UN essai. Juste, c'est « du premier coup » : deux
            // critères. Faux, l'essai est consommé et le poste est bloqué jusqu'à la fin du mode.
            1 if !criteria.trial_used => LoginStanding {
                escapes_slowdown: true,
                password_counts: true,
                trial: Some(if criteria.address {
                    TrialKind::Address
                } else {
                    TrialKind::Key
                }),
            },
            // Essai déjà consommé, ou aucun critère : bloqué sans essai. Le mot de passe, même
            // juste, est traité comme faux (le même chemin, les mêmes compteurs).
            _ => LoginStanding {
                escapes_slowdown: false,
                password_counts: false,
                trial: None,
            },
        },
    }
}

/// Ce que la règle décide d'une session valide présentée (BR-TRUST-007, 013).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStanding {
    /// La session sert.
    Allowed,
    /// La session sert et l'adresse devient retenue : une session plus une clé prouvée
    /// (BR-TRUST-007).
    AllowedAndLearnAddress,
    /// Mode attaque : une session présentée seule (ni adresse retenue ni clé) est refusée, sans essai
    /// et sans être détruite (Q12, Q14 point 2).
    Refused,
}

/// Le verdict de la règle pour l'usage d'une session valide. `address` : l'adresse de la connexion est
/// retenue pour le compte ; `key` : la preuve d'une clé inscrite du compte accompagne la requête.
pub fn judge_session(mode: Mode, address: bool, key: bool) -> SessionStanding {
    match (mode, address, key) {
        (_, false, true) => SessionStanding::AllowedAndLearnAddress,
        (Mode::Attack, false, false) => SessionStanding::Refused,
        _ => SessionStanding::Allowed,
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
            trial_used: false,
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
                    password_counts: true,
                    trial: None,
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
                            password_counts: true,
                            trial: None,
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

    // -----------------------------------------------------------------------------------------
    // Mode attaque (HRT-25) : la table de vérité COMPLÈTE, écrite à la main
    // -----------------------------------------------------------------------------------------

    fn attack_criteria(
        address: bool,
        key: bool,
        first_try: bool,
        trial_used: bool,
    ) -> LoginCriteria {
        LoginCriteria {
            address,
            key,
            first_try,
            trial_used,
        }
    }

    /// (adresse, clé, essai déjà utilisé) -> (échappe au ralentissement, mot de passe pris en compte,
    /// essai consommé). BR-TRUST-011, 012, 015, 016 : deux critères, reconnu ; un seul critère, UN essai
    /// (par critère présenté) ; essai utilisé ou aucun critère, bloqué sans essai.
    #[allow(clippy::type_complexity)]
    const ATTACK_TABLE: [(bool, bool, bool, bool, bool, Option<TrialKind>); 8] = [
        (true, true, false, true, true, None),
        (true, true, true, true, true, None),
        (true, false, false, true, true, Some(TrialKind::Address)),
        (true, false, true, false, false, None),
        (false, true, false, true, true, Some(TrialKind::Key)),
        (false, true, true, false, false, None),
        (false, false, false, false, false, None),
        (false, false, true, false, false, None),
    ];

    #[test]
    fn the_whole_truth_table_in_attack_mode_is_the_one_written_by_hand() {
        for (address, key, trial_used, escapes, counts, trial) in ATTACK_TABLE {
            // « Du premier coup » ne change rien en mode attaque : c'est l'essai qui en tient lieu.
            for first_try in [false, true] {
                let standing = judge_login(
                    Mode::Attack,
                    attack_criteria(address, key, first_try, trial_used),
                );
                assert_eq!(
                    standing,
                    LoginStanding {
                        escapes_slowdown: escapes,
                        password_counts: counts,
                        trial
                    },
                    "adresse {address}, clé {key}, essai utilisé {trial_used}, premier coup {first_try}"
                );
            }
        }
    }

    #[test]
    fn every_combination_of_the_three_modes_is_covered_96_cases() {
        // 3 modes x (adresse, clé, premier coup, essai utilisé) = 48 combinaisons, chacune contre un mot
        // de passe juste ou faux = 96 cas. La règle ne voit pas le mot de passe : la table ne dépend que
        // des critères, et `password_counts` ne devient faux que dans un seul état.
        let mut cases = 0;
        for mode in [Mode::Normal, Mode::Alert, Mode::Attack] {
            for bits in 0..16_u8 {
                let c = attack_criteria(bits & 1 != 0, bits & 2 != 0, bits & 4 != 0, bits & 8 != 0);
                let standing = judge_login(mode, c);
                for _password_is_right in [false, true] {
                    cases += 1;
                    if mode != Mode::Attack {
                        assert!(standing.password_counts, "{mode:?} {c:?}");
                        assert_eq!(standing.trial, None, "{mode:?} {c:?}");
                    }
                    // Jamais d'essai sans exactement un critère.
                    if standing.trial.is_some() {
                        assert_eq!(u8::from(c.address) + u8::from(c.key), 1, "{c:?}");
                        assert!(!c.trial_used, "{c:?}");
                        assert!(standing.password_counts, "{c:?}");
                    }
                    // Un mot de passe jamais pris en compte sans que le poste soit bloqué.
                    if !standing.password_counts {
                        assert!(!standing.escapes_slowdown, "{c:?}");
                    }
                }
            }
        }
        assert_eq!(cases, 96);
    }

    #[test]
    fn an_used_trial_changes_nothing_outside_the_attack_mode_whatever_the_criteria() {
        // Les tables NORMAL et ALERTE, avec et sans essai déjà utilisé : l'essai n'existe pas hors du mode
        // attaque.
        for (address, key, first_try, escapes) in ALERT_TABLE {
            for trial_used in [false, true] {
                assert_eq!(
                    judge_login(
                        Mode::Alert,
                        attack_criteria(address, key, first_try, trial_used)
                    ),
                    LoginStanding {
                        escapes_slowdown: escapes,
                        password_counts: true,
                        trial: None
                    },
                    "alerte : adresse {address}, clé {key}, premier coup {first_try}, essai utilisé {trial_used}"
                );
                assert_eq!(
                    judge_login(
                        Mode::Normal,
                        attack_criteria(address, key, first_try, trial_used)
                    ),
                    LoginStanding {
                        escapes_slowdown: false,
                        password_counts: true,
                        trial: None
                    },
                    "normal : adresse {address}, clé {key}, premier coup {first_try}, essai utilisé {trial_used}"
                );
            }
        }
    }

    #[test]
    fn the_trial_is_taken_on_the_criterion_that_is_presented() {
        let by_address = judge_login(Mode::Attack, attack_criteria(true, false, false, false));
        let by_key = judge_login(Mode::Attack, attack_criteria(false, true, false, false));
        assert_eq!(by_address.trial, Some(TrialKind::Address));
        assert_eq!(by_key.trial, Some(TrialKind::Key));
        // Un essai consommé sur l'adresse ne bloque pas l'essai de la clé : ce sont deux critères.
        assert!(
            judge_login(Mode::Attack, attack_criteria(false, true, false, false)).password_counts
        );
    }

    /// (mode, adresse retenue, clé prouvée) -> verdict, écrit à la main : les 12 cas de `judge_session`.
    const SESSION_TABLE: [(Mode, bool, bool, SessionStanding); 12] = [
        (Mode::Normal, true, true, SessionStanding::Allowed),
        (Mode::Normal, true, false, SessionStanding::Allowed),
        (
            Mode::Normal,
            false,
            true,
            SessionStanding::AllowedAndLearnAddress,
        ),
        (Mode::Normal, false, false, SessionStanding::Allowed),
        (Mode::Alert, true, true, SessionStanding::Allowed),
        (Mode::Alert, true, false, SessionStanding::Allowed),
        (
            Mode::Alert,
            false,
            true,
            SessionStanding::AllowedAndLearnAddress,
        ),
        (Mode::Alert, false, false, SessionStanding::Allowed),
        (Mode::Attack, true, true, SessionStanding::Allowed),
        (Mode::Attack, true, false, SessionStanding::Allowed),
        (
            Mode::Attack,
            false,
            true,
            SessionStanding::AllowedAndLearnAddress,
        ),
        (Mode::Attack, false, false, SessionStanding::Refused),
    ];

    #[test]
    fn the_whole_session_table_is_the_one_written_by_hand() {
        for (mode, address, key, expected) in SESSION_TABLE {
            assert_eq!(
                judge_session(mode, address, key),
                expected,
                "{mode:?}, adresse {address}, clé {key}"
            );
        }
    }

    #[test]
    fn only_a_session_alone_in_attack_mode_is_ever_refused() {
        for mode in [Mode::Normal, Mode::Alert, Mode::Attack] {
            for address in [false, true] {
                for key in [false, true] {
                    let refused = judge_session(mode, address, key) == SessionStanding::Refused;
                    assert_eq!(
                        refused,
                        mode == Mode::Attack && !address && !key,
                        "{mode:?}, adresse {address}, clé {key}"
                    );
                }
            }
        }
    }
}
