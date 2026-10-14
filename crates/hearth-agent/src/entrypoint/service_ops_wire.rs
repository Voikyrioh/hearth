//! Les opérations de service vers les types du fil de `hearth-proto` (HRT-144, ADR-0045). Partagé par
//! `http/service_ops.rs` (lire, annuler, décider) et `ws/` (message `operation` du sujet `services`) : le
//! contrat JSON d'une opération n'est connu qu'ici.
//!
//! Ce qui sort : des codes (genre, statut, étape, cause, choix), des compteurs, des dates. Jamais un nom de
//! variable, une valeur, une commande, un chemin du serveur ni le nom d'un service : le plan et le journal
//! d'étapes n'en portent pas, et ce module ne lit ni `undo` ni `plan_json` autrement que par les noms
//! d'étapes.

use hearth_proto::api::service_ops::{
    DecisionView, MAX_VIEW_STEPS, OperationView, StepStateView, StepView,
};
use hearth_proto::api::services::{BusyInfo, LastAction};

use super::metrics_wire::date;
use crate::application::service_ops::OpSnapshot;
use crate::domain::ops::{OpStatus, OpSummary, StepState, decision_expiry, latest_state};

fn step_state(state: StepState) -> StepStateView {
    match state {
        StepState::Intent => StepStateView::Intent,
        StepState::Done => StepStateView::Done,
        StepState::Failed => StepStateView::Failed,
        StepState::Undone => StepStateView::Undone,
    }
}

/// Une opération telle que le client la suit. Les étapes sont celles du plan, dans l'ordre, qui ont déjà une
/// ligne au journal avec leur DERNIER état ; une étape pas encore commencée n'y est pas.
pub fn view(snapshot: &OpSnapshot) -> OperationView {
    let record = &snapshot.record;
    let steps = record
        .plan
        .steps()
        .iter()
        .filter_map(|planned| {
            latest_state(&snapshot.steps, &planned.name).map(|state| StepView {
                step: planned.name.as_str().to_owned(),
                state: step_state(state),
            })
        })
        .take(MAX_VIEW_STEPS)
        .collect();
    let decision = record.decision.as_ref().map(|offer| DecisionView {
        choices: offer
            .choices
            .iter()
            .map(|choice| choice.to_wire())
            .collect(),
        // Seule une décision demandée expire (15 minutes) ; une opération à vérifier attend sans limite.
        expires_at: (record.status == OpStatus::AwaitingDecision)
            .then(|| date(offer.since + decision_expiry())),
    });
    OperationView {
        id: record.id.clone(),
        key: record.service.as_str().to_owned(),
        kind: record.kind.to_wire(),
        status: record.status.to_wire(),
        step: record.step.as_ref().map(|step| step.as_str().to_owned()),
        steps,
        percent: record.progress.percent,
        items_done: record.progress.items_done,
        bytes_done: record.progress.bytes_done,
        bytes_total: record.progress.bytes_total,
        eta_s: None,
        cancellable: snapshot.cancellable(),
        decision,
        cause: record.cause.map(|cause| cause.to_wire()),
    }
}

/// `ServiceItem.busy` : l'opération qui tient le service (visible de tous, BR-LIFE-013).
pub fn busy(op: &OpSummary) -> BusyInfo {
    BusyInfo {
        kind: op.kind.as_str().to_owned(),
        by: op.account.clone(),
        since: date(op.started_at),
    }
}

/// `ServiceItem.last_action` : la dernière opération terminée du service.
pub fn last_action(op: &OpSummary) -> LastAction {
    LastAction {
        id: op.id.clone(),
        kind: op.kind.as_str().to_owned(),
        by: op.account.clone(),
        status: op.status.as_str().to_owned(),
        cause: op.cause.map(|cause| cause.as_str().to_owned()),
        forced: op.forced,
        finished_at: op.finished_at.map(date),
    }
}
