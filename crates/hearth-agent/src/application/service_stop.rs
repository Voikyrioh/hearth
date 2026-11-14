//! L'arrêt des conteneurs d'un service par une opération longue (suppression, sauvegarde « arrêter, sauvegarder,
//! relancer »), écrit UNE fois (HRT-85, HRT-97, `BR-LIFE-010`, `BR-DEL-013`, `BR-BACKUP-008`).
//!
//! Ce que fait [`stop_running`], dans cet ordre, parce que chaque ordre a sa raison :
//! 1. les conteneurs qui tournent (en marche, qui redémarrent, en pause) sont relevés au moteur ; une lecture qui
//!    échoue arrête tout, rien n'est arrêté ;
//! 2. l'**arrêt voulu** est écrit AVANT l'ordre (`BR-LIFE-010`) : un arrêt demandé par Hearth n'est pas une « erreur » ;
//! 3. l'ordre d'arrêt est donné à chaque conteneur avec le délai de grâce du service, l'attente est bornée
//!    (`wait_budget_with`) ; une coupure forcée est détectée (`stop_was_forced`) et inscrite à l'opération ;
//! 4. un refus net n'a rien arrêté : l'arrêt voulu est effacé ; un résultat inconnu le garde (l'arrêt a peut-être
//!    eu lieu).
//!
//! L'appelant juge l'issue et, pour la sauvegarde, relance ce que l'opération a arrêté (même après un échec).
//! Aucun texte du moteur ne sort d'ici : la cause est dans la liste fermée de `domain::ops`.

use std::time::Duration;

use tokio::time::Instant;

use super::delete::{StepShown, cause_of};
use super::ports::{Clock, ContainerEngine, EngineError, ServiceIntents, StopIntent};
use super::service_ops::{OpError, ServiceOps};
use super::services::ServicesRegistry;
use crate::domain::ops::{Cause, StopEvidence, stop_was_forced, wait_budget_with};
use crate::domain::services::{RunState, ServiceKey, TransitionKind};

/// Ce dont l'arrêt a besoin de l'agent.
pub(crate) struct StopContext<'a> {
    pub registry: &'a ServicesRegistry,
    pub ops: &'a ServiceOps,
    pub engine: &'a dyn ContainerEngine,
    pub intents: &'a dyn ServiceIntents,
    pub clock: &'a dyn Clock,
}

/// Ce qu'il faut arrêter, et comment.
pub(crate) struct StopRequest<'a> {
    pub key: &'a ServiceKey,
    /// L'opération du journal qui porte l'arrêt.
    pub operation: &'a str,
    /// L'étape du journal qui échoue si l'arrêt échoue.
    pub step: &'static str,
    pub containers: &'a [String],
    pub grace_s: u32,
    /// Le compte qui a demandé l'arrêt (identifiant technique).
    pub account: String,
    pub margin: Duration,
    pub floor: Duration,
}

/// Ce qui a été arrêté.
#[derive(Debug, Default)]
pub(crate) struct Stopped {
    /// Les conteneurs que CETTE opération a arrêtés (à relancer par elle seule).
    pub containers: Vec<String>,
    /// Une coupure forcée a eu lieu (le délai de grâce n'a pas suffi).
    pub forced: bool,
}

/// Pourquoi l'arrêt n'a pas abouti, avec ce qui avait déjà été fait.
#[derive(Debug)]
pub(crate) struct StopFailure {
    pub cause: Cause,
    pub stopped: Vec<String>,
    pub forced: bool,
}

fn journal<T>(result: Result<T, OpError>, what: &'static str) {
    if let Err(error) = result {
        tracing::warn!(%error, write = what, "journal de l'arrêt non écrit");
    }
}

async fn forget_stop(ctx: &StopContext<'_>, key: &ServiceKey) {
    if let Err(error) = ctx.intents.clear(key).await {
        tracing::warn!(%error, "arrêt voulu non effacé");
    }
    ctx.registry.forget_stop_requested(key);
}

/// Arrête les conteneurs du service qui tournent. Rien à arrêter : `Ok` vide.
pub(crate) async fn stop_running(
    ctx: &StopContext<'_>,
    request: &StopRequest<'_>,
) -> Result<Stopped, StopFailure> {
    let fail = |cause, stopped: Vec<String>, forced| StopFailure {
        cause,
        stopped,
        forced,
    };
    let mut running = Vec::new();
    for id in request.containers {
        match ctx.engine.inspect(id).await {
            Ok(Some(observation))
                if matches!(
                    observation.run_state,
                    RunState::Running | RunState::Restarting | RunState::Paused
                ) =>
            {
                running.push(id.clone());
            }
            Ok(_) => {}
            Err(error) => {
                journal(
                    ctx.ops.failed(request.operation, request.step).await,
                    "failed",
                );
                return Err(fail(cause_of(error), Vec::new(), false));
            }
        }
    }
    if running.is_empty() {
        return Ok(Stopped::default());
    }
    let intent = StopIntent {
        key: request.key.clone(),
        container_id: running.first().cloned().unwrap_or_default(),
        account: request.account.clone(),
        at: ctx.clock.now(),
    };
    if let Err(error) = ctx.intents.save(&intent).await {
        tracing::error!(%error, "arrêt voulu non écrit : l'ordre d'arrêt n'est pas donné");
        journal(
            ctx.ops.failed(request.operation, request.step).await,
            "failed",
        );
        return Err(fail(Cause::StepFailed, Vec::new(), false));
    }
    ctx.registry.note_stop_requested(request.key, intent.at);
    ctx.registry
        .begin_transition(request.key, TransitionKind::Stopping);
    let _shown = StepShown {
        registry: ctx.registry,
        key: request.key.clone(),
    };
    let grace = Duration::from_secs(u64::from(request.grace_s));
    let wait = wait_budget_with(request.grace_s, request.margin, request.floor);
    let mut done = Stopped::default();
    for id in &running {
        let started = Instant::now();
        let ordered = match tokio::time::timeout(wait, ctx.engine.stop(id, grace)).await {
            Ok(result) => result,
            Err(_) => Err(EngineError::Timeout),
        };
        match ordered {
            Ok(()) | Err(EngineError::NotFound) => {
                done.containers.push(id.clone());
                let evidence = match ctx.engine.inspect(id).await {
                    Ok(Some(observation)) => StopEvidence::ExitCode {
                        code: observation.exit_code,
                        waited: started.elapsed(),
                    },
                    _ => StopEvidence::Took(started.elapsed()),
                };
                if stop_was_forced(evidence, request.grace_s) {
                    done.forced = true;
                    journal(ctx.ops.mark_forced(request.operation).await, "forced");
                }
            }
            Err(error) => {
                journal(
                    ctx.ops.failed(request.operation, request.step).await,
                    "failed",
                );
                // Un refus net : rien n'a été arrêté, l'arrêt voulu n'a pas lieu d'être. Un résultat inconnu le
                // garde : l'arrêt a peut-être eu lieu.
                if matches!(error, EngineError::Refused | EngineError::Denied) {
                    forget_stop(ctx, request.key).await;
                }
                return Err(fail(cause_of(error), done.containers, done.forced));
            }
        }
    }
    for id in &running {
        ctx.registry.reread(id).await;
    }
    Ok(done)
}
