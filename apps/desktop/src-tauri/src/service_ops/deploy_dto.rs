//! Ce que la coquille échange avec l'interface pour déployer une image (HRT-69, ADR-0039, `BR-DEPLOY-*`).
//!
//! - **Entrées** (`DeployCheckInputDto`, `DeployRequestDto`) : la valeur d'une variable, son nom et la commande
//!   sont des secrets. `Debug` n'en montre que le nombre ; ils ne sont jamais tracés ni gardés ici. La
//!   consultation n'en reçoit aucun (BR-DEPLOY-033).
//! - **Sorties** : copies fidèles de `hearth_proto::api::deploy`, chaque énumération porte `Unknown` (une valeur
//!   plus récente que ce client se lit « inconnue », jamais comme une erreur). Aucun texte de l'agent n'est
//!   repris : l'interface choisit ses phrases d'après les codes. Les nombres sont bornés pour `specta` (`f64`).

use hearth_proto::api::deploy::{
    DeployAccept, DeployCheck, DeployCheckRequest, DeployEnv, DeployPort, DeployProtocol,
    DeployResources, DeploySpec, DiskCheck, EngineCapacity, NameStatus, PortCheck, PortStatus,
};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::link_dto::{InvalidField, LinkFailure};
use crate::services::dto::EngineStateDto;

/// Le plus grand entier qu'un nombre de l'interface représente sans perte.
const MAX_SAFE: f64 = 9_007_199_254_740_991.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DeployProtocolDto {
    Tcp,
    Udp,
}

impl From<DeployProtocolDto> for DeployProtocol {
    fn from(protocol: DeployProtocolDto) -> Self {
        match protocol {
            DeployProtocolDto::Tcp => Self::Tcp,
            DeployProtocolDto::Udp => Self::Udp,
        }
    }
}

impl From<DeployProtocol> for DeployProtocolDto {
    fn from(protocol: DeployProtocol) -> Self {
        match protocol {
            DeployProtocol::Tcp => Self::Tcp,
            DeployProtocol::Udp => Self::Udp,
        }
    }
}

/// Une correspondance de ports d'un formulaire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeployPortDto {
    pub host_port: u16,
    pub container_port: u16,
    pub protocol: DeployProtocolDto,
}

impl From<DeployPortDto> for DeployPort {
    fn from(port: DeployPortDto) -> Self {
        Self {
            host_port: port.host_port,
            container_port: port.container_port,
            protocol: port.protocol.into(),
        }
    }
}

/// Ce que pose la consultation : le nom, l'image, la version et les ports. Jamais un secret. Les dossiers de
/// données ne sont pas dans ce ticket (HRT-70) : aucun montage n'est envoyé.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeployCheckInputDto {
    pub name: String,
    pub image: String,
    pub tag: Option<String>,
    pub ports: Vec<DeployPortDto>,
}

impl From<DeployCheckInputDto> for DeployCheckRequest {
    fn from(input: DeployCheckInputDto) -> Self {
        Self {
            name: input.name,
            image: input.image,
            tag: input.tag,
            ports: input.ports.into_iter().map(Into::into).collect(),
            mounts: Vec::new(),
        }
    }
}

/// Une variable d'environnement : le nom ET la valeur sont des secrets.
#[derive(Clone, Deserialize, Type)]
pub struct DeployEnvDto {
    pub name: String,
    pub value: String,
}

impl std::fmt::Debug for DeployEnvDto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DeployEnvDto(***)")
    }
}

/// Mémoire en octets et processeur en millicœurs : obligatoires (BR-DEPLOY-038).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeployResourcesDto {
    pub memory_bytes: f64,
    pub cpu_millis: u32,
}

/// Les points d'attention validés dans le récapitulatif.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeployAcceptDto {
    pub reserved_ports: bool,
    pub low_disk: bool,
}

/// La demande de déploiement.
#[derive(Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeployRequestDto {
    pub name: String,
    pub image: String,
    pub tag: Option<String>,
    pub ports: Vec<DeployPortDto>,
    pub env: Vec<DeployEnvDto>,
    /// La ligne de commande telle que saisie (BR-DEPLOY-014), `None` : la commande de l'image.
    pub command: Option<String>,
    pub resources: DeployResourcesDto,
    pub accept: DeployAcceptDto,
}

impl std::fmt::Debug for DeployRequestDto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Ni valeur, ni nom de variable, ni commande : seulement leur nombre.
        f.debug_struct("DeployRequestDto")
            .field("name", &self.name)
            .field("image", &self.image)
            .field("tag", &self.tag)
            .field("ports", &self.ports)
            .field("env", &self.env.len())
            .field("command", &self.command.is_some())
            .field("resources", &self.resources)
            .field("accept", &self.accept)
            .finish()
    }
}

/// Un nombre d'octets de l'interface : entier, fini, positif, sans perte. Sinon la demande ne part pas.
fn exact_bytes(value: f64) -> Option<u64> {
    let valid = value.is_finite() && (0.0..=MAX_SAFE).contains(&value) && value.fract() == 0.0;
    valid.then_some(value as u64)
}

impl DeployRequestDto {
    /// Le corps envoyé à l'agent. Une mémoire qui n'est pas un entier exact : `InvalidInput`, rien ne part.
    /// Aucun dossier de données (HRT-70).
    pub fn into_spec(self) -> Result<DeploySpec, LinkFailure> {
        let memory_bytes =
            exact_bytes(self.resources.memory_bytes).ok_or(LinkFailure::InvalidInput {
                field: InvalidField::Other,
            })?;
        Ok(DeploySpec {
            name: self.name,
            image: self.image,
            tag: self.tag,
            env: self
                .env
                .into_iter()
                .map(|var| DeployEnv {
                    name: var.name,
                    value: var.value,
                })
                .collect(),
            ports: self.ports.into_iter().map(Into::into).collect(),
            mounts: Vec::new(),
            command: self.command,
            resources: DeployResources {
                cpu_millis: Some(self.resources.cpu_millis),
                memory_bytes: Some(memory_bytes),
            },
            accept: DeployAccept {
                reserved_ports: self.accept.reserved_ports,
                low_disk: self.accept.low_disk,
            },
            extra: std::collections::BTreeMap::new(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum NameStatusDto {
    Ok,
    Invalid,
    Taken,
    BeingDeleted,
    Unknown,
}

impl From<NameStatus> for NameStatusDto {
    fn from(status: NameStatus) -> Self {
        match status {
            NameStatus::Ok => Self::Ok,
            NameStatus::Invalid => Self::Invalid,
            NameStatus::Taken => Self::Taken,
            NameStatus::BeingDeleted => Self::BeingDeleted,
            NameStatus::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PortStatusDto {
    Free,
    HeldByService,
    HeldByUnmanaged,
    ReservedByStopped,
    HeldByProgram,
    AgentPort,
    Invalid,
    Unknown,
}

impl From<PortStatus> for PortStatusDto {
    fn from(status: PortStatus) -> Self {
        match status {
            PortStatus::Free => Self::Free,
            PortStatus::HeldByService => Self::HeldByService,
            PortStatus::HeldByUnmanaged => Self::HeldByUnmanaged,
            PortStatus::ReservedByStopped => Self::ReservedByStopped,
            PortStatus::HeldByProgram => Self::HeldByProgram,
            PortStatus::AgentPort => Self::AgentPort,
            PortStatus::Invalid => Self::Invalid,
            PortStatus::Unknown => Self::Unknown,
        }
    }
}

/// Le constat sur un port du serveur. `holder` : texte du serveur, jamais le nom d'un conteneur non adopté.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PortCheckDto {
    pub host_port: u16,
    pub protocol: DeployProtocolDto,
    pub status: PortStatusDto,
    pub holder: Option<String>,
    pub suggested: Option<u16>,
    pub privileged: bool,
}

impl From<&PortCheck> for PortCheckDto {
    fn from(check: &PortCheck) -> Self {
        // Destructuré sans `..` : un champ ajouté au fil force à décider s'il traverse.
        let PortCheck {
            host_port,
            protocol,
            status,
            holder,
            suggested,
            privileged,
        } = check;
        Self {
            host_port: *host_port,
            protocol: (*protocol).into(),
            status: (*status).into(),
            holder: holder.clone(),
            suggested: *suggested,
            privileged: *privileged,
        }
    }
}

/// L'espace libre là où le moteur range ses images.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiskCheckDto {
    pub free_bytes: f64,
    pub low: bool,
}

/// Les champs de texte qu'une consultation ou un refus peut désigner (liste fermée de l'agent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DeployFieldDto {
    Name,
    Image,
    Tag,
    Env,
    Ports,
    Mounts,
    Command,
    Resources,
}

impl DeployFieldDto {
    /// Le champ nommé par l'agent ; un nom que ce client ne connaît pas n'est pas un champ.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "name" => Self::Name,
            "image" => Self::Image,
            "tag" => Self::Tag,
            "env" => Self::Env,
            "ports" => Self::Ports,
            "mounts" => Self::Mounts,
            "command" => Self::Command,
            "resources" => Self::Resources,
            _ => return None,
        })
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Image => "image",
            Self::Tag => "tag",
            Self::Env => "env",
            Self::Ports => "ports",
            Self::Mounts => "mounts",
            Self::Command => "command",
            Self::Resources => "resources",
        }
    }
}

/// La réponse de la consultation. Indicative : le serveur refait tout au lancement (BR-DEPLOY-032).
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeployCheckDto {
    pub name: NameStatusDto,
    /// Les champs invalides, dans la liste fermée ; un nom inconnu est écarté.
    pub invalid_fields: Vec<String>,
    pub ports: Vec<PortCheckDto>,
    pub disk: Option<DiskCheckDto>,
    pub image_present: bool,
}

impl From<&DeployCheck> for DeployCheckDto {
    fn from(check: &DeployCheck) -> Self {
        // Destructuré sans `..`. Les dossiers (`mounts`) n'existent pas côté interface avant HRT-70.
        let DeployCheck {
            name,
            invalid_fields,
            ports,
            mounts: _,
            disk,
            image_present,
        } = check;
        Self {
            name: (*name).into(),
            invalid_fields: invalid_fields
                .iter()
                .filter_map(|field| DeployFieldDto::parse(field))
                .map(|field| field.as_str().to_owned())
                .collect(),
            ports: ports.iter().map(PortCheckDto::from).collect(),
            disk: disk.as_ref().map(|disk| {
                let DiskCheck { free_bytes, low } = disk;
                DiskCheckDto {
                    free_bytes: *free_bytes as f64,
                    low: *low,
                }
            }),
            image_present: *image_present,
        }
    }
}

/// Les bornes des curseurs de ressources et la part de la machine déjà engagée en mémoire (BR-DEPLOY-038).
/// Des totaux, jamais un nom de service.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EngineCapacityDto {
    pub cpus: u32,
    pub memory_bytes: f64,
    pub reserve_bytes: f64,
    pub managed_running_bytes: f64,
    pub unmanaged_running_bytes: f64,
}

impl From<&EngineCapacity> for EngineCapacityDto {
    fn from(capacity: &EngineCapacity) -> Self {
        let EngineCapacity { host, committed } = capacity;
        Self {
            cpus: host.cpus,
            memory_bytes: host.memory_bytes as f64,
            reserve_bytes: host.reserve_bytes as f64,
            managed_running_bytes: committed.memory.managed_running as f64,
            unmanaged_running_bytes: committed.memory.unmanaged_running as f64,
        }
    }
}

/// Pourquoi l'agent a refusé le déploiement, avant tout geste (rien n'est créé). Des CODES, jamais un texte.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeployRefusal {
    WrongPassword,
    PasswordRequired,
    TooManyAttempts {
        retry_after_s: u32,
    },
    Busy,
    EngineUnavailable {
        engine: Option<EngineStateDto>,
    },
    /// Une valeur du formulaire n'est pas valable : le champ et la raison (code stable de l'agent).
    Invalid {
        field: DeployFieldDto,
        reason: String,
    },
    SettingRefused,
    LimitsRequired,
    NameTaken,
    NameBeingDeleted,
    PortInUse {
        port: u16,
        protocol: DeployProtocolDto,
        status: PortStatusDto,
        holder: Option<String>,
        suggested: Option<u16>,
    },
    DeployLimit,
    NoSpace {
        #[serde(rename = "freeBytes")]
        free_bytes: Option<f64>,
    },
    Other,
}

/// L'issue de l'envoi : accepté (`202`, l'opération court à l'arrière-plan), refusé, ou résultat inconnu (le
/// lien est tombé avant la réponse : jamais rejoué).
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeployOutcome {
    Accepted {
        #[serde(rename = "operationId")]
        operation_id: String,
        #[serde(rename = "serviceKey")]
        service_key: String,
    },
    Refused {
        refusal: DeployRefusal,
    },
    Unknown {
        #[serde(rename = "opId")]
        op_id: String,
    },
}

/// Pourquoi l'agent a refusé l'annulation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CancelRefusal {
    WrongPassword,
    PasswordRequired,
    TooManyAttempts {
        retry_after_s: u32,
    },
    Busy,
    /// L'opération est introuvable (purgée, ou service que le compte ne voit pas).
    NotFound,
    /// Le point de non-retour est passé.
    TooLate,
    /// L'opération n'est pas dans un statut qui s'annule (déjà finie).
    NotCancellable,
    /// L'opération a changé pendant la demande : relire.
    Changed,
    Other,
}

/// L'issue d'une annulation.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CancelOutcome {
    /// L'agent a pris l'annulation : l'opération telle qu'elle est maintenant.
    Done {
        operation: Box<super::dto::OperationDto>,
    },
    Refused {
        refusal: CancelRefusal,
    },
    /// Le lien est tombé avant la réponse : on ne sait pas. Jamais rejouée ; l'état arrive par le flux.
    Unknown {
        #[serde(rename = "opId")]
        op_id: String,
    },
}
