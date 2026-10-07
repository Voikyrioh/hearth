//! Regroupement des événements identiques répétés (BR-AUDIT-007) : un compte lecture seule qui
//! boucle sur la lecture du journal, ou un client qui échoue en boucle, ne doit pas faire tourner
//! les 50 000 entrées et en chasser l'historique utile.
//!
//! **La clé** : le compte authentifié (ou « anonyme »), l'**adresse** de la connexion (Q14, point 9 :
//! « on regroupe par adresse »), l'action, le résultat, la cible (un compte existant ou le motif
//! statique d'une route) et la raison (une énumération). **Ni le nom du poste ni l'identifiant saisi** :
//! ce sont des valeurs que le client fait varier à volonté. L'adresse est celle de la connexion TCP,
//! jamais un en-tête.
//!
//! **Fenêtre qui s'allonge** : le premier événement d'un groupe est écrit tout de suite ; les suivants
//! sont comptés ; **une** entrée de synthèse (la dernière occurrence, avec le nombre d'autres fois) est
//! écrite à la fin de la fenêtre, au compte exact. Tant que le groupe revient, la fenêtre suivante
//! double (1, 2, 4, 8 puis 15 minutes, plafond `MAX_WINDOW`). Après `QUIET_RESET` sans occurrence, le
//! groupe est oublié : la fenêtre repart à une minute.
//!
//! Le nombre de groupes suivis est plafonné. Au débordement, les événements nouveaux ne sont plus
//! écrits un par un : ils sont comptés dans **un groupe de débordement unique**, résumé par une entrée
//! « activité trop variée, N événements regroupés » dont la fenêtre s'allonge de la même façon.
//!
//! **Risque dit** : une attaque qui change d'adresse à chaque tentative ne regroupe plus rien (un
//! groupe par adresse) ; seuls la fenêtre qui s'allonge et le plafond de la table bornent alors le
//! volume (voir ADR-0024).
//! Fonction pure : aucune horloge, aucune E/S.

use std::collections::HashMap;

use time::{Duration, OffsetDateTime};

use super::action::AuditAction;
use super::event::{AuditEvent, OutcomeKind};

/// Durée de la première fenêtre d'un groupe.
pub const REPEAT_WINDOW: Duration = Duration::seconds(60);
/// Plafond de la fenêtre qui s'allonge.
pub const MAX_WINDOW: Duration = Duration::minutes(15);
/// Sans occurrence depuis ce délai, un groupe est oublié et sa fenêtre repart à `REPEAT_WINDOW`.
pub const QUIET_RESET: Duration = Duration::minutes(30);
/// Groupes suivis en même temps, au plus.
pub const MAX_TRACKED: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    /// `None` : anonyme (aucun compte authentifié).
    account: Option<String>,
    /// Adresse de la connexion (`None` : ligne de commande, assistant).
    address: Option<String>,
    action: AuditAction,
    outcome: OutcomeKind,
    target: Option<String>,
    reason: Option<String>,
}

impl Key {
    fn of(event: &AuditEvent) -> Self {
        Self {
            account: event.actor.account.as_ref().map(ToString::to_string),
            address: event.actor.origin.addr().map(str::to_owned),
            action: event.action,
            outcome: event.outcome.kind(),
            target: event.target.text(),
            // Raison SANS donnée variable : la durée d'une attente ne fait pas un groupe neuf.
            reason: event.outcome.reason().map(|reason| reason.group_text()),
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

    /// La fenêtre est finie à `now` : la synthèse à écrire, et si le groupe continue. Un groupe
    /// répété continue avec une fenêtre double ; un groupe qui n'a pas été répété s'oublie dès sa
    /// première fenêtre, et plus tard après `QUIET_RESET` sans occurrence.
    fn close(&mut self, now: OffsetDateTime, overflow: bool) -> (Option<AuditEvent>, bool) {
        let summary = (self.suppressed > 0).then(|| {
            if overflow {
                self.last.clone().as_overflow(self.suppressed)
            } else {
                self.last.clone().with_repeats(self.suppressed)
            }
        });
        let repeated = self.suppressed > 0;
        let quiet = (now - self.last_seen).abs() >= QUIET_RESET;
        let keep = !quiet && (repeated || self.interval > REPEAT_WINDOW);
        if keep {
            if repeated {
                self.interval = (self.interval * 2_i32).min(MAX_WINDOW);
            }
            self.started = now;
            self.suppressed = 0;
        }
        (summary, keep)
    }
}

/// Les fenêtres en cours.
#[derive(Debug, Default)]
pub struct RepeatFilter {
    windows: HashMap<Key, Window>,
    /// Groupe de débordement : ses occurrences ne sont écrites que dans sa synthèse.
    overflow: Option<Window>,
}

impl RepeatFilter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ce qu'il faut écrire pour cet événement : rien (compté dans sa fenêtre), l'événement
    /// lui-même (premier de son groupe), précédé des synthèses des fenêtres qui viennent de finir.
    pub fn admit(&mut self, event: AuditEvent) -> Vec<AuditEvent> {
        let mut out = self.due(event.at);
        let key = Key::of(&event);
        if let Some(window) = self.windows.get_mut(&key) {
            window.count(event);
            return out;
        }
        if self.windows.len() >= MAX_TRACKED {
            // Débordement : compté, pas écrit un par un.
            match &mut self.overflow {
                Some(window) => window.count(event),
                None => self.overflow = Some(Window::opened(event, 1)),
            }
            return out;
        }
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
            out.extend(summary);
            if !keep {
                self.windows.remove(&key);
            }
        }
        if let Some(window) = self.overflow.as_mut()
            && window.ended_at(now)
        {
            let (summary, keep) = window.close(now, true);
            out.extend(summary);
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

    /// Une attaque : une tentative toutes les 15 secondes pendant `hours` heures, depuis une adresse
    /// que `address` donne pour la tentative `n`. Chaque tentative produit les trois entrées de la
    /// connexion refusée (refus, refus en attente, blocage temporaire), comme la PR #23. Rend le
    /// nombre d'entrées écrites (premiers événements et synthèses) et la somme des répétitions.
    fn attack(hours: i64, address: impl Fn(i64) -> String) -> (usize, usize) {
        let mut filter = RepeatFilter::new();
        let mut written = 0;
        let mut firsts = 0_usize;
        let mut repeats = 0_usize;
        let attempts = hours * 3600 / 15;
        let mut events = 0;
        for n in 0..attempts {
            let addr = address(n);
            for kind in 0..3 {
                let mut event = event_from(n * 15, Some("lucas"), "poste", &addr);
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
                for entry in filter.admit(event) {
                    written += 1;
                    firsts += usize::from(entry.repeat_count == 0);
                    repeats += entry.repeat_count as usize;
                }
                events += 1;
            }
        }
        for entry in filter.drain() {
            written += 1;
            repeats += entry.repeat_count as usize;
        }
        // Rien ne se perd : chaque événement est écrit tout de suite ou compté, au compte exact.
        assert_eq!(firsts + repeats, events);
        (written, events)
    }

    #[test]
    fn the_window_grows_one_two_four_eight_then_fifteen_minutes_with_the_exact_count() {
        let mut filter = RepeatFilter::new();
        let mut at = Vec::new();
        let mut counts = Vec::new();
        let mut first = 0;
        for n in 0..(3 * 3600 / 15) {
            for entry in filter.admit(denied(n * 15, "lucas", "10.0.0.1")) {
                if entry.repeat_count == 0 {
                    first += 1;
                } else {
                    at.push(entry.at.unix_timestamp() - t(0).unix_timestamp());
                    counts.push(entry.repeat_count);
                }
            }
        }
        assert_eq!(first, 1, "seul le premier est écrit tout de suite");
        // Les synthèses ferment des fenêtres de 1, 2, 4, 8 puis 15 minutes.
        let gaps: Vec<i64> = at.windows(2).map(|pair| (pair[1] - pair[0]) / 60).collect();
        assert_eq!(gaps[..3], [2, 4, 8], "{gaps:?}");
        assert!(gaps[3..].iter().all(|gap| *gap == 15), "{gaps:?}");
        let pending: u32 = filter.drain().iter().map(|e| e.repeat_count).sum();
        let total: u32 = counts.iter().sum::<u32>() + pending + 1;
        assert_eq!(
            total, 720,
            "toutes les tentatives comptées, au compte exact"
        );
        assert!(at.len() <= 16, "{}", at.len());
    }

    #[test]
    fn a_group_quiet_for_thirty_minutes_starts_again_at_one_minute() {
        let mut filter = RepeatFilter::new();
        for n in 0..(2 * 3600 / 15) {
            filter.admit(denied(n * 15, "lucas", "10.0.0.1"));
        }
        // Le groupe est à sa fenêtre plafonnée ; 31 minutes de silence, puis une occurrence.
        let later = 2 * 3600 + 31 * 60;
        filter.due(t(later - 1));
        let written = filter.admit(denied(later, "lucas", "10.0.0.1"));
        assert!(
            written
                .iter()
                .any(|e| e.repeat_count == 0 && e.at == t(later)),
            "l'occurrence est de nouveau écrite tout de suite"
        );
        // Et sa fenêtre est de nouveau d'une minute.
        filter.admit(denied(later + 10, "lucas", "10.0.0.1"));
        assert_eq!(filter.due(t(later + 60)).len(), 1);
    }

    #[test]
    fn three_hours_of_attack_from_one_address_write_a_few_dozen_entries_not_hundreds() {
        let (written, events) = attack(3, |_| "10.0.0.1".into());
        assert_eq!(events, 2160);
        assert!(written <= 3 * 17, "{written} entrées");
    }

    #[test]
    fn an_attack_that_changes_address_every_time_is_bounded_only_by_the_table_the_known_risk() {
        // Une adresse neuve à chaque tentative : un groupe par adresse, rien ne se regroupe. Le
        // volume suit alors le nombre de tentatives (trois entrées par tentative) : c'est le risque
        // accepté par le détenteur (Q14, point 9), dit dans l'ADR-0024.
        let (written, events) = attack(3, |n| format!("2001:db8::{n:x}"));
        assert_eq!(written, events);
        // Plus vite que la table ne se vide, le plafond joue : au plus MAX_TRACKED groupes écrits un
        // par un, le reste dans UNE synthèse de débordement.
        let mut filter = RepeatFilter::new();
        let mut single = 0;
        for n in 0..5000 {
            single += filter
                .admit(event_from(
                    n % 50,
                    Some("lucas"),
                    "poste",
                    &format!("10.1.{}.{}", n / 250, n % 250),
                ))
                .len();
        }
        assert_eq!(single, MAX_TRACKED);
        let summaries = filter.due(t(200));
        assert_eq!(
            summaries
                .iter()
                .filter(|e| matches!(e.outcome, Outcome::Denied(Reason::TooVaried)))
                .count(),
            1
        );
    }
}
