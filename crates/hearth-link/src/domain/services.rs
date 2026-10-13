//! Les services et le moteur de conteneurs vus par la liaison (HRT-62, ADR-0038) : règles pures.
//!
//! - Quels sujets le client demande à un agent donné : le sujet `services` n'est envoyé qu'à un agent
//!   qui annonce la capacité `services` dans `/hello`. Un sujet inconnu fait refuser TOUT le message
//!   `subscribe`, `metrics` compris : sans cette garde, un client récent couperait les mesures d'un
//!   agent ancien.
//! - Comment un message du sujet (`services_snapshot`, `services_delta`) se fusionne dans l'état tenu
//!   par la liaison. L'interface ne reçoit jamais un lot partiel : elle reçoit l'état complet, déjà
//!   fusionné et trié par nom sans tenir compte de la casse (`BR-SVC-012`).
//!
//! Rien ici ne porte une valeur ou un nom de variable d'environnement ni une commande : les types du
//! protocole n'en ont pas dans cette tranche.

use hearth_proto::api::services::{CAPABILITY_SERVICES, ServiceItem, ServicesState};
use hearth_proto::stream::{ServicesMessage, Topic};

/// L'agent sert-il le sujet `services` ? Un `/hello` sans `capabilities` (agent ancien) est une liste
/// vide : non.
pub fn agent_serves_services(capabilities: &[String]) -> bool {
    capabilities
        .iter()
        .any(|capability| capability == CAPABILITY_SERVICES)
}

/// Les sujets à demander à un agent : les mesures, la fin de session et la mise à jour de l'agent
/// toujours ; le journal d'activité si le client le veut ; les services seulement si l'agent les sert.
pub fn subscription_topics(capabilities: &[String], audit: bool) -> Vec<Topic> {
    let mut topics = vec![Topic::Metrics, Topic::Session, Topic::Update];
    if audit {
        topics.push(Topic::Audit);
    }
    if agent_serves_services(capabilities) {
        topics.push(Topic::Services);
    }
    topics
}

/// Clé de tri d'un service : le nom sans tenir compte de la casse, la clé stable départage.
fn sort_key(item: &ServiceItem) -> (String, String) {
    (item.name.to_lowercase(), item.key.clone())
}

/// Fusionne un message du sujet dans l'état tenu. Rend vrai si l'état a changé (et doit être annoncé).
///
/// - `services_snapshot` remplace tout, quel que soit son `rev` : un `rev` plus petit que le dernier vu
///   est une remise à zéro (l'agent a redémarré), pas un message ancien.
/// - `services_delta` ne s'applique que sur un état déjà connu et un `rev` strictement plus grand : un
///   lot rejoué, périmé ou arrivé avant l'instantané est ignoré (l'agent renvoie un instantané à un
///   abonné qui a perdu des lots, le client n'a aucune logique de trou).
/// - `service_action` n'est pas émis par cette tranche : ignoré.
pub fn apply(current: &mut Option<ServicesState>, message: ServicesMessage) -> bool {
    match message {
        ServicesMessage::ServicesSnapshot(mut state) => {
            state.services.sort_by_cached_key(sort_key);
            *current = Some(state);
            true
        }
        ServicesMessage::ServicesDelta(delta) => {
            let Some(state) = current.as_mut() else {
                return false;
            };
            if delta.rev <= state.rev {
                return false;
            }
            state.rev = delta.rev;
            if let Some(engine) = delta.engine {
                state.engine = engine;
            }
            for key in &delta.removed {
                state.services.retain(|item| &item.key != key);
            }
            for item in delta.upserts {
                match state
                    .services
                    .iter_mut()
                    .find(|existing| existing.key == item.key)
                {
                    Some(existing) => *existing = item,
                    None => state.services.push(item),
                }
            }
            state.services.sort_by_cached_key(sort_key);
            true
        }
        ServicesMessage::ServiceAction(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use hearth_proto::api::services::{
        EngineState, EngineView, HostInfo, ImageInfo, InitSystem, MIN_ENGINE_VERSION, RestartMode,
        ServiceOrigin, ServiceState, ServicesDelta,
    };

    use super::*;

    fn engine(state: EngineState) -> EngineView {
        EngineView {
            state,
            name: Some("Docker Engine".into()),
            version: Some("24.0.7".into()),
            min_version: MIN_ENGINE_VERSION.into(),
            agent_account: "root".into(),
            agent_is_admin: true,
            host: HostInfo {
                os_id: "debian".into(),
                os_like: vec![],
                init: InitSystem::Systemd,
                declarative: false,
            },
            last_ready_at: None,
            checked_at: "2026-10-13T10:00:00Z".into(),
        }
    }

    fn item(key: &str, name: &str, state: ServiceState) -> ServiceItem {
        ServiceItem {
            key: key.into(),
            name: name.into(),
            origin: ServiceOrigin::External,
            driver: None,
            unit: None,
            removed_on_stop: false,
            compose_project: None,
            state,
            since: None,
            created_at: None,
            never_started: false,
            exit_code: None,
            restarts_10m: 0,
            image: ImageInfo {
                reference: Some("nginx".into()),
                tag: Some("1.27".into()),
                short_id: "abcdef012345".into(),
            },
            ports: vec![],
            host_network: false,
            mounts: vec![],
            restart: RestartMode::None,
            busy: None,
            last_action: None,
        }
    }

    fn snapshot(rev: u64, services: Vec<ServiceItem>) -> ServicesMessage {
        ServicesMessage::ServicesSnapshot(ServicesState {
            rev,
            engine: engine(EngineState::Ready),
            services,
        })
    }

    fn delta(rev: u64, upserts: Vec<ServiceItem>, removed: Vec<&str>) -> ServicesMessage {
        ServicesMessage::ServicesDelta(ServicesDelta {
            rev,
            engine: None,
            upserts,
            removed: removed.into_iter().map(str::to_owned).collect(),
        })
    }

    fn names(state: &Option<ServicesState>) -> Vec<String> {
        state
            .as_ref()
            .map(|state| state.services.iter().map(|s| s.name.clone()).collect())
            .unwrap_or_default()
    }

    /// Le premier test de HRT-62 : un agent sans capacité `services` ne reçoit aucun sujet `services`.
    #[test]
    fn an_agent_without_the_capability_is_never_asked_for_the_services_topic() {
        for capabilities in [vec![], vec!["autre".to_owned()]] {
            let topics = subscription_topics(&capabilities, true);
            assert!(!topics.contains(&Topic::Services), "{capabilities:?}");
            // Les mesures continuent : c'est tout l'enjeu de la garde.
            assert!(topics.contains(&Topic::Metrics));
            assert!(topics.contains(&Topic::Session));
            assert!(topics.contains(&Topic::Update));
            assert!(topics.contains(&Topic::Audit));
        }
        let topics = subscription_topics(&["services".to_owned()], false);
        assert_eq!(
            topics,
            vec![
                Topic::Metrics,
                Topic::Session,
                Topic::Update,
                Topic::Services
            ]
        );
    }

    #[test]
    fn a_snapshot_replaces_everything_and_sorts_without_regard_to_case() {
        let mut state = None;
        assert!(apply(
            &mut state,
            snapshot(
                9,
                vec![
                    item("c_2", "zeta", ServiceState::Running),
                    item("c_1", "Alpha", ServiceState::Stopped),
                    item("c_3", "beta", ServiceState::Running),
                ]
            )
        ));
        assert_eq!(names(&state), ["Alpha", "beta", "zeta"]);
        // Un rev plus petit : l'agent a redémarré, c'est une remise à zéro, pas un message ancien.
        assert!(apply(
            &mut state,
            snapshot(1, vec![item("c_9", "neuf", ServiceState::Running)])
        ));
        assert_eq!(names(&state), ["neuf"]);
        assert_eq!(state.as_ref().map(|s| s.rev), Some(1));
    }

    #[test]
    fn a_delta_upserts_removes_and_keeps_the_order() {
        let mut state = None;
        apply(
            &mut state,
            snapshot(
                1,
                vec![
                    item("c_1", "alpha", ServiceState::Running),
                    item("c_2", "beta", ServiceState::Running),
                ],
            ),
        );
        // Un renommage garde la clé : même service.
        let mut changed = item("c_2", "Omega", ServiceState::Error);
        changed.exit_code = Some(137);
        assert!(apply(
            &mut state,
            delta(
                2,
                vec![changed, item("c_3", "gamma", ServiceState::Running)],
                vec!["c_1"]
            )
        ));
        assert_eq!(names(&state), ["gamma", "Omega"]);
        let current = state.as_ref().expect("état");
        assert_eq!(current.rev, 2);
        assert_eq!(current.services[1].state, ServiceState::Error);
    }

    #[test]
    fn a_delta_carries_the_engine_only_when_it_changed() {
        let mut state = None;
        apply(&mut state, snapshot(1, vec![]));
        let mut with_engine = ServicesDelta {
            rev: 2,
            engine: Some(engine(EngineState::Stopped)),
            upserts: vec![],
            removed: vec![],
        };
        assert!(apply(
            &mut state,
            ServicesMessage::ServicesDelta(with_engine.clone())
        ));
        assert_eq!(
            state.as_ref().map(|s| s.engine.state),
            Some(EngineState::Stopped)
        );
        with_engine.rev = 3;
        with_engine.engine = None;
        assert!(apply(
            &mut state,
            ServicesMessage::ServicesDelta(with_engine)
        ));
        assert_eq!(
            state.as_ref().map(|s| s.engine.state),
            Some(EngineState::Stopped),
            "un lot sans moteur garde le moteur"
        );
    }

    #[test]
    fn a_stale_replayed_or_premature_delta_is_ignored() {
        let mut state = None;
        // Avant l'instantané : rien à fusionner.
        assert!(!apply(&mut state, delta(5, vec![], vec![])));
        assert!(state.is_none());
        apply(
            &mut state,
            snapshot(4, vec![item("c_1", "a", ServiceState::Running)]),
        );
        let before = state.clone();
        for rev in [3, 4] {
            assert!(!apply(
                &mut state,
                delta(
                    rev,
                    vec![item("c_2", "b", ServiceState::Running)],
                    vec!["c_1"]
                )
            ));
            assert_eq!(state, before, "rev {rev}");
        }
    }

    #[test]
    fn an_action_notice_changes_nothing() {
        use hearth_proto::stream::ServiceActionNotice;
        let mut state = None;
        apply(&mut state, snapshot(1, vec![]));
        let before = state.clone();
        let notice = ServiceActionNotice {
            action_id: "a".into(),
            key: "c_1".into(),
            kind: "start".into(),
            phase: "running".into(),
            outcome: None,
            cause: None,
            forced: false,
        };
        assert!(!apply(&mut state, ServicesMessage::ServiceAction(notice)));
        assert_eq!(state, before);
    }
}
