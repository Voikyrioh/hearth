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

use hearth_proto::api::services::{CAPABILITY_SERVICES, EngineView, ServiceItem, ServicesState};
use hearth_proto::stream::{ServicesMessage, Topic};

use super::time::WallTime;

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

/// Quand (heure de CE poste) la vue du moteur actuelle a été reçue. La liaison est l'étage qui reçoit le
/// message : c'est elle qui date, pas l'interface (un état rejoué ou relu du disque garde sa vraie date de
/// réception). Une vue identique à la précédente (un lot qui ne la republie pas) garde son repère ; une vue
/// nouvelle, ou aucun repère connu, prend `now`.
pub fn engine_received_at(
    before: Option<&EngineView>,
    after: &EngineView,
    stamp: Option<WallTime>,
    now: WallTime,
) -> WallTime {
    match (before, stamp) {
        (Some(before), Some(stamp)) if before == after => stamp,
        _ => now,
    }
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
            list_read_at: None,
            list_degraded: false,
            at: None,
            list_current_at: None,
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
            ports_omitted: 0,
            host_network: false,
            mounts: vec![],
            mounts_omitted: 0,
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
    fn a_delta_flips_the_degraded_list_flag_on_then_off() {
        let mut state = None;
        apply(
            &mut state,
            snapshot(1, vec![item("c_1", "a", ServiceState::Running)]),
        );
        let flag = |state: &Option<ServicesState>| {
            let engine = &state.as_ref().expect("état").engine;
            (engine.list_degraded, engine.list_read_at.clone())
        };
        assert_eq!(flag(&state), (false, None));
        let mut degraded = engine(EngineState::Ready);
        degraded.list_degraded = true;
        degraded.list_read_at = Some("2026-10-13T09:58:00Z".into());
        assert!(apply(
            &mut state,
            ServicesMessage::ServicesDelta(ServicesDelta {
                rev: 2,
                engine: Some(degraded.clone()),
                upserts: vec![],
                removed: vec![],
            })
        ));
        assert_eq!(flag(&state), (true, Some("2026-10-13T09:58:00Z".into())));
        // Un lot sans moteur garde le drapeau : la liste reste dite non relisible.
        assert!(apply(&mut state, delta(3, vec![], vec![])));
        assert_eq!(flag(&state), (true, Some("2026-10-13T09:58:00Z".into())));
        degraded.list_degraded = false;
        degraded.list_read_at = Some("2026-10-13T10:01:00Z".into());
        assert!(apply(
            &mut state,
            ServicesMessage::ServicesDelta(ServicesDelta {
                rev: 4,
                engine: Some(degraded),
                upserts: vec![],
                removed: vec![],
            })
        ));
        assert_eq!(flag(&state), (false, Some("2026-10-13T10:01:00Z".into())));
        // Les services n'ont pas bougé pendant ce temps.
        assert_eq!(names(&state), ["a"]);
    }

    #[test]
    fn the_freshness_dates_and_omitted_counts_travel_untouched_and_a_bare_delta_keeps_the_view() {
        let mut state = None;
        let mut view = engine(EngineState::Ready);
        view.at = Some("2026-10-13T10:05:00Z".into());
        view.list_current_at = Some("2026-10-13T10:02:00Z".into());
        view.list_degraded = true;
        let mut cut = item("c_1", "a", ServiceState::Running);
        cut.ports_omitted = 40;
        cut.mounts_omitted = 3;
        apply(
            &mut state,
            ServicesMessage::ServicesSnapshot(ServicesState {
                rev: 1,
                engine: view.clone(),
                services: vec![cut],
            }),
        );
        // Un lot sans vue du moteur : la vue (donc ses deux dates) est celle d'avant, rien n'est recalculé ici.
        assert!(apply(
            &mut state,
            delta(2, vec![item("c_2", "b", ServiceState::Running)], vec![])
        ));
        let kept = state.as_ref().expect("état");
        assert_eq!(kept.engine, view);
        assert_eq!(
            (
                kept.services[0].ports_omitted,
                kept.services[0].mounts_omitted
            ),
            (40, 3)
        );
        // Un agent plus ancien : les dates restent absentes, jamais inventées.
        let mut old = None;
        apply(&mut old, snapshot(1, vec![]));
        let old_engine = &old.as_ref().expect("état").engine;
        assert_eq!(
            (&old_engine.at, &old_engine.list_current_at),
            (&None, &None)
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
    fn the_engine_view_is_dated_by_its_reception_and_a_repeated_view_keeps_its_date() {
        let first = WallTime::from_millis(1_000);
        let later = WallTime::from_millis(9_000);
        let view = engine(EngineState::Ready);
        // Première vue, ou aucun repère connu : l'instant de réception.
        assert_eq!(engine_received_at(None, &view, None, first), first);
        assert_eq!(engine_received_at(Some(&view), &view, None, later), later);
        // Un lot qui republie la même vue ne la rajeunit pas.
        assert_eq!(
            engine_received_at(Some(&view), &view, Some(first), later),
            first
        );
        // Une vue différente est datée de sa réception.
        let mut changed = view.clone();
        changed.state = EngineState::Stopped;
        assert_eq!(
            engine_received_at(Some(&view), &changed, Some(first), later),
            later
        );
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
