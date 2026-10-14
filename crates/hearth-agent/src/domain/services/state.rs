//! Ce que l'agent observe d'un conteneur et l'état de service qui en découle (`BR-SVC-004` à
//! `BR-SVC-006`, `BR-SVC-012`).
//!
//! `derive_state` est une fonction pure. Elle ne connaît ni action de Hearth en cours, ni déploiement
//! (tranches 3 et 5 : `deploying`) ; `derive_unit_state` donne l'état d'un service supervisé dont le
//! conteneur a disparu, d'après son unité (HRT-63).
//!
//! Une [`Observation`] ne porte AUCUNE valeur de variable d'environnement. Les NOMS des variables et la
//! commande (détail d'un service, BR-SVC-008) y sont dans [`Hidden`], dont `Debug` ne montre rien ;
//! l'adaptateur du moteur les copie bornés et laisse les valeurs dans la réponse du moteur.

use std::cmp::Ordering;
use std::collections::VecDeque;

use time::{Duration, OffsetDateTime};

use super::hidden::Hidden;
use super::image::ImageName;
use super::key::ServiceKey;
use super::unit::{UnitActivity, UnitStatus};

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
    /// Ports publiés au-delà de `MAX_PORTS`, non gardés (`BR-SVC-021`).
    pub ports_omitted: u32,
    /// Réseau « host » : le service n'a pas de port individuel.
    pub host_network: bool,
    pub mounts: Vec<MountInfo>,
    /// Montages au-delà de `MAX_MOUNTS`, non gardés (`BR-SVC-021`).
    pub mounts_omitted: u32,
    pub restart: RestartPolicy,
    /// Le moteur supprime le conteneur à son arrêt (`AutoRemove`).
    pub auto_remove: bool,
    /// Projet Compose (information seulement).
    pub compose_project: Option<String>,
    /// Noms des variables d'environnement (jamais une valeur), bornés (`MAX_ENV_NAMES`, `bounded`).
    pub env_names: Vec<Hidden>,
    /// Variables au-delà de `MAX_ENV_NAMES`, non gardées.
    pub env_omitted: u32,
    /// La commande lancée (entrée puis arguments, bornés) ; `None` si le moteur n'en annonce aucune.
    pub command: Option<Vec<Hidden>>,
    /// `Config.StopTimeout` du conteneur, en secondes, s'il en déclare un.
    pub stop_timeout_s: Option<u32>,
    /// Lancé avec un terminal (`Config.Tty`) : une seule sortie, flux brut (journaux, BR-SLOG-012).
    pub tty: bool,
    /// Le pilote de journaux (`HostConfig.LogConfig.Type`) : `none` ne se relit pas (BR-SLOG-017).
    pub log_driver: Option<String>,
    /// La clé `h_<ULID>` que l'étiquette `io.hearth.service-id` AFFIRME (ADR-0037). **Une affirmation, jamais
    /// une identité** : `Config.Labels` contient aussi les étiquettes de l'IMAGE (`LABEL`), qu'un auteur
    /// d'image publique peut poser sans aucun accès au serveur ; l'ULID d'un service, lui, est affiché à tous
    /// les comptes. Ne JAMAIS s'en servir pour authentifier, publier, rendre visible ni compter un conteneur
    /// comme « service de Hearth » sans deux preuves de plus : la clé est celle d'un service connu de la
    /// base de l'agent ET l'identifiant du conteneur est un de ceux que l'agent a consignés
    /// (`RecordedContainers`). Son seul usage : la reprise des opérations (`infrastructure::ops_observer`),
    /// avec cette double vérification. Une étiquette qui n'a pas la forme d'une clé est ignorée.
    pub hearth_key: Option<ServiceKey>,
}

impl Observation {
    /// Un conteneur que la liste du moteur montre mais dont l'inspection a échoué : sa ligne reste
    /// dans la liste dans l'état « non reconnu » (`BR-SVC-004`) plutôt que de disparaître ou de
    /// geler toute la liste (`BR-SVC-001`). Le nom vient de la liste ; à défaut, l'identifiant court.
    pub fn unreadable(id: &str, name: &str) -> Self {
        let name = if name.is_empty() {
            id.chars().take(12).collect()
        } else {
            name.to_owned()
        };
        Self {
            id: id.to_owned(),
            name,
            created_at: None,
            run_state: RunState::Unknown,
            exit_code: None,
            started_at: None,
            finished_at: None,
            image: ImageName {
                reference: None,
                tag: None,
                short_id: String::new(),
            },
            ports: Vec::new(),
            ports_omitted: 0,
            host_network: false,
            mounts: Vec::new(),
            mounts_omitted: 0,
            restart: RestartPolicy::None,
            auto_remove: false,
            compose_project: None,
            env_names: Vec::new(),
            env_omitted: 0,
            command: None,
            stop_timeout_s: None,
            tty: false,
            log_driver: None,
            hearth_key: None,
        }
    }
}

/// L'état d'un service et ce qui s'y rattache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceStateKind {
    Running,
    /// Démarrage en cours : seulement pour un service supervisé dont l'unité démarre (HRT-63).
    Starting,
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

/// Combien de temps un service supervisé reste « Démarrage » sans que son conteneur apparaisse (ou
/// reparaisse) : au-delà, l'unité se dit active mais le moteur n'a rien, l'état n'est plus « Démarrage »
/// mais « non reconnu » (HRT-63). Sans cette borne, un conteneur qui finit sous une unité restée active
/// laisserait un service « Démarrage » à vie.
pub const STARTING_MAX: Duration = Duration::minutes(10);

/// L'état d'un service supervisé dont le conteneur n'existe pas (HRT-63, `BR-SVC-001`, `BR-SVC-005`,
/// `BR-SVC-016`) : le système recrée le conteneur à chaque démarrage, l'état vient donc de l'unité.
///
/// - unité qui démarre, ou qui attend de se relancer après un arrêt (`auto-restart`) : « démarrage ».
///   UNE relance n'est pas une boucle (`BR-SVC-006` : trois démarrages en dix minutes) : « redémarre en
///   boucle » vient du compteur `recent_starts`, pas de l'attente de relance ;
/// - « démarrage » dure au plus `STARTING_MAX` depuis `since` ; ensuite « non reconnu » ;
/// - unité en échec : « en erreur » (le système a constaté l'échec) ; arrêtée : « arrêté » ;
/// - `last_exit` : code du dernier arrêt vu par cet agent (jamais relu du système : il n'est pas
///   affiché après un redémarrage de l'agent) ; `since` : le dernier instant connu du service.
pub fn derive_unit_state(
    status: &UnitStatus,
    last_exit: Option<i64>,
    since: Option<OffsetDateTime>,
    recent_starts: u32,
    now: OffsetDateTime,
) -> Derived {
    let mut state = match status.activity {
        UnitActivity::Activating if status.auto_restart && recent_starts >= RESTART_LOOP_STARTS => {
            ServiceStateKind::RestartLoop
        }
        UnitActivity::Activating | UnitActivity::Active => ServiceStateKind::Starting,
        UnitActivity::Deactivating => ServiceStateKind::Stopping,
        UnitActivity::Inactive => ServiceStateKind::Stopped,
        UnitActivity::Failed => ServiceStateKind::Error,
        UnitActivity::Other => ServiceStateKind::Unknown,
    };
    if state == ServiceStateKind::Starting && since.is_some_and(|at| now - at > STARTING_MAX) {
        state = ServiceStateKind::Unknown;
    }
    let exit_code = match state {
        ServiceStateKind::Stopped | ServiceStateKind::Error | ServiceStateKind::RestartLoop => {
            last_exit
        }
        _ => None,
    };
    Derived {
        state,
        since,
        never_started: false,
        exit_code,
    }
}

/// Délai d'arrêt d'un service qui n'en déclare aucun (celui du moteur : 10 secondes).
pub const DEFAULT_STOP_TIMEOUT_S: u32 = 10;

/// Le délai d'arrêt du service, en secondes (`BR-SVC-007`) : celui de son unité pour un service
/// supervisé (c'est le système qui l'arrête), sinon celui que le conteneur déclare, sinon 10.
pub fn stop_grace_s(observation: &Observation, unit: Option<&UnitStatus>) -> u32 {
    unit.and_then(|status| status.timeout_stop_s)
        .or(observation.stop_timeout_s)
        .unwrap_or(DEFAULT_STOP_TIMEOUT_S)
}

/// L'état d'un service supervisé (HRT-63), conteneur présent ou non :
///
/// - conteneur présent et qui n'est pas arrêté (en marche, en pause, en relance, en suppression) :
///   l'état du conteneur, comme tout autre service (`derive_state`) ;
/// - conteneur arrêté, jamais démarré ou absent, et unité connue : l'état de l'UNITÉ
///   (`derive_unit_state`). C'est le système du serveur qui relance ou non, donc lui qui sait si
///   l'arrêt est voulu ou si l'unité est en échec (`BR-SVC-005`) ;
/// - unité inconnue (superviseur muet) : l'état du conteneur s'il existe, sinon « non reconnu ».
///
/// `last_exit` : dernier code de sortie vu par CET agent pour l'unité (conteneur absent) ;
/// `last_seen` : dernier instant où le service a été vu (conteneur absent).
pub fn derive_supervised(
    observation: &Observation,
    container_present: bool,
    status: Option<&UnitStatus>,
    recent_starts: u32,
    last_exit: Option<i64>,
    last_seen: Option<OffsetDateTime>,
    now: OffsetDateTime,
) -> Derived {
    let stopped_shape = matches!(
        observation.run_state,
        RunState::Exited | RunState::Dead | RunState::Created
    );
    if container_present && !stopped_shape {
        return derive_state(observation, recent_starts, false);
    }
    match (status, container_present) {
        (Some(status), true) => {
            let mut derived = derive_unit_state(
                status,
                observation.exit_code,
                observation.finished_at.or(observation.created_at),
                recent_starts,
                now,
            );
            derived.never_started = observation.run_state == RunState::Created;
            derived
        }
        (Some(status), false) => {
            derive_unit_state(status, last_exit, last_seen, recent_starts, now)
        }
        (None, true) => derive_state(observation, recent_starts, false),
        (None, false) => Derived {
            state: ServiceStateKind::Unknown,
            since: last_seen,
            never_started: false,
            exit_code: None,
        },
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
            ports_omitted: 0,
            host_network: false,
            mounts: vec![],
            mounts_omitted: 0,
            restart: RestartPolicy::None,
            auto_remove: false,
            compose_project: None,
            env_names: vec![],
            env_omitted: 0,
            command: None,
            stop_timeout_s: None,
            tty: false,
            log_driver: None,
            hearth_key: None,
        }
    }

    fn unit(activity: UnitActivity, auto_restart: bool) -> UnitStatus {
        UnitStatus {
            activity,
            auto_restart,
            restarts: true,
            timeout_stop_s: Some(120),
        }
    }

    #[test]
    fn a_supervised_service_without_container_takes_its_state_from_the_unit() {
        let state = |activity, auto_restart| {
            derive_unit_state(
                &unit(activity, auto_restart),
                Some(1),
                Some(datetime(12)),
                0,
                datetime(12),
            )
        };
        // Arrêtée : « arrêté », avec le code du dernier arrêt vu.
        let stopped = state(UnitActivity::Inactive, false);
        assert_eq!(stopped.state, ServiceStateKind::Stopped);
        assert_eq!(
            (stopped.exit_code, stopped.since),
            (Some(1), Some(datetime(12)))
        );
        // En échec constaté par le système : « en erreur ».
        assert_eq!(
            state(UnitActivity::Failed, false).state,
            ServiceStateKind::Error
        );
        // Qui attend de se relancer UNE fois, ou qui démarre : « démarrage » (la boucle est comptée).
        assert_eq!(
            state(UnitActivity::Activating, true).state,
            ServiceStateKind::Starting
        );
        let starting = state(UnitActivity::Activating, false);
        assert_eq!(starting.state, ServiceStateKind::Starting);
        assert_eq!(starting.exit_code, None, "pas de code pour un démarrage");
        assert_eq!(
            state(UnitActivity::Deactivating, false).state,
            ServiceStateKind::Stopping
        );
        assert_eq!(
            state(UnitActivity::Other, false).state,
            ServiceStateKind::Unknown
        );
        // Un agent qui n'a rien vu s'arrêter n'affiche aucun code.
        let unseen = derive_unit_state(
            &unit(UnitActivity::Failed, false),
            None,
            None,
            0,
            datetime(12),
        );
        assert_eq!((unseen.exit_code, unseen.since), (None, None));
        assert!(!unseen.never_started);
    }

    #[test]
    fn one_wait_for_a_relaunch_is_a_start_and_only_the_count_makes_a_loop() {
        let waiting = unit(UnitActivity::Activating, true);
        let at = |starts| {
            derive_unit_state(
                &waiting,
                Some(137),
                Some(datetime(12)),
                starts,
                datetime(12),
            )
        };
        // Un arrêt, une relance en attente : « démarrage », pas une boucle.
        assert_eq!(at(0).state, ServiceStateKind::Starting);
        assert_eq!(at(2).state, ServiceStateKind::Starting);
        // Trois démarrages non demandés en dix minutes : la boucle.
        assert_eq!(at(3).state, ServiceStateKind::RestartLoop);
        assert_eq!(at(3).exit_code, Some(137));
    }

    #[test]
    fn a_starting_service_without_container_is_bounded_in_time() {
        let active = unit(UnitActivity::Active, false);
        let seen = datetime(12);
        let at = |later: Duration| derive_unit_state(&active, None, Some(seen), 0, seen + later);
        assert_eq!(at(Duration::ZERO).state, ServiceStateKind::Starting);
        assert_eq!(at(STARTING_MAX).state, ServiceStateKind::Starting);
        let late = at(STARTING_MAX + Duration::seconds(1));
        assert_eq!(late.state, ServiceStateKind::Unknown);
        assert_eq!(late.since, Some(seen));
        // Sans date connue, rien à borner.
        assert_eq!(
            derive_unit_state(&active, None, None, 0, seen).state,
            ServiceStateKind::Starting
        );
        // Une unité arrêtée ou en échec ne se périme pas.
        let failed = unit(UnitActivity::Failed, false);
        assert_eq!(
            derive_unit_state(&failed, None, Some(seen), 0, seen + Duration::days(3)).state,
            ServiceStateKind::Error
        );
    }

    #[test]
    fn the_stop_grace_is_the_unit_s_then_the_container_s_then_ten_seconds() {
        let mut web = observation(RunState::Running, None);
        assert_eq!(stop_grace_s(&web, None), 10);
        web.stop_timeout_s = Some(30);
        assert_eq!(stop_grace_s(&web, None), 30);
        assert_eq!(
            stop_grace_s(&web, Some(&unit(UnitActivity::Active, false))),
            120,
            "le délai de l'unité l'emporte pour un service supervisé"
        );
        let unlimited = UnitStatus {
            timeout_stop_s: None,
            ..unit(UnitActivity::Active, false)
        };
        assert_eq!(stop_grace_s(&web, Some(&unlimited)), 30);
    }

    #[test]
    fn a_supervised_service_follows_its_container_when_it_runs_and_its_unit_when_it_does_not() {
        let failed = unit(UnitActivity::Failed, false);
        let inactive = unit(UnitActivity::Inactive, false);
        // Un conteneur qui tourne : son état, quoi que dise l'unité.
        let running = observation(RunState::Running, None);
        let derived = derive_supervised(&running, true, Some(&failed), 0, None, None, datetime(12));
        assert_eq!(derived.state, ServiceStateKind::Running);
        let derived = derive_supervised(&running, true, Some(&failed), 3, None, None, datetime(12));
        assert_eq!(derived.state, ServiceStateKind::RestartLoop);
        // Un conteneur arrêté avec un code non nul : « arrêté » si le système l'a arrêté, « en
        // erreur » s'il constate l'échec.
        let exited = observation(RunState::Exited, Some(143));
        let derived =
            derive_supervised(&exited, true, Some(&inactive), 0, None, None, datetime(12));
        assert_eq!(derived.state, ServiceStateKind::Stopped);
        assert_eq!(derived.exit_code, Some(143));
        let derived = derive_supervised(&exited, true, Some(&failed), 0, None, None, datetime(12));
        assert_eq!(derived.state, ServiceStateKind::Error);
        // Un conteneur jamais démarré garde sa marque.
        let created = observation(RunState::Created, Some(0));
        let derived =
            derive_supervised(&created, true, Some(&inactive), 0, None, None, datetime(12));
        assert!(derived.never_started);
        // Conteneur absent : l'unité décide, avec le dernier code vu et le dernier instant vu.
        let derived = derive_supervised(
            &exited,
            false,
            Some(&failed),
            0,
            Some(137),
            Some(datetime(12)),
            datetime(12),
        );
        assert_eq!(derived.state, ServiceStateKind::Error);
        assert_eq!(
            (derived.exit_code, derived.since),
            (Some(137), Some(datetime(12)))
        );
        // Superviseur muet : le conteneur s'il existe, sinon « non reconnu » (jamais « arrêté » à tort).
        let derived = derive_supervised(&exited, true, None, 0, None, None, datetime(12));
        assert_eq!(derived.state, ServiceStateKind::Error);
        let derived = derive_supervised(
            &exited,
            false,
            None,
            0,
            Some(1),
            Some(datetime(12)),
            datetime(12),
        );
        assert_eq!(derived.state, ServiceStateKind::Unknown);
        assert_eq!(derived.exit_code, None);
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
