//! Regroupement des événements identiques répétés (BR-AUDIT-007) : un compte lecture seule qui
//! boucle sur la lecture du journal, ou un client qui échoue en boucle, ne doit pas faire tourner
//! les 50 000 entrées et en chasser l'historique utile.
//!
//! Les événements **identiques** (même compte, même origine, même action, même résultat) sont
//! groupés par fenêtre de 60 secondes : le premier est écrit tout de suite ; les suivants de la
//! fenêtre ne sont pas écrits un par un, ils sont comptés ; **une** entrée de synthèse (la
//! dernière occurrence, avec `repeat_count` = le nombre d'autres fois) est écrite à la fin de la
//! fenêtre. Fonction pure : aucune horloge, aucune E/S ; le nombre de groupes suivis est plafonné.

use std::collections::HashMap;

use time::{Duration, OffsetDateTime};

use super::action::AuditAction;
use super::event::{AuditEvent, OriginKind, OutcomeKind};

/// Durée de la fenêtre d'un groupe.
pub const REPEAT_WINDOW: Duration = Duration::seconds(60);
/// Groupes suivis en même temps, au plus. Au-delà, un événement nouveau est écrit sans être
/// groupé : l'intégrité d'abord, le plafond protège la mémoire.
pub const MAX_TRACKED: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    account: Option<String>,
    kind: OriginKind,
    name: Option<String>,
    addr: Option<String>,
    action: AuditAction,
    outcome: OutcomeKind,
}

impl Key {
    fn of(event: &AuditEvent) -> Self {
        Self {
            account: event.actor.account.as_ref().map(ToString::to_string),
            kind: event.actor.origin.kind(),
            name: event.actor.origin.name().map(str::to_owned),
            addr: event.actor.origin.addr().map(str::to_owned),
            action: event.action,
            outcome: event.outcome.kind(),
        }
    }
}

#[derive(Debug)]
struct Window {
    started: OffsetDateTime,
    suppressed: u32,
    last: AuditEvent,
}

impl Window {
    fn ended_at(&self, now: OffsetDateTime) -> bool {
        now - self.started >= REPEAT_WINDOW
    }

    /// L'entrée de synthèse, s'il y a eu des répétitions.
    fn summary(self) -> Option<AuditEvent> {
        (self.suppressed > 0).then(|| self.last.with_repeats(self.suppressed))
    }
}

/// Les fenêtres en cours.
#[derive(Debug, Default)]
pub struct RepeatFilter {
    windows: HashMap<Key, Window>,
}

impl RepeatFilter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ce qu'il faut écrire pour cet événement : rien (compté dans sa fenêtre), l'événement
    /// lui-même (premier d'une fenêtre), précédé de la synthèse de la fenêtre qui vient de finir.
    pub fn admit(&mut self, event: AuditEvent) -> Vec<AuditEvent> {
        let key = Key::of(&event);
        let mut out = Vec::new();
        if let Some(window) = self.windows.get_mut(&key) {
            if !window.ended_at(event.at) {
                window.suppressed = window.suppressed.saturating_add(1);
                window.last = event;
                return out;
            }
            if let Some(ended) = self.windows.remove(&key).and_then(Window::summary) {
                out.push(ended);
            }
        }
        if self.windows.len() >= MAX_TRACKED {
            out.extend(self.due(event.at));
        }
        if self.windows.len() < MAX_TRACKED {
            self.windows.insert(
                key,
                Window {
                    started: event.at,
                    suppressed: 0,
                    last: event.clone(),
                },
            );
        }
        out.push(event);
        out
    }

    /// Les synthèses des fenêtres finies à `now` (les fenêtres sans répétition s'oublient en
    /// silence).
    pub fn due(&mut self, now: OffsetDateTime) -> Vec<AuditEvent> {
        let ended: Vec<Key> = self
            .windows
            .iter()
            .filter(|(_, window)| window.ended_at(now))
            .map(|(key, _)| key.clone())
            .collect();
        ended
            .into_iter()
            .filter_map(|key| self.windows.remove(&key).and_then(Window::summary))
            .collect()
    }

    /// Fenêtres suivies (tests).
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

    fn denied(seconds: i64, account: &str, addr: &str) -> AuditEvent {
        AuditEvent::new(
            t(seconds),
            Actor::new(
                Some(Username::parse(account).unwrap()),
                Origin::client(Some("poste"), addr),
            ),
            AuditAction::AuditRead,
            Target::Route("/audit"),
            Outcome::Denied(Reason::ReadOnly),
        )
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
        assert_eq!(summaries[0].action, AuditAction::AuditRead);
        assert!(filter.due(t(600)).is_empty(), "une seule synthèse");
        assert_eq!(filter.tracked(), 0);
    }

    #[test]
    fn a_new_event_after_the_window_brings_the_summary_then_opens_a_new_window() {
        let mut filter = RepeatFilter::new();
        filter.admit(denied(0, "lucas", "10.0.0.1"));
        filter.admit(denied(10, "lucas", "10.0.0.1"));
        filter.admit(denied(20, "lucas", "10.0.0.1"));
        let written = filter.admit(denied(61, "lucas", "10.0.0.1"));
        assert_eq!(written.len(), 2);
        assert_eq!(written[0].repeat_count, 2);
        assert_eq!(written[1].repeat_count, 0);
        assert_eq!(written[1].at, t(61));
    }

    #[test]
    fn without_repetition_there_is_no_summary() {
        let mut filter = RepeatFilter::new();
        filter.admit(denied(0, "lucas", "10.0.0.1"));
        assert!(filter.due(t(61)).is_empty());
        assert_eq!(filter.admit(denied(200, "lucas", "10.0.0.1")).len(), 1);
    }

    #[test]
    fn events_that_differ_are_not_grouped() {
        let mut filter = RepeatFilter::new();
        assert_eq!(filter.admit(denied(0, "lucas", "10.0.0.1")).len(), 1);
        assert_eq!(
            filter.admit(denied(1, "paul", "10.0.0.1")).len(),
            1,
            "compte"
        );
        assert_eq!(
            filter.admit(denied(2, "lucas", "10.0.0.2")).len(),
            1,
            "origine"
        );
        let mut failed = denied(3, "lucas", "10.0.0.1");
        failed.outcome = Outcome::Failed(Reason::Internal);
        assert_eq!(filter.admit(failed).len(), 1, "résultat");
        let mut other = denied(4, "lucas", "10.0.0.1");
        other.action = AuditAction::AccountsRead;
        assert_eq!(filter.admit(other).len(), 1, "action");
        assert_eq!(filter.tracked(), 5);
    }

    #[test]
    fn the_tracked_groups_are_bounded_and_the_overflow_is_written_not_lost() {
        let mut filter = RepeatFilter::new();
        for n in 0..MAX_TRACKED {
            filter.admit(denied(0, "lucas", &format!("10.1.{}.{}", n / 250, n % 250)));
        }
        assert_eq!(filter.tracked(), MAX_TRACKED);
        let written = filter.admit(denied(1, "lucas", "10.9.9.9"));
        assert_eq!(written.len(), 1, "écrit tel quel");
        assert_eq!(filter.tracked(), MAX_TRACKED);
        // Les fenêtres finies libèrent de la place.
        let written = filter.admit(denied(100, "lucas", "10.9.9.8"));
        assert_eq!(written.len(), 1);
        assert_eq!(filter.tracked(), 1);
    }
}
