//! Ce que la bibliothèque annonce à l'interface (un flux d'événements par `LinkManager`).

use std::sync::Arc;

use hearth_proto::api::machine::MachineResponse;
use hearth_proto::api::metrics::Sample;
use hearth_proto::fingerprint::Fingerprint;
use serde_json::Value;

use super::pending_ops::{OperationId, Outcome};
use super::server::ServerId;
use super::state::{Blocked, LinkState, Status};
use super::time::{Mono, WallTime};

/// L'état du lien tel que l'interface le reçoit (`link://state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateInfo {
    pub state: LinkState,
    pub blocked: Option<Blocked>,
    /// Depuis quand cet état est affiché.
    pub since: WallTime,
    /// Heure du dernier contact (bandeau « Dernier contact à {heure} »).
    pub last_contact_at: Option<WallTime>,
    /// Prochaine tentative automatique, s'il y en a une de planifiée.
    pub next_retry_at: Option<WallTime>,
    /// Tentatives échouées depuis le dernier succès (BR-RESIL-018).
    pub failed_attempts: u32,
}

impl StateInfo {
    /// Convertit l'état de la machine (instants monotones) en dates murales.
    pub fn from_status(
        status: &Status,
        now: Mono,
        wall_now: WallTime,
        last_contact_at: Option<WallTime>,
    ) -> Self {
        Self {
            state: status.state,
            blocked: status.blocked,
            since: to_wall(status.since, now, wall_now),
            last_contact_at,
            next_retry_at: status.next_retry_at.map(|at| to_wall(at, now, wall_now)),
            failed_attempts: status.failed_attempts,
        }
    }

    /// Ce qui compte pour décider d'annoncer un changement : tout sauf le dernier contact, qui
    /// bouge à chaque message.
    pub fn same_display(&self, other: &Self) -> bool {
        self.state == other.state
            && self.blocked == other.blocked
            && self.since == other.since
            && self.next_retry_at == other.next_retry_at
            && self.failed_attempts == other.failed_attempts
    }
}

/// Date murale d'un instant monotone, par rapport à « maintenant ».
pub fn to_wall(at: Mono, now: Mono, wall_now: WallTime) -> WallTime {
    if at <= now {
        wall_now.minus(now.since(at))
    } else {
        wall_now.plus(at.since(now))
    }
}

/// Pourquoi une session a pris fin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEnd {
    Expired,
    Revoked,
}

#[derive(Debug, Clone)]
pub enum Event {
    /// Changement d'état du lien d'un serveur.
    State { server: ServerId, info: StateInfo },
    /// Un échantillon de mesures (chaque seconde).
    Metrics {
        server: ServerId,
        sample: Arc<Sample>,
    },
    /// Identité de la machine et historique (à la connexion, et quand une carte apparaît).
    Snapshot {
        server: ServerId,
        machine: Arc<MachineResponse>,
        history: Arc<Vec<Sample>>,
    },
    /// Issue d'une action restée incertaine pendant une coupure (BR-RESIL-010).
    Operation {
        server: ServerId,
        id: OperationId,
        outcome: Outcome,
    },
    /// La session a pris fin (BR-RESIL-012, 014).
    SessionEnded { server: ServerId, kind: SessionEnd },
    /// Le certificat présenté n'est plus celui qui a été confirmé (BR-CONN-003).
    FingerprintChanged {
        server: ServerId,
        expected: Fingerprint,
        presented: Fingerprint,
    },
    /// Événement du journal d'activité (administrateurs abonnés).
    Audit { server: ServerId, event: Value },
}

impl Event {
    pub fn server(&self) -> &ServerId {
        match self {
            Self::State { server, .. }
            | Self::Metrics { server, .. }
            | Self::Snapshot { server, .. }
            | Self::Operation { server, .. }
            | Self::SessionEnded { server, .. }
            | Self::FingerprintChanged { server, .. }
            | Self::Audit { server, .. } => server,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monotonic_instants_map_onto_wall_time() {
        let now = Mono::from_millis(10_000);
        let wall = WallTime::from_millis(1_000_000);
        assert_eq!(
            to_wall(Mono::from_millis(7_000), now, wall),
            WallTime::from_millis(997_000)
        );
        assert_eq!(
            to_wall(Mono::from_millis(12_500), now, wall),
            WallTime::from_millis(1_002_500)
        );
    }

    #[test]
    fn the_last_contact_alone_is_not_a_display_change() {
        let base = StateInfo {
            state: LinkState::Connected,
            blocked: None,
            since: WallTime::from_millis(1),
            last_contact_at: Some(WallTime::from_millis(5)),
            next_retry_at: None,
            failed_attempts: 0,
        };
        let later = StateInfo {
            last_contact_at: Some(WallTime::from_millis(9)),
            ..base
        };
        assert!(base.same_display(&later));
        let offline = StateInfo {
            state: LinkState::Offline,
            ..base
        };
        assert!(!base.same_display(&offline));
    }
}
