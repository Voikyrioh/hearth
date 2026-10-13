//! Plan d'arrêt borné (conception technique 11.3 ; BR-JEU-002, BR-JEU-038 ; ADR-0052).
//!
//! La fiche déclare COMMENT arrêter son jeu ; ce module la transforme en plan, sans rien exécuter. Règles :
//!
//! - un élément `untested` (séquence d'arrêt, séquence d'enregistrement) n'est JAMAIS exécuté ni cru : le plan
//!   dit « décision » (Arrêter quand même, ou Annuler) ou « absent » ;
//! - après un ordre de sortie, l'arrêt du moteur est demandé AUSSITÔT (sinon le service, sorti de lui-même, est
//!   relancé par sa règle de redémarrage) : tout plan se termine par l'arrêt du moteur avec le délai de la fiche ;
//! - six étapes au plus, 20 minutes au plus, délai d'arrêt de 10 à 600 secondes (vérifié au chargement, I9).

use super::loading::model::Fiche;
use super::loading::schema::fiche::{Proof, Step, StepKind, StopSignal, WorldSaved};

/// Une étape déroulée par l'agent. Les textes viennent de la fiche, jamais de l'utilisateur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlannedStep {
    /// Un ordre littéral envoyé par l'entrée standard. `quits` : point de non-retour (plus d'annulation).
    StdinLine { line: String, quits: bool },
    /// Attendre une sous-chaîne littérale dans les journaux postérieurs à l'étape précédente.
    WaitLog {
        contains: String,
        timeout_s: u32,
        required: bool,
    },
    /// Attendre, avec décompte : « Hearth ne peut pas savoir quand le jeu a fini ».
    Wait { seconds: u32 },
}

/// Signal que le moteur envoie à l'arrêt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    ImageDefault,
    Sigterm,
    Sigint,
}

/// Pourquoi une décision est demandée (BR-JEU-038).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionReason {
    /// Le jeu ne sait pas enregistrer son monde à l'arrêt, et la fiche ne déclare aucune façon de le faire.
    NotSaved,
    /// La fiche déclare une séquence, mais elle n'a pas été essayée : elle n'est pas exécutée.
    UntestedSequence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopPlan {
    /// L'image enregistre le monde sur le signal : l'agent demande l'arrêt au moteur et annonce la coupure
    /// forcée au-delà du délai.
    ByImageOnSignal { grace_s: u32, signal: Signal },
    /// Séquence essayée : les étapes dans l'ordre, puis l'arrêt du moteur aussitôt.
    Sequence {
        steps: Vec<PlannedStep>,
        grace_s: u32,
        signal: Signal,
    },
    /// Rien d'exécutable : décision `{ Arrêter quand même | Annuler }` ; « Arrêter quand même » demande
    /// l'arrêt au moteur avec le délai de la fiche.
    NeedsDecision {
        reason: DecisionReason,
        grace_s: u32,
        signal: Signal,
    },
}

impl StopPlan {
    pub fn grace_s(&self) -> u32 {
        match self {
            Self::ByImageOnSignal { grace_s, .. }
            | Self::Sequence { grace_s, .. }
            | Self::NeedsDecision { grace_s, .. } => *grace_s,
        }
    }

    /// Vrai si l'agent enverra un texte au jeu (pour l'écran de confirmation).
    pub fn sends_orders(&self) -> bool {
        matches!(self, Self::Sequence { steps, .. } if steps.iter().any(|s| matches!(s, PlannedStep::StdinLine { .. })))
    }

    /// Durée au pire de la séquence, sans le délai d'arrêt.
    pub fn sequence_seconds(&self) -> u32 {
        match self {
            Self::Sequence { steps, .. } => steps.iter().map(step_seconds).sum(),
            _ => 0,
        }
    }
}

fn step_seconds(step: &PlannedStep) -> u32 {
    match step {
        PlannedStep::StdinLine { .. } => 0,
        PlannedStep::WaitLog { timeout_s, .. } => *timeout_s,
        PlannedStep::Wait { seconds } => *seconds,
    }
}

fn signal_of(signal: StopSignal) -> Signal {
    match signal {
        StopSignal::ImageDefault => Signal::ImageDefault,
        StopSignal::Sigterm => Signal::Sigterm,
        StopSignal::Sigint => Signal::Sigint,
    }
}

fn plan_steps(steps: &[Step]) -> Vec<PlannedStep> {
    steps
        .iter()
        .filter_map(|step| match step.kind {
            StepKind::StdinLine => Some(PlannedStep::StdinLine {
                line: step.line.clone()?,
                quits: step.quits?,
            }),
            StepKind::WaitLog => Some(PlannedStep::WaitLog {
                contains: step.contains.clone()?,
                timeout_s: step.timeout_s?,
                required: step.required?,
            }),
            StepKind::Wait => Some(PlannedStep::Wait {
                seconds: step.seconds?,
            }),
            StepKind::Unknown => None,
        })
        .collect()
}

/// Le plan d'arrêt d'une fiche.
pub fn stop_plan(fiche: &Fiche) -> StopPlan {
    let stop = &fiche.file.stop;
    let signal = signal_of(stop.signal);
    let grace_s = stop.grace_s;
    match stop.world_saved {
        WorldSaved::ByImageOnSignal => StopPlan::ByImageOnSignal { grace_s, signal },
        WorldSaved::NotSaved => StopPlan::NeedsDecision {
            reason: DecisionReason::NotSaved,
            grace_s,
            signal,
        },
        WorldSaved::BySequence => match &stop.sequence {
            Some(sequence) if sequence.status == Proof::Tested && !sequence.step.is_empty() => {
                StopPlan::Sequence {
                    steps: plan_steps(&sequence.step),
                    grace_s,
                    signal,
                }
            }
            _ => StopPlan::NeedsDecision {
                reason: DecisionReason::UntestedSequence,
                grace_s,
                signal,
            },
        },
    }
}

/// Enregistrer sans arrêter, avant une copie à chaud : `None` quand la séquence est absente ou `untested`
/// (restent alors « Arrêter, sauvegarder, relancer » et la copie à chaud avec son avertissement).
/// Une séquence d'enregistrement ne quitte jamais le jeu (vérifié au chargement).
pub fn save_plan(fiche: &Fiche) -> Option<Vec<PlannedStep>> {
    let save = fiche.file.save.as_ref()?;
    if save.status != Proof::Tested || save.step.is_empty() {
        return None;
    }
    let steps = plan_steps(&save.step);
    if steps
        .iter()
        .any(|s| matches!(s, PlannedStep::StdinLine { quits: true, .. }))
    {
        return None;
    }
    Some(steps)
}

/// Le signe « prêt » n'est cru que s'il est `tested`.
pub fn ready_sign(fiche: &Fiche) -> Option<&str> {
    let ready = fiche.file.ready.as_ref()?;
    (ready.status == Proof::Tested).then_some(ready.log_contains.as_str())
}

/// La lecture de la version du jeu n'est crue que si elle est `tested`.
pub fn game_version_reader(fiche: &Fiche) -> Option<(&str, Vec<char>)> {
    let reader = fiche.file.game_version.as_ref()?;
    (reader.status == Proof::Tested).then(|| {
        (
            reader.after.as_str(),
            reader
                .until_any
                .iter()
                .filter_map(|u| u.chars().next())
                .collect(),
        )
    })
}
