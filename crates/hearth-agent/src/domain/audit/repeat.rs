//! Regroupement des événements identiques répétés (BR-AUDIT-007) : un compte lecture seule qui
//! boucle sur la lecture du journal, ou un client qui échoue en boucle, ne doit pas faire tourner
//! les 50 000 entrées et en chasser l'historique utile.
//!
//! **Deux niveaux.**
//!
//! 1. **Le groupe par adresse** (Q14, point 9 : « on regroupe par adresse »). Clé : le compte
//!    authentifié (ou « anonyme »), l'**adresse** de la connexion, l'action, le résultat, la cible et la
//!    raison. **Ni le nom du poste ni l'identifiant saisi** : un client les fait varier à volonté. Le
//!    premier événement du groupe est écrit tout de suite ; les suivants sont comptés ; **une** entrée de
//!    synthèse (la dernière occurrence, au compte exact) est écrite à la fin de la fenêtre. Tant que le
//!    groupe revient, la fenêtre suivante double (1, 2, 4, 8 puis 15 minutes, `MAX_WINDOW`) ; sans
//!    occurrence depuis `QUIET_RESET` il est oublié.
//! 2. **Le plafond par famille** (Q14, point 10 : volume borné). Famille = la clé SANS adresse. Au plus
//!    `MAX_ADDRESSES` adresses distinctes obtiennent leur groupe par fenêtre de famille. Les tentatives
//!    des adresses suivantes ne sont **pas écrites une par une** : elles sont comptées, et **une**
//!    entrée de synthèse « N tentatives depuis M adresses » est écrite à la fin de la fenêtre de la
//!    famille (compte exact des tentatives ; adresses distinctes comptées jusqu'à `MAX_TRACKED_ADDRESSES`,
//!    puis plafonnées : la mémoire ne grandit pas avec le nombre d'adresses de l'attaquant ; aucune
//!    adresse n'est écrite). La fenêtre de la famille s'allonge de la même façon, et aussi dès qu'elle a vu
//!    `ADDRESSES_BEFORE_GROWING` adresses distinctes, dépassement ou non.
//!
//! Aucune tentative n'est perdue : chaque événement est écrit tout de suite, compté dans un groupe, ou
//! compté dans une famille ; la somme des entrées et des synthèses est le nombre réel d'événements.
//!
//! Le nombre de groupes et de familles suivis est plafonné. Au débordement, les événements nouveaux ne
//! sont plus écrits un par un : ils sont comptés dans **un groupe de débordement unique**, résumé par
//! une entrée « activité trop variée, N événements regroupés » dont la fenêtre s'allonge de même.
//! Fonction pure : aucune horloge, aucune E/S.

use std::collections::{HashMap, HashSet};

use time::{Duration, OffsetDateTime};

use super::action::AuditAction;
use super::event::{AuditEvent, OutcomeKind};

/// Durée de la première fenêtre d'un groupe ou d'une famille.
pub const REPEAT_WINDOW: Duration = Duration::seconds(60);
/// Plafond de la fenêtre qui s'allonge.
pub const MAX_WINDOW: Duration = Duration::minutes(15);
/// Sans occurrence depuis ce délai, un groupe est oublié et sa fenêtre repart à `REPEAT_WINDOW`.
pub const QUIET_RESET: Duration = Duration::minutes(30);
/// Groupes (et familles) suivis en même temps, au plus.
pub const MAX_TRACKED: usize = 1024;
/// Adresses distinctes qui ont chacune leur groupe, par famille et par fenêtre de famille.
pub const MAX_ADDRESSES: usize = 8;
/// À partir de ce nombre d'adresses distinctes dans une fenêtre de famille, la fenêtre suivante double,
/// même sans dépassement : sous le plafond, une attaque qui change d'adresse à chaque tentative et reste
/// lente (quatre par minute) ne resterait pas sans frein.
pub const ADDRESSES_BEFORE_GROWING: usize = 4;
/// Adresses distinctes comptées au-delà de `MAX_ADDRESSES`, par famille et par fenêtre : au-delà, le
/// nombre d'adresses de la synthèse est plafonné à cette valeur (la mémoire est bornée).
pub const MAX_TRACKED_ADDRESSES: usize = 256;

/// La clé sans adresse : ce que le serveur connaît ou borne.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Family {
    /// `None` : anonyme (aucun compte authentifié).
    account: Option<String>,
    action: AuditAction,
    outcome: OutcomeKind,
    target: Option<String>,
    reason: Option<String>,
}

/// La clé d'un groupe : la famille et l'adresse de la connexion (`None` : ligne de commande).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    family: Family,
    address: Option<String>,
}

impl Key {
    fn of(event: &AuditEvent) -> Self {
        Self {
            family: Family {
                account: event.actor.account.as_ref().map(ToString::to_string),
                action: event.action,
                outcome: event.outcome.kind(),
                target: event.target.text(),
                // Raison SANS donnée variable : la durée d'une attente ne fait pas un groupe neuf.
                reason: event.outcome.reason().map(|reason| reason.group_text()),
            },
            address: event.actor.origin.addr().map(str::to_owned),
        }
    }
}

#[derive(Debug)]
struct Window {
    started: OffsetDateTime,
    /// Durée de la fenêtre en cours.
    interval: Duration,
    /// Occurrences comptées sans être écrites.
    suppressed: u32,
    /// La dernière occurrence : sa date, son origine, tout ce que la synthèse reprend.
    last: AuditEvent,
    /// Date de la dernière occurrence (écrite ou comptée).
    last_seen: OffsetDateTime,
}

impl Window {
    fn opened(event: AuditEvent, suppressed: u32) -> Self {
        Self {
            started: event.at,
            interval: REPEAT_WINDOW,
            suppressed,
            last_seen: event.at,
            last: event,
        }
    }

    fn count(&mut self, event: AuditEvent) {
        self.suppressed = self.suppressed.saturating_add(1);
        self.last_seen = event.at;
        self.last = event;
    }

    fn ended_at(&self, now: OffsetDateTime) -> bool {
        now - self.started >= self.interval
    }

    /// La fenêtre est finie à `now` : la dernière occurrence et le nombre d'occurrences comptées à
    /// résumer (s'il y en a), et si le groupe continue. Une fenêtre qui a compté des occurrences, ou
    /// que `full` dit saturée, continue avec une fenêtre double ; sinon elle s'oublie dès sa première
    /// fenêtre, et plus tard après `QUIET_RESET` sans occurrence.
    fn close(&mut self, now: OffsetDateTime, full: bool) -> (Option<(AuditEvent, u32)>, bool) {
        let summary = (self.suppressed > 0).then(|| (self.last.clone(), self.suppressed));
        let grows = self.suppressed > 0 || full;
        let quiet = (now - self.last_seen).abs() >= QUIET_RESET;
        let keep = !quiet && (grows || self.interval > REPEAT_WINDOW);
        if keep {
            if grows {
                self.interval = (self.interval * 2_i32).min(MAX_WINDOW);
            }
            self.started = now;
            self.suppressed = 0;
        }
        (summary, keep)
    }
}

/// Une famille : sa fenêtre compte les tentatives des adresses qui n'ont pas eu leur groupe.
#[derive(Debug)]
struct FamilyWindow {
    window: Window,
    /// Adresses qui ont eu leur groupe dans cette fenêtre de famille.
    admitted: usize,
    /// Adresses distinctes comptées sans groupe (au plus `MAX_TRACKED_ADDRESSES`).
    overflow: HashSet<String>,
}

/// Les fenêtres en cours.
#[derive(Debug, Default)]
pub struct RepeatFilter {
    windows: HashMap<Key, Window>,
    families: HashMap<Family, FamilyWindow>,
    /// Groupe de débordement : ses occurrences ne sont écrites que dans sa synthèse.
    overflow: Option<Window>,
}

impl RepeatFilter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ce qu'il faut écrire pour cet événement : rien (compté dans son groupe ou sa famille),
    /// l'événement lui-même (premier de son groupe), précédé des synthèses des fenêtres qui viennent
    /// de finir.
    pub fn admit(&mut self, event: AuditEvent) -> Vec<AuditEvent> {
        let mut out = self.due(event.at);
        let key = Key::of(&event);
        if let Some(window) = self.windows.get_mut(&key) {
            window.count(event);
            return out;
        }
        let known_family = self.families.contains_key(&key.family);
        if self.windows.len() >= MAX_TRACKED
            || (!known_family && self.families.len() >= MAX_TRACKED)
        {
            // Débordement : compté, pas écrit un par un.
            match &mut self.overflow {
                Some(window) => window.count(event),
                None => self.overflow = Some(Window::opened(event, 1)),
            }
            return out;
        }
        let family = self
            .families
            .entry(key.family.clone())
            .or_insert_with(|| FamilyWindow {
                window: Window::opened(event.clone(), 0),
                admitted: 0,
                overflow: HashSet::new(),
            });
        if family.admitted >= MAX_ADDRESSES {
            // Plafond par clé sans adresse : l'adresse n'a pas son groupe, ses tentatives sont
            // comptées dans la famille.
            if let Some(address) = &key.address
                && family.overflow.len() < MAX_TRACKED_ADDRESSES
            {
                family.overflow.insert(address.clone());
            }
            family.window.count(event);
            return out;
        }
        family.admitted += 1;
        family.window.last_seen = event.at;
        self.windows.insert(key, Window::opened(event.clone(), 0));
        out.push(event);
        out
    }

    /// Les synthèses des fenêtres finies à `now` (les groupes sans répétition s'oublient en
    /// silence).
    pub fn due(&mut self, now: OffsetDateTime) -> Vec<AuditEvent> {
        let ended: Vec<Key> = self
            .windows
            .iter()
            .filter(|(_, window)| window.ended_at(now))
            .map(|(key, _)| key.clone())
            .collect();
        let mut out = Vec::new();
        for key in ended {
            let Some(window) = self.windows.get_mut(&key) else {
                continue;
            };
            let (summary, keep) = window.close(now, false);
            out.extend(summary.map(|(event, count)| event.with_repeats(count)));
            if !keep {
                self.windows.remove(&key);
            }
        }
        let ended: Vec<Family> = self
            .families
            .iter()
            .filter(|(_, family)| family.window.ended_at(now))
            .map(|(key, _)| key.clone())
            .collect();
        for key in ended {
            let Some(family) = self.families.get_mut(&key) else {
                continue;
            };
            let full = family.admitted >= ADDRESSES_BEFORE_GROWING;
            let (summary, keep) = family.window.close(now, full);
            if let Some((event, attempts)) = summary {
                let addresses = u32::try_from(family.overflow.len()).unwrap_or(u32::MAX);
                out.push(event.with_addresses(attempts, addresses));
            }
            family.admitted = 0;
            family.overflow.clear();
            if !keep {
                self.families.remove(&key);
            }
        }
        if let Some(window) = self.overflow.as_mut()
            && window.ended_at(now)
        {
            let (summary, keep) = window.close(now, false);
            out.extend(summary.map(|(event, count)| event.as_overflow(count)));
            if !keep {
                self.overflow = None;
            }
        }
        out.sort_by_key(|event| event.at);
        out
    }

    /// Toutes les synthèses en attente, fenêtres finies ou non : à l'arrêt de l'agent.
    pub fn drain(&mut self) -> Vec<AuditEvent> {
        let mut out: Vec<AuditEvent> = self
            .windows
            .drain()
            .map(|(_, window)| window)
            .filter(|window| window.suppressed > 0)
            .map(|window| window.last.with_repeats(window.suppressed))
            .collect();
        out.extend(
            self.families
                .drain()
                .map(|(_, family)| family)
                .filter(|family| family.window.suppressed > 0)
                .map(|family| {
                    let addresses = u32::try_from(family.overflow.len()).unwrap_or(u32::MAX);
                    family
                        .window
                        .last
                        .with_addresses(family.window.suppressed, addresses)
                }),
        );
        if let Some(window) = self.overflow.take() {
            out.push(window.last.as_overflow(window.suppressed));
        }
        out.sort_by_key(|event| event.at);
        out
    }

    /// Groupes suivis (tests).
    pub fn tracked(&self) -> usize {
        self.windows.len()
    }

    /// Familles suivies (tests).
    pub fn families(&self) -> usize {
        self.families.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::accounts::Username;
    use crate::domain::audit::{Actor, Origin, Outcome, Reason, Target};

    fn t(seconds: i64) -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH + Duration::seconds(1_790_000_000 + seconds)
    }

    fn event_from(seconds: i64, account: Option<&str>, host: &str, addr: &str) -> AuditEvent {
        AuditEvent::new(
            t(seconds),
            Actor::new(
                account.map(|name| Username::parse(name).unwrap()),
                Origin::client(Some(host), addr),
            ),
            AuditAction::AuditRead,
            Target::Route("/audit"),
            Outcome::Denied(Reason::ReadOnly),
        )
    }

    fn denied(seconds: i64, account: &str, addr: &str) -> AuditEvent {
        event_from(seconds, Some(account), "poste", addr)
    }

    #[test]
    fn the_first_event_is_written_the_identical_ones_of_the_window_are_only_counted() {
        let mut filter = RepeatFilter::new();
        assert_eq!(filter.admit(denied(0, "lucas", "10.0.0.1")).len(), 1);
        for second in 1..60 {
            assert!(filter.admit(denied(second, "lucas", "10.0.0.1")).is_empty());
        }
        assert_eq!(filter.tracked(), 1);
    }

    #[test]
    fn the_host_does_not_make_events_different_but_the_address_does() {
        let mut filter = RepeatFilter::new();
        let mut written = 0;
        for n in 0..5000 {
            let event = event_from(n % 59, Some("lucas"), &format!("poste-{n}"), "10.0.0.1");
            written += filter.admit(event).len();
        }
        assert_eq!(written, 1, "un seul groupe, un seul premier");
        assert_eq!(filter.tracked(), 1);
        let summaries = filter.due(t(60));
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].repeat_count, 4999);
        // Une autre adresse est un autre groupe : le journal dit d'où viennent les refus.
        assert_eq!(
            filter
                .admit(event_from(61, Some("lucas"), "poste-0", "10.0.0.2"))
                .len(),
            1
        );
        // La synthèse porte l'origine de la dernière occurrence.
        assert_eq!(summaries[0].actor.origin.name(), Some("poste-4999"));
    }

    #[test]
    fn one_summary_is_written_when_the_window_ends() {
        let mut filter = RepeatFilter::new();
        filter.admit(denied(0, "lucas", "10.0.0.1"));
        for second in 1..=999 {
            filter.admit(denied(second % 59 + 1, "lucas", "10.0.0.1"));
        }
        assert!(filter.due(t(59)).is_empty(), "la fenêtre n'est pas finie");
        let summaries = filter.due(t(60));
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].repeat_count, 999);
        assert!(filter.due(t(600)).is_empty(), "une seule synthèse");
        // BR-AUDIT-007 (Q14, point 9) : un groupe répété reste suivi, sa fenêtre suivante double ; il
        // ne s'oublie qu'après 30 minutes sans occurrence.
        assert_eq!(filter.tracked(), 1);
        assert!(filter.due(t(60 + 31 * 60)).is_empty());
        assert_eq!(filter.tracked(), 0);
    }

    #[test]
    fn a_new_event_after_the_window_brings_the_summary_then_opens_a_new_window() {
        let mut filter = RepeatFilter::new();
        filter.admit(denied(0, "lucas", "10.0.0.1"));
        filter.admit(denied(10, "lucas", "10.0.0.1"));
        filter.admit(denied(20, "lucas", "10.0.0.1"));
        let written = filter.admit(denied(61, "lucas", "10.0.0.1"));
        // La fenêtre qui s'allonge (BR-AUDIT-007) : la synthèse de la première fenêtre part, et
        // l'événement suivant est compté dans la fenêtre suivante, de deux minutes.
        assert_eq!(written.len(), 1);
        assert_eq!(written[0].repeat_count, 2);
        assert!(filter.due(t(61 + 119)).is_empty());
        let next = filter.due(t(61 + 120));
        assert_eq!(next.len(), 1);
        assert_eq!(next[0].repeat_count, 1);
        assert_eq!(next[0].at, t(61));
    }

    #[test]
    fn without_repetition_there_is_no_summary() {
        let mut filter = RepeatFilter::new();
        filter.admit(denied(0, "lucas", "10.0.0.1"));
        assert!(filter.due(t(61)).is_empty());
        assert_eq!(filter.admit(denied(200, "lucas", "10.0.0.1")).len(), 1);
    }

    #[test]
    fn what_the_server_knows_makes_events_different() {
        let mut filter = RepeatFilter::new();
        assert_eq!(filter.admit(denied(0, "lucas", "10.0.0.1")).len(), 1);
        assert_eq!(
            filter.admit(denied(1, "paul", "10.0.0.1")).len(),
            1,
            "compte"
        );
        assert_eq!(
            filter.admit(event_from(2, None, "poste", "10.0.0.1")).len(),
            1,
            "anonyme"
        );
        let mut failed = denied(3, "lucas", "10.0.0.1");
        failed.outcome = Outcome::Failed(Reason::Internal);
        assert_eq!(filter.admit(failed).len(), 1, "résultat");
        let mut other = denied(4, "lucas", "10.0.0.1");
        other.action = AuditAction::AccountsRead;
        assert_eq!(filter.admit(other).len(), 1, "action");
        let mut target = denied(5, "lucas", "10.0.0.1");
        target.target = Target::Account(Username::parse("marie").unwrap());
        assert_eq!(filter.admit(target).len(), 1, "cible");
        let mut reason = denied(6, "lucas", "10.0.0.1");
        reason.outcome = Outcome::Denied(Reason::Validation);
        assert_eq!(filter.admit(reason).len(), 1, "raison");
        assert_eq!(filter.tracked(), 7);
    }

    #[test]
    fn three_targeted_accounts_in_a_minute_leave_three_traces() {
        let mut filter = RepeatFilter::new();
        let mut written = 0;
        for name in ["marie", "paul", "carl"] {
            for second in 0..5 {
                let mut event = denied(second, "lucas", "10.0.0.1");
                event.target = Target::Account(Username::parse(name).unwrap());
                written += filter.admit(event).len();
            }
        }
        assert_eq!(written, 3);
        assert_eq!(filter.due(t(60)).len(), 3);
    }

    #[test]
    fn anonymous_events_of_one_action_and_one_address_group_whatever_the_host() {
        let mut filter = RepeatFilter::new();
        let mut written = 0;
        for n in 0..3000 {
            written += filter
                .admit(event_from(n % 59, None, &format!("p{n}"), "10.0.0.9"))
                .len();
        }
        assert_eq!(written, 1);
        assert_eq!(filter.due(t(60))[0].repeat_count, 2999);
    }

    #[test]
    fn the_overflow_is_one_summary_never_one_entry_per_event() {
        let mut filter = RepeatFilter::new();
        for n in 0..MAX_TRACKED {
            filter.admit(denied(0, &format!("user{n:04}"), "10.0.0.1"));
        }
        assert_eq!(filter.tracked(), MAX_TRACKED);
        // Au-delà : rien n'est écrit, tout est compté.
        for n in 0..500 {
            assert!(
                filter
                    .admit(denied(1, &format!("autre{n:04}"), "10.0.0.1"))
                    .is_empty()
            );
        }
        assert_eq!(filter.tracked(), MAX_TRACKED);
        let summaries = filter.due(t(61));
        let overflow: Vec<_> = summaries
            .iter()
            .filter(|event| matches!(event.outcome, Outcome::Denied(Reason::TooVaried)))
            .collect();
        assert_eq!(overflow.len(), 1);
        assert_eq!(overflow[0].repeat_count, 500);
        assert_eq!(
            overflow[0].actor.account, None,
            "les comptes diffèrent : aucun"
        );
        assert_eq!(overflow[0].target, Target::None);
        assert_eq!(summaries.len(), 1, "les groupes sans répétition s'oublient");
        assert_eq!(filter.tracked(), 0);
    }

    #[test]
    fn draining_gives_every_pending_summary_before_the_windows_end() {
        let mut filter = RepeatFilter::new();
        filter.admit(denied(0, "lucas", "10.0.0.1"));
        filter.admit(denied(1, "lucas", "10.0.0.1"));
        filter.admit(denied(2, "paul", "10.0.0.1"));
        let drained = filter.drain();
        assert_eq!(drained.len(), 1, "seul le groupe répété a une synthèse");
        assert_eq!(drained[0].repeat_count, 1);
        assert!(filter.drain().is_empty());
        assert_eq!(filter.tracked(), 0);
    }

    #[test]
    fn many_addresses_of_one_family_write_eight_entries_and_one_summary_with_the_exact_counts() {
        // Restaure « l'adresse ne fait pas de chaque requête le premier d'un groupe » : 3 000 adresses
        // différentes du même compte, de la même action, de la même raison.
        let mut filter = RepeatFilter::new();
        let mut written = 0;
        for n in 0..3000 {
            written += filter
                .admit(event_from(
                    n % 59,
                    Some("lucas"),
                    &format!("poste-{n}"),
                    &format!("2001:db8::{n:x}"),
                ))
                .len();
        }
        assert_eq!(written, MAX_ADDRESSES, "huit adresses, huit premiers");
        assert_eq!(filter.tracked(), MAX_ADDRESSES);
        assert_eq!(filter.families(), 1);
        let summaries = filter.due(t(60));
        assert_eq!(summaries.len(), 1, "une seule synthèse de famille");
        assert_eq!(summaries[0].repeat_count, 3000 - 8, "tentatives exactes");
        assert_eq!(
            summaries[0].addresses as usize, MAX_TRACKED_ADDRESSES,
            "adresses comptées jusqu'au plafond"
        );
        assert_eq!(
            summaries[0].actor.origin.addr(),
            Some(""),
            "aucune adresse écrite"
        );
        let record = summaries[0].clone().into_record(1);
        assert_eq!(
            record.reason.as_deref(),
            Some("lecture seule (2992 tentatives depuis 256 adresses)")
        );
    }

    #[test]
    fn anonymous_events_of_one_action_from_many_addresses_make_eight_entries_and_a_summary() {
        let mut filter = RepeatFilter::new();
        let mut written = 0;
        for n in 0..3000 {
            written += filter
                .admit(event_from(
                    n % 59,
                    None,
                    &format!("p{n}"),
                    &format!("10.{}.{}.1", n / 250, n % 250),
                ))
                .len();
        }
        assert_eq!(written, MAX_ADDRESSES);
        let summary = filter.due(t(60));
        assert_eq!(summary[0].repeat_count, 2992);
    }

    #[test]
    fn the_distinct_addresses_counted_per_family_never_grow_past_their_bound() {
        let mut filter = RepeatFilter::new();
        let total = MAX_TRACKED_ADDRESSES + 5000;
        for n in 0..total {
            filter.admit(event_from(
                0,
                None,
                "p",
                &format!("10.{}.{}.{}", n / 62_500, (n / 250) % 250, n % 250),
            ));
        }
        let family = filter.families.values().next().unwrap();
        assert_eq!(family.overflow.len(), MAX_TRACKED_ADDRESSES);
        let summary = filter.due(t(60));
        assert_eq!(
            summary[0].addresses as usize, MAX_TRACKED_ADDRESSES,
            "plafonné"
        );
        assert_eq!(
            summary[0].repeat_count as usize,
            total - MAX_ADDRESSES,
            "les tentatives, elles, restent exactes"
        );
    }

    /// Une attaque : une tentative toutes les `step_ms` pendant `hours` heures ; la tentative `n` vient
    /// de l'adresse `address(n)` et vise le compte `account(n)`. Chaque tentative produit les trois
    /// entrées de la connexion refusée (refus, refus en attente, blocage temporaire), comme la
    /// PR #23. Rend (entrées écrites, événements). **Aucune tentative n'est perdue** : premiers
    /// événements + répétitions des synthèses = événements.
    fn attack(
        hours: i64,
        step_ms: i64,
        address: impl Fn(i64) -> String,
        account: impl Fn(i64) -> Option<String>,
    ) -> (usize, usize) {
        let mut filter = RepeatFilter::new();
        let (mut written, mut firsts, mut repeats, mut events) =
            (0_usize, 0_usize, 0_usize, 0_usize);
        let attempts = hours * 3_600_000 / step_ms;
        let mut count = |entries: Vec<AuditEvent>| {
            for entry in entries {
                written += 1;
                firsts += usize::from(entry.repeat_count == 0);
                repeats += entry.repeat_count as usize;
            }
        };
        for n in 0..attempts {
            let addr = address(n);
            let who = account(n);
            for kind in 0..3 {
                let mut event = event_from(0, who.as_deref(), "poste", &addr);
                event.at = t(0) + Duration::milliseconds(n * step_ms);
                event.action = if kind == 2 {
                    AuditAction::LoginLocked
                } else {
                    AuditAction::Login
                };
                event.outcome = Outcome::Denied(if kind == 0 {
                    Reason::InvalidCredentials
                } else {
                    Reason::TooManyAttempts {
                        retry_after_s: 30 + n as u64 % 60,
                    }
                });
                count(filter.admit(event));
                events += 1;
            }
        }
        count(filter.drain());
        assert_eq!(
            firsts + repeats,
            events,
            "aucune tentative perdue dans les comptes"
        );
        assert!(filter.windows.len() <= MAX_TRACKED && filter.families.len() <= MAX_TRACKED);
        (written, events)
    }

    /// Heures pour faire tourner les 50 000 entrées du journal au rythme de `written` par `hours`.
    fn hours_to_rotate(written: usize, hours: i64) -> f64 {
        50_000.0 / (written as f64 / hours as f64)
    }

    #[test]
    fn measure_1_three_hours_one_attempt_per_15_seconds_a_new_address_each_time() {
        let (written, events) = attack(
            3,
            15_000,
            |n| format!("2001:db8::{n:x}"),
            |_| Some("marie".into()),
        );
        let hours = hours_to_rotate(written, 3);
        eprintln!(
            "MESURE 1 : {written} entrées / {events} événements, 50 000 lignes en {hours:.0} h"
        );
        assert!(written <= 450, "{written}");
        assert!(hours > 72.0, "plusieurs jours : {hours}");
    }

    #[test]
    fn measure_2_the_maximum_rate_gives_the_same_entries_bounded_by_time_not_by_attempts() {
        // 50 tentatives par seconde pendant 3 heures, adresse neuve à chaque fois.
        let (written, events) = attack(
            3,
            20,
            |n| format!("2001:db8::{n:x}"),
            |_| Some("marie".into()),
        );
        let hours = hours_to_rotate(written, 3);
        eprintln!(
            "MESURE 2 : {written} entrées / {events} événements, 50 000 lignes en {hours:.0} h"
        );
        assert!(written <= 450, "{written}");
        assert!(hours > 72.0, "{hours}");
    }

    #[test]
    fn measure_3_ten_accounts_targeted_at_once_each_with_a_new_address_every_time() {
        // L'attaquant change aussi d'identifiant (dix comptes existants, à tour de rôle) : ce qui
        // borne est le nombre de comptes (une famille par compte, action, résultat et raison).
        let (written, events) = attack(
            3,
            1_500,
            |n| format!("2001:db8::{n:x}"),
            |n| Some(format!("compte{}", n % 10)),
        );
        let hours = hours_to_rotate(written, 3);
        eprintln!(
            "MESURE 3 : {written} entrées / {events} événements, 50 000 lignes en {hours:.0} h"
        );
        assert!(written <= 4_500, "{written}");
        assert!(hours > 24.0, "{hours}");
    }

    #[test]
    fn measure_4_one_address_does_not_regress() {
        let (written, events) = attack(3, 15_000, |_| "10.0.0.1".into(), |_| Some("marie".into()));
        eprintln!("MESURE 4 : {written} entrées / {events} événements");
        assert_eq!(events, 2160);
        assert!(written <= 48, "{written}");
    }

    #[test]
    fn unknown_identifiers_share_one_anonymous_family_whatever_their_number() {
        // Un identifiant inexistant n'a pas de compte au journal : tous tombent dans la même famille.
        let (written, _) = attack(1, 100, |n| format!("2001:db8::{n:x}"), |_| None);
        eprintln!("ANONYME : {written} entrées pour une heure à 10 tentatives par seconde");
        assert!(written <= 200, "{written}");
    }

    #[test]
    fn the_family_window_grows_when_the_ceiling_is_reached_even_without_overflow() {
        // Exactement huit adresses neuves par minute : jamais de dépassement, mais la fenêtre de la
        // famille s'allonge quand même, sinon huit entrées par minute passeraient pour toujours.
        let mut filter = RepeatFilter::new();
        let mut firsts = 0;
        for n in 0..(3 * 60) {
            for k in 0..8 {
                let event = event_from(n * 60 + k, Some("lucas"), "p", &format!("10.9.{n}.{k}"));
                firsts += filter
                    .admit(event)
                    .iter()
                    .filter(|entry| entry.repeat_count == 0)
                    .count();
            }
        }
        assert!(firsts < 8 * 20, "{firsts}");
    }
}
