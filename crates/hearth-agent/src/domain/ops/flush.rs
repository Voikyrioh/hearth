//! La cadence des messages `operation` d'un abonné du flux (HRT-144, ADR-0045 : au plus un message par
//! 500 ms et par opération). Règle pure, le temps est un paramètre (millisecondes d'une horloge
//! monotone).
//!
//! - un message part tout de suite si l'opération n'a rien envoyé depuis 500 ms ;
//! - sinon il est RETENU, et le dernier état retenu d'une opération remplace le précédent (les états
//!   intermédiaires se fondent, c'est le but) ;
//! - ce qui est retenu part à l'échéance (`due`) : **le dernier état d'une opération n'est jamais perdu**,
//!   en particulier son état final ;
//! - l'état tenu est borné ([`MAX_TRACKED_OPS`]) : un agent qui voit passer des milliers d'opérations ne
//!   fait pas grossir la mémoire d'une connexion.

use std::collections::HashMap;

pub use hearth_proto::api::service_ops::PROGRESS_MIN_INTERVAL_MS as MIN_INTERVAL_MS;

/// Combien d'opérations la cadence suit en même temps pour un abonné. Au-delà, les opérations les plus
/// anciennement servies sans état retenu sont oubliées (elles repartiront tout de suite à leur prochain
/// message).
pub const MAX_TRACKED_OPS: usize = 256;

struct Entry<T> {
    /// Quand ce suivi a envoyé pour la dernière fois (ms).
    last_sent: u64,
    /// L'état retenu en attendant l'échéance.
    pending: Option<T>,
}

pub struct OpFlush<T> {
    entries: HashMap<String, Entry<T>>,
}

impl<T> Default for OpFlush<T> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }
}

impl<T> OpFlush<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Un nouvel état de l'opération `id` à `now_ms` : `Some(état)` s'il part tout de suite, `None` s'il est
    /// retenu jusqu'à l'échéance.
    pub fn offer(&mut self, now_ms: u64, id: &str, value: T) -> Option<T> {
        if let Some(entry) = self.entries.get_mut(id) {
            if now_ms.saturating_sub(entry.last_sent) < MIN_INTERVAL_MS && now_ms >= entry.last_sent
            {
                entry.pending = Some(value);
                return None;
            }
            // Une horloge qui recule ne retient pas non plus indéfiniment : le suivi repart de `now_ms`.
            if now_ms < entry.last_sent {
                entry.last_sent = now_ms;
                entry.pending = Some(value);
                return None;
            }
            entry.last_sent = now_ms;
            entry.pending = None;
            return Some(value);
        }
        self.make_room();
        self.entries.insert(
            id.to_owned(),
            Entry {
                last_sent: now_ms,
                pending: None,
            },
        );
        Some(value)
    }

    /// Les états retenus dont l'échéance est atteinte à `now_ms`, par opération la plus anciennement servie
    /// d'abord.
    pub fn due(&mut self, now_ms: u64) -> Vec<T> {
        let mut ready: Vec<(u64, String)> = self
            .entries
            .iter()
            .filter(|(_, entry)| {
                entry.pending.is_some()
                    && now_ms >= entry.last_sent
                    && now_ms - entry.last_sent >= MIN_INTERVAL_MS
            })
            .map(|(id, entry)| (entry.last_sent, id.clone()))
            .collect();
        ready.sort();
        ready
            .into_iter()
            .filter_map(|(_, id)| {
                let entry = self.entries.get_mut(&id)?;
                entry.last_sent = now_ms;
                entry.pending.take()
            })
            .collect()
    }

    /// Le prochain instant (ms) où `due` rendra quelque chose, s'il y a un état retenu.
    pub fn next_deadline(&self) -> Option<u64> {
        self.entries
            .values()
            .filter(|entry| entry.pending.is_some())
            .map(|entry| entry.last_sent.saturating_add(MIN_INTERVAL_MS))
            .min()
    }

    /// Oublie l'opération (elle a disparu pour cet abonné) : ni suivi ni état retenu.
    pub fn forget(&mut self, id: &str) {
        self.entries.remove(id);
    }

    /// Le nombre d'opérations suivies.
    pub fn tracked(&self) -> usize {
        self.entries.len()
    }

    /// Fait de la place avant d'ajouter une opération : retire d'abord les suivis sans état retenu, les plus
    /// anciennement servis en premier. Un état retenu n'est jamais jeté pour faire de la place.
    fn make_room(&mut self) {
        if self.entries.len() < MAX_TRACKED_OPS {
            return;
        }
        let mut idle: Vec<(u64, String)> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.pending.is_none())
            .map(|(id, entry)| (entry.last_sent, id.clone()))
            .collect();
        idle.sort();
        let excess = self.entries.len() + 1 - MAX_TRACKED_OPS;
        for (_, id) in idle.into_iter().take(excess) {
            self.entries.remove(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_state_goes_at_once_and_the_next_ones_wait_for_the_interval() {
        let mut flush = OpFlush::new();
        assert_eq!(flush.offer(1_000, "a", 1), Some(1));
        // Dans les 500 ms : retenu, le dernier remplace les précédents.
        assert_eq!(flush.offer(1_100, "a", 2), None);
        assert_eq!(flush.offer(1_400, "a", 3), None);
        assert_eq!(flush.next_deadline(), Some(1_500));
        assert!(flush.due(1_499).is_empty());
        assert_eq!(flush.due(1_500), vec![3]);
        assert_eq!(flush.next_deadline(), None);
        // L'échéance a servi : un nouvel état attend de nouveau 500 ms.
        assert_eq!(flush.offer(1_600, "a", 4), None);
        assert_eq!(flush.due(2_000), vec![4]);
    }

    #[test]
    fn an_operation_quiet_for_the_interval_sends_at_once_and_a_sent_state_clears_the_held_one() {
        let mut flush = OpFlush::new();
        assert_eq!(flush.offer(0, "a", 1), Some(1));
        assert_eq!(flush.offer(500, "a", 2), Some(2));
        assert_eq!(flush.offer(999, "a", 3), None);
        assert_eq!(flush.offer(1_000, "a", 4), Some(4));
        // Ce qui était retenu est remplacé par l'envoi : rien à rendre ensuite.
        assert!(flush.due(5_000).is_empty());
    }

    #[test]
    fn operations_are_throttled_each_on_their_own() {
        let mut flush = OpFlush::new();
        assert_eq!(flush.offer(0, "a", "a1"), Some("a1"));
        assert_eq!(flush.offer(10, "b", "b1"), Some("b1"));
        assert_eq!(flush.offer(20, "a", "a2"), None);
        assert_eq!(flush.offer(30, "b", "b2"), None);
        assert_eq!(flush.due(500), vec!["a2"]);
        assert_eq!(flush.due(510), vec!["b2"]);
    }

    #[test]
    fn the_final_state_is_never_lost_even_when_it_arrives_inside_the_interval() {
        let mut flush = OpFlush::new();
        assert_eq!(flush.offer(0, "a", "running"), Some("running"));
        assert_eq!(flush.offer(5, "a", "succeeded"), None);
        assert_eq!(flush.due(505), vec!["succeeded"]);
    }

    #[test]
    fn a_clock_going_backwards_neither_sends_early_nor_loses_a_state() {
        let mut flush = OpFlush::new();
        assert_eq!(flush.offer(10_000, "a", 1), Some(1));
        assert_eq!(flush.offer(5_000, "a", 2), None);
        assert!(flush.due(5_000).is_empty());
        assert_eq!(flush.due(5_500), vec![2]);
    }

    #[test]
    fn what_is_tracked_is_bounded_and_a_held_state_is_never_dropped_to_make_room() {
        let mut flush = OpFlush::new();
        for index in 0..MAX_TRACKED_OPS {
            assert!(flush.offer(0, &format!("op{index}"), index).is_some());
        }
        // Une opération retient un état : elle ne sera pas évincée.
        assert_eq!(flush.offer(1, "op0", 9_999), None);
        for index in MAX_TRACKED_OPS..MAX_TRACKED_OPS + 50 {
            flush.offer(2, &format!("op{index}"), index);
            assert!(flush.tracked() <= MAX_TRACKED_OPS);
        }
        assert_eq!(flush.due(600).first(), Some(&9_999));
    }

    #[test]
    fn a_forgotten_operation_restarts_clean() {
        let mut flush = OpFlush::new();
        flush.offer(0, "a", 1);
        flush.offer(1, "a", 2);
        flush.forget("a");
        assert_eq!(flush.tracked(), 0);
        assert!(flush.due(10_000).is_empty());
        assert_eq!(flush.offer(2, "a", 3), Some(3));
    }
}
