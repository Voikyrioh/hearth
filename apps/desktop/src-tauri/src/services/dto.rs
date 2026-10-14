//! Ce que la coquille dit à l'interface sur le moteur de conteneurs et les services d'un serveur.
//!
//! Copie fidèle de `hearth_proto::api::services` pour ce qu'affiche la section Services : chaque
//! conversion DESTRUCTURE le type du protocole sans `..` : un champ ajouté au protocole ne compile pas
//! ici sans qu'on décide de le montrer ou de l'ignorer par écrit. Les champs ignorés le sont à dessein :
//! `busy` et `last_action` (les actes, tranche suivante). Le détail d'un service (HRT-63) porte les montages,
//! la façon de redémarrer, le délai d'arrêt, le repère d'adoption, les NOMS des variables et la commande
//! (`HiddenText` : `Debug` ne montre rien, jamais dans une trace). Jamais une VALEUR de variable : elles ne
//! viennent que de `reveal_service_env`, à la demande, et ne sont gardées nulle part par la coquille.
//!
//! Les dates restent des textes RFC 3339 (UTC) : l'interface les lit. Les nombres sont bornés pour
//! `specta` (pas de `u64` ni de `i64`).

use hearth_proto::api::services::{
    AdoptionInfo, Concealed, EngineState, EngineView, EnvReveal, EnvVariable, HostInfo, ImageInfo,
    InitSystem, MountKind, PortProtocol, PortScope, RestartMode, ServiceDriver, ServiceItem,
    ServiceMount, ServiceOrigin, ServicePort, ServiceState, ServicesState,
};
use serde::Serialize;
use specta::Type;

/// Le droit « gérer les services » du compte connecté (HRT-78, ADR-0041, BR-GRANT-011), en lecture seule.
/// `known` faux : l'agent n'annonce pas `service-rights` (BR-GRANT-016), `granted` est alors toujours faux.
/// `account` : l'identifiant du compte connecté, texte de l'agent (l'interface le valide avant tout affichage).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ServiceRightsDto {
    pub known: bool,
    pub granted: bool,
    pub account: String,
}

impl From<hearth_link::ServiceRightsRead> for ServiceRightsDto {
    fn from(read: hearth_link::ServiceRightsRead) -> Self {
        Self {
            known: read.agent_knows,
            granted: read.granted,
            account: read.account,
        }
    }
}

/// Un texte que l'écran montre mais qu'aucune trace ne doit contenir : nom ou valeur d'une variable
/// d'environnement, argument de la commande de lancement. Sur le fil vers l'interface c'est un simple texte ;
/// `Debug` ne montre rien (`HiddenText(***)`), il n'y a pas de `Display`.
#[derive(Clone, PartialEq, Eq, Serialize, Type)]
#[serde(transparent)]
pub struct HiddenText(String);

impl std::fmt::Debug for HiddenText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HiddenText(***)")
    }
}

impl From<&Concealed> for HiddenText {
    fn from(text: &Concealed) -> Self {
        Self(text.expose().to_owned())
    }
}

/// Une variable révélée (`BR-SVC-009`). Jamais gardée par la coquille : rendue à l'appelant, c'est tout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub struct EnvVariableDto {
    pub name: HiddenText,
    pub value: HiddenText,
}

/// La réponse de `reveal_service_env`, dans l'ordre du moteur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub struct EnvRevealDto {
    pub variables: Vec<EnvVariableDto>,
}

impl From<&EnvReveal> for EnvRevealDto {
    fn from(reveal: &EnvReveal) -> Self {
        let EnvReveal { variables } = reveal;
        Self {
            variables: variables
                .iter()
                .map(|variable| {
                    let EnvVariable { name, value } = variable;
                    EnvVariableDto {
                        name: name.into(),
                        value: value.into(),
                    }
                })
                .collect(),
        }
    }
}

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
    /// Heure de l'agent à la fabrication de la vue (HRT-72) ; absente pour un agent plus ancien.
    pub at: Option<String>,
    /// Dernier moment où la liste était à jour (HRT-72) ; absent si aucune liste n'a été lue.
    #[serde(rename = "listCurrentAt")]
    pub list_current_at: Option<String>,
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
            at,
            list_current_at,
            // HRT-79 : le nombre de conteneurs non gérés n'est pas encore montré par le client.
            external_count: _,
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
            at: at.clone(),
            list_current_at: list_current_at.clone(),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MountKindDto {
    Folder,
    Volume,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub struct MountDto {
    pub kind: MountKindDto,
    /// Dossier du serveur, ou nom du volume géré par le moteur.
    pub source: String,
    pub target: String,
    #[serde(rename = "readOnly")]
    pub read_only: bool,
}

impl From<&ServiceMount> for MountDto {
    fn from(mount: &ServiceMount) -> Self {
        let ServiceMount {
            kind,
            source,
            target,
            read_only,
        } = mount;
        Self {
            kind: match kind {
                MountKind::Folder => MountKindDto::Folder,
                MountKind::Volume => MountKindDto::Volume,
                MountKind::Unknown => MountKindDto::Unknown,
            },
            source: source.clone(),
            target: target.clone(),
            read_only: *read_only,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RestartDto {
    UnlessStopped,
    Always,
    OnFailure,
    None,
    Supervisor,
    Unknown,
}

impl From<RestartMode> for RestartDto {
    fn from(mode: RestartMode) -> Self {
        match mode {
            RestartMode::UnlessStopped => Self::UnlessStopped,
            RestartMode::Always => Self::Always,
            RestartMode::OnFailure => Self::OnFailure,
            RestartMode::None => Self::None,
            RestartMode::Supervisor => Self::Supervisor,
            RestartMode::Unknown => Self::Unknown,
        }
    }
}

/// Qui a adopté un service, et quand (HRT-79).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub struct AdoptionDto {
    pub by: String,
    pub at: String,
}

impl From<&AdoptionInfo> for AdoptionDto {
    fn from(info: &AdoptionInfo) -> Self {
        let AdoptionInfo { by, at } = info;
        Self {
            by: by.clone(),
            at: at.clone(),
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
    /// Ports publiés au-delà de la borne de l'agent, non listés (`BR-SVC-021`).
    #[serde(rename = "portsOmitted")]
    pub ports_omitted: u32,
    #[serde(rename = "hostNetwork")]
    pub host_network: bool,
    pub mounts: Vec<MountDto>,
    #[serde(rename = "mountsOmitted")]
    pub mounts_omitted: u32,
    pub restart: RestartDto,
    /// Noms des variables (jamais une valeur). Vide aussi pour un service adopté dont le détail est réservé.
    #[serde(rename = "envNames")]
    pub env_names: Vec<HiddenText>,
    #[serde(rename = "envNamesOmitted")]
    pub env_names_omitted: u32,
    /// La commande lancée ; `None` : « Commande de l'image ».
    pub command: Option<Vec<HiddenText>>,
    /// Délai d'arrêt en secondes.
    #[serde(rename = "graceS")]
    pub grace_s: Option<u32>,
    pub adopted: Option<AdoptionDto>,
    /// Service adopté dont la commande et les noms de variables sont réservés à ce compte.
    #[serde(rename = "detailsReserved")]
    pub details_reserved: bool,
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
            ports_omitted,
            host_network,
            // Le détail d'un service (HRT-63) : montages, redémarrage, noms de variables (jamais une valeur),
            // commande, délai d'arrêt, repère d'adoption et détail réservé (HRT-79).
            mounts,
            mounts_omitted,
            restart,
            env_names,
            env_names_omitted,
            command,
            grace_s,
            adopted,
            details_reserved,
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
            ports_omitted: *ports_omitted,
            host_network: *host_network,
            mounts: mounts.iter().map(MountDto::from).collect(),
            mounts_omitted: *mounts_omitted,
            restart: (*restart).into(),
            env_names: env_names.iter().map(HiddenText::from).collect(),
            env_names_omitted: *env_names_omitted,
            command: command
                .as_ref()
                .map(|args| args.iter().map(HiddenText::from).collect()),
            grace_s: *grace_s,
            adopted: adopted.as_ref().map(AdoptionDto::from),
            details_reserved: *details_reserved,
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
        /// Combien de services l'agent a envoyés sans que cette version sache les lire (écartés, comptés
        /// jusqu'au prochain état complet) : l'ancien état d'un tel service reste affiché.
        unreadable: u32,
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
            unreadable: services.unreadable(),
            services: services.iter().map(ServiceDto::from).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
pub struct ServicesEvent {
    #[serde(rename = "serverId")]
    pub server_id: String,
    /// Croît strictement par serveur : l'interface écarte tout événement dont `seq` n'est pas supérieur
    /// au dernier connu (rejeu à l'abonnement compris).
    pub seq: u32,
    /// Quand (millisecondes depuis l'époque, heure de ce poste) LA LIAISON a reçu la vue du moteur de cet
    /// état : le repère de l'âge de la liste. Il vient de l'étage qui reçoit le message, donc un état rejoué
    /// ou relu du disque garde sa vraie date. `None` : inconnue (fichier d'avant ce repère, agent sans la
    /// capacité) ; l'interface ne la remplace jamais par l'heure à laquelle elle traite l'événement.
    #[serde(rename = "engineReceivedAt")]
    pub engine_received_at: Option<f64>,
    /// Les noms de variables et la commande de cet état ont été LUS sur le serveur depuis le démarrage de la
    /// liaison. Faux pour une liste relue du disque : elle est écrite sans eux (ils peuvent porter des secrets),
    /// donc leur absence y veut dire « pas lus », jamais « aucune variable » ni « commande de l'image ».
    #[serde(rename = "detailsKnown")]
    pub details_known: bool,
    pub view: ServicesViewDto,
}
