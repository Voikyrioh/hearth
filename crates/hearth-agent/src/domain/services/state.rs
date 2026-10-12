//! Ce que l'agent observe d'un conteneur et l'état de service qui en découle (`BR-SVC-004` à
//! `BR-SVC-006`, `BR-SVC-012`).
//!
//! `derive_state` est une fonction pure. Dans cette tranche (lecture seule) elle ne connaît ni action
//! de Hearth en cours, ni déploiement, ni superviseur : ces branches arrivent aux tranches 3 et 5
//! (`starting`, `stopping`, `deploying`).
//!
//! Une [`Observation`] ne porte AUCUNE valeur de variable d'environnement, aucun nom de variable et
//! aucune commande : l'adaptateur du moteur les laisse dans la réponse du moteur et ne les copie pas.

use std::cmp::Ordering;
use std::collections::VecDeque;

use time::{Duration, OffsetDateTime};

use super::image::ImageName;

/// Fenêtre de comptage des démarrages non demandés (`BR-SVC-006`).
pub const RESTART_LOOP_WINDOW: Duration = Duration::minutes(10);
/// Nombre de démarrages non demandés dans la fenêtre à partir duquel un service « redémarre en
/// boucle » (`BR-SVC-006`).
pub const RESTART_LOOP_STARTS: u32 = 3;

/// L'état d'exécution que le moteur donne au conteneur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    Created,
    Running,
    Paused,
    Restarting,
    Removing,
    Exited,
    Dead,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortProtocol {
    Tcp,
    Udp,
    Sctp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortScope {
    All,
    Local,
}

/// Un port publié sur le serveur (les ports non publiés ne sont pas des ports du serveur).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortBinding {
    pub host_port: u16,
    pub container_port: u16,
    pub protocol: PortProtocol,
    pub scope: PortScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountType {
    Folder,
    Volume,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountInfo {
    pub kind: MountType,
    pub source: String,
    pub target: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartPolicy {
    None,
    Always,
    UnlessStopped,
    OnFailure,
}

/// Un conteneur tel que le moteur le décrit, réduit à ce que la liste des services montre.
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    /// Identifiant complet du conteneur.
    pub id: String,
    /// Nom, sans le « / » initial du moteur.
    pub name: String,
    pub created_at: Option<OffsetDateTime>,
    pub run_state: RunState,
    pub exit_code: Option<i64>,
    pub started_at: Option<OffsetDateTime>,
    pub finished_at: Option<OffsetDateTime>,
    pub image: ImageName,
    pub ports: Vec<PortBinding>,
    /// Réseau « host » : le service n'a pas de port individuel.
    pub host_network: bool,
    pub mounts: Vec<MountInfo>,
    pub restart: RestartPolicy,
    /// Le moteur supprime le conteneur à son arrêt (`AutoRemove`).
    pub auto_remove: bool,
    /// Projet Compose (information seulement).
    pub compose_project: Option<String>,
}

/// L'état d'un service et ce qui s'y rattache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceStateKind {
    Running,
    Stopped,
    Error,
    RestartLoop,
    Paused,
    Stopping,
    Unknown,
}

/// Résultat de [`derive_state`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Derived {
    pub state: ServiceStateKind,
    /// Depuis quand le service est dans cet état.
    pub since: Option<OffsetDateTime>,
    /// Créé mais jamais démarré (`BR-SVC-002`).
    pub never_started: bool,
    /// Code de sortie du dernier arrêt, s'il y en a eu un.
    pub exit_code: Option<i64>,
}

/// Dérive l'état du service depuis l'observation (`BR-SVC-004` à `BR-SVC-006`).
///
/// - `recent_starts` : démarrages non demandés par Hearth dans les 10 dernières minutes, comptés par
///   l'agent (le compteur de relances du moteur ignore systemd et les redémarrages à la main) ;
/// - `stop_requested` : un arrêt a été demandé depuis Hearth sur ce conteneur. Toujours `false` dans
///   cette tranche (aucune action) ; il sert aux tranches suivantes.
pub fn derive_state(
    observation: &Observation,
    recent_starts: u32,
    stop_requested: bool,
) -> Derived {
    let (state, since) = match observation.run_state {
        RunState::Running => (
            if recent_starts >= RESTART_LOOP_STARTS {
                ServiceStateKind::RestartLoop
            } else {
                ServiceStateKind::Running
            },
            observation.started_at,
        ),
        RunState::Restarting => (
            ServiceStateKind::RestartLoop,
            observation.finished_at.or(observation.started_at),
        ),
        RunState::Paused => (ServiceStateKind::Paused, observation.started_at),
        RunState::Created => (ServiceStateKind::Stopped, observation.created_at),
        RunState::Exited => {
            let code = observation.exit_code.unwrap_or(0);
            let state = if code == 0 || stop_requested {
                ServiceStateKind::Stopped
            } else {
                ServiceStateKind::Error
            };
            (state, observation.finished_at)
        }
        RunState::Dead => (ServiceStateKind::Error, observation.finished_at),
        RunState::Removing => (ServiceStateKind::Stopping, observation.finished_at),
        RunState::Unknown => (ServiceStateKind::Unknown, None),
    };
    let never_started = observation.run_state == RunState::Created;
    // Le code de sortie n'a de sens que pour un conteneur qui a tourné puis s'est arrêté.
    let exit_code = match observation.run_state {
        RunState::Exited | RunState::Dead | RunState::Restarting => observation.exit_code,
        _ => None,
    };
    Derived {
        state,
        since,
        never_started,
        exit_code,
    }
}

/// Les démarrages des 10 dernières minutes d'un service, comptés par l'agent (fenêtre glissante, en
/// mémoire). Un démarrage est un instant de début d'exécution (`StartedAt`) distinct du précédent.
#[derive(Debug, Clone, Default)]
pub struct StartWindow {
    starts: VecDeque<OffsetDateTime>,
}

impl StartWindow {
    pub fn new() -> Self {
        Self::default()
    }

    /// Note un début d'exécution vu dans une observation ; le même instant n'est compté qu'une fois.
    pub fn observe(&mut self, started_at: OffsetDateTime) {
        if self.starts.back() != Some(&started_at) {
            self.starts.push_back(started_at);
        }
    }

    /// Démarrages dans les 10 minutes qui précèdent `now` (les plus anciens sont oubliés).
    pub fn count(&mut self, now: OffsetDateTime) -> u32 {
        while self
            .starts
            .front()
            .is_some_and(|at| now - *at > RESTART_LOOP_WINDOW)
        {
            self.starts.pop_front();
        }
        u32::try_from(self.starts.len()).unwrap_or(u32::MAX)
    }
}

/// L'ordre de la liste : par nom, sans tenir compte de la casse, indépendant de l'état
/// (`BR-SVC-012`). Un nom identique à la casse près se départage par le nom exact, puis la clé.
pub fn compare_names(a: &str, b: &str) -> Ordering {
    a.to_lowercase()
        .cmp(&b.to_lowercase())
        .then_with(|| a.cmp(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::image::image_name;

    /// 2026-10-13 00:00:00 UTC plus `hours` heures.
    fn datetime(hours: i64) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_791_849_600 + hours * 3600).expect("date")
    }

    fn observation(run_state: RunState, exit_code: Option<i64>) -> Observation {
        Observation {
            id: "a".repeat(64),
            name: "web".into(),
            created_at: Some(datetime(9)),
            run_state,
            exit_code,
            started_at: Some(datetime(10)),
            finished_at: Some(datetime(11)),
            image: image_name("nginx:1.27", "sha256:0123456789abcdef"),
            ports: vec![],
            host_network: false,
            mounts: vec![],
            restart: RestartPolicy::None,
            auto_remove: false,
            compose_project: None,
        }
    }

    #[test]
    fn derive_state_exit_codes() {
        // 0 -> stopped
        let derived = derive_state(&observation(RunState::Exited, Some(0)), 1, false);
        assert_eq!(derived.state, ServiceStateKind::Stopped);
        assert_eq!(derived.exit_code, Some(0));
        // 137 sans arrêt voulu -> error
        let derived = derive_state(&observation(RunState::Exited, Some(137)), 1, false);
        assert_eq!(derived.state, ServiceStateKind::Error);
        assert_eq!(derived.exit_code, Some(137));
        // un arrêt demandé depuis Hearth reste « arrêté », même avec 137 (les tranches suivantes)
        let derived = derive_state(&observation(RunState::Exited, Some(137)), 1, true);
        assert_eq!(derived.state, ServiceStateKind::Stopped);
        assert_eq!(derived.exit_code, Some(137));
        // restarting -> restart_loop
        let derived = derive_state(&observation(RunState::Restarting, Some(1)), 1, false);
        assert_eq!(derived.state, ServiceStateKind::RestartLoop);
        // created -> stopped + jamais démarré
        let derived = derive_state(&observation(RunState::Created, Some(0)), 0, false);
        assert_eq!(derived.state, ServiceStateKind::Stopped);
        assert!(derived.never_started);
        assert_eq!(derived.exit_code, None);
        assert_eq!(derived.since, Some(datetime(9)));
    }

    #[test]
    fn a_running_service_loops_from_three_unrequested_starts() {
        let running = observation(RunState::Running, None);
        assert_eq!(
            derive_state(&running, 1, false).state,
            ServiceStateKind::Running
        );
        assert_eq!(
            derive_state(&running, 2, false).state,
            ServiceStateKind::Running
        );
        assert_eq!(
            derive_state(&running, RESTART_LOOP_STARTS, false).state,
            ServiceStateKind::RestartLoop
        );
        // Le moteur dit lui-même qu'il relance : boucle, quel que soit le compte (code 0 compris).
        let restarting = observation(RunState::Restarting, Some(0));
        assert_eq!(
            derive_state(&restarting, 0, false).state,
            ServiceStateKind::RestartLoop
        );
    }

    #[test]
    fn every_other_run_state_has_its_service_state() {
        let state = |run_state| derive_state(&observation(run_state, None), 0, false).state;
        assert_eq!(state(RunState::Running), ServiceStateKind::Running);
        assert_eq!(state(RunState::Paused), ServiceStateKind::Paused);
        assert_eq!(state(RunState::Dead), ServiceStateKind::Error);
        assert_eq!(state(RunState::Removing), ServiceStateKind::Stopping);
        assert_eq!(state(RunState::Unknown), ServiceStateKind::Unknown);
    }

    #[test]
    fn the_start_window_counts_ten_minutes_and_each_start_once() {
        let t0 = datetime(10);
        let mut window = StartWindow::new();
        window.observe(t0);
        window.observe(t0); // la même observation, relue : un seul démarrage
        assert_eq!(window.count(t0), 1);
        window.observe(t0 + Duration::minutes(3));
        window.observe(t0 + Duration::minutes(6));
        assert_eq!(window.count(t0 + Duration::minutes(6)), 3);
        // Le premier sort de la fenêtre après 10 minutes.
        assert_eq!(window.count(t0 + Duration::minutes(11)), 2);
        assert_eq!(window.count(t0 + Duration::minutes(30)), 0);
    }

    #[test]
    fn names_sort_without_regard_to_case_or_state() {
        let mut names = vec!["web", "Api", "db", "API", "Zed", "alpha"];
        names.sort_by(|a, b| compare_names(a, b));
        assert_eq!(names, vec!["alpha", "API", "Api", "db", "web", "Zed"]);
    }
}
