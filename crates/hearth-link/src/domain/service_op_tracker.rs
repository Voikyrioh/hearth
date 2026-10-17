//! Le suivi des opérations de service tenu par la liaison (HRT-84, ADR-0045, BR-SVC-029, BR-RESIL-010) :
//! règles pures. L'agent dit une opération par le message `operation` du sujet `services` : à
//! l'abonnement pour chaque opération en cours que le compte voit, puis à chaque changement. Ce suiveur
//! décide ce qui est annoncé à l'interface :
//!
//! - un état identique à celui déjà annoncé ne l'est pas deux fois ;
//! - une opération qui court encore au retour du lien est reconstruite par les messages de
//!   l'abonnement, SANS rien annoncer de plus (aucun message « reprise ») ;
//! - l'issue d'une opération terminée est annoncée UNE fois, même si l'agent la répète ou si le client la
//!   relit après une coupure ;
//! - une opération qui n'a pas reparu après le retour du lien est à relire sur l'agent (`unconfirmed`),
//!   puis rendue par sa lecture ou oubliée si l'agent ne la connaît plus (24 heures passées, droit retiré).

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use hearth_proto::api::service_ops::{OperationStatus, OperationView};

/// Combien d'issues déjà annoncées on retient pour ne pas les annoncer deux fois.
const FINISHED_MEMORY: usize = 256;

/// Le statut est-il définitif ? `needs_attention` ne l'est pas : l'agent la garde et attend une décision.
pub fn is_finished(status: OperationStatus) -> bool {
    matches!(
        status,
        OperationStatus::Cancelled
            | OperationStatus::RolledBack
            | OperationStatus::Succeeded
            | OperationStatus::Failed
            | OperationStatus::Interrupted
    )
}

struct Tracked {
    view: Arc<OperationView>,
    /// Revue depuis le dernier retour du lien.
    confirmed: bool,
}

/// Les opérations en cours d'un serveur, et la mémoire des issues déjà annoncées.
#[derive(Default)]
pub struct OpTracker {
    live: HashMap<String, Tracked>,
    finished: VecDeque<String>,
}

impl OpTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Un message `operation` (ou la lecture d'une opération) : rend ce qu'il faut annoncer, rien si
    /// l'état est déjà connu tel quel.
    pub fn apply(&mut self, view: OperationView) -> Option<Arc<OperationView>> {
        if self.finished.iter().any(|id| *id == view.id) {
            return None;
        }
        if is_finished(view.status) {
            self.live.remove(&view.id);
            if self.finished.len() >= FINISHED_MEMORY {
                self.finished.pop_front();
            }
            self.finished.push_back(view.id.clone());
            return Some(Arc::new(view));
        }
        if let Some(known) = self.live.get_mut(&view.id) {
            known.confirmed = true;
            if *known.view == view {
                return None;
            }
        }
        let view = Arc::new(view);
        self.live.insert(
            view.id.clone(),
            Tracked {
                view: view.clone(),
                confirmed: true,
            },
        );
        Some(view)
    }

    /// Le lien est revenu : plus rien n'est confirmé tant que l'agent ne l'a pas redit.
    pub fn link_restored(&mut self) {
        for tracked in self.live.values_mut() {
            tracked.confirmed = false;
        }
    }

    /// Les opérations que l'agent n'a pas redites depuis le retour du lien : à relire.
    pub fn unconfirmed(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .live
            .iter()
            .filter(|(_, tracked)| !tracked.confirmed)
            .map(|(id, _)| id.clone())
            .collect();
        ids.sort();
        ids
    }

    /// L'agent ne connaît plus cette opération : on l'oublie. Rend vrai si elle était suivie.
    pub fn forget(&mut self, id: &str) -> bool {
        self.live.remove(id).is_some()
    }

    /// Combien d'opérations sont suivies en cours.
    pub fn live_len(&self) -> usize {
        self.live.len()
    }
}

#[cfg(test)]
mod tests {
    use hearth_proto::api::service_ops::OperationKind;

    use super::*;

    fn view(id: &str, status: OperationStatus, percent: Option<u8>) -> OperationView {
        OperationView {
            id: id.to_owned(),
            key: "h_01J".to_owned(),
            kind: OperationKind::Deploy,
            status,
            step: None,
            steps: Vec::new(),
            percent,
            items_done: None,
            bytes_done: None,
            bytes_total: None,
            eta_s: None,
            cancellable: true,
            decision: None,
            cause: None,
        }
    }

    #[test]
    fn a_running_operation_rebuilt_after_a_cut_announces_nothing_new() {
        let mut tracker = OpTracker::new();
        let running = view("A", OperationStatus::Running, Some(40));
        assert!(tracker.apply(running.clone()).is_some());
        tracker.link_restored();
        assert_eq!(tracker.unconfirmed(), ["A"]);
        // L'agent la redit à l'abonnement, identique : rien à annoncer, elle est confirmée.
        assert!(tracker.apply(running).is_none());
        assert!(tracker.unconfirmed().is_empty());
        assert_eq!(tracker.live_len(), 1);
    }

    #[test]
    fn a_finished_operation_is_announced_exactly_once() {
        let mut tracker = OpTracker::new();
        tracker.apply(view("A", OperationStatus::Running, Some(90)));
        let done = view("A", OperationStatus::Succeeded, Some(100));
        assert!(tracker.apply(done.clone()).is_some());
        assert_eq!(tracker.live_len(), 0);
        // Répétée par l'agent, ou relue après une coupure : jamais deux fois.
        assert!(tracker.apply(done).is_none());
        // Un état « en cours » en retard ne la ressuscite pas.
        assert!(
            tracker
                .apply(view("A", OperationStatus::Running, Some(95)))
                .is_none()
        );
    }

    #[test]
    fn an_operation_finished_during_the_cut_is_announced_once_when_read_back() {
        let mut tracker = OpTracker::new();
        tracker.apply(view("A", OperationStatus::Running, Some(10)));
        tracker.link_restored();
        // L'agent ne la redit pas : à relire.
        assert_eq!(tracker.unconfirmed(), ["A"]);
        let read = tracker.apply(view("A", OperationStatus::Failed, None));
        assert_eq!(read.expect("annoncée").status, OperationStatus::Failed);
        assert!(tracker.unconfirmed().is_empty());
    }

    #[test]
    fn a_changed_state_is_announced_and_an_identical_one_is_not() {
        let mut tracker = OpTracker::new();
        assert!(
            tracker
                .apply(view("A", OperationStatus::Running, Some(10)))
                .is_some()
        );
        assert!(
            tracker
                .apply(view("A", OperationStatus::Running, Some(10)))
                .is_none()
        );
        assert!(
            tracker
                .apply(view("A", OperationStatus::Running, Some(20)))
                .is_some()
        );
    }

    #[test]
    fn needs_attention_and_awaiting_decision_stay_live() {
        for status in [
            OperationStatus::NeedsAttention,
            OperationStatus::AwaitingDecision,
        ] {
            assert!(!is_finished(status));
        }
        for status in [
            OperationStatus::Cancelled,
            OperationStatus::RolledBack,
            OperationStatus::Succeeded,
            OperationStatus::Failed,
            OperationStatus::Interrupted,
        ] {
            assert!(is_finished(status));
        }
        assert!(!is_finished(OperationStatus::Unknown));
    }

    #[test]
    fn an_operation_the_agent_no_longer_knows_is_forgotten() {
        let mut tracker = OpTracker::new();
        tracker.apply(view("A", OperationStatus::Running, None));
        tracker.link_restored();
        assert!(tracker.forget("A"));
        assert!(!tracker.forget("A"));
        assert!(tracker.unconfirmed().is_empty());
    }

    #[test]
    fn the_memory_of_announced_outcomes_is_bounded() {
        let mut tracker = OpTracker::new();
        for n in 0..(FINISHED_MEMORY + 10) {
            tracker.apply(view(&format!("op{n}"), OperationStatus::Succeeded, None));
        }
        assert_eq!(tracker.finished.len(), FINISHED_MEMORY);
    }
}
