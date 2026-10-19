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
/// - en pause, en arrêt, en déploiement, état non reconnu : jamais pilotable (BR-LIFE-003, BR-DEPLOY-024).
pub fn check_state(gesture: Gesture, state: ServiceStateKind) -> Result<(), StateRefusal> {
    use ServiceStateKind::{
        Deploying, Error, Paused, RestartLoop, Running, Starting, Stopped, Stopping, Unknown,
    };
    match (gesture, state) {
        (_, Paused | Stopping | Deploying | Unknown) => Err(StateRefusal::NotDrivable(state)),
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

/// Au bout de combien de temps on cesse d'attendre le pilote : le délai d'arrêt du service plus 30
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

/// Ce que l'agent sait de l'arrêt qui vient d'avoir lieu, pour juger s'il a été forcé (BR-LIFE-009).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopEvidence {
    /// Le moteur rend le code de sortie de l'exécution arrêtée ; l'agent a mesuré son attente de l'ordre.
    ExitCode { code: Option<i64>, waited: Duration },
    /// Le pilote ne rend pas de code : la durée de l'arrêt seule, mesurée au plus près de lui (le système
    /// pour une unité, la fin d'exécution du moteur pour un redémarrage, l'attente de l'agent à défaut).
    Took(Duration),
    /// Rien de fiable : un redémarrage dont l'arrêt n'a pas pu être daté.
    Unknown,
}

/// L'arrêt a-t-il été forcé ?
///
/// - avec le code de sortie : le processus est mort sous `SIGKILL` (137) APRÈS que le délai de grâce s'est
///   écoulé. Un 137 plus tôt est un autre tueur (mémoire épuisée, ordre venu d'ailleurs) : ce n'est pas
///   l'arrêt de Hearth qui l'a forcé ;
/// - avec la seule durée : l'arrêt a pris tout son délai (le pilote envoie `SIGKILL` à l'échéance). Limite
///   assumée, dite dans l'ADR-0037 : un arrêt propre qui finit pile à l'échéance est pris pour forcé ;
/// - sans rien : jamais forcé (on ne l'annonce pas sans le savoir).
///
/// Un délai illimité ou illisible (zéro) ne se dépasse pas.
pub fn stop_was_forced(evidence: StopEvidence, grace_s: u32) -> bool {
    let grace = Duration::from_secs(u64::from(grace_s));
    match evidence {
        StopEvidence::ExitCode { code, waited } => {
            code == Some(KILLED_EXIT_CODE) && waited + FORCED_TOLERANCE >= grace
        }
        StopEvidence::Took(took) => grace_s > 0 && took >= grace,
        StopEvidence::Unknown => false,
    }
}

/// Ce que la surveillance qui suit une action terminée guette (BR-LIFE-005, BR-LIFE-011).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Watch {
    /// Le service vient d'être lancé : s'arrête-t-il aussitôt ?
    Started,
    /// Le service vient d'être arrêté : repart-il tout seul ?
    Stopped,
}

impl Watch {
    /// Ce que le geste laisse à surveiller une fois réussi.
    pub const fn after(gesture: Gesture) -> Self {
        if gesture.leaves_running() {
            Self::Started
        } else {
            Self::Stopped
        }
    }
}

/// Ce que le pilote dit d'un service à un instant, quel qu'il soit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sighting {
    Running,
    /// Le pilote le relance (moteur « en relance », unité « en activation »).
    Restarting,
    Stopped,
    /// Le conteneur ou l'unité n'existe plus.
    Gone,
    /// Le pilote n'a pas répondu : on ne sait rien.
    Unknown,
}

/// La cause que la surveillance pose sur l'action terminée, ou `None` tant que rien n'est à dire : un service
/// lancé qui s'arrête ou que le pilote relance (`stopped_right_after_start`, BR-LIFE-005), un service arrêté
/// qui tourne de nouveau (`restarted_by_host`, BR-LIFE-011). Le code de sortie n'y entre pas : un code zéro
/// est une fin normale mais l'action a quand même échoué à tenir le service en marche.
pub const fn cause_after(watch: Watch, seen: Sighting) -> Option<Cause> {
    match (watch, seen) {
        (Watch::Started, Sighting::Stopped | Sighting::Restarting | Sighting::Gone) => {
            Some(Cause::StoppedRightAfterStart)
        }
        (Watch::Stopped, Sighting::Running | Sighting::Restarting) => Some(Cause::RestartedByHost),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use ServiceStateKind::{
        Deploying, Error, Paused, RestartLoop, Running, Starting, Stopped, Stopping, Unknown,
    };

    use super::*;

    const EVERY_STATE: [ServiceStateKind; 9] = [
        Running,
        Starting,
        Stopped,
        Error,
        RestartLoop,
        Paused,
        Stopping,
        Deploying,
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
                Paused | Stopping | Deploying | Unknown => {
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
        let exit = |code, waited_ms| StopEvidence::ExitCode {
            code,
            waited: Duration::from_millis(waited_ms),
        };
        assert!(stop_was_forced(exit(Some(137), 10_000), grace));
        assert!(stop_was_forced(exit(Some(137), 9_200), grace));
        assert!(!stop_was_forced(exit(Some(137), 2_000), grace));
        assert!(!stop_was_forced(exit(Some(0), 10_000), grace));
        assert!(!stop_was_forced(exit(Some(143), 10_000), grace));
        assert!(!stop_was_forced(exit(None, 10_000), grace));
    }

    #[test]
    fn a_stop_known_by_its_duration_is_forced_only_when_it_used_its_whole_delay() {
        let took = |s| StopEvidence::Took(Duration::from_secs(s));
        assert!(stop_was_forced(took(120), 120));
        assert!(stop_was_forced(took(150), 120));
        assert!(!stop_was_forced(took(3), 120));
        // Un délai illimité ou illisible (zéro) ne se dépasse pas.
        assert!(!stop_was_forced(took(500), 0));
        // Sans rien de fiable, jamais annoncé.
        assert!(!stop_was_forced(StopEvidence::Unknown, 120));
    }

    #[test]
    fn the_watch_after_an_action_says_only_what_it_guards_against() {
        use Sighting as S;
        for seen in [S::Stopped, S::Restarting, S::Gone] {
            assert_eq!(
                cause_after(Watch::Started, seen),
                Some(Cause::StoppedRightAfterStart),
                "{seen:?}"
            );
        }
        for seen in [S::Running, S::Unknown] {
            assert_eq!(cause_after(Watch::Started, seen), None, "{seen:?}");
        }
        for seen in [S::Running, S::Restarting] {
            assert_eq!(
                cause_after(Watch::Stopped, seen),
                Some(Cause::RestartedByHost),
                "{seen:?}"
            );
        }
        for seen in [S::Stopped, S::Gone, S::Unknown] {
            assert_eq!(cause_after(Watch::Stopped, seen), None, "{seen:?}");
        }
        assert_eq!(Watch::after(Gesture::Start), Watch::Started);
        assert_eq!(Watch::after(Gesture::Restart), Watch::Started);
        assert_eq!(Watch::after(Gesture::Stop), Watch::Stopped);
    }

    #[test]
    fn the_gestures_know_what_they_leave_behind() {
        assert!(Gesture::Stop.stops_the_service() && !Gesture::Stop.leaves_running());
        assert!(Gesture::Start.leaves_running() && !Gesture::Start.stops_the_service());
        assert!(Gesture::Restart.leaves_running() && Gesture::Restart.stops_the_service());
    }
}
