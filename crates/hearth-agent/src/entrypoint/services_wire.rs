//! Conversions entre le registre des services (domaine et application) et les types du fil de
//! `hearth-proto`. Partagé par `http/` (`GET /services`, `POST /engine/recheck`) et `ws/` (sujet
//! `services`) : le contrat JSON des services n'est connu qu'ici.
//!
//! Aucune VALEUR de variable d'environnement ne passe par ici : le registre n'en contient pas (elles ne
//! se lisent que par `GET /services/{key}/env`, `env_reveal`). Les noms de variables et la commande
//! (détail d'un service, BR-SVC-007 et BR-SVC-008) passent dans le type `Concealed` du fil.

use super::metrics_wire::date;
use crate::application::ports::EnvEntry;
use crate::application::ports::HostDescription;
use crate::application::services::{EngineStatus, ServiceView, ServicesChange, ServicesSnapshot};
use crate::domain::services::{
    EngineState, MountType, PortProtocol, PortScope, RestartPolicy, ServiceStateKind, stop_grace_s,
};
use hearth_proto::api::services::{
    Concealed, EngineState as WireEngineState, EngineView, EnvReveal, EnvVariable, HostInfo,
    ImageInfo, InitSystem, MIN_ENGINE_VERSION, MountKind, PortProtocol as WireProtocol,
    PortScope as WireScope, RestartMode, ServiceDriver, ServiceItem, ServiceMount, ServiceOrigin,
    ServicePort, ServiceState as WireState, ServicesDelta, ServicesState,
};

pub fn engine_state(state: EngineState) -> WireEngineState {
    match state {
        EngineState::Checking => WireEngineState::Checking,
        EngineState::Ready => WireEngineState::Ready,
        EngineState::Stopped => WireEngineState::Stopped,
        EngineState::AccessDenied => WireEngineState::AccessDenied,
        EngineState::Unresponsive => WireEngineState::Unresponsive,
        EngineState::Absent => WireEngineState::Absent,
        EngineState::UnsupportedEngine => WireEngineState::UnsupportedEngine,
        EngineState::UnsupportedVersion => WireEngineState::UnsupportedVersion,
        EngineState::Disabled => WireEngineState::Disabled,
    }
}

fn service_state(state: ServiceStateKind) -> WireState {
    match state {
        ServiceStateKind::Running => WireState::Running,
        ServiceStateKind::Starting => WireState::Starting,
        ServiceStateKind::Stopped => WireState::Stopped,
        ServiceStateKind::Error => WireState::Error,
        ServiceStateKind::RestartLoop => WireState::RestartLoop,
        ServiceStateKind::Paused => WireState::Paused,
        ServiceStateKind::Stopping => WireState::Stopping,
        ServiceStateKind::Unknown => WireState::Unknown,
    }
}

fn host(host: &HostDescription) -> HostInfo {
    HostInfo {
        os_id: host.os_id.clone(),
        os_like: host.os_like.clone(),
        init: if host.systemd {
            InitSystem::Systemd
        } else {
            InitSystem::Other
        },
        declarative: host.declarative,
    }
}

pub fn engine_view(status: &EngineStatus, description: &HostDescription) -> EngineView {
    EngineView {
        state: engine_state(status.state),
        name: status.name.clone(),
        version: status.version.clone(),
        min_version: MIN_ENGINE_VERSION.to_owned(),
        agent_account: description.agent_account.clone(),
        agent_is_admin: description.agent_is_admin,
        host: host(description),
        last_ready_at: status.last_ready_at.map(date),
        checked_at: date(status.checked_at),
        list_read_at: status.list_read_at.map(date),
        list_degraded: status.list_degraded,
        at: Some(date(status.at)),
        list_current_at: status.list_current_at.map(date),
    }
}

pub fn service_item(view: &ServiceView) -> ServiceItem {
    let observation = &view.observation;
    ServiceItem {
        key: view.key.as_str().to_owned(),
        name: observation.name.clone(),
        // Dans cette tranche aucun service n'est créé par Hearth.
        origin: ServiceOrigin::External,
        // Un service lancé sous une unité est piloté par son superviseur, tout autre par le moteur.
        driver: Some(if view.supervision.is_some() {
            ServiceDriver::Supervisor
        } else {
            ServiceDriver::Engine
        }),
        unit: view
            .supervision
            .as_ref()
            .map(|supervision| supervision.unit.as_str().to_owned()),
        removed_on_stop: observation.auto_remove,
        compose_project: observation.compose_project.clone(),
        state: service_state(view.derived.state),
        since: view.derived.since.map(date),
        created_at: observation.created_at.map(date),
        never_started: view.derived.never_started,
        exit_code: view.derived.exit_code,
        restarts_10m: view.restarts_10m,
        image: ImageInfo {
            reference: observation.image.reference.clone(),
            tag: observation.image.tag.clone(),
            short_id: observation.image.short_id.clone(),
        },
        ports: observation
            .ports
            .iter()
            .map(|port| ServicePort {
                host_port: port.host_port,
                container_port: port.container_port,
                protocol: match port.protocol {
                    PortProtocol::Tcp => WireProtocol::Tcp,
                    PortProtocol::Udp => WireProtocol::Udp,
                    PortProtocol::Sctp => WireProtocol::Sctp,
                },
                scope: match port.scope {
                    PortScope::All => WireScope::All,
                    PortScope::Local => WireScope::Local,
                },
            })
            .collect(),
        ports_omitted: observation.ports_omitted,
        host_network: observation.host_network,
        mounts: observation
            .mounts
            .iter()
            .map(|mount| ServiceMount {
                kind: match mount.kind {
                    MountType::Folder => MountKind::Folder,
                    MountType::Volume => MountKind::Volume,
                },
                source: mount.source.clone(),
                target: mount.target.clone(),
                read_only: mount.read_only,
            })
            .collect(),
        mounts_omitted: observation.mounts_omitted,
        // Le système relance un service supervisé, pas le moteur (BR-SVC-007).
        restart: if view.supervision.is_some() {
            RestartMode::Supervisor
        } else {
            match observation.restart {
                RestartPolicy::None => RestartMode::None,
                RestartPolicy::Always => RestartMode::Always,
                RestartPolicy::UnlessStopped => RestartMode::UnlessStopped,
                RestartPolicy::OnFailure => RestartMode::OnFailure,
            }
        },
        env_names: observation
            .env_names
            .iter()
            .map(|name| Concealed::new(name.expose()))
            .collect(),
        env_names_omitted: observation.env_omitted,
        command: observation.command.as_ref().map(|parts| {
            parts
                .iter()
                .map(|part| Concealed::new(part.expose()))
                .collect()
        }),
        grace_s: Some(stop_grace_s(
            observation,
            view.supervision
                .as_ref()
                .and_then(|supervision| supervision.status.as_ref()),
        )),
        busy: None,
        last_action: None,
    }
}

/// La réponse de `GET /services/{key}/env` : noms et valeurs. Les valeurs quittent ici leur type
/// `Secret` pour le fil ; la réponse ne doit ni se stocker ni se tracer (`Cache-Control: no-store`).
pub fn env_reveal(entries: &[EnvEntry]) -> EnvReveal {
    EnvReveal {
        variables: entries
            .iter()
            .map(|entry| EnvVariable {
                name: Concealed::new(entry.name.expose()),
                value: Concealed::new(entry.value.expose()),
            })
            .collect(),
    }
}

pub fn snapshot(snapshot: &ServicesSnapshot) -> ServicesState {
    ServicesState {
        rev: snapshot.rev,
        engine: engine_view(&snapshot.engine, &snapshot.host),
        services: snapshot.services.iter().map(service_item).collect(),
    }
}

/// Un lot de changements ; `host` sert à l'état du moteur quand il change.
pub fn delta(change: &ServicesChange, host: &HostDescription) -> ServicesDelta {
    ServicesDelta {
        rev: change.rev,
        engine: change
            .engine
            .as_ref()
            .map(|status| engine_view(status, host)),
        upserts: change.upserts.iter().map(service_item).collect(),
        removed: change
            .removed
            .iter()
            .map(|key| key.as_str().to_owned())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::image::image_name;
    use crate::domain::services::{Hidden, Observation, RunState, ServiceKey, derive_state};

    fn view(name: &str, run_state: RunState) -> ServiceView {
        let observation = Observation {
            id: "ab".repeat(32),
            name: name.to_owned(),
            created_at: None,
            run_state,
            exit_code: None,
            started_at: None,
            finished_at: None,
            image: image_name("busybox:1.36", "sha256:0123456789abcdef"),
            ports: vec![],
            ports_omitted: 0,
            host_network: false,
            mounts: vec![],
            mounts_omitted: 0,
            restart: RestartPolicy::None,
            auto_remove: false,
            compose_project: None,
            env_names: vec![Hidden::new("TOKEN_NAME")],
            env_omitted: 0,
            command: Some(vec![Hidden::new("run"), Hidden::new("--flag=SENTINEL")]),
            stop_timeout_s: None,
            tty: false,
            log_driver: None,
        };
        let key = ServiceKey::for_container(&observation.id).expect("key");
        let derived = derive_state(&observation, 0, false);
        ServiceView {
            key,
            observation,
            derived,
            restarts_10m: 0,
            supervision: None,
        }
    }

    #[test]
    fn a_service_item_is_external_engine_driven_and_carries_no_future_field() {
        let item = service_item(&view("web", RunState::Running));
        assert_eq!(item.origin, ServiceOrigin::External);
        assert_eq!(item.state, WireState::Running);
        assert!(item.key.starts_with("c_"));
        assert_eq!(item.driver, Some(ServiceDriver::Engine));
        assert!(item.unit.is_none());
        assert!(item.busy.is_none() && item.last_action.is_none());
        assert_eq!(item.image.reference.as_deref(), Some("busybox"));
        // Le détail : noms et commande (pas de valeur), délai d'arrêt par défaut.
        assert_eq!(item.env_names.len(), 1);
        assert_eq!(item.command.as_ref().map(Vec::len), Some(2));
        assert_eq!(item.grace_s, Some(10));
        assert!(!format!("{item:?}").contains("SENTINEL"));
    }

    #[test]
    fn a_supervised_service_names_its_unit_its_driver_and_its_own_grace() {
        use crate::application::services::Supervision;
        use crate::domain::services::{UnitActivity, UnitName, UnitStatus};
        let mut supervised = view("terraria", RunState::Running);
        let unit = UnitName::parse("docker-terraria.service").expect("unit");
        supervised.key = ServiceKey::for_unit(&unit);
        supervised.supervision = Some(Supervision {
            unit,
            status: Some(UnitStatus {
                activity: UnitActivity::Active,
                auto_restart: false,
                restarts: true,
                timeout_stop_s: Some(150),
            }),
            container_present: true,
        });
        let item = service_item(&supervised);
        assert_eq!(item.key, "u_docker-terraria.service");
        assert_eq!(item.driver, Some(ServiceDriver::Supervisor));
        assert_eq!(item.unit.as_deref(), Some("docker-terraria.service"));
        assert_eq!(item.restart, RestartMode::Supervisor);
        assert_eq!(item.grace_s, Some(150));
    }

    #[test]
    fn every_engine_state_has_its_wire_code() {
        let all = [
            (EngineState::Checking, "checking"),
            (EngineState::Ready, "ready"),
            (EngineState::Stopped, "stopped"),
            (EngineState::AccessDenied, "access_denied"),
            (EngineState::Unresponsive, "unresponsive"),
            (EngineState::Absent, "absent"),
            (EngineState::UnsupportedEngine, "unsupported_engine"),
            (EngineState::UnsupportedVersion, "unsupported_version"),
            (EngineState::Disabled, "disabled"),
        ];
        for (state, code) in all {
            assert_eq!(
                serde_json::to_value(engine_state(state)).expect("json"),
                serde_json::json!(code)
            );
        }
    }
}
