//! Déployer une image (HRT-68, ADR-0039, BR-DEPLOY-*) : le corps de `POST /deployments` et de
//! `POST /deployments/check`, la réponse de la vérification, et les bornes de la machine
//! (`GET /engine/capacity`).
//!
//! **Le plan est fermé** (ADR-0041, BR-DEPLOY-035) : [`DeploySpec`] n'a aucun champ pour un mode privilégié,
//! un réseau du serveur, un périphérique ou une option de sécurité. Un membre inconnu du corps est gardé dans
//! `extra` pour que l'agent le REFUSE (`PLAN_SETTING_REFUSED`), au lieu de l'ignorer en silence.
//!
//! **Aucun secret n'est lisible par `Debug`** : la valeur d'une variable et la commande sont masquées. La
//! vérification (`DeployCheckRequest`) ne reçoit jamais un secret : ni valeur, ni nom de variable, ni commande.
//! Les valeurs de variables, leurs noms et la commande ne sont jamais signés (voir
//! [`crate::api::service_spec::deploy_digest`]).

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::lenient::Items;

/// Capacité annoncée par `GET /hello` quand l'agent sait déployer une image (HRT-68) : `POST /deployments`,
/// `POST /deployments/check` et `GET /engine/capacity` existent. Un client n'offre le déploiement qu'à un agent
/// qui l'annonce ; un agent ancien n'a pas ces routes (`404`).
pub const CAPABILITY_SERVICE_DEPLOY: &str = "service-deploy";

/// Variables d'un déploiement, au plus (BR-DEPLOY-002).
pub const MAX_ENV_VARS: usize = 100;
/// Correspondances de ports, au plus (BR-DEPLOY-002).
pub const MAX_PORT_MAPPINGS: usize = 50;
/// Dossiers de données, au plus (BR-DEPLOY-002).
pub const MAX_DEPLOY_MOUNTS: usize = 20;
/// Caractères d'une valeur de variable, au plus (BR-DEPLOY-006).
pub const MAX_ENV_VALUE_CHARS: usize = 8_192;
/// Caractères de la commande de lancement, au plus (BR-DEPLOY-014).
pub const MAX_COMMAND_CHARS: usize = 4_096;
/// Déploiements en même temps sur un serveur (BR-DEPLOY-023).
pub const MAX_CONCURRENT_DEPLOYMENTS: usize = 2;
/// Une opération de déploiement qui dure plus longtemps est arrêtée (BR-DEPLOY-034), en secondes.
pub const DEPLOY_TIMEOUT_S: u64 = 30 * 60;
/// En dessous de cet espace libre (octets) sur le disque des images, l'agent avertit (BR-DEPLOY-018).
pub const LOW_DISK_BYTES: u64 = 10 * 1024 * 1024 * 1024;

/// Protocole d'un port (BR-DEPLOY-007). Tcp par défaut.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum DeployProtocol {
    #[default]
    Tcp,
    Udp,
}

impl DeployProtocol {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Udp => "udp",
        }
    }
}

/// Une correspondance de port : un port du serveur vers un port du service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DeployPort {
    pub host_port: u16,
    pub container_port: u16,
    #[serde(default)]
    pub protocol: DeployProtocol,
}

/// Une variable d'environnement. Le nom ET la valeur sont des secrets pour l'agent (type `Secret`) : ils
/// ne sont ni signés, ni journalisés, ni tracés.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeployEnv {
    pub name: String,
    #[serde(default)]
    pub value: String,
}

impl fmt::Debug for DeployEnv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeployEnv(***)")
    }
}

/// Où se range un dossier de données.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum DeployPlace {
    /// Sous la racine de données de Hearth : un sous-dossier par service, par point de montage.
    #[default]
    Managed,
    /// Un autre dossier du serveur (HRT-70) : refusé par cette version de l'agent.
    Host,
}

/// Un dossier de données.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DeployMount {
    /// Le chemin DANS le service (absolu).
    pub target: String,
    #[serde(default)]
    pub place: DeployPlace,
    /// Le chemin sur le serveur, pour `place = host` seulement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_path: Option<String>,
    #[serde(default)]
    pub read_only: bool,
    /// « Créer ce dossier au lancement », pour `place = host`.
    #[serde(default)]
    pub create: bool,
    /// « Je confirme », pour `place = host`.
    #[serde(default)]
    pub confirmed: bool,
}

/// Les limites du service (BR-DEPLOY-038) : OBLIGATOIRES. Absentes, l'agent refuse (`LIMITS_REQUIRED`).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default,
)]
pub struct DeployResources {
    /// Processeur en millicœurs (1 000 = un cœur).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_millis: Option<u32>,
    /// Mémoire en octets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_bytes: Option<u64>,
}

/// Les points d'attention validés par l'utilisateur dans le récapitulatif.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default,
)]
pub struct DeployAccept {
    /// Un port réservé par un service arrêté (BR-DEPLOY-008).
    #[serde(default)]
    pub reserved_ports: bool,
    /// « Continuer malgré le manque de place » (BR-DEPLOY-018).
    #[serde(default)]
    pub low_disk: bool,
}

/// Corps de `POST /deployments`. Le membre `reauth` de la confirmation voyage à côté (voir `api::reauth`).
#[derive(Clone, Serialize, Deserialize)]
pub struct DeploySpec {
    pub name: String,
    pub image: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(default)]
    pub env: Vec<DeployEnv>,
    #[serde(default)]
    pub ports: Vec<DeployPort>,
    #[serde(default)]
    pub mounts: Vec<DeployMount>,
    /// La ligne de commande, découpée par l'agent (jamais passée à un interpréteur, BR-DEPLOY-014).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default)]
    pub resources: DeployResources,
    #[serde(default)]
    pub accept: DeployAccept,
    /// Tout membre que ce type ne connaît pas (hors `reauth`) : l'agent refuse le déploiement, il n'ignore
    /// jamais un réglage qu'il ne comprend pas (BR-DEPLOY-035).
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl fmt::Debug for DeploySpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Ni valeur, ni nom de variable, ni commande : seulement leur nombre.
        f.debug_struct("DeploySpec")
            .field("name", &self.name)
            .field("image", &self.image)
            .field("tag", &self.tag)
            .field("env", &self.env.len())
            .field("ports", &self.ports)
            .field("mounts", &self.mounts.len())
            .field("command", &self.command.is_some())
            .field("resources", &self.resources)
            .finish_non_exhaustive()
    }
}

/// Corps de `POST /deployments/check` : la même chose, sans aucun secret (ni variables, ni commande).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeployCheckRequest {
    pub name: String,
    pub image: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(default)]
    pub ports: Vec<DeployPort>,
    #[serde(default)]
    pub mounts: Vec<DeployMount>,
}

/// Réponse de `POST /deployments` : l'opération suivie (`GET /operations/services/{id}`) et la clé que le
/// service portera.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeployAccepted {
    pub operation_id: String,
    pub service_key: String,
}

/// Le constat sur le nom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NameStatus {
    Ok,
    /// Le nom ne respecte pas BR-DEPLOY-003.
    Invalid,
    /// Pris par un service, un conteneur ou un déploiement (sans dire lequel, BR-ADOPT-022).
    Taken,
    /// Les données d'un service supprimé s'effacent encore (BR-DEL-019).
    BeingDeleted,
    #[serde(other)]
    Unknown,
}

/// Le constat sur un port du serveur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortStatus {
    Free,
    /// Tenu par un service de Hearth ou adopté en marche (nommé dans `holder`).
    HeldByService,
    /// Tenu par un conteneur hors Hearth, jamais nommé (BR-ADOPT-022).
    HeldByUnmanaged,
    /// Réservé par un service arrêté : avertit, ne bloque pas (BR-DEPLOY-008).
    ReservedByStopped,
    /// Écouté par un programme du serveur (nommé dans `holder` quand l'agent le sait).
    HeldByProgram,
    /// C'est le port de l'agent lui-même (BR-DEPLOY-007).
    AgentPort,
    /// Doublon dans le formulaire, ou port hors bornes.
    Invalid,
    #[serde(other)]
    Unknown,
}

/// Un port vérifié.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortCheck {
    pub host_port: u16,
    pub protocol: DeployProtocol,
    pub status: PortStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holder: Option<String>,
    /// Le plus petit port libre au-dessus, à défaut le plus grand libre en dessous (BR-DEPLOY-009).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested: Option<u16>,
    /// Sous 1024 : avertissement (BR-DEPLOY-007).
    #[serde(default)]
    pub privileged: bool,
}

/// Le constat sur un dossier de données.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MountStatus {
    Ok,
    /// Chemin dans le service invalide, ou doublon.
    Invalid,
    /// Dossier du serveur protégé (BR-DEPLOY-012).
    Refused,
    /// « Autre dossier du serveur » : pas encore disponible dans cette version (HRT-70).
    NotAvailable,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MountCheck {
    pub target: String,
    pub status: MountStatus,
}

/// L'espace libre là où le moteur range ses images (BR-DEPLOY-018). Absent si l'agent ne peut pas le lire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskCheck {
    pub free_bytes: u64,
    pub low: bool,
}

/// Réponse de `POST /deployments/check`. Une consultation : rien n'est écrit, rien n'est journalisé.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeployCheck {
    pub name: NameStatus,
    /// Les champs de texte invalides (`name`, `image`, `tag`, `ports`, `mounts`), par code stable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invalid_fields: Vec<String>,
    #[serde(default)]
    pub ports: Items<PortCheck>,
    #[serde(default)]
    pub mounts: Items<MountCheck>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk: Option<DiskCheck>,
    /// L'image est déjà sur le serveur : elle sera utilisée telle quelle (BR-DEPLOY-018).
    pub image_present: bool,
}

/// Le plafond de chaque curseur et la part de la machine déjà engagée (`GET /engine/capacity`, BR-DEPLOY-038).
/// Les chiffres sont lus sur la machine, pas dans un conteneur. Aucun nom de service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapacityHost {
    /// Processeurs logiques.
    pub cpus: u32,
    pub memory_bytes: u64,
    /// Réserve du système et de Hearth : le plus grand de 1 Go et de 5 % de la mémoire.
    pub reserve_bytes: u64,
}

/// Une somme de limites, gérée ou non, en marche ou toute (un total, jamais un nom).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Committed {
    pub managed_running: u64,
    pub unmanaged_running: u64,
    pub managed_all: u64,
    pub unmanaged_all: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CommittedTotals {
    pub memory: Committed,
    pub cpu_millis: Committed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCapacity {
    pub host: CapacityHost,
    pub committed: CommittedTotals,
}

/// Le plus grand de 1 Gio et de 5 % de la mémoire (BR-DEPLOY-038).
pub fn reserve_bytes(memory_bytes: u64) -> u64 {
    const GIB: u64 = 1024 * 1024 * 1024;
    GIB.max(memory_bytes / 20)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_shows_a_value_a_variable_name_or_the_command() {
        let spec = DeploySpec {
            name: "web".into(),
            image: "nginx".into(),
            tag: None,
            env: vec![DeployEnv {
                name: "DB_PASSWORD".into(),
                value: "SENTINEL-VALUE".into(),
            }],
            ports: vec![],
            mounts: vec![],
            command: Some("run --token SENTINEL-CMD".into()),
            resources: DeployResources::default(),
            accept: DeployAccept::default(),
            extra: BTreeMap::new(),
        };
        let shown = format!("{spec:?} {:?}", spec.env[0]);
        assert!(
            !shown.contains("SENTINEL") && !shown.contains("DB_PASSWORD"),
            "{shown}"
        );
    }

    #[test]
    fn a_member_this_type_does_not_know_is_kept_to_be_refused_and_reauth_is_among_them() {
        let spec: DeploySpec = serde_json::from_value(serde_json::json!({
            "name": "web", "image": "nginx", "privileged": true, "reauth": {"password": "x"}
        }))
        .expect("lu");
        assert_eq!(spec.extra.len(), 2);
        assert!(spec.extra.contains_key("privileged"));
        assert_eq!(spec.ports.len(), 0);
        assert_eq!(spec.resources, DeployResources::default());
    }

    #[test]
    fn the_capability_name_is_the_documented_one() {
        assert_eq!(CAPABILITY_SERVICE_DEPLOY, "service-deploy");
    }

    #[test]
    fn the_reserve_is_one_gibibyte_or_five_percent() {
        const GIB: u64 = 1024 * 1024 * 1024;
        assert_eq!(reserve_bytes(8 * GIB), GIB);
        assert_eq!(reserve_bytes(64 * GIB), 64 * GIB / 20);
    }
}
