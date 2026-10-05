//! Regroupement des événements identiques répétés (BR-AUDIT-007) : un compte lecture seule qui
//! boucle sur la lecture du journal, ou un client qui échoue en boucle, ne doit pas faire tourner
//! les 50 000 entrées et en chasser l'historique utile.
//!
//! **La clé ne contient que ce que le serveur connaît ou borne** : le compte authentifié (ou
//! « anonyme »), l'action, le résultat, la cible (un compte existant ou le motif statique d'une
//! route) et la raison (une énumération). **Ni le nom du poste ni l'adresse** : ce sont des valeurs
//! que le client fait varier à volonté (en-tête libre, plage IPv6). Les événements de même clé sont
//! groupés par fenêtre de 60 secondes : le premier est écrit tout de suite ; les suivants sont
//! comptés ; **une** entrée de synthèse (la dernière occurrence, donc son origine, avec le nombre
//! d'autres fois) est écrite à la fin de la fenêtre.
//!
//! Le nombre de groupes suivis est plafonné (garde-fou : avec cette clé un seul compte n'en
//! produit qu'un nombre borné par les cibles et raisons possibles). Au débordement, les événements
//! nouveaux ne sont plus écrits un par un : ils sont comptés dans **un groupe de débordement
//! unique**, résumé par une entrée « activité trop variée, N événements regroupés ».
//! Fonction pure : aucune horloge, aucune E/S.

use std::collections::HashMap;

use time::{Duration, OffsetDateTime};

use super::action::AuditAction;
use super::event::{AuditEvent, OutcomeKind};

/// Durée de la fenêtre d'un groupe.
pub const REPEAT_WINDOW: Duration = Duration::seconds(60);
/// Groupes suivis en même temps, au plus.
pub const MAX_TRACKED: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    /// `None` : anonyme (aucun compte authentifié).
    account: Option<String>,
    action: AuditAction,
    outcome: OutcomeKind,
    target: Option<String>,
    reason: Option<String>,
}

impl Key {
    fn of(event: &AuditEvent) -> Self {
        Self {
            account: event.actor.account.as_ref().map(ToString::to_string),
            action: event.action,
            outcome: event.outcome.kind(),
            target: event.target.text(),
            reason: event.outcome.reason().map(|reason| reason.text()),
        }
    }
}

#[derive(Debug)]
struct Window {
    started: OffsetDateTime,
    /// Occurrences comptées sans être écrites.
    suppressed: u32,
    /// La dernière occurrence : sa date, son origine, tout ce que la synthèse reprend.
    last: AuditEvent,
}

impl Window {
    fn ended_at(&self, now: OffsetDateTime) -> bool {
        now - self.started >= REPEAT_WINDOW
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
    /// lui-même (premier de sa fenêtre), précédé des synthèses des fenêtres qui viennent de finir.
    pub fn admit(&mut self, event: AuditEvent) -> Vec<AuditEvent> {
        let mut out = self.due(event.at);
        let key = Key::of(&event);
        if let Some(window) = self.windows.get_mut(&key) {
            window.suppressed = window.suppressed.saturating_add(1);
            window.last = event;
            return out;
        }
        if self.windows.len() >= MAX_TRACKED {
            // Débordement : compté, pas écrit un par un.
            match &mut self.overflow {
                Some(window) => {
                    window.suppressed = window.suppressed.saturating_add(1);
                    window.last = event;
                }
                None => {
                    self.overflow = Some(Window {
                        started: event.at,
                        suppressed: 1,
                        last: event,
                    });
                }
            }
            return out;
        }
        self.windows.insert(
            key,
            Window {
                started: event.at,
                suppressed: 0,
                last: event.clone(),
            },
        );
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
        let mut out: Vec<AuditEvent> = ended
            .into_iter()
            .filter_map(|key| self.windows.remove(&key))
            .filter(|window| window.suppressed > 0)
            .map(|window| window.last.with_repeats(window.suppressed))
            .collect();
        if self.overflow.as_ref().is_some_and(|w| w.ended_at(now))
            && let Some(window) = self.overflow.take()
        {
            out.push(window.last.as_overflow(window.suppressed));
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
    fn the_host_and_the_address_do_not_make_events_different() {
        let mut filter = RepeatFilter::new();
        let mut written = 0;
        for n in 0..5000 {
            let event = event_from(
                n % 59,
                Some("lucas"),
                &format!("poste-{n}"),
                &format!("2001:db8::{n:x}"),
            );
            written += filter.admit(event).len();
        }
        assert_eq!(written, 1, "un seul groupe, un seul premier");
        assert_eq!(filter.tracked(), 1);
        let summaries = filter.due(t(60));
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].repeat_count, 4999);
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
    fn anonymous_events_of_one_action_group_whatever_the_addresses() {
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
}
