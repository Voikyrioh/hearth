//! Les opérations de service (HRT-83, ADR-0045) : démarrer, déployer, modifier, supprimer, sauvegarder,
//! restaurer, reprendre un monde. Un seul modèle pour tous : un genre fermé, un statut fermé, des étapes.
//!
//! Cette tranche ne déclare que les TYPES et les constantes du lot : aucune route ne les sert encore, le
//! message `operation` du sujet `services` (au plus un par 500 ms et par opération) rejoint
//! `stream::ServicesMessage` avec le suivi côté client (HRT-84), qui lit ce corps tel quel. Rien d'ici ne
//! porte une valeur, un nom de variable ou une commande : le plan d'une opération n'a jamais de secret.
//!
//! Évolution : toute énumération que le client lit tolère une valeur inconnue (`Unknown`), tout champ
//! ajouté plus tard est facultatif (`serde(default)`).

use serde::{Deserialize, Serialize};

/// Capacité annoncée par `GET /hello` quand l'agent sert la description des services et leurs
/// opérations (modifier, supprimer, opérations génériques). Déclarée par HRT-83, annoncée quand une
/// première route existe (HRT-64, HRT-68) : un client n'affiche rien sans elle.
pub const CAPABILITY_SERVICE_OPS: &str = "service-ops";
/// Capacité : processeur et mémoire d'un service, consommation (lot suivant).
pub const CAPABILITY_SERVICE_RESOURCES: &str = "service-resources";
/// Capacité : parcourir, lire et éditer les fichiers d'un service (lot suivant).
pub const CAPABILITY_SERVICE_FILES: &str = "service-files";
/// Capacité : envois et téléchargements reprenables (lot suivant).
pub const CAPABILITY_SERVICE_TRANSFERS: &str = "service-transfers";
/// Capacité : sauvegardes et restaurations (lot suivant).
pub const CAPABILITY_SERVICE_BACKUPS: &str = "service-backups";
/// Capacité : catalogue de fiches de serveur de jeu (lot suivant).
pub const CAPABILITY_CATALOG: &str = "catalog";
/// Capacité : un serveur de jeu au quotidien (lot suivant).
pub const CAPABILITY_GAME_SERVICES: &str = "game-services";

/// Une décision en attente expire en 15 minutes, vers « annulé » (BR-JEU-006).
pub const DECISION_EXPIRY_S: u64 = 15 * 60;
/// Une opération terminée est purgée de la base 24 h après sa fin ; une opération `needs_attention` jamais.
pub const FINISHED_RETENTION_S: u64 = 24 * 60 * 60;
/// Un message `operation` au plus par ce délai et par opération.
pub const PROGRESS_MIN_INTERVAL_MS: u64 = 500;

/// Le genre d'une opération (liste fermée, ADR-0045).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Start,
    Stop,
    Restart,
    Deploy,
    Update,
    Delete,
    Resources,
    Backup,
    Restore,
    WorldImport,
    /// Un genre que ce client ne connaît pas.
    #[serde(other)]
    Unknown,
}

/// Le statut d'une opération (liste fermée, ADR-0045).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    Accepted,
    Running,
    AwaitingDecision,
    Cancelling,
    Cancelled,
    RollingBack,
    RolledBack,
    Succeeded,
    Failed,
    /// L'agent a redémarré en plein geste, l'état est cohérent et dit : rien n'attend.
    Interrupted,
    /// L'état est ambigu : le verrou du service est repris, une décision est demandée.
    NeedsAttention,
    /// Un statut que ce client ne connaît pas.
    #[serde(other)]
    Unknown,
}

/// Un choix offert au compte (liste fermée).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationChoice {
    /// Poursuivre l'opération.
    Continue,
    /// Annuler : ce que l'opération avait créé est retiré.
    Cancel,
    /// Revenir à l'état d'avant.
    Revert,
    /// Garder l'état tel qu'il est.
    Keep,
    /// « J'ai vérifié » : le compte a regardé l'état, l'opération se ferme sans rien changer.
    Verified,
    /// Un choix que ce client ne connaît pas.
    #[serde(other)]
    Unknown,
}

/// Pourquoi l'opération est dans son statut (liste fermée de codes ; l'interface les met en phrases).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationCause {
    /// Rien n'avait été exécuté quand l'agent s'est arrêté.
    NotExecuted,
    /// L'agent s'est arrêté ; le service tourne avec la configuration d'avant.
    RunsWithPrevious,
    /// L'agent s'est arrêté ; le service tourne avec la nouvelle configuration.
    RunsWithNew,
    /// Un geste destructif était commencé sans être fini.
    IntentWithoutDone,
    /// Un geste destructif a échoué.
    StepFailed,
    /// Deux états coexistent (par exemple deux conteneurs).
    TwoStates,
    /// L'état n'a pas pu être lu (moteur injoignable).
    StateUnknown,
    /// La décision demandée n'a pas reçu de réponse en 15 minutes.
    DecisionExpired,
    /// Le compte a annulé.
    CancelledByAccount,
    /// Le compte a pris la décision.
    DecidedByAccount,
    /// Une cause que ce client ne connaît pas.
    #[serde(other)]
    Unknown,
}

/// État d'une étape du journal : `intent` est écrit AVANT le geste, `done` après.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStateView {
    Intent,
    Done,
    Failed,
    Undone,
    #[serde(other)]
    Unknown,
}

/// Une étape dans le suivi : son nom (code stable), son état.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepView {
    pub step: String,
    pub state: StepStateView,
}

/// La décision attendue du compte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionView {
    pub choices: Vec<OperationChoice>,
    /// Quand la décision expire (RFC 3339, UTC).
    pub expires_at: String,
}

/// Une opération telle que le client la suit (corps du message `operation`). Aucun secret, aucune valeur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationView {
    pub id: String,
    /// La clé du service (`h_`, `u_`, `c_`).
    pub key: String,
    pub kind: OperationKind,
    pub status: OperationStatus,
    /// L'étape en cours (code stable), absente avant la première.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<StepView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items_done: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes_done: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes_total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eta_s: Option<u64>,
    /// L'annulation est encore possible (avant le point de non-retour du genre).
    #[serde(default)]
    pub cancellable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<DecisionView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<OperationCause>,
}

/// Corps d'une décision (acte `OperationDecide`, déclaré avec sa route par le ticket qui la sert).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationDecision {
    pub choice: OperationChoice,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_unknown_value_reads_as_unknown_never_as_an_error() {
        let view: OperationView = serde_json::from_value(json!({
            "id": "01J", "key": "h_01J", "kind": "teleport", "status": "levitating",
            "cause": "something_new", "future_field": 1,
            "steps": [{ "step": "x", "state": "dreaming" }],
            "decision": { "choices": ["continue", "dance"], "expires_at": "2026-10-13T10:00:00.000Z" }
        }))
        .expect("tolérant");
        assert_eq!(view.kind, OperationKind::Unknown);
        assert_eq!(view.status, OperationStatus::Unknown);
        assert_eq!(view.cause, Some(OperationCause::Unknown));
        assert_eq!(view.steps[0].state, StepStateView::Unknown);
        assert_eq!(
            view.decision.expect("décision").choices,
            [OperationChoice::Continue, OperationChoice::Unknown]
        );
    }

    #[test]
    fn a_minimal_operation_round_trips_and_omits_what_it_does_not_have() {
        let view = OperationView {
            id: "01J".into(),
            key: "h_01J".into(),
            kind: OperationKind::Update,
            status: OperationStatus::NeedsAttention,
            step: None,
            steps: Vec::new(),
            percent: None,
            items_done: None,
            bytes_done: None,
            bytes_total: None,
            eta_s: None,
            cancellable: false,
            decision: None,
            cause: Some(OperationCause::IntentWithoutDone),
        };
        let value = serde_json::to_value(&view).expect("json");
        assert_eq!(
            value,
            json!({
                "id": "01J", "key": "h_01J", "kind": "update", "status": "needs_attention",
                "cancellable": false, "cause": "intent_without_done"
            })
        );
        assert_eq!(
            serde_json::from_value::<OperationView>(value).expect("lu"),
            view
        );
    }

    #[test]
    fn the_wire_names_are_the_ones_of_the_technical_design() {
        assert_eq!(
            serde_json::to_value(OperationKind::WorldImport).expect("json"),
            json!("world_import")
        );
        assert_eq!(
            serde_json::to_value(OperationStatus::AwaitingDecision).expect("json"),
            json!("awaiting_decision")
        );
        assert_eq!(CAPABILITY_SERVICE_OPS, "service-ops");
        assert_eq!(DECISION_EXPIRY_S, 900);
    }
}
