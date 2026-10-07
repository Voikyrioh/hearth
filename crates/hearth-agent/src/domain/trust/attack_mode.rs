//! L'état du mode attaque et son cycle de vie (HRT-25, ADR-0025, BR-TRUST-012, 019 à 021, 027).
//!
//! Fonctions pures : le temps est un paramètre, **aucune horloge murale n'entre dans une durée de 30
//! minutes** tant que la machine n'a pas redémarré. Les durées se mesurent sur deux horloges qui ne
//! reculent pas : l'horloge monotone de l'agent (sortie automatique) et le temps écoulé depuis le
//! démarrage du noyau (fenêtre de redémarrage, garde de réactivation). Régler l'heure, la reculer d'un
//! an ou perdre NTP ne change rien.
//!
//! Le mode attaque est **global au serveur** et **persistant** : un redémarrage du service ou une mise à
//! jour de l'agent le retrouve actif, sans fenêtre (BR-TRUST-021). Seul un **démarrage du système**
//! (l'identifiant de démarrage du noyau a changé) le suspend 30 minutes, pendant lesquelles s'applique
//! le régime d'alerte (anti-enfermement, BR-TRUST-020).

use time::{Duration, OffsetDateTime};

/// Suspension après un démarrage de la machine : le temps écoulé depuis le démarrage du noyau, pendant
/// lequel le mode se comporte comme une ALERTE (BR-TRUST-020).
pub const WINDOW: Duration = Duration::minutes(30);

/// Sortie automatique : le mode s'arrête quand aucune tentative n'a été refusée pendant cette durée
/// (BR-TRUST-019), mesurée sur l'horloge monotone.
pub const QUIET: Duration = Duration::minutes(30);

/// Une activation faite moins de cette durée après la fin de la précédente la **prolonge** : même
/// identifiant d'activation, essais non rendus (Q14 point 8, BR-TRUST-012).
pub const REARM: Duration = Duration::minutes(30);

/// Comment la dernière activation s'est terminée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndHow {
    /// Un administrateur l'a désactivé depuis le client.
    Manual,
    /// Elle s'est arrêtée toute seule (`QUIET`).
    Auto,
    /// Par la sous-commande `hearth-agent attack-mode off`.
    Cli,
}

impl EndHow {
    /// Le code stocké (`attack_mode.ended_how`).
    pub fn code(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Auto => "auto",
            Self::Cli => "cli",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        [Self::Manual, Self::Auto, Self::Cli]
            .into_iter()
            .find(|how| how.code() == code)
    }
}

/// L'état stocké : la ligne unique `attack_mode`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Stored {
    pub active: bool,
    /// Identifiant de l'activation (ULID). Reste après la fin : une réactivation proche le retrouve.
    pub activation_id: Option<String>,
    pub activated_at: Option<OffsetDateTime>,
    /// Identifiant de l'administrateur qui l'a activé (figé).
    pub activated_by: Option<String>,
    pub ended_at: Option<OffsetDateTime>,
    pub ended_how: Option<EndHow>,
    /// L'identifiant de démarrage et le temps écoulé (secondes) au moment de la fin : la garde de
    /// réactivation se mesure dessus, sans horloge murale.
    pub ended_boot_id: Option<String>,
    pub ended_uptime_s: Option<u64>,
    /// L'identifiant de démarrage vu au dernier lancement du service.
    pub last_boot_id: Option<String>,
    /// L'identifiant du démarrage pendant lequel la fenêtre est (ou a été) ouverte.
    pub window_boot_id: Option<String>,
    /// Réservé à toute commande future qui redémarre ou éteint la machine à la demande de Hearth : le
    /// démarrage courant, noté AVANT d'appeler le système. Rien ne l'écrit encore.
    pub remote_reboot_boot_id: Option<String>,
}

/// Ce que le noyau dit du démarrage en cours (port `BootInfo`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Boot {
    /// L'identifiant de ce démarrage ; `None` s'il est illisible (jamais de fenêtre alors).
    pub id: Option<String>,
    /// Le temps écoulé depuis le démarrage du noyau.
    pub uptime: Duration,
}

/// L'état du mode attaque à cet instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effective {
    Off,
    Active,
    /// Fenêtre de redémarrage : le régime d'alerte s'applique, pendant `remaining`.
    Suspended {
        remaining: Duration,
    },
}

/// L'état effectif : éteint si `active` est faux ; suspendu si la fenêtre de ce démarrage est ouverte
/// et que le temps écoulé depuis le démarrage est inférieur à `WINDOW` ; actif sinon. **Se calcule, ne
/// se stocke pas.**
pub fn effective(stored: &Stored, boot: &Boot) -> Effective {
    if !stored.active {
        return Effective::Off;
    }
    match (&boot.id, &stored.window_boot_id) {
        (Some(now), Some(window)) if now == window && boot.uptime < WINDOW => {
            Effective::Suspended {
                remaining: WINDOW - boot.uptime.max(Duration::ZERO),
            }
        }
        _ => Effective::Active,
    }
}

/// Au lancement du service, une fois. Rend l'état à stocker et si la fenêtre vient de s'ouvrir (une
/// entrée de journal « suspendu »).
///
/// - Identifiant illisible : rien ne change, **jamais de fenêtre** (l'inverse l'ouvrirait à chaque mise à
///   jour de l'agent, qu'une session administrateur peut déclencher à distance).
/// - Identifiant inchangé : redémarrage du service ou mise à jour de l'agent, aucune fenêtre
///   (BR-TRUST-021).
/// - Identifiant différent, mode actif, redémarrage non demandé par Hearth, et la machine vient de
///   démarrer (`uptime < WINDOW`) : la fenêtre s'ouvre. Sans plafond du nombre de fenêtres (risque
///   assumé, Q14 point 4).
/// - Identifiant différent mais machine démarrée depuis longtemps (base restaurée d'une sauvegarde) :
///   pas de fenêtre, le mode reste actif ; `hearth-agent attack-mode off` reste possible.
pub fn on_start(stored: Stored, boot: &Boot) -> (Stored, bool) {
    let Some(now) = &boot.id else {
        return (stored, false);
    };
    let rebooted = stored.last_boot_id.as_ref().is_some_and(|last| last != now);
    // Le démarrage qui vient de finir est celui que la commande de redémarrage avait noté.
    let by_hearth = stored.last_boot_id.is_some()
        && stored.remote_reboot_boot_id.is_some()
        && stored.remote_reboot_boot_id == stored.last_boot_id;
    let opens = stored.active && rebooted && !by_hearth && boot.uptime < WINDOW;
    let next = Stored {
        last_boot_id: Some(now.clone()),
        window_boot_id: if opens {
            Some(now.clone())
        } else {
            stored.window_boot_id.clone()
        },
        remote_reboot_boot_id: if rebooted {
            None
        } else {
            stored.remote_reboot_boot_id.clone()
        },
        ..stored
    };
    (next, opens)
}

/// La fenêtre est-elle terminée sans que la reprise ait été constatée ? Vrai quand le mode est actif et
/// qu'une fenêtre est notée mais ne s'applique plus : le temps écoulé a dépassé `WINDOW`, ou la machine a
/// redémarré depuis. La tâche périodique journalise alors la reprise et efface la marque : le mode est
/// actif, même identifiant d'activation, essais non rendus.
pub fn window_over(stored: &Stored, boot: &Boot) -> bool {
    stored.active
        && stored.window_boot_id.is_some()
        && !matches!(effective(stored, boot), Effective::Suspended { .. })
}

/// Ce que fait une activation demandée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activation {
    /// Déjà actif : rien ne change (idempotent).
    Already,
    /// Nouvelle activation : un nouvel identifiant, les essais précédents sont supprimés.
    Fresh,
    /// Moins de `REARM` après la fin de la précédente : même identifiant, essais non rendus.
    Prolong,
}

/// Que faire d'une demande d'activation, d'après l'état stocké ?
///
/// La garde se mesure sur le temps écoulé depuis le démarrage du noyau quand c'est le même démarrage
/// que celui de la fin ; sinon (machine redémarrée, identifiant illisible) sur l'horloge murale, et une
/// date de fin dans le futur (horloge reculée) compte comme récente : le doute profite à la prudence
/// (moins d'essais, jamais plus). **Borne** (HRT-28, BR-TRUST-047) : machine redémarrée et identifiant
/// lisible, la garde tombe après `REARM` de marche même si l'heure a été reculée.
pub fn plan_activation(stored: &Stored, boot: &Boot, now: OffsetDateTime) -> Activation {
    if stored.active {
        return Activation::Already;
    }
    if stored.activation_id.is_none() {
        return Activation::Fresh;
    }
    let same_boot = match (&boot.id, &stored.ended_boot_id, stored.ended_uptime_s) {
        (Some(current), Some(ended), Some(at)) if current == ended => Some(at),
        _ => None,
    };
    let recent = match same_boot {
        Some(ended_uptime_s) => {
            let ended = Duration::seconds(i64::try_from(ended_uptime_s).unwrap_or(i64::MAX));
            // Même démarrage : le temps écoulé ne recule jamais.
            boot.uptime - ended < REARM
        }
        None => {
            let wall_recent = stored.ended_at.is_some_and(|ended| now - ended < REARM);
            // Machine redémarrée depuis la fin (démarrage lisible et différent) : la fin a eu lieu AVANT
            // ce démarrage, donc le temps écoulé depuis elle est au moins le temps de marche. Passé
            // `REARM` de marche, la garde tombe, quelle que soit l'heure murale (BR-TRUST-047). Avant,
            // l'horloge murale ne peut que rallonger la garde (une fin « dans le futur » est récente).
            let other_boot = matches!(
                (&boot.id, &stored.ended_boot_id),
                (Some(current), Some(ended)) if current != ended
            );
            wall_recent && (!other_boot || boot.uptime < REARM)
        }
    };
    if recent {
        Activation::Prolong
    } else {
        Activation::Fresh
    }
}

/// La sortie automatique est-elle due ? `last` : l'instant monotone de la dernière tentative refusée (ou
/// de l'activation) ; `now` : l'instant monotone courant.
pub fn quiet_elapsed(last: Duration, now: Duration) -> bool {
    now - last >= QUIET
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at() -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH + Duration::days(20_000)
    }

    fn boot(id: Option<&str>, uptime_min: i64) -> Boot {
        Boot {
            id: id.map(str::to_owned),
            uptime: Duration::minutes(uptime_min),
        }
    }

    fn active(window: Option<&str>) -> Stored {
        Stored {
            active: true,
            activation_id: Some("A1".into()),
            activated_at: Some(at()),
            last_boot_id: Some("B1".into()),
            window_boot_id: window.map(str::to_owned),
            ..Stored::default()
        }
    }

    #[test]
    fn an_inactive_mode_is_off_whatever_the_boot() {
        for window in [None, Some("B1")] {
            let stored = Stored {
                active: false,
                window_boot_id: window.map(str::to_owned),
                ..active(None)
            };
            for id in [None, Some("B1"), Some("B2")] {
                for uptime in [0, 10, 29, 30, 600] {
                    assert_eq!(effective(&stored, &boot(id, uptime)), Effective::Off);
                }
            }
        }
    }

    #[test]
    fn the_effective_state_over_every_combination_of_window_boot_and_uptime() {
        // (identifiant courant, fenêtre notée, temps écoulé en minutes) -> état attendu, écrit à la main.
        let table: [(Option<&str>, Option<&str>, i64, Effective); 16] = [
            // Aucune fenêtre notée : actif, quoi que dise le démarrage.
            (Some("B1"), None, 0, Effective::Active),
            (Some("B1"), None, 10, Effective::Active),
            (None, None, 10, Effective::Active),
            (Some("B2"), None, 600, Effective::Active),
            // Fenêtre de ce démarrage, moins de 30 minutes : suspendu, temps restant exact.
            (
                Some("B1"),
                Some("B1"),
                0,
                Effective::Suspended {
                    remaining: Duration::minutes(30),
                },
            ),
            (
                Some("B1"),
                Some("B1"),
                10,
                Effective::Suspended {
                    remaining: Duration::minutes(20),
                },
            ),
            (
                Some("B1"),
                Some("B1"),
                29,
                Effective::Suspended {
                    remaining: Duration::minutes(1),
                },
            ),
            // 30 minutes pile et au-delà : le mode reprend.
            (Some("B1"), Some("B1"), 30, Effective::Active),
            (Some("B1"), Some("B1"), 31, Effective::Active),
            (Some("B1"), Some("B1"), 600, Effective::Active),
            // Fenêtre d'un AUTRE démarrage : elle ne s'applique pas.
            (Some("B2"), Some("B1"), 5, Effective::Active),
            // Identifiant illisible : jamais suspendu.
            (None, Some("B1"), 5, Effective::Active),
            (None, Some("B1"), 0, Effective::Active),
            (
                Some("B1"),
                Some("B1"),
                15,
                Effective::Suspended {
                    remaining: Duration::minutes(15),
                },
            ),
            (
                Some("B2"),
                Some("B2"),
                1,
                Effective::Suspended {
                    remaining: Duration::minutes(29),
                },
            ),
            (Some("B2"), Some("B1"), 0, Effective::Active),
        ];
        for (id, window, uptime, expected) in table {
            assert_eq!(
                effective(&active(window), &boot(id, uptime)),
                expected,
                "démarrage {id:?}, fenêtre {window:?}, {uptime} min"
            );
        }
    }

    #[test]
    fn a_service_restart_on_the_same_boot_opens_no_window_and_keeps_the_mode() {
        let (next, opens) = on_start(active(None), &boot(Some("B1"), 3));
        assert!(!opens);
        assert_eq!(next, active(None));
        assert_eq!(
            effective(&next, &boot(Some("B1"), 3)),
            Effective::Active,
            "BR-TRUST-021 : le mode survit au redémarrage du service"
        );
    }

    #[test]
    fn a_new_boot_of_the_machine_opens_the_window_for_that_boot_only() {
        let (next, opens) = on_start(active(None), &boot(Some("B2"), 1));
        assert!(opens);
        assert_eq!(next.last_boot_id.as_deref(), Some("B2"));
        assert_eq!(next.window_boot_id.as_deref(), Some("B2"));
        assert_eq!(
            effective(&next, &boot(Some("B2"), 1)),
            Effective::Suspended {
                remaining: Duration::minutes(29)
            }
        );
        // Le service redémarre pendant la fenêtre : il la retrouve avec le temps restant exact, sans
        // la rallonger.
        let (again, opens_again) = on_start(next.clone(), &boot(Some("B2"), 11));
        assert!(!opens_again);
        assert_eq!(again, next);
        assert_eq!(
            effective(&again, &boot(Some("B2"), 11)),
            Effective::Suspended {
                remaining: Duration::minutes(19)
            }
        );
        // Un service qui démarre après la fenêtre ne la rouvre pas.
        let (late, opens_late) = on_start(next, &boot(Some("B2"), 45));
        assert!(!opens_late);
        assert_eq!(effective(&late, &boot(Some("B2"), 45)), Effective::Active);
    }

    #[test]
    fn an_unreadable_boot_identifier_never_opens_a_window() {
        for stored in [active(None), active(Some("B1"))] {
            let (next, opens) = on_start(stored.clone(), &boot(None, 1));
            assert!(!opens);
            assert_eq!(next, stored, "rien n'est touché");
        }
    }

    #[test]
    fn a_mode_that_is_off_opens_no_window_but_the_boot_is_noted() {
        let stored = Stored {
            active: false,
            ..active(None)
        };
        let (next, opens) = on_start(stored, &boot(Some("B2"), 1));
        assert!(!opens);
        assert_eq!(next.last_boot_id.as_deref(), Some("B2"));
        assert_eq!(next.window_boot_id, None);
    }

    #[test]
    fn the_first_start_after_the_migration_has_no_previous_boot_and_opens_nothing() {
        let stored = Stored {
            last_boot_id: None,
            ..active(None)
        };
        let (next, opens) = on_start(stored, &boot(Some("B1"), 1));
        assert!(!opens);
        assert_eq!(next.last_boot_id.as_deref(), Some("B1"));
    }

    #[test]
    fn a_reboot_asked_by_hearth_opens_no_window_and_the_mark_is_cleared() {
        let stored = Stored {
            remote_reboot_boot_id: Some("B1".into()),
            ..active(None)
        };
        let (next, opens) = on_start(stored, &boot(Some("B2"), 1));
        assert!(!opens, "même rallumée par un réveil réseau");
        assert_eq!(next.remote_reboot_boot_id, None);
        assert_eq!(next.window_boot_id, None);
        // Une marque qui ne correspond pas au démarrage qui vient de finir ne protège pas.
        let stale = Stored {
            remote_reboot_boot_id: Some("B0".into()),
            ..active(None)
        };
        let (next, opens) = on_start(stale, &boot(Some("B2"), 1));
        assert!(opens);
        assert_eq!(next.remote_reboot_boot_id, None);
    }

    #[test]
    fn a_backup_restored_long_after_boot_keeps_the_mode_active_without_a_window() {
        // Base restaurée d'une sauvegarde où le mode était actif : l'identifiant de démarrage noté est
        // celui d'un autre démarrage, mais la machine tourne depuis des heures.
        let (next, opens) = on_start(active(None), &boot(Some("B9"), 600));
        assert!(!opens);
        assert_eq!(
            effective(&next, &boot(Some("B9"), 600)),
            Effective::Active,
            "ni suspendu ni éteint en silence"
        );
        // Restaurée juste après un démarrage : la fenêtre (régime d'alerte, 30 min) s'ouvre, ce qui
        // n'enferme personne.
        let (next, opens) = on_start(active(None), &boot(Some("B9"), 2));
        assert!(opens);
        assert!(matches!(
            effective(&next, &boot(Some("B9"), 2)),
            Effective::Suspended { .. }
        ));
    }

    #[test]
    fn the_end_of_the_window_is_detected_once_and_not_while_it_applies() {
        let open = active(Some("B2"));
        assert!(!window_over(&open, &boot(Some("B2"), 5)));
        assert!(window_over(&open, &boot(Some("B2"), 30)));
        // La machine a redémarré depuis : la marque est périmée.
        assert!(window_over(&open, &boot(Some("B3"), 1)));
        assert!(!window_over(&active(None), &boot(Some("B2"), 90)));
        let off = Stored {
            active: false,
            ..open
        };
        assert!(!window_over(&off, &boot(Some("B2"), 90)));
    }

    fn ended(how: EndHow, boot_id: Option<&str>, uptime_s: Option<u64>, ago_min: i64) -> Stored {
        Stored {
            active: false,
            ended_at: Some(at() - Duration::minutes(ago_min)),
            ended_how: Some(how),
            ended_boot_id: boot_id.map(str::to_owned),
            ended_uptime_s: uptime_s,
            ..active(None)
        }
    }

    #[test]
    fn a_first_activation_is_fresh_and_an_active_mode_is_left_alone() {
        assert_eq!(
            plan_activation(&Stored::default(), &boot(Some("B1"), 5), at()),
            Activation::Fresh
        );
        assert_eq!(
            plan_activation(&active(None), &boot(Some("B1"), 5), at()),
            Activation::Already
        );
    }

    #[test]
    fn a_reactivation_under_thirty_minutes_prolongs_the_activation_on_the_same_boot() {
        // Fin à 600 s d'uptime ; on réactive à 600 s + 29 min 59 s, puis à 600 s + 30 min.
        let stored = ended(EndHow::Manual, Some("B1"), Some(600), 0);
        let near = Boot {
            id: Some("B1".into()),
            uptime: Duration::seconds(600) + Duration::minutes(30) - Duration::seconds(1),
        };
        let far = Boot {
            id: Some("B1".into()),
            uptime: Duration::seconds(600) + Duration::minutes(30),
        };
        assert_eq!(plan_activation(&stored, &near, at()), Activation::Prolong);
        assert_eq!(plan_activation(&stored, &far, at()), Activation::Fresh);
    }

    #[test]
    fn the_wall_clock_cannot_move_the_guard_while_the_boot_is_the_same() {
        let stored = ended(EndHow::Auto, Some("B1"), Some(600), 0);
        let recent = Boot {
            id: Some("B1".into()),
            uptime: Duration::seconds(600) + Duration::minutes(5),
        };
        let old = Boot {
            id: Some("B1".into()),
            uptime: Duration::seconds(600) + Duration::hours(3),
        };
        for skew in [
            Duration::days(-365),
            Duration::days(365),
            Duration::hours(-5),
            Duration::ZERO,
        ] {
            assert_eq!(
                plan_activation(&stored, &recent, at() + skew),
                Activation::Prolong,
                "horloge murale décalée de {skew}"
            );
            assert_eq!(
                plan_activation(&stored, &old, at() + skew),
                Activation::Fresh,
                "horloge murale décalée de {skew}"
            );
        }
    }

    #[test]
    fn after_a_reboot_the_guard_falls_back_on_the_wall_clock_and_doubt_means_prolong() {
        let other_boot = boot(Some("B2"), 3);
        let recent = ended(EndHow::Manual, Some("B1"), Some(600), 10);
        assert_eq!(
            plan_activation(&recent, &other_boot, at()),
            Activation::Prolong
        );
        let old = ended(EndHow::Manual, Some("B1"), Some(600), 31);
        assert_eq!(plan_activation(&old, &other_boot, at()), Activation::Fresh);
        // Horloge reculée : la fin est « dans le futur », donc récente.
        let future = ended(EndHow::Manual, Some("B1"), Some(600), -120);
        assert_eq!(
            plan_activation(&future, &other_boot, at()),
            Activation::Prolong
        );
        // Identifiant illisible : l'horloge murale aussi.
        assert_eq!(
            plan_activation(&recent, &boot(None, 3), at()),
            Activation::Prolong
        );
        assert_eq!(
            plan_activation(&old, &boot(None, 3), at()),
            Activation::Fresh
        );
    }

    #[test]
    fn a_rebooted_machine_with_the_clock_set_back_keeps_the_guard_for_thirty_minutes_of_uptime_only()
     {
        // Fin sous le démarrage B1 ; la machine redémarre (B2) et l'heure est reculée d'un an.
        let stored = ended(EndHow::Manual, Some("B1"), Some(600), 10);
        let back = Duration::days(-365);
        let before = boot(Some("B2"), 29);
        let after = boot(Some("B2"), 31);
        assert_eq!(
            plan_activation(&stored, &before, at() + back),
            Activation::Prolong,
            "avant 30 minutes de marche, la garde tient"
        );
        assert_eq!(
            plan_activation(&stored, &after, at() + back),
            Activation::Fresh,
            "après 30 minutes de marche, l'heure reculée ne la tient plus"
        );
        // Même scénario avec une heure avancée, ou honnête : jamais plus tôt qu'avant la borne.
        for skew in [Duration::days(365), Duration::ZERO, Duration::hours(-5)] {
            assert_eq!(
                plan_activation(&stored, &after, at() + skew),
                Activation::Fresh
            );
        }
        // Même démarrage : inchangé (le temps de marche seul décide, l'heure murale est sans effet).
        let same = ended(EndHow::Manual, Some("B1"), Some(600), 10);
        let recent = Boot {
            id: Some("B1".into()),
            uptime: Duration::seconds(600) + Duration::minutes(5),
        };
        assert_eq!(
            plan_activation(&same, &recent, at() + back),
            Activation::Prolong
        );
        // Identifiant de démarrage illisible : l'horloge murale seule, comme avant.
        assert_eq!(
            plan_activation(&stored, &boot(None, 31), at() + back),
            Activation::Prolong
        );
    }

    #[test]
    fn the_automatic_exit_is_due_after_exactly_thirty_quiet_minutes() {
        let last = Duration::seconds(100);
        assert!(!quiet_elapsed(last, last));
        assert!(!quiet_elapsed(
            last,
            last + QUIET - Duration::milliseconds(1)
        ));
        assert!(quiet_elapsed(last, last + QUIET));
        assert!(quiet_elapsed(last, last + QUIET * 4));
        // L'horloge monotone ne recule pas ; si elle le faisait, rien ne sortirait.
        assert!(!quiet_elapsed(last, last - Duration::minutes(40)));
    }

    #[test]
    fn the_end_codes_round_trip() {
        for how in [EndHow::Manual, EndHow::Auto, EndHow::Cli] {
            assert_eq!(EndHow::from_code(how.code()), Some(how));
        }
        assert_eq!(EndHow::from_code("ailleurs"), None);
    }
}
