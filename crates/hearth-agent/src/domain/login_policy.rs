//! Décision d'une tentative de connexion : ce que savent les trois compteurs et la liste des
//! adresses connues, et ce qu'il en résulte (ADR-0022, BR-CONN-006, 007, 018, 019, 020).
//!
//! Fonctions pures, sans E/S, horloge en paramètre. Le cas d'usage lit les états, appelle ces
//! fonctions, écrit les états rendus.
//!
//! - Le compteur du **couple** (identifiant, adresse exacte) s'applique toujours, y compris à une
//!   adresse connue : la connaître ne dispense jamais du mot de passe, et ne protège pas des
//!   essais répétés depuis cette adresse.
//! - Le compteur par **origine** (adresse IPv4, préfixe /64 IPv6) et le ralentissement par
//!   **identifiant** ne s'appliquent qu'aux adresses **non connues** du compte visé. Les échecs
//!   d'une adresse connue ne les nourrissent pas non plus : usurper une adresse connue ne gêne
//!   personne d'autre.

use time::{Duration, OffsetDateTime};

use super::identifier_slowdown::{self, Slowdown};
use super::lockout::{
    LockoutDecision, LockoutEvent, LockoutState, admits_in_queue, step, step_address,
};

/// Connexions en cours (en attente de leur tour ou en vérification), toutes adresses confondues.
/// Inférieur à la capacité du hacheur (4 calculs et 32 en attente, ADR-0009) : une connexion
/// admise ne reçoit jamais `503 BUSY` du hacheur.
pub const MAX_LOGINS_IN_FLIGHT: usize = 32;
/// Places de ce total réservées aux adresses connues du compte visé : les autres n'en prennent
/// jamais plus de `MAX_LOGINS_IN_FLIGHT - RESERVED_FOR_KNOWN`.
pub const RESERVED_FOR_KNOWN: usize = 8;

/// Ce que l'on sait au moment d'une tentative.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Standing {
    /// Couple identifiant + adresse exacte.
    pub pair: LockoutState,
    /// Origine de l'adresse (tous identifiants confondus).
    pub origin: LockoutState,
    /// Identifiant saisi (tous clients confondus).
    pub identifier: Slowdown,
    /// L'adresse est-elle connue du compte visé ? Faux pour un identifiant qui n'existe pas.
    pub known: bool,
}

/// Attente la plus longue imposée par l'une des décisions, s'il y en a une.
pub fn longest_wait(decisions: &[LockoutDecision]) -> Option<Duration> {
    decisions
        .iter()
        .filter_map(|decision| match decision {
            LockoutDecision::Blocked { retry_after } => Some(*retry_after),
            LockoutDecision::Allowed => None,
        })
        .max()
}

fn longest(waits: [Option<Duration>; 3]) -> Option<Duration> {
    waits.into_iter().flatten().max()
}

fn wait_of(decision: LockoutDecision) -> Option<Duration> {
    longest_wait(&[decision])
}

/// La tentative est-elle admise avant même de vérifier le mot de passe ? `None` : oui. Sinon,
/// l'attente à annoncer (la plus longue des attentes qui s'appliquent).
pub fn admit(standing: &Standing, now: OffsetDateTime) -> Option<Duration> {
    let pair = wait_of(step(standing.pair, LockoutEvent::Attempt, now).1);
    if standing.known {
        return pair;
    }
    longest([
        pair,
        wait_of(step_address(standing.origin, LockoutEvent::Attempt, now).1),
        identifier_slowdown::remaining(&standing.identifier, now),
    ])
}

/// Le mot de passe (ou l'identifiant) était faux. Rend les nouveaux états et l'attente que cet
/// échec impose, s'il y en a une (elle est annoncée à la réponse qui le refuse).
pub fn after_failure(standing: Standing, now: OffsetDateTime) -> (Standing, Option<Duration>) {
    let (pair, pair_decision) = step(standing.pair, LockoutEvent::Failed, now);
    if standing.known {
        return (Standing { pair, ..standing }, wait_of(pair_decision));
    }
    let (origin, origin_decision) = step_address(standing.origin, LockoutEvent::Failed, now);
    let (identifier, identifier_wait) =
        identifier_slowdown::record_failure(standing.identifier, now);
    (
        Standing {
            pair,
            origin,
            identifier,
            known: false,
        },
        longest([
            wait_of(pair_decision),
            wait_of(origin_decision),
            identifier_wait,
        ]),
    )
}

/// La connexion a réussi : seul le compteur du couple repart à zéro. Le compteur par origine et
/// le ralentissement par identifiant ne se remettent pas à zéro par un succès (un attaquant
/// intercalerait sinon une connexion valide) : ils s'éteignent avec le temps.
pub fn after_success(standing: Standing, now: OffsetDateTime) -> Standing {
    let (pair, _) = step(standing.pair, LockoutEvent::Succeeded, now);
    Standing { pair, ..standing }
}

/// Une connexion de plus peut-elle être admise ? `total` : connexions déjà en cours, toutes
/// adresses confondues ; `address_in_flight` : celles de cette adresse (celle en cours de
/// traitement comprise). Les adresses inconnues ne prennent pas les places réservées aux
/// adresses connues.
pub fn admits_login(total: usize, address_in_flight: usize, known: bool) -> bool {
    let ceiling = if known {
        MAX_LOGINS_IN_FLIGHT
    } else {
        MAX_LOGINS_IN_FLIGHT - RESERVED_FOR_KNOWN
    };
    total < ceiling && admits_in_queue(address_in_flight)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::lockout::{
        ADDRESS_FAILURES_BEFORE_LOCK, FAILURES_BEFORE_LOCK, MAX_LOCK, MAX_WAITING_PER_ADDRESS,
    };

    fn t0() -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH + Duration::days(20_000)
    }

    fn fresh(known: bool) -> Standing {
        Standing {
            pair: LockoutState::default(),
            origin: LockoutState::default(),
            identifier: Slowdown::default(),
            known,
        }
    }

    /// Une attaque : `count` échecs venus d'adresses toutes différentes (un couple neuf et une
    /// origine neuve à chaque fois), contre le même identifiant. Chaque échec est fait quand
    /// l'attente de l'identifiant est finie (l'attaquant n'attend que ce qu'il doit). Rend l'état
    /// de l'identifiant, l'instant du dernier échec, et les attentes imposées.
    fn attack_from_many_addresses(count: u32) -> (Slowdown, OffsetDateTime, Vec<Option<Duration>>) {
        let mut identifier = Slowdown::default();
        let mut now = t0();
        let mut last = t0();
        let mut waits = vec![];
        for _ in 0..count {
            let standing = Standing {
                identifier,
                ..fresh(false)
            };
            assert_eq!(admit(&standing, now), None, "l'attaquant est admis");
            let (next, wait) = after_failure(standing, now);
            identifier = next.identifier;
            waits.push(wait);
            last = now;
            now = identifier.wait_until.unwrap_or(now) + Duration::seconds(1);
        }
        (identifier, last, waits)
    }

    #[test]
    fn an_attack_from_many_addresses_is_slowed_by_the_identifier() {
        let (_, _, waits) = attack_from_many_addresses(16);
        // Dix essais gratuits, puis une attente croissante, jamais au-delà de deux minutes.
        assert!(waits[..10].iter().all(Option::is_none));
        assert_eq!(waits[10], Some(Duration::seconds(2)));
        assert_eq!(waits[11], Some(Duration::seconds(4)));
        assert_eq!(waits[15], Some(Duration::seconds(64)));
        assert!(
            waits
                .iter()
                .flatten()
                .all(|wait| *wait <= Duration::seconds(120))
        );
    }

    #[test]
    fn the_slowdown_applies_to_a_brand_new_address() {
        let (identifier, last, _) = attack_from_many_addresses(12);
        let newcomer = Standing {
            identifier,
            ..fresh(false)
        };
        assert!(admit(&newcomer, last + Duration::seconds(1)).is_some());
    }

    #[test]
    fn the_wait_is_never_longer_than_two_minutes_however_long_the_attack() {
        let (identifier, last, waits) = attack_from_many_addresses(200);
        assert_eq!(
            waits.last().copied().flatten(),
            Some(Duration::seconds(120))
        );
        let standing = Standing {
            identifier,
            ..fresh(false)
        };
        let until = identifier.wait_until.expect("attente");
        assert_eq!(until - last, Duration::seconds(120));
        assert_eq!(
            admit(&standing, until),
            None,
            "une tentative reste possible"
        );
    }

    #[test]
    fn a_known_address_is_never_slowed_by_the_failures_of_others() {
        let (identifier, last, _) = attack_from_many_addresses(30);
        let during_the_wait = last + Duration::seconds(1);
        let attacked = Standing {
            identifier,
            ..fresh(true)
        };
        assert_eq!(admit(&attacked, during_the_wait), None);
        let stranger = Standing {
            known: false,
            ..attacked
        };
        assert!(admit(&stranger, during_the_wait).is_some());
    }

    #[test]
    fn a_known_address_is_never_blocked_by_the_origin_counter() {
        let mut origin = LockoutState::default();
        let mut now = t0();
        for _ in 0..ADDRESS_FAILURES_BEFORE_LOCK {
            (origin, _) = step_address(origin, LockoutEvent::Failed, now);
            now += Duration::seconds(1);
        }
        assert!(origin.locked_until.is_some(), "l'origine est bloquée");
        let known = Standing {
            origin,
            ..fresh(true)
        };
        assert_eq!(admit(&known, now), None);
        let stranger = Standing {
            known: false,
            ..known
        };
        assert!(admit(&stranger, now).is_some());
    }

    #[test]
    fn a_known_address_keeps_the_pair_counter_with_a_wrong_password() {
        // Adresse connue usurpée : chaque essai à mauvais mot de passe compte dans le couple.
        let mut standing = fresh(true);
        let mut now = t0();
        let mut wait = None;
        for _ in 0..FAILURES_BEFORE_LOCK {
            (standing, wait) = after_failure(standing, now);
            now += Duration::seconds(1);
        }
        assert_eq!(wait, Some(Duration::seconds(60)));
        assert!(admit(&standing, now).is_some(), "le couple verrouille");
        let wait = admit(&standing, now).expect("attente");
        assert!(wait <= MAX_LOCK);
    }

    #[test]
    fn the_failures_of_a_known_address_feed_neither_the_origin_nor_the_identifier() {
        let mut standing = fresh(true);
        let now = t0();
        for _ in 0..4 {
            (standing, _) = after_failure(standing, now);
        }
        assert_eq!(standing.origin, LockoutState::default());
        assert_eq!(standing.identifier, Slowdown::default());
        assert_eq!(standing.pair.failures, 4);
    }

    #[test]
    fn an_unknown_address_feeds_the_three_counters() {
        let (standing, _) = after_failure(fresh(false), t0());
        assert_eq!(standing.pair.failures, 1);
        assert_eq!(standing.origin.failures, 1);
        assert_eq!(standing.identifier.failures, 1);
    }

    #[test]
    fn a_non_existent_identifier_is_slowed_exactly_like_an_existing_one() {
        // Même état, même entrée : aucune différence possible. L'existence du compte n'entre que
        // par `known`, qui est faux dans les deux cas pour une adresse jamais connectée.
        let existing = after_failure(fresh(false), t0());
        let missing = after_failure(fresh(false), t0());
        assert_eq!(existing, missing);
        let (identifier, _, existing_waits) = attack_from_many_addresses(14);
        let (identifier_again, _, missing_waits) = attack_from_many_addresses(14);
        assert_eq!(identifier, identifier_again);
        assert_eq!(existing_waits, missing_waits);
    }

    #[test]
    fn a_success_resets_only_the_pair_counter() {
        let mut standing = fresh(false);
        for _ in 0..4 {
            (standing, _) = after_failure(standing, t0());
        }
        let after = after_success(standing, t0());
        assert_eq!(after.pair, LockoutState::default());
        assert_eq!(after.origin, standing.origin);
        assert_eq!(after.identifier, standing.identifier);
    }

    #[test]
    fn the_announced_wait_is_the_longest_of_the_applicable_waits() {
        let identifier = Slowdown {
            failures: 20,
            wait_until: Some(t0() + Duration::seconds(90)),
            last_failure_at: Some(t0()),
        };
        let origin = LockoutState {
            failures: 20,
            locked_until: Some(t0() + Duration::seconds(30)),
            window_started_at: Some(t0()),
        };
        let standing = Standing {
            identifier,
            origin,
            ..fresh(false)
        };
        assert_eq!(admit(&standing, t0()), Some(Duration::seconds(90)));
    }

    #[test]
    fn unknown_addresses_never_take_the_places_reserved_for_known_ones() {
        let ceiling = MAX_LOGINS_IN_FLIGHT - RESERVED_FOR_KNOWN;
        assert!(admits_login(ceiling - 1, 0, false));
        assert!(!admits_login(ceiling, 0, false), "plafond des inconnues");
        assert!(admits_login(ceiling, 0, true), "place réservée");
        assert!(admits_login(MAX_LOGINS_IN_FLIGHT - 1, 0, true));
        assert!(
            !admits_login(MAX_LOGINS_IN_FLIGHT, 0, true),
            "plafond total"
        );
    }

    #[test]
    fn the_per_address_queue_still_applies_to_known_addresses() {
        assert!(admits_login(0, MAX_WAITING_PER_ADDRESS, true));
        assert!(!admits_login(0, MAX_WAITING_PER_ADDRESS + 1, true));
        assert!(!admits_login(0, MAX_WAITING_PER_ADDRESS + 1, false));
    }

    #[test]
    fn the_reservation_leaves_room_to_the_unknown_and_stays_under_the_hasher() {
        const { assert!(RESERVED_FOR_KNOWN > 0) };
        const { assert!(MAX_LOGINS_IN_FLIGHT > RESERVED_FOR_KNOWN) };
        assert!(admits_login(0, 0, false));
    }
}
