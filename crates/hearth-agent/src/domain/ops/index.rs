//! Ce que la liste des services dit des opérations (HRT-144, ADR-0045) : `ServiceItem.busy` (l'opération qui
//! tient le service, visible de tous, BR-LIFE-013) et `ServiceItem.last_action` (la dernière opération
//! terminée) sont des PROJECTIONS du journal des opérations, jamais un état à part. L'index les tient en
//! mémoire, alimenté par chaque changement d'opération et rebâti du journal au démarrage.
//!
//! Borné : au plus [`MAX_INDEXED_SERVICES`] services gardent une « dernière action » (les plus récemment
//! servis restent). Une opération non terminée est toujours gardée.

use std::collections::HashMap;

use time::OffsetDateTime;

use super::kind::{Cause, OpKind, OpStatus};
use crate::domain::services::key::ServiceKey;

/// Combien de services gardent une dernière action en mémoire.
pub const MAX_INDEXED_SERVICES: usize = 1_000;

/// Ce qu'on retient d'une opération pour la liste : jamais le plan, jamais une étape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpSummary {
    pub id: String,
    pub kind: OpKind,
    /// Le compte qui l'a lancée, tel que l'agent l'a figé.
    pub account: String,
    pub status: OpStatus,
    pub cause: Option<Cause>,
    pub forced: bool,
    pub started_at: OffsetDateTime,
    pub finished_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServiceOpsEntry {
    /// L'opération non terminée qui tient le service.
    pub open: Option<OpSummary>,
    /// La dernière opération terminée.
    pub last: Option<OpSummary>,
}

#[derive(Debug, Default)]
pub struct OpsIndex {
    services: HashMap<ServiceKey, ServiceOpsEntry>,
}

impl OpsIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Range l'état d'une opération. Une opération terminée remplace « la dernière action » seulement si
    /// elle est plus récente que celle qui y est ; elle libère l'opération ouverte si c'est la sienne.
    ///
    /// **Idempotent et monotone** : un état non terminal qui arrive APRÈS la fin de son opération (deux
    /// publications dans l'ordre inverse de leur lecture) ne la rouvre jamais, donc ne laisse jamais `busy`
    /// collé sur un service libre. Une opération n'est jamais rouverte par un état plus ancien : une
    /// opération qui a commencé avant la dernière terminée du service, ou avant l'ouverte qui y est, est
    /// périmée (le verrou du service interdit deux opérations à la fois).
    pub fn apply(&mut self, key: &ServiceKey, summary: OpSummary) {
        let entry = self.services.entry(key.clone()).or_default();
        if !summary.status.is_terminal() {
            let stale_against_last = entry
                .last
                .as_ref()
                .is_some_and(|last| last.id == summary.id || last.started_at > summary.started_at);
            let stale_against_open = entry
                .open
                .as_ref()
                .is_some_and(|open| open.id != summary.id && open.started_at > summary.started_at);
            if stale_against_last || stale_against_open {
                return;
            }
        }
        if summary.status.is_terminal() {
            if entry
                .open
                .as_ref()
                .is_some_and(|open| open.id == summary.id)
            {
                entry.open = None;
            }
            let newer = entry.last.as_ref().is_none_or(|last| later(&summary, last));
            if newer {
                entry.last = Some(summary);
            }
        } else {
            entry.open = Some(summary);
        }
        self.bound();
    }

    pub fn get(&self, key: &ServiceKey) -> Option<&ServiceOpsEntry> {
        self.services.get(key)
    }

    /// L'opération non terminée de ce service.
    pub fn busy(&self, key: &ServiceKey) -> Option<&OpSummary> {
        self.services.get(key).and_then(|entry| entry.open.as_ref())
    }

    /// La dernière opération terminée de ce service.
    pub fn last_action(&self, key: &ServiceKey) -> Option<&OpSummary> {
        self.services.get(key).and_then(|entry| entry.last.as_ref())
    }

    pub fn len(&self) -> usize {
        self.services.len()
    }

    pub fn is_empty(&self) -> bool {
        self.services.is_empty()
    }

    /// Retire les services sans opération ouverte dont la dernière action est la plus ancienne, jusqu'à
    /// rentrer dans la borne. Un service qui a une opération ouverte n'est jamais retiré.
    fn bound(&mut self) {
        if self.services.len() <= MAX_INDEXED_SERVICES {
            return;
        }
        let mut idle: Vec<(Option<OffsetDateTime>, ServiceKey)> = self
            .services
            .iter()
            .filter(|(_, entry)| entry.open.is_none())
            .map(|(key, entry)| {
                (
                    entry.last.as_ref().and_then(|last| last.finished_at),
                    key.clone(),
                )
            })
            .collect();
        idle.sort();
        let excess = self.services.len() - MAX_INDEXED_SERVICES;
        for (_, key) in idle.into_iter().take(excess) {
            self.services.remove(&key);
        }
    }
}

fn later(candidate: &OpSummary, current: &OpSummary) -> bool {
    match (candidate.finished_at, current.finished_at) {
        (Some(a), Some(b)) => a >= b,
        (Some(_), None) => true,
        (None, _) => false,
    }
}

#[cfg(test)]
mod tests {
    use time::Duration;

    use super::*;

    fn key(text: &str) -> ServiceKey {
        ServiceKey::parse(text).unwrap()
    }

    fn summary(id: &str, status: OpStatus, finished_after_s: Option<i64>) -> OpSummary {
        let start = OffsetDateTime::UNIX_EPOCH + Duration::days(20_000);
        OpSummary {
            id: id.to_owned(),
            kind: OpKind::Start,
            account: "marie".to_owned(),
            status,
            cause: None,
            forced: false,
            started_at: start,
            finished_at: finished_after_s.map(|s| start + Duration::seconds(s)),
        }
    }

    #[test]
    fn an_open_operation_is_busy_until_it_finishes_and_then_becomes_the_last_action() {
        let service = key("c_0123abcd");
        let mut index = OpsIndex::new();
        index.apply(&service, summary("A", OpStatus::Running, None));
        assert_eq!(index.busy(&service).map(|op| op.id.as_str()), Some("A"));
        assert!(index.last_action(&service).is_none());
        index.apply(&service, summary("A", OpStatus::Succeeded, Some(5)));
        assert!(index.busy(&service).is_none());
        assert_eq!(
            index.last_action(&service).map(|op| op.id.as_str()),
            Some("A")
        );
    }

    #[test]
    fn an_older_finished_operation_never_replaces_a_newer_last_action() {
        let service = key("c_0123abcd");
        let mut index = OpsIndex::new();
        index.apply(&service, summary("NEW", OpStatus::Succeeded, Some(50)));
        index.apply(&service, summary("OLD", OpStatus::Failed, Some(10)));
        assert_eq!(
            index.last_action(&service).map(|op| op.id.as_str()),
            Some("NEW")
        );
    }

    #[test]
    fn a_stale_open_state_never_reopens_a_finished_operation_so_busy_never_sticks() {
        let service = key("c_0123abcd");
        let mut index = OpsIndex::new();
        // L'état final arrive avant un état plus ancien de la même opération.
        index.apply(&service, summary("A", OpStatus::Cancelled, Some(5)));
        index.apply(&service, summary("A", OpStatus::Running, None));
        index.apply(&service, summary("A", OpStatus::Cancelling, None));
        assert!(index.busy(&service).is_none());
        assert_eq!(
            index.last_action(&service).map(|op| op.status),
            Some(OpStatus::Cancelled)
        );
        // Même chose rejoué : le résultat ne change plus (idempotent).
        index.apply(&service, summary("A", OpStatus::Cancelled, Some(5)));
        assert!(index.busy(&service).is_none());
        // Une opération plus ancienne que la dernière terminée ne rouvre rien non plus.
        let mut newer = summary("NEW", OpStatus::Succeeded, Some(50));
        newer.started_at += time::Duration::seconds(100);
        newer.finished_at = Some(newer.started_at + time::Duration::seconds(1));
        index.apply(&service, newer);
        index.apply(&service, summary("OLD", OpStatus::Running, None));
        assert!(index.busy(&service).is_none());
        // Une opération vraiment nouvelle, elle, ouvre bien le service.
        let mut fresh = summary("FRESH", OpStatus::Running, None);
        fresh.started_at += time::Duration::seconds(500);
        index.apply(&service, fresh);
        assert_eq!(index.busy(&service).map(|op| op.id.as_str()), Some("FRESH"));
    }

    #[test]
    fn finishing_another_operation_does_not_free_the_open_one() {
        let service = key("c_0123abcd");
        let mut index = OpsIndex::new();
        index.apply(&service, summary("B", OpStatus::Running, None));
        index.apply(&service, summary("A", OpStatus::Succeeded, Some(5)));
        assert_eq!(index.busy(&service).map(|op| op.id.as_str()), Some("B"));
    }

    #[test]
    fn the_index_is_bounded_and_a_service_with_an_open_operation_is_never_dropped() {
        let mut index = OpsIndex::new();
        let busy = key("c_busy0001");
        index.apply(&busy, summary("BUSY", OpStatus::NeedsAttention, None));
        for number in 0..MAX_INDEXED_SERVICES + 20 {
            let service = key(&format!("c_svc{number:06}"));
            index.apply(
                &service,
                summary(&format!("OP{number}"), OpStatus::Succeeded, Some(1)),
            );
            assert!(index.len() <= MAX_INDEXED_SERVICES);
        }
        assert!(index.busy(&busy).is_some());
    }
}
