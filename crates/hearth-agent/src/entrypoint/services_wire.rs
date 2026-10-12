//! Conversions entre le registre des services (domaine et application) et les types du fil de
//! `hearth-proto`. Partagé par `http/` (`GET /services`, `POST /engine/recheck`) et `ws/` (sujet
//! `services`) : le contrat JSON des services n'est connu qu'ici.
//!
//! Aucune valeur de variable d'environnement, aucun nom de variable et aucune commande ne passe par
//! ici : le registre n'en contient pas.

use super::metrics_wire::date;
use crate::application::ports::HostDescription;
use crate::application::services::{EngineStatus, ServiceView, ServicesChange, ServicesSnapshot};
use crate::domain::services::{
    EngineState, MountType, PortProtocol, PortScope, RestartPolicy, ServiceStateKind,
};
use hearth_proto::api::services::{
    EngineState as WireEngineState, EngineView, HostInfo, ImageInfo, InitSystem,
    MIN_ENGINE_VERSION, MountKind, PortProtocol as WireProtocol, PortScope as WireScope,
    RestartMode, ServiceItem, ServiceMount, ServiceOrigin, ServicePort, ServiceState as WireState,
    ServicesDelta, ServicesState,
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
    }
}

pub fn service_item(view: &ServiceView) -> ServiceItem {
    let observation = &view.observation;
    ServiceItem {
        key: view.key.as_str().to_owned(),
        name: observation.name.clone(),
        // Dans cette tranche aucun service n'est créé par Hearth.
        origin: ServiceOrigin::External,
        driver: None,
        unit: None,
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
        restart: match observation.restart {
            RestartPolicy::None => RestartMode::None,
            RestartPolicy::Always => RestartMode::Always,
            RestartPolicy::UnlessStopped => RestartMode::UnlessStopped,
            RestartPolicy::OnFailure => RestartMode::OnFailure,
        },
        busy: None,
        last_action: None,
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
    use crate::domain::services::{Observation, RunState, ServiceKey, derive_state};

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
            host_network: false,
            mounts: vec![],
            restart: RestartPolicy::None,
            auto_remove: false,
            compose_project: None,
        };
        let key = ServiceKey::for_container(&observation.id).expect("key");
        let derived = derive_state(&observation, 0, false);
        ServiceView {
            key,
            observation,
            derived,
            restarts_10m: 0,
        }
    }

    #[test]
    fn a_service_item_is_external_and_carries_no_future_field() {
        let item = service_item(&view("web", RunState::Running));
        assert_eq!(item.origin, ServiceOrigin::External);
        assert_eq!(item.state, WireState::Running);
        assert!(item.key.starts_with("c_"));
        assert!(item.driver.is_none() && item.unit.is_none());
        assert!(item.busy.is_none() && item.last_action.is_none());
        assert_eq!(item.image.reference.as_deref(), Some("busybox"));
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
