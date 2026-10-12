//! L'état du moteur de conteneurs : classement d'une sonde et anti-clignotement (`BR-ENGINE-002`,
//! `BR-ENGINE-005` à `BR-ENGINE-013`).
//!
//! Fonctions pures : la sonde elle-même (socket, `/_ping`, `/version`) est faite par l'adaptateur
//! du port `ContainerEngine` ; ce module ne fait que lire son résultat.

use std::time::Duration;

/// Les états du moteur. `Checking` est l'intitulé de la première détection, pas un des huit états
/// (`BR-ENGINE-019`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EngineState {
    Checking,
    Ready,
    Stopped,
    AccessDenied,
    Unresponsive,
    Absent,
    UnsupportedEngine,
    UnsupportedVersion,
    Disabled,
}

impl EngineState {
    /// Le moteur existe mais Hearth ne peut pas s'en servir (`BR-ENGINE-002`) : la dernière liste
    /// de services reste utile (grisée, datée par le client).
    pub fn is_inaccessible(self) -> bool {
        matches!(
            self,
            EngineState::Stopped | EngineState::AccessDenied | EngineState::Unresponsive
        )
    }

    /// Dans ces états, aucune liste ancienne n'est conservée (`BR-SVC-018`).
    pub fn keeps_no_list(self) -> bool {
        matches!(
            self,
            EngineState::Absent
                | EngineState::UnsupportedEngine
                | EngineState::UnsupportedVersion
                | EngineState::Disabled
        )
    }
}

/// Version de l'API du moteur (`1.41`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ApiVersion {
    pub major: u32,
    pub minor: u32,
}

/// Docker Engine 20.10 parle l'API 1.41 : la version minimale prise en charge (ADR-0036).
pub const MIN_API_VERSION: ApiVersion = ApiVersion {
    major: 1,
    minor: 41,
};

impl ApiVersion {
    pub fn parse(text: &str) -> Option<Self> {
        let (major, minor) = text.trim().split_once('.')?;
        Some(Self {
            major: major.parse().ok()?,
            minor: minor.parse().ok()?,
        })
    }
}

/// Ce que le moteur annonce de lui-même.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Announced {
    pub name: String,
    pub version: String,
    pub api_version: ApiVersion,
}

/// Le résultat brut d'une sonde, avant classement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// Le socket n'existe pas.
    SocketMissing,
    /// Le socket existe mais personne n'écoute.
    Refused,
    /// L'ouverture du socket est refusée à l'agent (`EACCES`), ou le moteur répond 401/403.
    PermissionDenied,
    /// Connecté, mais pas de réponse dans les 5 secondes (ou réponse inexploitable).
    NoAnswer,
    Answered(Announced),
}

/// Une sonde : ce que l'adaptateur a vu, et si un moteur est installé sur ce serveur (binaire
/// `dockerd`, unité `docker.service` ou `docker.socket`), ce qui sépare « arrêté » d'« absent ».
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineProbe {
    pub outcome: ProbeOutcome,
    pub installed: bool,
}

/// Seul Docker est pris en charge : un moteur qui se présente comme compatible mais s'annonce
/// autrement (Podman) ne l'est pas (`BR-ENGINE-010`). Le nom est celui que le moteur annonce.
pub fn is_supported_engine(announced_name: &str) -> bool {
    let name = announced_name.to_lowercase();
    (name.contains("docker") || name.contains("moby"))
        && !name.contains("podman")
        && !name.contains("libpod")
}

/// Classe une sonde en un des états du moteur (`BR-ENGINE-002`).
///
/// `enabled = false` (`engine.enabled` de la configuration) donne `Disabled` sans regarder la sonde :
/// l'agent n'ouvre alors jamais le socket. Sans sonde et le pilotage permis, la première détection
/// n'est pas finie : `Checking`.
pub fn classify_probe(enabled: bool, probe: Option<&EngineProbe>) -> EngineState {
    if !enabled {
        return EngineState::Disabled;
    }
    let Some(probe) = probe else {
        return EngineState::Checking;
    };
    match &probe.outcome {
        // Aucun binaire ni unité : le serveur n'a pas de moteur. Sinon il est installé et éteint.
        ProbeOutcome::SocketMissing if !probe.installed => EngineState::Absent,
        // Un socket qui refuse la connexion est celui d'un moteur éteint (fichier resté).
        ProbeOutcome::SocketMissing | ProbeOutcome::Refused => EngineState::Stopped,
        ProbeOutcome::PermissionDenied => EngineState::AccessDenied,
        ProbeOutcome::NoAnswer => EngineState::Unresponsive,
        ProbeOutcome::Answered(announced) => {
            if !is_supported_engine(&announced.name) {
                EngineState::UnsupportedEngine
            } else if announced.api_version < MIN_API_VERSION {
                EngineState::UnsupportedVersion
            } else {
                EngineState::Ready
            }
        }
    }
}

/// Anti-clignotement de l'état affiché (`BR-ENGINE-012`, `BR-ENGINE-013`).
///
/// Un changement est publié tout de suite s'il n'y en a pas eu depuis `window` ; sinon le dernier
/// état observé est retenu et publié à la fin de la fenêtre, avec l'état réel du moment : un
/// aller-retour plus court que la fenêtre n'est jamais publié. Le temps est un décalage monotone
/// (`Duration` depuis un instant fixe), passé par l'appelant : la fonction est pure.
#[derive(Debug, Clone)]
pub struct Debounce {
    window: Duration,
    published: EngineState,
    last_publish: Option<Duration>,
    pending: Option<EngineState>,
}

impl Debounce {
    /// L'état publié au départ est `initial` (en pratique `Checking`) et n'ouvre aucune fenêtre :
    /// le premier état réel part tout de suite.
    pub fn new(window: Duration, initial: EngineState) -> Self {
        Self {
            window,
            published: initial,
            last_publish: None,
            pending: None,
        }
    }

    pub fn published(&self) -> EngineState {
        self.published
    }

    /// Un nouvel état observé à `now`. Rend l'état à publier tout de suite, s'il y en a un.
    pub fn observe(&mut self, now: Duration, observed: EngineState) -> Option<EngineState> {
        if observed == self.published {
            // Revenu à l'état publié avant la fin de la fenêtre : le changement retenu s'efface.
            self.pending = None;
            return None;
        }
        if self.window_open(now) {
            self.pending = Some(observed);
            return None;
        }
        Some(self.publish(now, observed))
    }

    /// À appeler à l'échéance rendue par [`Debounce::deadline`] : publie l'état retenu si la
    /// fenêtre est finie et qu'il diffère encore de l'état publié.
    pub fn flush(&mut self, now: Duration) -> Option<EngineState> {
        let pending = self.pending?;
        if self.window_open(now) {
            return None;
        }
        self.pending = None;
        if pending == self.published {
            return None;
        }
        Some(self.publish(now, pending))
    }

    /// Quand le changement retenu pourra partir, s'il y en a un.
    pub fn deadline(&self) -> Option<Duration> {
        self.pending?;
        self.last_publish.map(|at| at + self.window)
    }

    /// Publication immédiate, hors fenêtre : la demande explicite de l'utilisateur de vérifier à
    /// nouveau (« Vérifier à nouveau ») reflète le résultat tout de suite.
    pub fn force(&mut self, now: Duration, observed: EngineState) -> Option<EngineState> {
        self.pending = None;
        if observed == self.published {
            return None;
        }
        Some(self.publish(now, observed))
    }

    fn window_open(&self, now: Duration) -> bool {
        self.last_publish
            .is_some_and(|at| now.saturating_sub(at) < self.window)
    }

    fn publish(&mut self, now: Duration, state: EngineState) -> EngineState {
        self.published = state;
        self.last_publish = Some(now);
        self.pending = None;
        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answered(name: &str, api: &str) -> ProbeOutcome {
        ProbeOutcome::Answered(Announced {
            name: name.into(),
            version: "24.0.7".into(),
            api_version: ApiVersion::parse(api).expect("api"),
        })
    }

    fn probe(outcome: ProbeOutcome, installed: bool) -> EngineProbe {
        EngineProbe { outcome, installed }
    }

    #[test]
    fn classify_probe_covers_every_state() {
        let table = [
            // socket absent sans binaire ni unité -> absent
            (
                true,
                Some(probe(ProbeOutcome::SocketMissing, false)),
                EngineState::Absent,
            ),
            // socket absent avec unité docker.service -> arrêté
            (
                true,
                Some(probe(ProbeOutcome::SocketMissing, true)),
                EngineState::Stopped,
            ),
            // connexion refusée -> arrêté
            (
                true,
                Some(probe(ProbeOutcome::Refused, true)),
                EngineState::Stopped,
            ),
            // EACCES -> accès refusé
            (
                true,
                Some(probe(ProbeOutcome::PermissionDenied, true)),
                EngineState::AccessDenied,
            ),
            // connecté sans réponse en 5 s -> ne répond pas
            (
                true,
                Some(probe(ProbeOutcome::NoAnswer, true)),
                EngineState::Unresponsive,
            ),
            // un autre moteur -> non pris en charge
            (
                true,
                Some(probe(answered("Podman Engine", "1.41"), true)),
                EngineState::UnsupportedEngine,
            ),
            // API 1.40 -> version non prise en charge
            (
                true,
                Some(probe(answered("Docker Engine - Community", "1.40"), true)),
                EngineState::UnsupportedVersion,
            ),
            // API 1.41 -> prêt
            (
                true,
                Some(probe(answered("Docker Engine - Community", "1.41"), true)),
                EngineState::Ready,
            ),
            // pilotage désactivé -> désactivé, même sans sonde et même si la sonde dirait prêt
            (false, None, EngineState::Disabled),
            (
                false,
                Some(probe(answered("Docker Engine", "1.45"), true)),
                EngineState::Disabled,
            ),
            // première détection pas finie
            (true, None, EngineState::Checking),
        ];
        for (enabled, probe, expected) in table {
            assert_eq!(
                classify_probe(enabled, probe.as_ref()),
                expected,
                "{probe:?}"
            );
        }
    }

    #[test]
    fn the_eight_states_are_all_reachable_from_a_probe() {
        let reached: std::collections::HashSet<_> = [
            classify_probe(true, Some(&probe(ProbeOutcome::SocketMissing, false))),
            classify_probe(true, Some(&probe(ProbeOutcome::SocketMissing, true))),
            classify_probe(true, Some(&probe(ProbeOutcome::PermissionDenied, true))),
            classify_probe(true, Some(&probe(ProbeOutcome::NoAnswer, true))),
            classify_probe(true, Some(&probe(answered("Podman Engine", "1.41"), true))),
            classify_probe(true, Some(&probe(answered("Docker Engine", "1.30"), true))),
            classify_probe(true, Some(&probe(answered("Docker Engine", "1.43"), true))),
            classify_probe(false, None),
        ]
        .into_iter()
        .collect();
        assert_eq!(reached.len(), 8);
        assert!(!reached.contains(&EngineState::Checking));
    }

    #[test]
    fn an_api_version_compares_numerically() {
        let v = |text: &str| ApiVersion::parse(text).expect("version");
        assert!(v("1.9") < v("1.41"));
        assert!(v("1.41") < v("1.100"));
        assert!(v("2.0") > v("1.99"));
        assert_eq!(ApiVersion::parse("x.y"), None);
        assert_eq!(ApiVersion::parse("141"), None);
    }

    #[test]
    fn only_docker_is_a_supported_engine() {
        for name in [
            "Docker Engine - Community",
            "Docker Engine",
            "Docker Desktop",
        ] {
            assert!(is_supported_engine(name), "{name}");
        }
        for name in ["Podman Engine", "linux/amd64/fedora-40", "", "libpod"] {
            assert!(!is_supported_engine(name), "{name}");
        }
    }

    #[test]
    fn inaccessible_states_keep_the_list_and_missing_ones_drop_it() {
        for state in [
            EngineState::Stopped,
            EngineState::AccessDenied,
            EngineState::Unresponsive,
        ] {
            assert!(state.is_inaccessible() && !state.keeps_no_list());
        }
        for state in [
            EngineState::Absent,
            EngineState::UnsupportedEngine,
            EngineState::UnsupportedVersion,
            EngineState::Disabled,
        ] {
            assert!(state.keeps_no_list() && !state.is_inaccessible());
        }
        assert!(!EngineState::Ready.is_inaccessible() && !EngineState::Ready.keeps_no_list());
    }

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn debounce_publishes_the_first_change_at_once() {
        let mut d = Debounce::new(secs(5), EngineState::Checking);
        assert_eq!(
            d.observe(secs(0), EngineState::Ready),
            Some(EngineState::Ready)
        );
        assert_eq!(d.observe(secs(1), EngineState::Ready), None);
    }

    #[test]
    fn debounce_holds_a_change_made_inside_the_window_and_publishes_it_at_the_end() {
        let mut d = Debounce::new(secs(5), EngineState::Checking);
        d.observe(secs(0), EngineState::Ready);
        // Le moteur tombe à 2 s : retenu jusqu'à 5 s.
        assert_eq!(d.observe(secs(2), EngineState::Stopped), None);
        assert_eq!(d.deadline(), Some(secs(5)));
        assert_eq!(d.flush(secs(4)), None);
        assert_eq!(d.flush(secs(5)), Some(EngineState::Stopped));
        assert_eq!(d.published(), EngineState::Stopped);
        assert_eq!(d.deadline(), None);
    }

    #[test]
    fn debounce_never_shows_a_round_trip_shorter_than_the_window() {
        let mut d = Debounce::new(secs(5), EngineState::Checking);
        d.observe(secs(0), EngineState::Ready);
        assert_eq!(d.observe(secs(1), EngineState::Stopped), None);
        // Revenu à « prêt » avant l'échéance : rien n'est publié du tout.
        assert_eq!(d.observe(secs(2), EngineState::Ready), None);
        assert_eq!(d.deadline(), None);
        assert_eq!(d.flush(secs(10)), None);
        assert_eq!(d.published(), EngineState::Ready);
    }

    #[test]
    fn debounce_changes_at_most_once_per_window_even_when_the_engine_flaps() {
        let mut d = Debounce::new(secs(5), EngineState::Checking);
        let mut published = Vec::new();
        let flapping = [
            EngineState::Ready,
            EngineState::Stopped,
            EngineState::Ready,
            EngineState::Stopped,
            EngineState::Ready,
        ];
        for (second, state) in flapping.into_iter().enumerate() {
            let now = secs(second as u64);
            if let Some(state) = d.observe(now, state) {
                published.push((now, state));
            }
            if let Some(state) = d.flush(now) {
                published.push((now, state));
            }
        }
        // À 0 s « prêt » part ; ensuite chaque aller-retour est retenu puis annulé : rien ne part.
        assert_eq!(published, vec![(secs(0), EngineState::Ready)]);
        // Une fois la fenêtre finie, le changement réel du moment part.
        assert_eq!(
            d.observe(secs(5), EngineState::Stopped),
            Some(EngineState::Stopped)
        );
        // Et le suivant attend de nouveau la fin de sa fenêtre.
        assert_eq!(d.observe(secs(6), EngineState::Ready), None);
        assert_eq!(d.flush(secs(10)), Some(EngineState::Ready));
    }

    #[test]
    fn a_forced_check_reflects_the_result_at_once() {
        let mut d = Debounce::new(secs(5), EngineState::Checking);
        d.observe(secs(0), EngineState::Ready);
        assert_eq!(
            d.force(secs(1), EngineState::Stopped),
            Some(EngineState::Stopped)
        );
        // La fenêtre repart de la publication forcée.
        assert_eq!(d.observe(secs(2), EngineState::Ready), None);
        assert_eq!(d.deadline(), Some(secs(6)));
    }
}
