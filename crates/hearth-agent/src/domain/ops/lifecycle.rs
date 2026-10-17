//! Démarrer, arrêter, redémarrer un service : les règles pures (HRT-64, BR-LIFE-002, BR-LIFE-004 à
//! BR-LIFE-006, BR-LIFE-009 à BR-LIFE-012, BR-LIFE-014, BR-LIFE-019, BR-LIFE-021).
//!
//! Rien ici ne parle au moteur, à systemd ni à la base : ce sont les décisions qui précèdent l'ordre (l'état
//! permet-il le geste ? qui pilote ? combien attendre ?) et celles qui le suivent (forcé ? quelle cause ?).
//! L'exécution est `application/service_lifecycle.rs`.
//!
//! **Les trois gestes ne détruisent rien** : ni un arrêt, ni un démarrage ne perd une donnée que l'agent
//! garde, et le compte peut les rejouer. Les plans n'ont donc aucune étape destructive : au redémarrage de
//! l'agent, une action en cours est `interrupted` (« résultat inconnu »), son verrou est rendu, rien n'est
//! rejoué (BR-LIFE-020, `recover`).

use std::time::Duration;

use super::kind::{Cause, OpKind};
use super::plan::{OpPlan, PlanError, PlannedStep, StepName};
use crate::domain::services::{EngineState, ServiceStateKind};

/// L'un des trois gestes de cycle de vie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Gesture {
    Start,
    Stop,
    Restart,
}

impl Gesture {
    pub const ALL: [Gesture; 3] = [Self::Start, Self::Stop, Self::Restart];

    /// Le genre d'opération du journal.
    pub const fn kind(self) -> OpKind {
        match self {
            Self::Start => OpKind::Start,
            Self::Stop => OpKind::Stop,
            Self::Restart => OpKind::Restart,
        }
    }

    /// Le nom de l'unique étape du plan (code stable du journal et du suivi).
    pub const fn step(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
        }
    }

    /// Le plan figé à l'acceptation : une seule étape, NON destructive (voir l'en-tête du module).
    pub fn plan(self) -> Result<OpPlan, PlanError> {
        let name = StepName::parse(self.step()).ok_or(PlanError::StepName)?;
        OpPlan::new(
            vec![PlannedStep {
                name,
                destructive: false,
            }],
            None,
        )
    }

    /// Arrêter et redémarrer donnent au service son délai de grâce ; démarrer n'en a pas.
    pub const fn stops_the_service(self) -> bool {
        matches!(self, Self::Stop | Self::Restart)
    }

    /// Le service doit tourner à la fin du geste.
    pub const fn leaves_running(self) -> bool {
        matches!(self, Self::Start | Self::Restart)
    }
}

/// Ce que le pilotage d'un service refuse d'après son état (BR-LIFE-014).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateRefusal {
    /// Le service est déjà dans l'état visé : l'action n'a aucun effet et le dit. Porte l'état.
    AlreadyInState(ServiceStateKind),
    /// Le service n'est pas dans un état où ce geste a un sens (en pause, en arrêt, état non reconnu, ou, pour
    /// redémarrer, pas en marche) : rien n'est fait, Hearth ne convertit jamais un geste en un autre.
    NotDrivable(ServiceStateKind),
}

/// L'état permet-il le geste ? Correspondance exhaustive : un état de plus oblige à choisir (BR-LIFE-014).
///
/// - démarrer : un service arrêté ou en erreur ; en marche, en démarrage ou « redémarre en boucle » : déjà
///   dans l'état ;
/// - arrêter : un service en marche, en démarrage ou qui redémarre en boucle (arrêter l'emporte sur la
///   boucle) ; arrêté ou en erreur : déjà dans l'état ;
/// - redémarrer : un service en marche ou qui redémarre en boucle seulement. Redémarrer un service arrêté
///   n'est PAS converti en démarrage (BR-LIFE-014).
/// - en pause, en arrêt, état non reconnu : jamais pilotable (BR-LIFE-003).
pub fn check_state(gesture: Gesture, state: ServiceStateKind) -> Result<(), StateRefusal> {
    use ServiceStateKind::{
        Error, Paused, RestartLoop, Running, Starting, Stopped, Stopping, Unknown,
    };
    match (gesture, state) {
        (_, Paused | Stopping | Unknown) => Err(StateRefusal::NotDrivable(state)),
        (Gesture::Start, Stopped | Error) => Ok(()),
        (Gesture::Start, Running | Starting | RestartLoop) => {
            Err(StateRefusal::AlreadyInState(state))
        }
        (Gesture::Stop, Running | Starting | RestartLoop) => Ok(()),
        (Gesture::Stop, Stopped | Error) => Err(StateRefusal::AlreadyInState(state)),
        (Gesture::Restart, Running | RestartLoop) => Ok(()),
        (Gesture::Restart, Starting | Stopped | Error) => Err(StateRefusal::NotDrivable(state)),
    }
}

/// Le moteur est-il prêt à recevoir un ordre ? Tout autre état est refusé avant l'acceptation, le service
/// n'est pas modifié (BR-LIFE-019).
pub fn engine_ready(state: EngineState) -> bool {
    state == EngineState::Ready
}

/// Combien d'ordres de grâce plus tard on cesse d'attendre le moteur : le délai d'arrêt du service plus 30
/// secondes, au moins 60 secondes (BR-LIFE-021).
pub fn wait_budget(grace_s: u32) -> Duration {
    wait_budget_with(grace_s, WAIT_MARGIN, WAIT_FLOOR)
}

/// La marge ajoutée au délai d'arrêt pour l'attente d'une action (`BR-LIFE-021`).
pub const WAIT_MARGIN: Duration = Duration::from_secs(30);

/// L'attente d'une action ne descend jamais sous cette durée (`BR-LIFE-021`).
pub const WAIT_FLOOR: Duration = Duration::from_secs(60);

/// L'attente d'une action pour une marge et un plancher donnés : le délai d'arrêt plus la marge, au moins le
/// plancher. La production passe 30 et 60 secondes ; un test passe des durées courtes pour éprouver le dépassement
/// sans attendre une minute.
pub fn wait_budget_with(grace_s: u32, margin: Duration, floor: Duration) -> Duration {
    Duration::from_secs(u64::from(grace_s))
        .saturating_add(margin)
        .max(floor)
}

/// Pendant combien de temps Hearth surveille un service qu'il vient de lancer : s'il s'arrête dans ce délai,
/// il est annoncé (BR-LIFE-005).
pub const STARTUP_WATCH: Duration = Duration::from_secs(10);

/// Pendant combien de temps Hearth surveille un service qu'il vient d'arrêter : s'il repart tout seul dans ce
/// délai, c'est une règle du serveur qui le relance (BR-LIFE-011).
pub const RESTART_WATCH: Duration = Duration::from_secs(30);

/// Tolérance sur le délai de grâce pour juger qu'un arrêt a été forcé : le moteur mesure son délai à sa
/// façon, une arrivée une seconde avant l'échéance reste une échéance.
const FORCED_TOLERANCE: Duration = Duration::from_secs(1);

/// Le code de sortie d'un processus tué (`SIGKILL`).
pub const KILLED_EXIT_CODE: i64 = 137;

/// L'arrêt d'un service piloté par le moteur a-t-il été forcé ? Il l'est quand le processus est mort sous un
/// `SIGKILL` (code 137) APRÈS que le délai de grâce s'est écoulé (BR-LIFE-009). Un 137 plus tôt est un autre
/// tueur (mémoire épuisée, ordre venu d'ailleurs) : ce n'est pas l'arrêt de Hearth qui l'a forcé.
pub fn engine_stop_was_forced(exit_code: Option<i64>, elapsed: Duration, grace_s: u32) -> bool {
    exit_code == Some(KILLED_EXIT_CODE)
        && elapsed + FORCED_TOLERANCE >= Duration::from_secs(u64::from(grace_s))
}

/// L'arrêt d'un service piloté par le système du serveur a-t-il été forcé ? Le système ne rend pas le code de
/// sortie : un arrêt qui a pris tout son délai d'arrêt est tenu pour forcé (le système envoie `SIGKILL` à
/// l'échéance). Limite assumée, dite dans l'ADR-0037 : un arrêt propre qui finit pile à l'échéance est pris
/// pour un arrêt forcé.
pub fn supervised_stop_was_forced(elapsed: Duration, grace_s: u32) -> bool {
    grace_s > 0 && elapsed >= Duration::from_secs(u64::from(grace_s))
}

/// Ce qui est arrivé à un service surveillé après un arrêt voulu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AfterStop {
    /// Il est resté arrêté pendant toute la surveillance.
    StayedStopped,
    /// Il tourne de nouveau : une règle du serveur l'a relancé.
    CameBack,
}

/// La cause d'une opération terminée par un arrêt que le serveur a défait (BR-LIFE-011).
pub const fn cause_after_stop(after: AfterStop) -> Option<Cause> {
    match after {
        AfterStop::StayedStopped => None,
        AfterStop::CameBack => Some(Cause::RestartedByHost),
    }
}

/// Un démarrage dont le programme s'est arrêté aussitôt (BR-LIFE-005). Un code zéro est une fin normale mais
/// l'action a quand même échoué à tenir le service en marche : la cause est la même, le code de sortie fait la
/// différence dans le message du client.
pub const fn cause_of_early_exit() -> Cause {
    Cause::StoppedRightAfterStart
}

#[cfg(test)]
mod tests {
    use ServiceStateKind::{
        Error, Paused, RestartLoop, Running, Starting, Stopped, Stopping, Unknown,
    };

    use super::*;

    const EVERY_STATE: [ServiceStateKind; 8] = [
        Running,
        Starting,
        Stopped,
        Error,
        RestartLoop,
        Paused,
        Stopping,
        Unknown,
    ];

    #[test]
    fn each_gesture_has_a_valid_plan_without_a_destructive_step() {
        for gesture in Gesture::ALL {
            let plan = gesture.plan().unwrap();
            assert_eq!(plan.steps().len(), 1);
            assert_eq!(plan.steps()[0].name.as_str(), gesture.step());
            assert!(plan.destructive_steps().next().is_none(), "{gesture:?}");
            assert!(plan.no_return().is_none());
            assert!(gesture.kind().is_lifecycle());
        }
    }

    #[test]
    fn the_state_table_of_the_three_gestures() {
        // (geste, état) -> résultat, tel que BR-LIFE-003 et BR-LIFE-014 le disent.
        for state in EVERY_STATE {
            let start = check_state(Gesture::Start, state);
            let stop = check_state(Gesture::Stop, state);
            let restart = check_state(Gesture::Restart, state);
            match state {
                Stopped | Error => {
                    assert_eq!(start, Ok(()), "{state:?}");
                    assert_eq!(stop, Err(StateRefusal::AlreadyInState(state)), "{state:?}");
                    assert_eq!(restart, Err(StateRefusal::NotDrivable(state)), "{state:?}");
                }
                Running | RestartLoop => {
                    assert_eq!(start, Err(StateRefusal::AlreadyInState(state)), "{state:?}");
                    assert_eq!(stop, Ok(()), "{state:?}");
                    assert_eq!(restart, Ok(()), "{state:?}");
                }
                Starting => {
                    assert_eq!(start, Err(StateRefusal::AlreadyInState(state)));
                    assert_eq!(stop, Ok(()));
                    assert_eq!(restart, Err(StateRefusal::NotDrivable(state)));
                }
                Paused | Stopping | Unknown => {
                    for result in [start, stop, restart] {
                        assert_eq!(result, Err(StateRefusal::NotDrivable(state)), "{state:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn restarting_a_stopped_service_is_never_turned_into_a_start() {
        assert_eq!(
            check_state(Gesture::Restart, Stopped),
            Err(StateRefusal::NotDrivable(Stopped))
        );
    }

    #[test]
    fn only_a_ready_engine_receives_an_order() {
        assert!(engine_ready(EngineState::Ready));
        for state in [
            EngineState::Checking,
            EngineState::Stopped,
            EngineState::AccessDenied,
            EngineState::Unresponsive,
            EngineState::Absent,
            EngineState::UnsupportedEngine,
            EngineState::UnsupportedVersion,
            EngineState::Disabled,
        ] {
            assert!(!engine_ready(state), "{state:?}");
        }
    }

    #[test]
    fn the_wait_is_the_grace_plus_thirty_seconds_and_at_least_sixty() {
        assert_eq!(wait_budget(10), Duration::from_secs(60));
        assert_eq!(wait_budget(0), Duration::from_secs(60));
        assert_eq!(wait_budget(30), Duration::from_secs(60));
        assert_eq!(wait_budget(31), Duration::from_secs(61));
        assert_eq!(wait_budget(120), Duration::from_secs(150));
        assert_eq!(
            wait_budget(u32::MAX),
            Duration::from_secs(u64::from(u32::MAX) + 30)
        );
    }

    #[test]
    fn the_margin_and_the_floor_can_be_shortened_for_a_test_and_the_formula_stays_the_same() {
        let margin = Duration::from_millis(100);
        let floor = Duration::from_millis(500);
        assert_eq!(wait_budget_with(0, margin, floor), floor);
        assert_eq!(
            wait_budget_with(2, margin, floor),
            Duration::from_millis(2_100)
        );
        assert_eq!(
            wait_budget_with(10, WAIT_MARGIN, WAIT_FLOOR),
            wait_budget(10)
        );
    }

    #[test]
    fn a_kill_after_the_grace_is_a_forced_stop_and_an_earlier_one_is_not() {
        let grace = 10;
        assert!(engine_stop_was_forced(
            Some(137),
            Duration::from_secs(10),
            grace
        ));
        assert!(engine_stop_was_forced(
            Some(137),
            Duration::from_millis(9_200),
            grace
        ));
        assert!(!engine_stop_was_forced(
            Some(137),
            Duration::from_secs(2),
            grace
        ));
        assert!(!engine_stop_was_forced(
            Some(0),
            Duration::from_secs(10),
            grace
        ));
        assert!(!engine_stop_was_forced(
            Some(143),
            Duration::from_secs(10),
            grace
        ));
        assert!(!engine_stop_was_forced(
            None,
            Duration::from_secs(10),
            grace
        ));
    }

    #[test]
    fn a_supervised_stop_is_forced_only_when_it_used_its_whole_delay() {
        assert!(supervised_stop_was_forced(Duration::from_secs(120), 120));
        assert!(supervised_stop_was_forced(Duration::from_secs(150), 120));
        assert!(!supervised_stop_was_forced(Duration::from_secs(3), 120));
        // Un délai illimité ou illisible (zéro) ne se dépasse pas.
        assert!(!supervised_stop_was_forced(Duration::from_secs(500), 0));
    }

    #[test]
    fn a_service_that_comes_back_is_told_and_one_that_stays_stopped_says_nothing() {
        assert_eq!(cause_after_stop(AfterStop::StayedStopped), None);
        assert_eq!(
            cause_after_stop(AfterStop::CameBack),
            Some(Cause::RestartedByHost)
        );
        assert_eq!(cause_of_early_exit(), Cause::StoppedRightAfterStart);
    }

    #[test]
    fn the_gestures_know_what_they_leave_behind() {
        assert!(Gesture::Stop.stops_the_service() && !Gesture::Stop.leaves_running());
        assert!(Gesture::Start.leaves_running() && !Gesture::Start.stops_the_service());
        assert!(Gesture::Restart.leaves_running() && Gesture::Restart.stops_the_service());
    }
}
