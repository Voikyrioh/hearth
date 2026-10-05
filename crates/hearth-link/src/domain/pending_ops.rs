//! Opérations en suspens : une action envoyée dont la réponse n'est peut-être pas arrivée
//! (BR-RESIL-009, BR-RESIL-010).
//!
//! Une action porte une clé d'opération (`Idempotency-Key`). Si le lien tombe avant la réponse,
//! elle passe à « résultat inconnu » et n'est **jamais** rejouée automatiquement : au retour du
//! lien, on demande à l'agent ce qu'il est advenu de la clé (`GET /operations/{id}`) et on annonce
//! l'une des trois issues : fait pendant la coupure, non exécuté, résultat inconnu.

use std::collections::BTreeMap;
use std::time::Duration;

use hearth_proto::api::operations::OperationStatus;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use super::time::WallTime;

/// Au-delà, une opération en suspens est abandonnée comme « résultat inconnu » (l'agent ne
/// conserve de toute façon les opérations que 24 h).
pub const ABANDON_AFTER: Duration = Duration::from_secs(24 * 3600);

/// Une opération « en cours » côté agent est relue au plus ce nombre de fois avant de conclure
/// « résultat inconnu ».
pub const MAX_RUNNING_CHECKS: u32 = 5;

/// Nombre maximal d'opérations suivies en même temps (borne mémoire, BR-RESIL-017).
pub const MAX_PENDING: usize = 256;

/// Clé d'opération : 1 à 64 caractères, lettres, chiffres, tiret, souligné (un ULID en pratique).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct OperationId(String);

impl TryFrom<String> for OperationId {
    type Error = InvalidOperationId;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<OperationId> for String {
    fn from(value: OperationId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("clé d'opération invalide")]
pub struct InvalidOperationId;

impl OperationId {
    pub fn parse(text: &str) -> Result<Self, InvalidOperationId> {
        let valid = !text.is_empty()
            && text.len() <= 64
            && text
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if valid {
            Ok(Self(text.to_owned()))
        } else {
            Err(InvalidOperationId)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for OperationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    /// Envoyée, réponse attendue, lien tenu.
    InFlight,
    /// Le lien est tombé avant la réponse : on ne sait pas.
    Unknown,
}

/// Ne contient ni corps de requête ni secret : seulement la clé, `MÉTHODE /chemin` et des dates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingOp {
    pub id: OperationId,
    /// `MÉTHODE /chemin`, pour l'affichage.
    pub kind: String,
    pub sent_at: WallTime,
    pub phase: Phase,
    /// Nombre de fois où l'agent a répondu « en cours ».
    checks: u32,
}

/// Issue annoncée à l'utilisateur au retour du lien.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// « Fait pendant la coupure » : l'agent a exécuté l'action ; `result` est sa réponse.
    DoneDuringOutage { result: Option<Value> },
    /// « Non exécuté, tu peux relancer » : l'agent ne l'a jamais reçue, ou l'a refusée.
    NotExecuted,
    /// « Résultat inconnu, vérifie l'état » : l'agent s'est arrêté pendant l'exécution, ou
    /// l'exécution dure encore, ou plus personne ne s'en souvient.
    StillUnknown,
}

/// Ce que l'agent a répondu à `GET /operations/{id}`.
#[derive(Debug, Clone, PartialEq)]
pub enum Lookup {
    Status {
        status: OperationStatus,
        result: Option<Value>,
    },
    /// `404` : l'agent n'a jamais reçu cette clé.
    NotFound,
    /// Pas de réponse exploitable (lien retombé, erreur) : on réessaiera au prochain retour.
    Unreachable,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Resolution {
    /// Issue définitive ; l'opération n'est plus suivie.
    Final(Outcome),
    /// Pas encore tranché : l'opération reste suivie, relire plus tard.
    CheckAgain,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PendingError {
    #[error("cette clé d'opération est déjà suivie")]
    Duplicate,
    #[error("trop d'opérations en suspens")]
    TooMany,
}

#[derive(Debug, Default)]
pub struct PendingOps {
    ops: BTreeMap<OperationId, PendingOp>,
}

impl PendingOps {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.ops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    pub fn get(&self, id: &OperationId) -> Option<&PendingOp> {
        self.ops.get(id)
    }

    /// Les opérations suivies, pour les mettre sur disque (elles survivent à un redémarrage).
    pub fn snapshot(&self) -> Vec<PendingOp> {
        self.ops.values().cloned().collect()
    }

    /// Reprend des opérations lues sur disque. L'application a redémarré : ce qui était en vol
    /// est « résultat inconnu ». Les opérations de plus de 24 h et celles au-delà de
    /// [`MAX_PENDING`] sont abandonnées ; rend les clés abandonnées (« résultat inconnu »).
    pub fn restore(&mut self, saved: Vec<PendingOp>, now: WallTime) -> Vec<OperationId> {
        let mut abandoned = Vec::new();
        for mut op in saved {
            if now.since(op.sent_at) >= ABANDON_AFTER
                || self.ops.len() >= MAX_PENDING
                || self.ops.contains_key(&op.id)
            {
                abandoned.push(op.id);
                continue;
            }
            op.phase = Phase::Unknown;
            self.ops.insert(op.id.clone(), op);
        }
        abandoned
    }

    /// Solde tout en « résultat inconnu » sans interroger l'agent (autre compte, nouvelle
    /// empreinte : la clé d'opération ne dit plus rien). Rend les clés soldées.
    pub fn settle_all(&mut self) -> Vec<OperationId> {
        let ids: Vec<OperationId> = self.ops.keys().cloned().collect();
        self.ops.clear();
        ids
    }

    /// Une action part : on la suit.
    pub fn register(
        &mut self,
        id: OperationId,
        kind: String,
        now: WallTime,
    ) -> Result<(), PendingError> {
        if self.ops.contains_key(&id) {
            return Err(PendingError::Duplicate);
        }
        if self.ops.len() >= MAX_PENDING {
            return Err(PendingError::TooMany);
        }
        self.ops.insert(
            id.clone(),
            PendingOp {
                id,
                kind,
                sent_at: now,
                phase: Phase::InFlight,
                checks: 0,
            },
        );
        Ok(())
    }

    /// La réponse est arrivée : plus rien à suivre. Vrai si l'opération était suivie.
    pub fn complete(&mut self, id: &OperationId) -> bool {
        self.ops.remove(id).is_some()
    }

    /// Une seule opération en vol devient « résultat inconnu » (sa requête a échoué sans que le
    /// flux soit tombé). Vrai si elle était en vol.
    pub fn mark_unknown(&mut self, id: &OperationId) -> bool {
        match self.ops.get_mut(id) {
            Some(op) if op.phase == Phase::InFlight => {
                op.phase = Phase::Unknown;
                true
            }
            _ => false,
        }
    }

    /// Le lien est tombé : toutes les opérations en vol passent à « résultat inconnu ».
    /// Rend celles qui viennent de basculer.
    pub fn link_lost(&mut self) -> Vec<OperationId> {
        let mut lost = Vec::new();
        for op in self.ops.values_mut() {
            if op.phase == Phase::InFlight {
                op.phase = Phase::Unknown;
                lost.push(op.id.clone());
            }
        }
        lost
    }

    /// Les opérations à demander à l'agent au retour du lien.
    pub fn to_resolve(&self) -> Vec<OperationId> {
        self.ops
            .values()
            .filter(|op| op.phase == Phase::Unknown)
            .map(|op| op.id.clone())
            .collect()
    }

    /// Range la réponse de l'agent. `None` si la clé n'est pas suivie.
    pub fn resolve(
        &mut self,
        id: &OperationId,
        lookup: Lookup,
        now: WallTime,
    ) -> Option<Resolution> {
        let op = self.ops.get_mut(id)?;
        let resolution = decide(op, lookup, now);
        if matches!(resolution, Resolution::Final(_)) {
            self.ops.remove(id);
        }
        Some(resolution)
    }
}

fn decide(op: &mut PendingOp, lookup: Lookup, now: WallTime) -> Resolution {
    // Trop vieille : l'agent l'a oubliée (rétention 24 h), personne ne peut plus trancher.
    if now.since(op.sent_at) >= ABANDON_AFTER {
        return Resolution::Final(Outcome::StillUnknown);
    }
    match lookup {
        Lookup::Status { status, result } => match status {
            OperationStatus::Succeeded => Resolution::Final(Outcome::DoneDuringOutage { result }),
            // Une réponse 4xx a été rendue : l'action a été refusée, donc pas exécutée.
            OperationStatus::Failed => Resolution::Final(Outcome::NotExecuted),
            OperationStatus::Interrupted => Resolution::Final(Outcome::StillUnknown),
            OperationStatus::Running => {
                op.checks = op.checks.saturating_add(1);
                if op.checks >= MAX_RUNNING_CHECKS {
                    Resolution::Final(Outcome::StillUnknown)
                } else {
                    Resolution::CheckAgain
                }
            }
        },
        Lookup::NotFound => Resolution::Final(Outcome::NotExecuted),
        Lookup::Unreachable => Resolution::CheckAgain,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const T0: WallTime = WallTime::from_millis(1_000_000);

    fn id(text: &str) -> OperationId {
        OperationId::parse(text).unwrap()
    }

    fn pending_with(text: &str) -> PendingOps {
        let mut ops = PendingOps::new();
        ops.register(id(text), "PUT /me/password".into(), T0)
            .unwrap();
        ops
    }

    fn lost(text: &str) -> PendingOps {
        let mut ops = pending_with(text);
        assert_eq!(ops.link_lost(), vec![id(text)]);
        ops
    }

    #[test]
    fn keys_are_validated_like_the_agent_does() {
        assert!(OperationId::parse("01J9ZY0G3Q8M2K6W4T7V5N1B9D").is_ok());
        assert!(OperationId::parse("a-b_c").is_ok());
        assert!(OperationId::parse(&"x".repeat(64)).is_ok());
        assert!(OperationId::parse("").is_err());
        assert!(OperationId::parse(&"x".repeat(65)).is_err());
        assert!(OperationId::parse("a b").is_err());
        assert!(OperationId::parse("a/b").is_err());
    }

    #[test]
    fn a_response_before_the_link_drops_forgets_the_operation() {
        let mut ops = pending_with("A");
        assert!(ops.complete(&id("A")));
        assert!(ops.is_empty());
        assert!(ops.link_lost().is_empty());
        assert!(!ops.complete(&id("A")));
    }

    #[test]
    fn a_dropped_link_makes_in_flight_operations_unknown_and_nothing_replays_them() {
        let mut ops = pending_with("A");
        assert_eq!(ops.get(&id("A")).unwrap().phase, Phase::InFlight);
        assert_eq!(ops.link_lost(), vec![id("A")]);
        assert_eq!(ops.get(&id("A")).unwrap().phase, Phase::Unknown);
        // Une seconde perte ne re-signale pas ce qui est déjà inconnu.
        assert!(ops.link_lost().is_empty());
        assert_eq!(ops.to_resolve(), vec![id("A")]);
    }

    #[test]
    fn one_operation_can_become_unknown_alone() {
        let mut ops = pending_with("A");
        ops.register(id("B"), "x".into(), T0).unwrap();
        assert!(ops.mark_unknown(&id("A")));
        assert!(!ops.mark_unknown(&id("A")), "déjà inconnue");
        assert!(!ops.mark_unknown(&id("Z")), "inconnue du suivi");
        assert_eq!(ops.to_resolve(), vec![id("A")]);
        assert_eq!(ops.get(&id("B")).unwrap().phase, Phase::InFlight);
    }

    #[test]
    fn an_operation_still_in_flight_is_not_asked_about() {
        let ops = pending_with("A");
        assert!(ops.to_resolve().is_empty());
    }

    #[test]
    fn succeeded_means_done_during_the_outage() {
        let mut ops = lost("A");
        let result = Some(json!({ "sessions_closed": 1 }));
        let resolution = ops.resolve(
            &id("A"),
            Lookup::Status {
                status: OperationStatus::Succeeded,
                result: result.clone(),
            },
            T0,
        );
        assert_eq!(
            resolution,
            Some(Resolution::Final(Outcome::DoneDuringOutage { result }))
        );
        assert!(ops.is_empty());
    }

    #[test]
    fn not_found_means_not_executed() {
        let mut ops = lost("A");
        assert_eq!(
            ops.resolve(&id("A"), Lookup::NotFound, T0),
            Some(Resolution::Final(Outcome::NotExecuted))
        );
        assert!(ops.is_empty());
    }

    #[test]
    fn a_refused_action_was_not_executed() {
        let mut ops = lost("A");
        let resolution = ops.resolve(
            &id("A"),
            Lookup::Status {
                status: OperationStatus::Failed,
                result: Some(json!({ "error": {} })),
            },
            T0,
        );
        assert_eq!(resolution, Some(Resolution::Final(Outcome::NotExecuted)));
    }

    #[test]
    fn interrupted_means_the_result_is_unknown() {
        let mut ops = lost("A");
        let resolution = ops.resolve(
            &id("A"),
            Lookup::Status {
                status: OperationStatus::Interrupted,
                result: None,
            },
            T0,
        );
        assert_eq!(resolution, Some(Resolution::Final(Outcome::StillUnknown)));
        assert!(ops.is_empty());
    }

    #[test]
    fn running_is_read_again_a_few_times_then_declared_unknown() {
        let mut ops = lost("A");
        let running = || Lookup::Status {
            status: OperationStatus::Running,
            result: None,
        };
        for _ in 1..MAX_RUNNING_CHECKS {
            assert_eq!(
                ops.resolve(&id("A"), running(), T0),
                Some(Resolution::CheckAgain)
            );
        }
        assert_eq!(
            ops.resolve(&id("A"), running(), T0),
            Some(Resolution::Final(Outcome::StillUnknown))
        );
        assert!(ops.is_empty());
    }

    #[test]
    fn an_unreachable_agent_keeps_the_operation_for_the_next_return() {
        let mut ops = lost("A");
        for _ in 0..50 {
            assert_eq!(
                ops.resolve(&id("A"), Lookup::Unreachable, T0),
                Some(Resolution::CheckAgain)
            );
        }
        assert_eq!(ops.len(), 1);
    }

    #[test]
    fn after_twenty_four_hours_the_operation_is_abandoned_as_unknown() {
        let mut ops = lost("A");
        let almost = T0.plus(ABANDON_AFTER).minus(Duration::from_millis(1));
        assert_eq!(
            ops.resolve(&id("A"), Lookup::Unreachable, almost),
            Some(Resolution::CheckAgain)
        );
        let due = T0.plus(ABANDON_AFTER);
        assert_eq!(
            ops.resolve(&id("A"), Lookup::NotFound, due),
            Some(Resolution::Final(Outcome::StillUnknown))
        );
        assert!(ops.is_empty());
    }

    #[test]
    fn pending_operations_survive_a_restart_as_unknown_and_old_ones_are_dropped() {
        let mut ops = pending_with("A");
        ops.register(id("B"), "POST /x".into(), T0.minus(ABANDON_AFTER))
            .unwrap();
        let saved = ops.snapshot();
        let text = serde_json::to_string(&saved).unwrap();
        let loaded: Vec<PendingOp> = serde_json::from_str(&text).unwrap();
        let mut fresh = PendingOps::new();
        let abandoned = fresh.restore(loaded, T0);
        assert_eq!(abandoned, vec![id("B")]);
        assert_eq!(fresh.get(&id("A")).unwrap().phase, Phase::Unknown);
        assert_eq!(fresh.to_resolve(), vec![id("A")]);
    }

    #[test]
    fn a_saved_key_that_is_not_a_valid_key_is_refused_on_load() {
        let text = r#"[{"id":"a/b","kind":"x","sent_at":1,"phase":"Unknown","checks":0}]"#;
        assert!(serde_json::from_str::<Vec<PendingOp>>(text).is_err());
    }

    #[test]
    fn settling_everything_forgets_the_operations_and_returns_their_keys() {
        let mut ops = lost("A");
        ops.register(id("B"), "x".into(), T0).unwrap();
        let mut settled = ops.settle_all();
        settled.sort();
        assert_eq!(settled, vec![id("A"), id("B")]);
        assert!(ops.is_empty());
    }

    #[test]
    fn an_unknown_key_resolves_to_nothing() {
        let mut ops = PendingOps::new();
        assert_eq!(ops.resolve(&id("A"), Lookup::NotFound, T0), None);
    }

    #[test]
    fn the_same_key_cannot_be_registered_twice() {
        let mut ops = pending_with("A");
        assert_eq!(
            ops.register(id("A"), "x".into(), T0),
            Err(PendingError::Duplicate)
        );
    }

    #[test]
    fn the_number_of_tracked_operations_is_bounded() {
        let mut ops = PendingOps::new();
        for n in 0..MAX_PENDING {
            ops.register(id(&format!("op{n}")), "x".into(), T0).unwrap();
        }
        assert_eq!(
            ops.register(id("one-more"), "x".into(), T0),
            Err(PendingError::TooMany)
        );
        assert!(ops.complete(&id("op0")));
        assert!(ops.register(id("one-more"), "x".into(), T0).is_ok());
    }
}
