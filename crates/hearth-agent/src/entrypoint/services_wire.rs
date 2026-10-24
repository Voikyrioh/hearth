//! Conversions entre le registre des services (domaine et application) et les types du fil de
//! `hearth-proto`. Partagé par `http/` (`GET /services`, `POST /engine/recheck`) et `ws/` (sujet
//! `services`) : le contrat JSON des services n'est connu qu'ici.
//!
//! Ce qui sort ici a déjà passé le filtre de publication du registre (HRT-79, `BR-ADOPT-001`) : ni nom,
//! ni image, ni port d'un conteneur non adopté n'arrive jusqu'à ces fonctions. Reste une décision par
//! destinataire (`Reader`) : pour un service adopté, la commande et les noms de variables ne sont
//! donnés qu'aux comptes qui ont le droit « gérer les services » (`BR-ADOPT-015`).
//!
//! Aucune VALEUR de variable d'environnement ne passe par ici : le registre n'en contient pas (elles ne
//! se lisent que par `GET /services/{key}/env`, `env_reveal`). Les noms de variables et la commande
//! (détail d'un service, BR-SVC-007 et BR-SVC-008) passent dans le type `Concealed` du fil.

use super::metrics_wire::date;
use super::service_ops_wire;
use crate::application::ports::EnvEntry;
use crate::application::ports::HostDescription;
use crate::application::services::{EngineStatus, ServiceView, ServicesChange, ServicesSnapshot};
use crate::domain::ops::{OpsIndex, may_see, origin_of};
use crate::domain::services::{
    EngineState, KeyKind, MountType, PortProtocol, PortScope, RestartPolicy, ServiceStateKind,
    may_read_details, stop_grace_s,
};
use hearth_proto::api::services::{
    AdoptionInfo, Concealed, EngineState as WireEngineState, EngineView, EnvReveal, EnvVariable,
    HostInfo, ImageInfo, InitSystem, MIN_ENGINE_VERSION, MountKind, PortProtocol as WireProtocol,
    PortScope as WireScope, RestartMode, ServiceDriver, ServiceItem, ServiceMount, ServiceOrigin,
    ServicePort, ServiceState as WireState, ServicesDelta, ServicesState,
};

/// Qui lit : le seul fait qui change le contenu d'un service publié.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reader {
    /// Le compte a le droit « gérer les services » (ADR-0041), quel que soit son rôle.
    pub may_manage_services: bool,
}

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

pub(crate) fn service_state(state: ServiceStateKind) -> WireState {
    match state {
        ServiceStateKind::Running => WireState::Running,
        ServiceStateKind::Starting => WireState::Starting,
        ServiceStateKind::Stopped => WireState::Stopped,
        ServiceStateKind::Error => WireState::Error,
        ServiceStateKind::RestartLoop => WireState::RestartLoop,
        ServiceStateKind::Paused => WireState::Paused,
        ServiceStateKind::Stopping => WireState::Stopping,
        ServiceStateKind::Deploying => WireState::Deploying,
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
        external_count: status.external_count,
    }
}

pub fn service_item(view: &ServiceView, reader: Reader) -> ServiceItem {
    let observation = &view.observation;
    // BR-ADOPT-015 : la commande et les noms de variables d'un service adopté sont retirés pour un
    // compte sans le droit. BR-ADOPT-016 : un service créé par Hearth reste lisible de tous.
    let details = may_read_details(view.adoption.as_ref(), reader.may_manage_services);
    ServiceItem {
        key: view.key.as_str().to_owned(),
        name: observation.name.clone(),
        origin: if view.key.kind() == KeyKind::Hearth {
            ServiceOrigin::Hearth
        } else {
            ServiceOrigin::External
        },
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
        env_names: if details {
            observation
                .env_names
                .iter()
                .map(|name| Concealed::new(name.expose()))
                .collect()
        } else {
            Vec::new()
        },
        env_names_omitted: if details { observation.env_omitted } else { 0 },
        command: observation
            .command
            .as_ref()
            .filter(|_| details)
            .map(|parts| {
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
        adopted: view.adoption.as_ref().map(|adoption| AdoptionInfo {
            by: adoption.by.clone(),
            at: date(adoption.at),
        }),
        details_reserved: !details,
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

/// `busy` et `last_action` d'un service : des PROJECTIONS du journal des opérations (HRT-144, BR-LIFE-013),
/// jamais un état à part. Absents tant que le service n'a aucune opération, ET absents pour un compte qui ne
/// peut pas lire les opérations de ce service (BR-OPS-001) : un service adopté est publié à tous les comptes
/// (détails masqués), mais ses opérations ne le sont qu'au droit « gérer les services ». Dire à un autre
/// qu'une opération y a eu lieu, qui l'a lancée, ou donner son identifiant, serait dire qu'elle existe.
fn with_operations(
    mut item: ServiceItem,
    view: &ServiceView,
    reader: Reader,
    ops: &OpsIndex,
) -> ServiceItem {
    let origin = origin_of(&view.key, view.adoption.is_some());
    if !may_see(origin, reader.may_manage_services) {
        return item;
    }
    // La liste lit la projection SANS attendre la porte de clôture (FIX:01M5P0Q3T8ZK6V2R9XH4DWB7NC) : une
    // clôture en cours peut la montrer « occupée » un instant ; la publication qui suit la rafraîchit.
    item.busy = ops.busy(&view.key).map(service_ops_wire::busy);
    item.last_action = ops
        .last_action(&view.key)
        .map(service_ops_wire::last_action);
    item
}

pub fn snapshot(snapshot: &ServicesSnapshot, reader: Reader, ops: &OpsIndex) -> ServicesState {
    ServicesState {
        rev: snapshot.rev,
        engine: engine_view(&snapshot.engine, &snapshot.host),
        services: snapshot
            .services
            .iter()
            .map(|view| with_operations(service_item(view, reader), view, reader, ops))
            .collect(),
    }
}

/// Un lot de changements ; `host` sert à l'état du moteur quand il change.
pub fn delta(
    change: &ServicesChange,
    host: &HostDescription,
    reader: Reader,
    ops: &OpsIndex,
) -> ServicesDelta {
    ServicesDelta {
        rev: change.rev,
        engine: change
            .engine
            .as_ref()
            .map(|status| engine_view(status, host)),
        upserts: change
            .upserts
            .iter()
            .map(|view| with_operations(service_item(view, reader), view, reader, ops))
            .collect(),
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
            memory_limit_bytes: None,
            cpu_millis: None,
            tty: false,
            log_driver: None,
            hearth_key: None,
        };
        let key = ServiceKey::for_container(&observation.id).expect("key");
        let derived = derive_state(&observation, 0, false);
        ServiceView {
            key,
            observation,
            derived,
            restarts_10m: 0,
            supervision: None,
            adoption: None,
        }
    }

    fn everyone() -> Reader {
        Reader {
            may_manage_services: false,
        }
    }

    #[test]
    fn a_service_item_is_external_engine_driven_and_carries_no_future_field() {
        let item = service_item(&view("web", RunState::Running), everyone());
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
        let item = service_item(&supervised, everyone());
        assert_eq!(item.key, "u_docker-terraria.service");
        assert_eq!(item.driver, Some(ServiceDriver::Supervisor));
        assert_eq!(item.unit.as_deref(), Some("docker-terraria.service"));
        assert_eq!(item.restart, RestartMode::Supervisor);
        assert_eq!(item.grace_s, Some(150));
    }

    fn adopted(mut view: ServiceView) -> ServiceView {
        view.adoption = Some(crate::domain::services::Adoption {
            by: "marie".to_owned(),
            at: time::OffsetDateTime::UNIX_EPOCH,
        });
        view
    }

    #[test]
    fn an_adopted_service_gives_its_command_and_variable_names_to_the_right_only() {
        let service = adopted(view("terraria", RunState::Running));
        let right = Reader {
            may_manage_services: true,
        };
        let with = service_item(&service, right);
        assert_eq!(with.env_names.len(), 1);
        assert_eq!(with.command.as_ref().map(Vec::len), Some(2));
        assert!(!with.details_reserved);
        assert_eq!(with.adopted.as_ref().map(|a| a.by.as_str()), Some("marie"));
        // Sans le droit : ni commande, ni noms, ni compte de noms coupés, et la réserve est dite.
        let without = service_item(&service, everyone());
        assert!(without.env_names.is_empty() && without.command.is_none());
        assert_eq!(without.env_names_omitted, 0);
        assert!(without.details_reserved);
        // Le reste du détail est ouvert à tous (BR-ADOPT-015).
        assert_eq!(without.image, with.image);
        assert_eq!(without.grace_s, with.grace_s);
        assert!(without.adopted.is_some());
        let text = serde_json::to_string(&without).expect("json");
        assert!(!text.contains("TOKEN_NAME") && !text.contains("SENTINEL"));
    }

    /// BR-OPS-001 (revue de la PR 94, B2) : un service adopté est publié à tous les comptes, mais ses
    /// opérations ne se lisent qu'avec le droit. `busy` et `last_action` (genre, compte, statut, cause,
    /// identifiant d'opération) n'en disent rien aux autres : ce serait dire qu'une opération existe.
    #[test]
    fn busy_and_last_action_of_an_adopted_service_are_for_the_right_only() {
        use crate::domain::ops::{OpKind, OpStatus, OpSummary};
        let summary = |id: &str, status: OpStatus, finished: bool| OpSummary {
            id: id.to_owned(),
            kind: OpKind::Deploy,
            account: "SENTINEL-ACCOUNT".to_owned(),
            status,
            cause: None,
            forced: false,
            started_at: time::OffsetDateTime::UNIX_EPOCH,
            finished_at: finished.then_some(time::OffsetDateTime::UNIX_EPOCH),
        };
        let service = adopted(view("terraria", RunState::Running));
        let mut index = OpsIndex::new();
        index.apply(
            &service.key,
            summary("SENTINEL-OP-1", OpStatus::Succeeded, true),
        );
        index.apply(
            &service.key,
            summary("SENTINEL-OP-2", OpStatus::Running, false),
        );
        let right = Reader {
            may_manage_services: true,
        };
        let with = with_operations(service_item(&service, right), &service, right, &index);
        assert!(with.busy.is_some());
        assert_eq!(
            with.last_action.as_ref().map(|last| last.id.as_str()),
            Some("SENTINEL-OP-1")
        );
        // Sans le droit : aucun des deux champs, et aucune trace de l'opération dans ce qui est publié.
        let without = with_operations(
            service_item(&service, everyone()),
            &service,
            everyone(),
            &index,
        );
        assert!(without.busy.is_none() && without.last_action.is_none());
        let text = serde_json::to_string(&without).expect("json");
        assert!(!text.contains("SENTINEL"), "{text}");
        // Un service créé par Hearth, lui, montre ses opérations à tous les comptes (BR-LIFE-013).
        let mut own = view("web", RunState::Running);
        own.key = ServiceKey::parse("h_01ARZ3NDEKTSV4RRFFQ69G5FAV").expect("key");
        let mut index = OpsIndex::new();
        index.apply(&own.key, summary("OP-HEARTH", OpStatus::Running, false));
        let item = with_operations(service_item(&own, everyone()), &own, everyone(), &index);
        assert!(item.busy.is_some());
    }

    #[test]
    fn a_service_made_by_hearth_keeps_its_details_for_every_account() {
        let mut service = view("web", RunState::Running);
        service.key = ServiceKey::parse("h_01ARZ3NDEKTSV4RRFFQ69G5FAV").expect("key");
        let item = service_item(&service, everyone());
        assert_eq!(item.origin, ServiceOrigin::Hearth);
        assert_eq!(item.env_names.len(), 1);
        assert_eq!(item.command.as_ref().map(Vec::len), Some(2));
        assert!(item.adopted.is_none() && !item.details_reserved);
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
