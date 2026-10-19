//! Les opérations de service de chaque serveur, tenues côté Rust (HRT-84) : celles qui courent, et les issues
//! que l'interface n'a pas encore acquittées. Un numéro de séquence croît strictement ; l'état est rejoué à
//! l'abonnement de l'interface (ADR-0013 point 3) : un changement annoncé avant l'écoute de la fenêtre n'est
//! pas perdu. Une issue est retenue jusqu'à son acquittement, donc annoncée UNE fois (BR-RESIL-010). Pur :
//! sans E/S ni Tauri.

use std::collections::{BTreeMap, HashMap, VecDeque};

use hearth_link::domain::service_op_tracker::is_finished;
use hearth_proto::api::service_ops::OperationView;

use super::dto::{OperationChangeDto, OperationDto, ServiceOperationEvent};

/// Issues retenues au plus par serveur tant que l'interface ne les a pas acquittées.
const MAX_FINISHED: usize = 50;

#[derive(Default)]
struct ServerOps {
    live: BTreeMap<String, (u32, OperationDto)>,
    finished: VecDeque<(u32, OperationDto)>,
}

#[derive(Default)]
pub struct OpsBook {
    seq: u32,
    servers: HashMap<String, ServerOps>,
}

impl OpsBook {
    fn next(&mut self) -> u32 {
        self.seq = self.seq.wrapping_add(1).max(1);
        self.seq
    }

    /// Un message `operation` (déjà dédoublonné par la liaison) : rangé, rendu avec son numéro.
    pub fn on_view(&mut self, server: &str, view: &OperationView) -> ServiceOperationEvent {
        let seq = self.next();
        let dto = OperationDto::from(view);
        let entry = self.servers.entry(server.to_owned()).or_default();
        entry.live.remove(&dto.id);
        if is_finished(view.status) {
            if entry.finished.len() >= MAX_FINISHED {
                entry.finished.pop_front();
            }
            entry.finished.push_back((seq, dto.clone()));
        } else {
            entry.live.insert(dto.id.clone(), (seq, dto.clone()));
        }
        ServiceOperationEvent {
            server_id: server.to_owned(),
            seq,
            change: OperationChangeDto::Upsert {
                operation: Box::new(dto),
            },
        }
    }

    /// L'agent ne connaît plus cette opération : elle n'est plus suivie.
    pub fn on_gone(&mut self, server: &str, id: &str) -> ServiceOperationEvent {
        let seq = self.next();
        if let Some(entry) = self.servers.get_mut(server) {
            entry.live.remove(id);
        }
        ServiceOperationEvent {
            server_id: server.to_owned(),
            seq,
            change: OperationChangeDto::Gone { id: id.to_owned() },
        }
    }

    /// Tout ce que l'interface doit connaître à son abonnement : les opérations en cours, puis les issues
    /// pas encore acquittées, chacune avec le numéro de son dernier changement.
    pub fn snapshot(&self) -> Vec<ServiceOperationEvent> {
        let mut events = Vec::new();
        let mut names: Vec<&String> = self.servers.keys().collect();
        names.sort();
        for name in names {
            let entry = &self.servers[name];
            let held = entry
                .live
                .values()
                .chain(entry.finished.iter())
                .map(|(seq, dto)| ServiceOperationEvent {
                    server_id: name.clone(),
                    seq: *seq,
                    change: OperationChangeDto::Upsert {
                        operation: Box::new(dto.clone()),
                    },
                });
            events.extend(held);
        }
        events
    }

    /// L'interface a annoncé ces issues : elles ne sont plus rejouées.
    pub fn ack(&mut self, server: &str, ids: &[String]) {
        if let Some(entry) = self.servers.get_mut(server) {
            entry.finished.retain(|(_, dto)| !ids.contains(&dto.id));
        }
    }

    /// Le serveur est retiré du carnet : on l'oublie.
    pub fn forget(&mut self, server: &str) {
        self.servers.remove(server);
    }
}

#[cfg(test)]
mod tests {
    use hearth_proto::api::service_ops::{OperationKind, OperationStatus};

    use super::*;

    fn view(id: &str, status: OperationStatus) -> OperationView {
        OperationView {
            id: id.to_owned(),
            key: "h_01J".to_owned(),
            kind: OperationKind::Deploy,
            status,
            step: None,
            steps: Vec::new(),
            percent: None,
            items_done: None,
            bytes_done: None,
            bytes_total: None,
            eta_s: None,
            cancellable: true,
            decision: None,
            cause: None,
        }
    }

    fn ids(events: &[ServiceOperationEvent]) -> Vec<String> {
        events
            .iter()
            .filter_map(|event| match &event.change {
                OperationChangeDto::Upsert { operation } => Some(operation.id.clone()),
                OperationChangeDto::Gone { .. } => None,
            })
            .collect()
    }

    #[test]
    fn a_window_that_subscribes_late_gets_the_running_operations_replayed() {
        let mut book = OpsBook::default();
        book.on_view("s1", &view("A", OperationStatus::Running));
        book.on_view("s1", &view("B", OperationStatus::Accepted));
        assert_eq!(ids(&book.snapshot()), ["A", "B"]);
    }

    #[test]
    fn the_sequence_grows_strictly_and_the_replay_keeps_each_last_number() {
        let mut book = OpsBook::default();
        let first = book.on_view("s1", &view("A", OperationStatus::Running));
        let second = book.on_view("s1", &view("A", OperationStatus::Running));
        assert!(second.seq > first.seq);
        let replayed = book.snapshot();
        assert_eq!(replayed.len(), 1);
        assert_eq!(replayed[0].seq, second.seq);
    }

    #[test]
    fn a_finished_operation_is_replayed_until_acknowledged_then_never_again() {
        let mut book = OpsBook::default();
        book.on_view("s1", &view("A", OperationStatus::Running));
        book.on_view("s1", &view("A", OperationStatus::Succeeded));
        // Elle ne court plus, mais son issue n'a pas été annoncée : elle est rejouée.
        assert_eq!(ids(&book.snapshot()), ["A"]);
        book.ack("s1", &["A".to_owned()]);
        assert!(book.snapshot().is_empty());
    }

    #[test]
    fn a_gone_operation_and_a_forgotten_server_leave_nothing() {
        let mut book = OpsBook::default();
        book.on_view("s1", &view("A", OperationStatus::Running));
        book.on_view("s2", &view("B", OperationStatus::Running));
        let gone = book.on_gone("s1", "A");
        assert!(matches!(gone.change, OperationChangeDto::Gone { .. }));
        book.forget("s2");
        assert!(book.snapshot().is_empty());
    }

    #[test]
    fn the_unacknowledged_outcomes_are_bounded() {
        let mut book = OpsBook::default();
        for n in 0..(MAX_FINISHED + 5) {
            book.on_view("s1", &view(&format!("op{n}"), OperationStatus::Succeeded));
        }
        assert_eq!(book.snapshot().len(), MAX_FINISHED);
    }
}
