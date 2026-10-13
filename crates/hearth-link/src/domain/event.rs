//! Ce que la bibliothèque annonce à l'interface (un flux d'événements par `LinkManager`).

use std::sync::Arc;

use hearth_proto::api::audit::AuditEventItem;
use hearth_proto::api::machine::MachineResponse;
use hearth_proto::api::metrics::Sample;
use hearth_proto::api::security::SecurityView;
use hearth_proto::api::services::ServicesState;
use hearth_proto::api::update::UpdateProgress;
use hearth_proto::fingerprint::Fingerprint;

use super::pending_ops::{OperationId, Outcome};
use super::server::ServerId;
use super::state::{Blocked, LinkState, Reason, Status};
use super::time::{Mono, WallTime};

/// L'état du lien tel que l'interface le reçoit (`link://state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateInfo {
    pub state: LinkState,
    pub blocked: Option<Blocked>,
    /// Pourquoi l'état d'arrêt est affiché (`SessionExpired`, `AccessRevoked`) : l'interface
    /// choisit son message et son action (formulaire de connexion, etc.).
    pub reason: Option<Reason>,
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
            reason: status.reason,
            since: to_wall(status.since, now, wall_now),
            last_contact_at,
            next_retry_at: status.next_retry_at.map(|at| to_wall(at, now, wall_now)),
            failed_attempts: status.failed_attempts,
        }
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
    /// Le mot de passe mémorisé est refusé : formulaire de connexion, identifiant prérempli.
    StoredPasswordRefused,
}

#[derive(Debug, Clone)]
pub enum Event {
    /// L'abonné a pris du retard : `skipped` événements ont été perdus, dont peut-être des
    /// changements d'état. Relire `LinkManager::states()` pour retrouver l'état courant.
    Lagged { skipped: u64 },
    /// Le fichier des opérations en suspens était illisible (en tout ou en partie) : des suivis
    /// ont pu être perdus. L'interface dit de vérifier l'état du serveur.
    OperationsLost { server: ServerId },
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
    /// L'heure écoulée AVANT l'instantané (à la connexion, `GET /metrics/history?window=1h`, 1 échantillon
    /// par 10 s) : le tableau de bord amorce sa courbe d'une heure dès l'ouverture (BR-DASH-010).
    /// Strictement plus ancienne que l'instantané ; jamais émise si la lecture échoue ou si rien n'est plus ancien.
    History {
        server: ServerId,
        samples: Arc<Vec<Sample>>,
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
    Audit {
        server: ServerId,
        event: Arc<AuditEventItem>,
    },
    /// Progression de la mise à jour de l'agent (sujet `update`, tout compte) : l'état courant à
    /// chaque connexion, puis chaque changement d'étape ou de pourcentage (HRT-17).
    AgentUpdate {
        server: ServerId,
        progress: Arc<UpdateProgress>,
    },
    /// État de sécurité du compte connecté (message `security` du flux, HRT-26) : une fois après
    /// l'authentification, puis à chaque changement (début ou fin d'une alerte, mode attaque).
    Security {
        server: ServerId,
        view: Arc<SecurityView>,
    },
    /// L'état du moteur de conteneurs et la liste des services (sujet `services`, HRT-62) : TOUJOURS
    /// l'état complet, déjà fusionné par la liaison (instantané ou lot), trié par nom sans tenir compte de la
    /// casse. Annoncé à la connexion, puis à chaque changement. Jamais émis pour un agent sans la capacité.
    ///
    /// `engine_received_at` : quand (heure de CE poste) la liaison a reçu la vue du moteur de cet état. C'est
    /// l'étage qui reçoit le message qui date : un état rejoué ou relu du disque garde sa vraie date.
    Services {
        server: ServerId,
        state: Arc<ServicesState>,
        engine_received_at: Option<WallTime>,
    },
    /// Un autre compte a fermé les sessions de ce compte : dit UNE fois par l'agent, à la première connexion qui
    /// suit (connexion saisie ou reconnexion silencieuse), avec l'identifiant de l'administrateur et la date
    /// (HRT-133, BR-ACCT-019). Jamais d'adresse.
    SessionsClosed {
        server: ServerId,
        by: String,
        at: String,
    },
    /// L'agent ne sert pas les services (pas de capacité `services` dans `/hello`) : le client ne lui a
    /// envoyé ni le sujet ni aucune route. Annoncé à chaque connexion à cet agent, avec sa version.
    ServicesUnsupported {
        server: ServerId,
        agent_version: String,
    },
}

impl Event {
    /// Le serveur concerné (aucun pour `Lagged`).
    pub fn server(&self) -> Option<&ServerId> {
        match self {
            Self::Lagged { .. } => None,
            Self::OperationsLost { server } => Some(server),
            Self::State { server, .. }
            | Self::Metrics { server, .. }
            | Self::Snapshot { server, .. }
            | Self::History { server, .. }
            | Self::Operation { server, .. }
            | Self::SessionEnded { server, .. }
            | Self::FingerprintChanged { server, .. }
            | Self::Audit { server, .. }
            | Self::AgentUpdate { server, .. }
            | Self::Security { server, .. }
            | Self::Services { server, .. }
            | Self::SessionsClosed { server, .. }
            | Self::ServicesUnsupported { server, .. } => Some(server),
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
}
