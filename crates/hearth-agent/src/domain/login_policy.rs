//! Décision d'une tentative de connexion : ce que savent les trois compteurs et la reconnaissance du
//! poste (règle « 2 critères sur 3 », ADR-0024), et ce qu'il en résulte (ADR-0022, ADR-0024,
//! BR-CONN-006, 007, 018, 019, 020).
//!
//! Fonctions pures, sans E/S, horloge en paramètre. Le cas d'usage lit les états, appelle ces
//! fonctions, écrit les états rendus.
//!
//! - Le compteur du **couple** (identifiant, adresse exacte) et celui de l'**adresse** (exacte)
//!   refusent avant toute vérification, comme avant HRT-20, quel que soit l'identifiant.
//! - Le ralentissement par **identifiant** (BR-CONN-018) ne refuse **qu'après** la vérification du
//!   mot de passe, que l'identifiant existe ou non (un haché factice sinon) : le même chemin, la
//!   même durée, la même réponse pour tout le monde. Seul le titulaire du mot de passe, depuis un
//!   poste que la règle « 2 critères sur 3 » reconnaît (`escapes_slowdown`, BR-CONN-019), passe
//!   malgré l'attente. Pour tous les autres, rien d'observable ne distingue un identifiant existant
//!   d'un identifiant inexistant (BR-CONN-013), même depuis une adresse retenue.
//! - Les échecs sont comptés **de la même façon** pour tous, que l'identifiant existe ou non. Les
//!   compteurs du couple et de l'adresse comptent tout échec. Le ralentissement par identifiant ne
//!   compte que les échecs d'une adresse inconnue de tous les comptes. Pendant une attente de
//!   l'identifiant, un mot de passe faux compte dans le couple pour tout le monde (le nombre
//!   d'essais d'une adresse connue usurpée reste borné) ; un mot de passe juste depuis une adresse
//!   inconnue du compte est refusé sans rien compter.

use time::{Duration, OffsetDateTime};

use super::identifier_slowdown::{self, Slowdown};
use super::lockout::{
    LockoutDecision, LockoutEvent, LockoutState, admits_in_queue, step, step_address,
};

/// Connexions en cours (en attente de leur tour ou en vérification), toutes adresses confondues.
/// Inférieur à la capacité du hacheur (4 calculs et 32 en attente, ADR-0009) : une connexion
/// admise ne reçoit jamais `503 BUSY` du hacheur.
pub const MAX_LOGINS_IN_FLIGHT: usize = 32;
/// Places de ce total réservées aux adresses déjà connues (d'un compte quelconque) : les autres
/// n'en prennent jamais plus de `MAX_LOGINS_IN_FLIGHT - RESERVED_FOR_KNOWN`.
pub const RESERVED_FOR_KNOWN: usize = 8;

/// Ce que l'on sait au moment d'une tentative.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoginState {
    /// Couple identifiant + adresse exacte.
    pub pair: LockoutState,
    /// Adresse exacte (tous identifiants confondus).
    pub address: LockoutState,
    /// Identifiant saisi (tous clients confondus).
    pub identifier: Slowdown,
    /// Ce poste échappe-t-il au ralentissement par identifiant ? Rendu par la règle « 2 critères sur 3 »
    /// (`trust::recognition::judge_login`, HRT-24), jamais décidé ici. Faux hors alerte (l'identifiant
    /// n'est pas ralenti, rien à éviter) et pour un identifiant qui n'existe pas.
    pub escapes_slowdown: bool,
    /// L'adresse est-elle connue d'un compte QUELCONQUE ? Les échecs d'une telle adresse ne nourrissent
    /// pas le ralentissement par identifiant, que l'identifiant existe ou non : sinon la
    /// progression du compteur dirait si l'adresse est connue du compte visé (BR-CONN-013).
    pub seen: bool,
}

/// Issue d'une tentative, une fois le mot de passe vérifié.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Connexion accordée.
    Granted,
    /// Mot de passe faux (ou identifiant inconnu) : échec compté ; attente éventuelle annoncée.
    Failed(Option<Duration>),
    /// Refusée parce que l'identifiant est ralenti, quel que soit le mot de passe : attente
    /// annoncée.
    Slowed(Duration),
    /// Refusée par le compteur du couple ou de l'adresse (relu dans la transaction) : rien n'est
    /// compté.
    Blocked(Duration),
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

fn wait_of(decision: LockoutDecision) -> Option<Duration> {
    longest_wait(&[decision])
}

fn longest(waits: &[Option<Duration>]) -> Option<Duration> {
    waits.iter().copied().flatten().max()
}

/// La tentative est-elle admise avant même de vérifier le mot de passe (compteurs du couple et de
/// l'adresse) ? `None` : oui. Sinon l'attente à annoncer. Le ralentissement par identifiant ne
/// joue pas ici (voir `conclude`).
pub fn admit(state: &LoginState, now: OffsetDateTime) -> Option<Duration> {
    longest(&[
        wait_of(step(state.pair, LockoutEvent::Attempt, now).1),
        wait_of(step_address(state.address, LockoutEvent::Attempt, now).1),
    ])
}

/// L'issue de la tentative, le mot de passe vérifié (`verified`, toujours faux pour un identifiant
/// qui n'existe pas). Rend les états à écrire et l'issue.
pub fn conclude(state: LoginState, verified: bool, now: OffsetDateTime) -> (LoginState, Verdict) {
    if let Some(wait) = admit(&state, now) {
        return (state, Verdict::Blocked(wait));
    }
    let (identifier, ident_wait) = identifier_slowdown::observe(&state.identifier, now);
    let state = LoginState {
        identifier,
        ..state
    };

    if let Some(slowed) = ident_wait {
        // L'identifiant est ralenti (ALERTE). Seul passe le poste que la règle « 2 critères sur 3 »
        // reconnaît (`escapes_slowdown`), avec le bon mot de passe : le mot de passe reste exigé.
        if verified && state.escapes_slowdown {
            return (success(state, now), Verdict::Granted);
        }
        // Refus identique pour tous. Un mot de passe faux compte dans le couple, pour tout le monde :
        // le nombre d'essais d'une adresse connue usurpée reste borné par ce compteur, et rien ne
        // distingue un identifiant existant d'un identifiant inexistant.
        if !verified {
            let (pair, decision) = step(state.pair, LockoutEvent::Failed, now);
            let wait = longest(&[Some(slowed), wait_of(decision)]).unwrap_or(slowed);
            return (LoginState { pair, ..state }, Verdict::Slowed(wait));
        }
        return (state, Verdict::Slowed(slowed));
    }

    if verified {
        return (success(state, now), Verdict::Granted);
    }
    // Échec compté de la même façon pour tous ; le ralentissement par identifiant ne compte que
    // les adresses inconnues de tous les comptes.
    let (pair, pair_decision) = step(state.pair, LockoutEvent::Failed, now);
    let (address, address_decision) = step_address(state.address, LockoutEvent::Failed, now);
    let (identifier, identifier_wait) = if state.seen {
        (state.identifier, None)
    } else {
        identifier_slowdown::record_failure(state.identifier, now)
    };
    (
        LoginState {
            pair,
            address,
            identifier,
            ..state
        },
        Verdict::Failed(longest(&[
            wait_of(pair_decision),
            wait_of(address_decision),
            identifier_wait,
        ])),
    )
}

/// Seul le compteur du couple repart à zéro. Le compteur par adresse et le ralentissement par
/// identifiant ne se remettent pas à zéro par un succès (un attaquant intercalerait sinon une
/// connexion valide) : ils s'éteignent avec le temps.
fn success(state: LoginState, now: OffsetDateTime) -> LoginState {
    let (pair, _) = step(state.pair, LockoutEvent::Succeeded, now);
    LoginState { pair, ..state }
}

/// Pourquoi une connexion n'est pas admise dans la file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueRefusal {
    /// Cette adresse a déjà tout son quota (une en cours, huit en attente).
    AddressFull,
    /// L'agent n'a plus de place pour ce type d'adresse (les inconnues s'arrêtent avant les
    /// places réservées).
    Saturated,
}

/// Une connexion de plus peut-elle être admise ? `total` : connexions déjà en cours, toutes
/// adresses confondues ; `address_in_flight` : celles de cette adresse (celle en cours de
/// traitement comprise). `known` : l'adresse est déjà connue de l'agent (une session valide, ou une
/// authentification réussie récente, d'un compte quelconque) : elle peut prendre les places
/// réservées. La décision ne dépend donc jamais de l'identifiant saisi (BR-CONN-013).
pub fn admit_login(
    total: usize,
    address_in_flight: usize,
    known: bool,
) -> Result<(), QueueRefusal> {
    if !admits_in_queue(address_in_flight) {
        return Err(QueueRefusal::AddressFull);
    }
    let ceiling = if known {
        MAX_LOGINS_IN_FLIGHT
    } else {
        MAX_LOGINS_IN_FLIGHT - RESERVED_FOR_KNOWN
    };
    if total >= ceiling {
        return Err(QueueRefusal::Saturated);
    }
    Ok(())
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

    /// Un état neuf ; une adresse connue du compte est forcément connue d'un compte.
    fn fresh(known: bool) -> LoginState {
        LoginState {
            pair: LockoutState::default(),
            address: LockoutState::default(),
            identifier: Slowdown::default(),
            escapes_slowdown: known,
            seen: known,
        }
    }

    /// Une attaque : `count` échecs venus d'adresses toutes différentes (un couple neuf et une
    /// adresse neuve à chaque fois), contre le même identifiant. Chaque échec est fait quand
    /// l'attente de l'identifiant est finie (l'attaquant n'attend que ce qu'il doit). Rend l'état
    /// de l'identifiant, l'instant du dernier échec, et les attentes imposées.
    fn attack_from_many_addresses(count: u32) -> (Slowdown, OffsetDateTime, Vec<Option<Duration>>) {
        let mut identifier = Slowdown::default();
        let mut now = t0();
        let mut last = t0();
        let mut waits = vec![];
        for _ in 0..count {
            let state = LoginState {
                identifier,
                ..fresh(false)
            };
            let (next, verdict) = conclude(state, false, now);
            let wait = match verdict {
                Verdict::Failed(wait) => wait,
                other => panic!("l'attaquant est admis et échoue : {other:?}"),
            };
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
    fn the_slowdown_refuses_a_brand_new_address_even_with_the_right_password() {
        let (identifier, last, _) = attack_from_many_addresses(12);
        let newcomer = LoginState {
            identifier,
            ..fresh(false)
        };
        let at = last + Duration::seconds(1);
        let (next, verdict) = conclude(newcomer, true, at);
        assert!(matches!(verdict, Verdict::Slowed(_)), "{verdict:?}");
        assert_eq!(
            next, newcomer,
            "un mot de passe juste n'est pas un échec : rien n'est compté"
        );
        let (next, verdict) = conclude(newcomer, false, at);
        assert!(matches!(verdict, Verdict::Slowed(_)), "{verdict:?}");
        assert_eq!(
            next.pair.failures, 1,
            "un mot de passe faux compte dans le couple"
        );
    }

    #[test]
    fn the_wait_is_never_longer_than_two_minutes_however_long_the_attack() {
        let (identifier, last, waits) = attack_from_many_addresses(200);
        assert_eq!(
            waits.last().copied().flatten(),
            Some(Duration::seconds(120))
        );
        let until = identifier.wait_until.expect("attente");
        assert_eq!(until - last, Duration::seconds(120));
        let state = LoginState {
            identifier,
            ..fresh(false)
        };
        let (_, verdict) = conclude(state, true, until);
        assert_eq!(verdict, Verdict::Granted, "une tentative reste possible");
    }

    #[test]
    fn the_owner_of_the_password_on_a_known_address_passes_during_the_attack() {
        let (identifier, last, _) = attack_from_many_addresses(30);
        let during_the_wait = last + Duration::seconds(1);
        let state = LoginState {
            identifier,
            ..fresh(true)
        };
        assert_eq!(conclude(state, true, during_the_wait).1, Verdict::Granted);
    }

    #[test]
    fn a_wrong_password_during_the_wait_counts_in_the_pair_whoever_sends_it() {
        let (identifier, last, _) = attack_from_many_addresses(30);
        let at = last + Duration::seconds(1);
        let known = LoginState {
            identifier,
            ..fresh(true)
        };
        let unknown = LoginState {
            escapes_slowdown: false,
            seen: false,
            ..known
        };
        let (after_known, verdict_known) = conclude(known, false, at);
        let (after_unknown, verdict_unknown) = conclude(unknown, false, at);
        // Même réponse, même compteur touché : rien ne distingue un compte à adresse connue d'un
        // identifiant inexistant.
        assert_eq!(verdict_known, verdict_unknown);
        assert!(matches!(verdict_known, Verdict::Slowed(_)));
        assert_eq!(after_known.pair.failures, 1);
        assert_eq!(after_unknown.pair.failures, 1);
        assert_eq!(after_known.address, LockoutState::default());
        assert_eq!(after_unknown.address, LockoutState::default());
    }

    #[test]
    fn a_spoofed_known_address_has_a_bounded_number_of_guesses_during_the_wait() {
        let (identifier, last, _) = attack_from_many_addresses(30);
        let mut state = LoginState {
            identifier,
            ..fresh(true)
        };
        let now = last + Duration::seconds(1);
        let mut guesses = 0;
        loop {
            if admit(&state, now).is_some() {
                break;
            }
            let (next, verdict) = conclude(state, false, now);
            assert!(matches!(verdict, Verdict::Slowed(_) | Verdict::Failed(_)));
            state = next;
            guesses += 1;
            assert!(guesses <= FAILURES_BEFORE_LOCK, "le couple verrouille");
        }
        assert_eq!(guesses, FAILURES_BEFORE_LOCK);
    }

    #[test]
    fn a_failure_from_an_address_known_to_an_account_does_not_feed_the_identifier() {
        // Que l'identifiant existe (adresse connue de SON compte) ou non (adresse connue d'un autre
        // compte, `known` faux, `seen` vrai) : le même compteur est touché, de la même façon.
        let of_the_account = fresh(true);
        let of_another = LoginState {
            escapes_slowdown: false,
            seen: true,
            ..fresh(false)
        };
        let (a, verdict_a) = conclude(of_the_account, false, t0());
        let (b, verdict_b) = conclude(of_another, false, t0());
        assert_eq!(verdict_a, verdict_b);
        assert_eq!(a.identifier, Slowdown::default());
        assert_eq!(b.identifier, Slowdown::default());
        assert_eq!((a.pair, a.address), (b.pair, b.address));
        // Une adresse inconnue de tous nourrit le ralentissement.
        let (c, _) = conclude(fresh(false), false, t0());
        assert_eq!(c.identifier.failures, 1);
    }

    #[test]
    fn a_known_address_is_blocked_by_its_own_address_counter_like_any_other() {
        // Aucune exemption du compteur par adresse : pas d'oracle d'existence (BR-CONN-013).
        let mut address = LockoutState::default();
        let mut now = t0();
        for _ in 0..ADDRESS_FAILURES_BEFORE_LOCK {
            (address, _) = step_address(address, LockoutEvent::Failed, now);
            now += Duration::seconds(1);
        }
        for known in [true, false] {
            let state = LoginState {
                address,
                ..fresh(known)
            };
            assert!(admit(&state, now).is_some(), "known = {known}");
        }
    }

    #[test]
    fn a_known_address_keeps_the_pair_counter_with_a_wrong_password() {
        let mut state = fresh(true);
        let mut now = t0();
        let mut verdict = Verdict::Granted;
        for _ in 0..FAILURES_BEFORE_LOCK {
            (state, verdict) = conclude(state, false, now);
            now += Duration::seconds(1);
        }
        assert_eq!(verdict, Verdict::Failed(Some(Duration::seconds(60))));
        let wait = admit(&state, now).expect("le couple verrouille");
        assert!(wait <= MAX_LOCK);
        assert!(matches!(conclude(state, true, now).1, Verdict::Blocked(_)));
    }

    #[test]
    fn an_existing_and_a_missing_identifier_get_the_same_verdicts_from_the_same_states() {
        // L'existence du compte n'entre que par `known` et par `verified`, tous deux faux pour un
        // identifiant inexistant : mêmes états, mêmes issues.
        let (identifier, last, _) = attack_from_many_addresses(14);
        for at in [
            t0(),
            last + Duration::seconds(1),
            last + Duration::minutes(5),
        ] {
            // Adresse connue d'un autre compte : même issue pour « marie » et pour « fantome ».
            let state = LoginState {
                identifier,
                seen: true,
                ..fresh(false)
            };
            assert_eq!(conclude(state, false, at), conclude(state, false, at));
        }
    }

    #[test]
    fn a_success_resets_only_the_pair_counter() {
        let mut state = fresh(false);
        for _ in 0..4 {
            (state, _) = conclude(state, false, t0());
        }
        let (after, verdict) = conclude(state, true, t0());
        assert_eq!(verdict, Verdict::Granted);
        assert_eq!(after.pair, LockoutState::default());
        assert_eq!(after.address, state.address);
        assert_eq!(after.identifier, state.identifier);
    }

    #[test]
    fn the_announced_wait_of_a_failure_is_the_longest_of_the_applicable_waits() {
        let identifier = Slowdown {
            failures: 20,
            wait_until: None,
            last_failure_at: Some(t0()),
            alerted_at: None,
        };
        let state = LoginState {
            identifier,
            ..fresh(false)
        };
        let (_, verdict) = conclude(state, false, t0() + Duration::seconds(1));
        // 21e échec : 2 s * 2^9 plafonné à 120 s.
        assert_eq!(verdict, Verdict::Failed(Some(Duration::seconds(120))));
    }

    #[test]
    fn unknown_addresses_never_take_the_places_reserved_for_known_ones() {
        let ceiling = MAX_LOGINS_IN_FLIGHT - RESERVED_FOR_KNOWN;
        assert_eq!(admit_login(ceiling - 1, 0, false), Ok(()));
        assert_eq!(
            admit_login(ceiling, 0, false),
            Err(QueueRefusal::Saturated),
            "plafond des inconnues"
        );
        assert_eq!(admit_login(ceiling, 0, true), Ok(()), "place réservée");
        assert_eq!(admit_login(MAX_LOGINS_IN_FLIGHT - 1, 0, true), Ok(()));
        assert_eq!(
            admit_login(MAX_LOGINS_IN_FLIGHT, 0, true),
            Err(QueueRefusal::Saturated),
            "plafond total"
        );
    }

    #[test]
    fn the_per_address_queue_still_applies_to_known_addresses() {
        assert_eq!(admit_login(0, MAX_WAITING_PER_ADDRESS, true), Ok(()));
        for known in [true, false] {
            assert_eq!(
                admit_login(0, MAX_WAITING_PER_ADDRESS + 1, known),
                Err(QueueRefusal::AddressFull)
            );
        }
    }

    #[test]
    fn the_reservation_leaves_room_to_the_unknown() {
        const { assert!(RESERVED_FOR_KNOWN > 0) };
        const { assert!(MAX_LOGINS_IN_FLIGHT > RESERVED_FOR_KNOWN) };
        assert_eq!(admit_login(0, 0, false), Ok(()));
    }
}
