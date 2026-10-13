//! Annuler, décider, expirer (HRT-83, ADR-0045, BR-JEU-006).
//!
//! - **Annuler** : chaque genre déclare son point de non-retour dans son plan. Avant lui, l'opération
//!   s'annule ; après, la réponse est `409 TOO_LATE_TO_CANCEL`.
//! - **Décider** : une opération `awaiting_decision` ou `needs_attention` n'avance que sur le choix d'un
//!   compte, parmi ceux qu'elle a offerts.
//! - **Expirer** : une décision sans réponse au bout de 15 minutes passe à « annulé ».

use hearth_proto::api::service_ops::DECISION_EXPIRY_S;
use hearth_proto::error::ErrorCode;
use time::{Duration, OffsetDateTime};

use super::kind::{Cause, Choice, OpStatus};
use super::plan::{OpPlan, StepRecord, StepState, latest_state};

/// Ce qu'il advient d'une demande d'annulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelVerdict {
    /// L'opération s'annule : ce qu'elle a créé est retiré.
    Cancel,
    /// Le point de non-retour est passé.
    TooLate,
    /// L'opération n'est pas dans un statut qui s'annule (déjà finie, déjà en annulation, à vérifier).
    NotCancellable,
}

impl CancelVerdict {
    /// Le code d'erreur de la réponse, pour un refus.
    pub const fn error_code(self) -> Option<ErrorCode> {
        match self {
            Self::Cancel => None,
            Self::TooLate => Some(ErrorCode::TooLateToCancel),
            Self::NotCancellable => Some(ErrorCode::Conflict),
        }
    }
}

/// Peut-on annuler cette opération ?
pub fn cancel_verdict(status: OpStatus, plan: &OpPlan, records: &[StepRecord]) -> CancelVerdict {
    if !matches!(
        status,
        OpStatus::Accepted | OpStatus::Running | OpStatus::AwaitingDecision
    ) {
        return CancelVerdict::NotCancellable;
    }
    let past_the_point = plan.no_return().is_some_and(|point| {
        matches!(
            latest_state(records, point),
            Some(StepState::Intent | StepState::Done)
        )
    });
    if past_the_point {
        CancelVerdict::TooLate
    } else {
        CancelVerdict::Cancel
    }
}

/// Le délai au bout duquel une décision sans réponse expire.
pub fn decision_expiry() -> Duration {
    // `DECISION_EXPIRY_S` vaut 900 : bien en dessous de `i64::MAX`.
    Duration::seconds(i64::try_from(DECISION_EXPIRY_S).unwrap_or(900))
}

/// La décision attend depuis `since` : a-t-elle expiré à `now` ? (Une horloge qui recule n'expire rien.)
pub fn decision_expired(since: OffsetDateTime, now: OffsetDateTime) -> bool {
    now - since >= decision_expiry()
}

/// Pourquoi un choix est refusé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChoiceRefused {
    /// L'opération n'attend rien.
    NotWaiting,
    /// Ce choix n'est pas dans ceux qu'elle a offerts.
    NotOffered,
    /// Ce choix ne s'applique pas à ce statut.
    NotApplicable,
}

/// Le statut où mène un choix, et la cause à retenir. `offered` est la liste que l'opération a offerte.
pub fn apply_choice(
    status: OpStatus,
    offered: &[Choice],
    choice: Choice,
) -> Result<(OpStatus, Cause), ChoiceRefused> {
    if !matches!(
        status,
        OpStatus::AwaitingDecision | OpStatus::NeedsAttention
    ) {
        return Err(ChoiceRefused::NotWaiting);
    }
    if !offered.contains(&choice) {
        return Err(ChoiceRefused::NotOffered);
    }
    let next = match (status, choice) {
        (OpStatus::AwaitingDecision, Choice::Continue) => OpStatus::Running,
        (OpStatus::AwaitingDecision, Choice::Cancel) => OpStatus::Cancelled,
        // « Revenir » démarre le retour arrière ; `rolled_back` n'est dit qu'une fois qu'il a eu lieu.
        (OpStatus::NeedsAttention, Choice::Revert) => OpStatus::RollingBack,
        (OpStatus::NeedsAttention, Choice::Keep) => OpStatus::Succeeded,
        (OpStatus::NeedsAttention, Choice::Verified) => OpStatus::Failed,
        _ => return Err(ChoiceRefused::NotApplicable),
    };
    debug_assert!(status.can_become(next));
    let cause = if choice == Choice::Cancel {
        Cause::CancelledByAccount
    } else {
        Cause::DecidedByAccount
    };
    Ok((next, cause))
}

#[cfg(test)]
mod tests {
    use super::super::plan::{PlannedStep, StepName, UndoInfo};
    use super::*;

    fn name(text: &str) -> StepName {
        StepName::parse(text).unwrap()
    }

    fn plan() -> OpPlan {
        let step = |text: &str, destructive| PlannedStep {
            name: name(text),
            destructive,
        };
        OpPlan::new(
            vec![
                step("pull", false),
                step("stop_old", true),
                step("create_new", true),
            ],
            Some(name("create_new")),
        )
        .unwrap()
    }

    fn row(seq: u32, step: &str, state: StepState) -> StepRecord {
        StepRecord {
            seq,
            step: name(step),
            state,
            undo: UndoInfo::default(),
            at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn an_operation_cancels_until_its_point_of_no_return() {
        let p = plan();
        for status in [
            OpStatus::Accepted,
            OpStatus::Running,
            OpStatus::AwaitingDecision,
        ] {
            assert_eq!(cancel_verdict(status, &p, &[]), CancelVerdict::Cancel);
            // Avant le point de non-retour, même après des gestes destructifs plus tôt dans le plan.
            let before = [row(1, "stop_old", StepState::Done)];
            assert_eq!(cancel_verdict(status, &p, &before), CancelVerdict::Cancel);
        }
    }

    #[test]
    fn after_the_point_of_no_return_the_answer_is_too_late_to_cancel() {
        let p = plan();
        for state in [StepState::Intent, StepState::Done] {
            let rows = [row(1, "create_new", state)];
            let verdict = cancel_verdict(OpStatus::Running, &p, &rows);
            assert_eq!(verdict, CancelVerdict::TooLate);
            assert_eq!(verdict.error_code(), Some(ErrorCode::TooLateToCancel));
        }
        assert_eq!(ErrorCode::TooLateToCancel.http_status(), 409);
        // Une étape défaite rouvre l'annulation.
        let undone = [
            row(1, "create_new", StepState::Intent),
            row(2, "create_new", StepState::Undone),
        ];
        assert_eq!(
            cancel_verdict(OpStatus::Running, &p, &undone),
            CancelVerdict::Cancel
        );
    }

    #[test]
    fn a_plan_without_a_point_of_no_return_always_cancels_while_it_runs() {
        let p = OpPlan::new(
            vec![PlannedStep {
                name: name("copy"),
                destructive: false,
            }],
            None,
        )
        .unwrap();
        assert_eq!(
            cancel_verdict(OpStatus::Running, &p, &[row(1, "copy", StepState::Done)]),
            CancelVerdict::Cancel
        );
    }

    #[test]
    fn a_finished_or_doubtful_operation_does_not_cancel() {
        let p = plan();
        for status in OpStatus::ALL {
            let cancellable = matches!(
                status,
                OpStatus::Accepted | OpStatus::Running | OpStatus::AwaitingDecision
            );
            assert_eq!(
                cancel_verdict(status, &p, &[]) == CancelVerdict::Cancel,
                cancellable,
                "{status:?}"
            );
        }
        assert_eq!(
            CancelVerdict::NotCancellable.error_code(),
            Some(ErrorCode::Conflict)
        );
        assert_eq!(CancelVerdict::Cancel.error_code(), None);
    }

    #[test]
    fn a_decision_expires_after_exactly_fifteen_minutes() {
        let since = OffsetDateTime::UNIX_EPOCH + Duration::hours(10);
        assert!(!decision_expired(since, since));
        assert!(!decision_expired(since, since + Duration::seconds(899)));
        assert!(decision_expired(since, since + Duration::seconds(900)));
        assert!(decision_expired(since, since + Duration::days(2)));
        // Une horloge qui recule n'expire rien.
        assert!(!decision_expired(since, since - Duration::hours(1)));
        assert_eq!(decision_expiry(), Duration::minutes(15));
    }

    #[test]
    fn a_choice_moves_the_operation_only_when_it_was_offered_and_applies() {
        use Choice::*;
        use OpStatus::*;
        let waiting = [Continue, Cancel];
        assert_eq!(
            apply_choice(AwaitingDecision, &waiting, Continue),
            Ok((Running, Cause::DecidedByAccount))
        );
        assert_eq!(
            apply_choice(AwaitingDecision, &waiting, Cancel),
            Ok((Cancelled, Cause::CancelledByAccount))
        );
        let attention = [Revert, Keep, Verified];
        assert_eq!(
            apply_choice(NeedsAttention, &attention, Revert),
            Ok((RollingBack, Cause::DecidedByAccount))
        );
        assert_eq!(
            apply_choice(NeedsAttention, &attention, Keep),
            Ok((Succeeded, Cause::DecidedByAccount))
        );
        assert_eq!(
            apply_choice(NeedsAttention, &attention, Verified),
            Ok((Failed, Cause::DecidedByAccount))
        );
        // Pas offert.
        assert_eq!(
            apply_choice(NeedsAttention, &[Verified], Revert),
            Err(ChoiceRefused::NotOffered)
        );
        // Offert mais d'un autre statut.
        assert_eq!(
            apply_choice(NeedsAttention, &[Continue], Continue),
            Err(ChoiceRefused::NotApplicable)
        );
        assert_eq!(
            apply_choice(AwaitingDecision, &[Revert], Revert),
            Err(ChoiceRefused::NotApplicable)
        );
        // L'opération n'attend rien.
        for status in [
            Accepted,
            Running,
            Cancelling,
            RollingBack,
            Cancelled,
            RolledBack,
            Succeeded,
            Failed,
            Interrupted,
        ] {
            assert_eq!(
                apply_choice(status, &Choice::ALL, Continue),
                Err(ChoiceRefused::NotWaiting),
                "{status:?}"
            );
        }
    }

    #[test]
    fn every_outcome_of_a_choice_is_a_legal_transition() {
        for status in [OpStatus::AwaitingDecision, OpStatus::NeedsAttention] {
            for choice in Choice::ALL {
                if let Ok((next, _)) = apply_choice(status, &Choice::ALL, choice) {
                    assert!(status.can_become(next), "{status:?} -> {next:?}");
                }
            }
        }
    }
}
