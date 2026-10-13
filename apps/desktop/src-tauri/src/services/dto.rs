//! Ce que la coquille dit à l'interface sur le moteur de conteneurs et les services d'un serveur.
//!
//! Copie fidèle de `hearth_proto::api::services` pour ce qu'affiche la section Services : chaque
//! conversion DESTRUCTURE le type du protocole sans `..` : un champ ajouté au protocole ne compile pas
//! ici sans qu'on décide de le montrer ou de l'ignorer par écrit. Les champs ignorés le sont à dessein :
//! `mounts` et `restart` (le détail d'un service, tranche suivante), `busy` et `last_action` (les actes,
//! tranche suivante). Jamais de valeur ni de nom de variable d'environnement, jamais de commande : le
//! protocole de cette tranche n'en porte pas, et cette copie non plus.
//!
//! Les dates restent des textes RFC 3339 (UTC) : l'interface les lit. Les nombres sont bornés pour
//! `specta` (pas de `u64` ni de `i64`).

use hearth_proto::api::services::{
    EngineState, EngineView, HostInfo, ImageInfo, InitSystem, PortProtocol, PortScope,
    ServiceDriver, ServiceItem, ServiceOrigin, ServicePort, ServiceState, ServicesState,
};
use serde::Serialize;
use specta::Type;

/// Événement de l'état des services d'un serveur (nom côté fenêtre).
pub const EVENT: &str = "link://services";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum EngineStateDto {
    Checking,
    Ready,
    Stopped,
    AccessDenied,
    Unresponsive,
    Absent,
    UnsupportedEngine,
    UnsupportedVersion,
    Disabled,
    /// Un état que ce client ne connaît pas : l'interface dit « non reconnu », sans deviner.
    Unknown,
}

impl From<EngineState> for EngineStateDto {
    fn from(state: EngineState) -> Self {
        match state {
            EngineState::Checking => Self::Checking,
            EngineState::Ready => Self::Ready,
            EngineState::Stopped => Self::Stopped,
            EngineState::AccessDenied => Self::AccessDenied,
            EngineState::Unresponsive => Self::Unresponsive,
            EngineState::Absent => Self::Absent,
            EngineState::UnsupportedEngine => Self::UnsupportedEngine,
            EngineState::UnsupportedVersion => Self::UnsupportedVersion,
            EngineState::Disabled => Self::Disabled,
            EngineState::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum InitDto {
    Systemd,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub struct HostDto {
    #[serde(rename = "osId")]
    pub os_id: String,
    #[serde(rename = "osLike")]
    pub os_like: Vec<String>,
    pub init: InitDto,
    pub declarative: bool,
}

impl From<&HostInfo> for HostDto {
    fn from(host: &HostInfo) -> Self {
        let HostInfo {
            os_id,
            os_like,
            init,
            declarative,
        } = host;
        Self {
            os_id: os_id.clone(),
            os_like: os_like.clone(),
            init: match init {
                InitSystem::Systemd => InitDto::Systemd,
                InitSystem::Other => InitDto::Other,
            },
            declarative: *declarative,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub struct EngineDto {
    pub state: EngineStateDto,
    pub name: Option<String>,
    pub version: Option<String>,
    #[serde(rename = "minVersion")]
    pub min_version: String,
    #[serde(rename = "agentAccount")]
    pub agent_account: String,
    #[serde(rename = "agentIsAdmin")]
    pub agent_is_admin: bool,
    pub host: HostDto,
    #[serde(rename = "lastReadyAt")]
    pub last_ready_at: Option<String>,
    #[serde(rename = "checkedAt")]
    pub checked_at: String,
    /// Date de la dernière lecture complète réussie de la liste ; absente si l'agent ne la donne pas.
    #[serde(rename = "listReadAt")]
    pub list_read_at: Option<String>,
    /// La liste affichée n'est peut-être plus à jour (relecture en échec, conteneurs illisibles, suivi suspendu).
    #[serde(rename = "listDegraded")]
    pub list_degraded: bool,
}

impl From<&EngineView> for EngineDto {
    fn from(engine: &EngineView) -> Self {
        let EngineView {
            state,
            name,
            version,
            min_version,
            agent_account,
            agent_is_admin,
            host,
            last_ready_at,
            checked_at,
            list_read_at,
            list_degraded,
        } = engine;
        Self {
            state: (*state).into(),
            name: name.clone(),
            version: version.clone(),
            min_version: min_version.clone(),
            agent_account: agent_account.clone(),
            agent_is_admin: *agent_is_admin,
            host: host.into(),
            last_ready_at: last_ready_at.clone(),
            checked_at: checked_at.clone(),
            list_read_at: list_read_at.clone(),
            list_degraded: *list_degraded,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ServiceStateDto {
    Running,
    Stopped,
    Starting,
    Stopping,
    Error,
    RestartLoop,
    Paused,
    Deploying,
    Unknown,
}

impl From<ServiceState> for ServiceStateDto {
    fn from(state: ServiceState) -> Self {
        match state {
            ServiceState::Running => Self::Running,
            ServiceState::Stopped => Self::Stopped,
            ServiceState::Starting => Self::Starting,
            ServiceState::Stopping => Self::Stopping,
            ServiceState::Error => Self::Error,
            ServiceState::RestartLoop => Self::RestartLoop,
            ServiceState::Paused => Self::Paused,
            ServiceState::Deploying => Self::Deploying,
            ServiceState::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OriginDto {
    Hearth,
    External,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DriverDto {
    Engine,
    Supervisor,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolDto {
    Tcp,
    Udp,
    Sctp,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ScopeDto {
    All,
    Local,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub struct PortDto {
    #[serde(rename = "hostPort")]
    pub host_port: u16,
    #[serde(rename = "containerPort")]
    pub container_port: u16,
    pub protocol: ProtocolDto,
    pub scope: ScopeDto,
}

impl From<&ServicePort> for PortDto {
    fn from(port: &ServicePort) -> Self {
        let ServicePort {
            host_port,
            container_port,
            protocol,
            scope,
        } = port;
        Self {
            host_port: *host_port,
            container_port: *container_port,
            protocol: match protocol {
                PortProtocol::Tcp => ProtocolDto::Tcp,
                PortProtocol::Udp => ProtocolDto::Udp,
                PortProtocol::Sctp => ProtocolDto::Sctp,
                PortProtocol::Unknown => ProtocolDto::Unknown,
            },
            scope: match scope {
                PortScope::All => ScopeDto::All,
                PortScope::Local => ScopeDto::Local,
                PortScope::Unknown => ScopeDto::Unknown,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub struct ImageDto {
    /// Absent : une image sans nom (« image sans nom {identifiant} », `BR-SVC-022`).
    #[serde(rename = "ref")]
    pub reference: Option<String>,
    pub tag: Option<String>,
    #[serde(rename = "shortId")]
    pub short_id: String,
}

impl From<&ImageInfo> for ImageDto {
    fn from(image: &ImageInfo) -> Self {
        let ImageInfo {
            reference,
            tag,
            short_id,
        } = image;
        Self {
            reference: reference.clone(),
            tag: tag.clone(),
            short_id: short_id.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub struct ServiceDto {
    pub key: String,
    pub name: String,
    pub origin: OriginDto,
    pub driver: Option<DriverDto>,
    pub unit: Option<String>,
    #[serde(rename = "removedOnStop")]
    pub removed_on_stop: bool,
    #[serde(rename = "composeProject")]
    pub compose_project: Option<String>,
    pub state: ServiceStateDto,
    pub since: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: Option<String>,
    #[serde(rename = "neverStarted")]
    pub never_started: bool,
    #[serde(rename = "exitCode")]
    pub exit_code: Option<i32>,
    #[serde(rename = "restarts10m")]
    pub restarts_10m: u32,
    pub image: ImageDto,
    pub ports: Vec<PortDto>,
    #[serde(rename = "hostNetwork")]
    pub host_network: bool,
}

impl From<&ServiceItem> for ServiceDto {
    fn from(item: &ServiceItem) -> Self {
        let ServiceItem {
            key,
            name,
            origin,
            driver,
            unit,
            removed_on_stop,
            compose_project,
            state,
            since,
            created_at,
            never_started,
            exit_code,
            restarts_10m,
            image,
            ports,
            host_network,
            // Le détail d'un service (tranche suivante) : pas montré par la liste.
            mounts: _,
            restart: _,
            // Les actes (tranche suivante) : jamais remplis par l'agent de cette tranche.
            busy: _,
            last_action: _,
        } = item;
        Self {
            key: key.clone(),
            name: name.clone(),
            origin: match origin {
                ServiceOrigin::Hearth => OriginDto::Hearth,
                ServiceOrigin::External => OriginDto::External,
                ServiceOrigin::Unknown => OriginDto::Unknown,
            },
            driver: driver.map(|driver| match driver {
                ServiceDriver::Engine => DriverDto::Engine,
                ServiceDriver::Supervisor => DriverDto::Supervisor,
                ServiceDriver::Unknown => DriverDto::Unknown,
            }),
            unit: unit.clone(),
            removed_on_stop: *removed_on_stop,
            compose_project: compose_project.clone(),
            state: (*state).into(),
            since: since.clone(),
            created_at: created_at.clone(),
            never_started: *never_started,
            exit_code: exit_code.map(|code| {
                i32::try_from(code).unwrap_or(if code < 0 { i32::MIN } else { i32::MAX })
            }),
            restarts_10m: *restarts_10m,
            image: image.into(),
            ports: ports.iter().map(PortDto::from).collect(),
            host_network: *host_network,
        }
    }
}

/// Ce que l'agent dit des services d'un serveur : l'état complet, ou « cet agent ne les gère pas ».
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServicesViewDto {
    Supported {
        engine: Box<EngineDto>,
        services: Vec<ServiceDto>,
    },
    /// Pas de capacité `services` dans `/hello` : ni sujet ni route envoyés à cet agent.
    Unsupported {
        #[serde(rename = "agentVersion")]
        agent_version: String,
    },
}

impl ServicesViewDto {
    /// L'état complet du protocole, tel que l'interface le reçoit.
    pub fn from_state(state: &ServicesState) -> Self {
        // `rev` n'est pas montré : la fusion est faite par la liaison, l'interface ne reçoit que des états.
        let ServicesState {
            rev: _,
            engine,
            services,
        } = state;
        Self::Supported {
            engine: Box::new(engine.into()),
            services: services.iter().map(ServiceDto::from).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub struct ServicesEvent {
    #[serde(rename = "serverId")]
    pub server_id: String,
    /// Croît strictement par serveur : l'interface écarte tout événement dont `seq` n'est pas supérieur
    /// au dernier connu (rejeu à l'abonnement compris).
    pub seq: u32,
    pub view: ServicesViewDto,
}
