//! Services (conteneurs) du serveur et état du moteur de conteneurs : `GET /services`,
//! `POST /engine/recheck` et le sujet `services` du flux (`stream::ServicesMessage`).
//!
//! Tranche 1 des déploiements (HRT-61) : lecture seule. Ce module ne porte AUCUNE valeur de
//! variable d'environnement, aucun nom de variable et aucune commande de lancement : ces données
//! ne quittent pas l'agent dans cette tranche (ADR-0038). Les champs des tranches suivantes
//! (`driver`, `unit`, `busy`, `last_action`) sont déclarés facultatifs et absents tant qu'aucune
//! tranche ne les remplit : un client récent lit un agent ancien, un client ancien ignore ce qu'il
//! ne connaît pas.
//!
//! Les énumérations que le client lit tolèrent une valeur inconnue (variante `Unknown`, ou `Other` pour
//! le système d'init) : un agent plus récent peut ajouter une valeur sans que le client déjà installé
//! ne perde tout l'instantané (la capacité annoncée et `API_VERSION` ne bougent pas pour si peu).
//!
//! Les dates sont des textes RFC 3339 en UTC. Les textes affichés viennent de l'interface,
//! indexés par ces codes.

use serde::{Deserialize, Serialize};

/// Capacité annoncée par `GET /hello` (`capabilities`) quand l'agent sert le sujet `services`.
/// Un client n'envoie le sujet `services` (ni aucune route des services) qu'à un agent qui l'annonce :
/// un sujet inconnu fait refuser TOUT le message `subscribe`, `metrics` compris.
pub const CAPABILITY_SERVICES: &str = "services";

/// Version minimale du moteur de conteneurs prise en charge (Docker Engine 20.10, API 1.41).
pub const MIN_ENGINE_VERSION: &str = "20.10";

/// Les états du moteur (`BR-ENGINE-002`), plus `checking` (`BR-ENGINE-019`) : l'intitulé de la
/// première détection, qui ne dure que le temps de la sonde.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineState {
    /// Première détection en cours (au plus 5 secondes).
    Checking,
    Ready,
    /// Le moteur est installé mais ne tourne pas.
    Stopped,
    /// Le moteur tourne mais l'agent n'a pas le droit de lui parler.
    AccessDenied,
    /// Le moteur n'a pas répondu dans les 5 secondes.
    Unresponsive,
    /// Aucun moteur de conteneurs sur ce serveur.
    Absent,
    /// Un autre moteur que Docker (Podman…).
    UnsupportedEngine,
    /// Docker, mais plus ancien que `MIN_ENGINE_VERSION`.
    UnsupportedVersion,
    /// Pilotage des services désactivé dans la configuration de l'agent (`engine.enabled = false`).
    Disabled,
    /// Un état que ce client ne connaît pas (ajouté par un agent plus récent).
    #[serde(other)]
    Unknown,
}

/// Le système d'init du serveur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InitSystem {
    Systemd,
    /// Tout autre système d'init, y compris un que ce client ne connaît pas.
    #[serde(other)]
    Other,
}

/// Ce que l'agent sait du serveur pour adapter l'aide de l'interface (`BR-ENGINE-005`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostInfo {
    /// `ID` de `/etc/os-release` (`debian`, `nixos`…), vide si inconnu.
    pub os_id: String,
    /// `ID_LIKE` de `/etc/os-release`.
    pub os_like: Vec<String>,
    pub init: InitSystem,
    /// Le serveur est configuré de façon déclarative (NixOS) : l'aide est un texte de configuration.
    pub declarative: bool,
}

/// L'état du moteur de conteneurs d'un serveur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineView {
    pub state: EngineState,
    /// Nom du moteur tel qu'il l'annonce (seulement s'il a répondu).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Version minimale prise en charge.
    pub min_version: String,
    /// Compte sous lequel l'agent tourne (`BR-ENGINE-008`) ; vide si l'agent ne le sait pas.
    pub agent_account: String,
    /// `true` si ce compte est l'administrateur du serveur (root).
    #[serde(default)]
    pub agent_is_admin: bool,
    pub host: HostInfo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_ready_at: Option<String>,
    pub checked_at: String,
    /// Date de la dernière lecture COMPLÈTE réussie de la liste des services (RFC 3339), absente
    /// tant qu'aucune n'a réussi. `checked_at` date la sonde du moteur, pas la liste : c'est ce champ
    /// qui permet d'écrire « Liste du moteur d'il y a {âge} » (HRT-61, `BR-SVC-001`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_read_at: Option<String>,
    /// `true` quand la liste affichée n'est peut-être pas à jour : la dernière relecture a échoué, des
    /// conteneurs sont illisibles (leur dernière description est gardée), ou le suivi en direct est
    /// suspendu. Le client dit alors « Impossible de lire les services » et date la liste avec
    /// `list_read_at`. Absent (donc `false`) pour un agent qui ne le sait pas.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub list_degraded: bool,
}

/// L'état d'un service (`BR-SVC-004`). `starting`, `stopping` et `deploying` appartiennent aux
/// tranches suivantes : cette tranche ne les produit jamais.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceState {
    Running,
    Stopped,
    Starting,
    Stopping,
    Error,
    RestartLoop,
    Paused,
    Deploying,
    /// État du moteur que l'agent ne sait pas nommer, ou état que ce client ne connaît pas.
    #[serde(other)]
    Unknown,
}

/// Qui a créé le service (`BR-SVC-003`). Dans cette tranche, tout service est `external`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceOrigin {
    Hearth,
    External,
    /// Une origine que ce client ne connaît pas.
    #[serde(other)]
    Unknown,
}

/// Qui pilote le service. Absent dans cette tranche (jamais rempli).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceDriver {
    Engine,
    Supervisor,
    /// Un pilote que ce client ne connaît pas.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortProtocol {
    Tcp,
    Udp,
    Sctp,
    /// Un protocole que ce client ne connaît pas.
    #[serde(other)]
    Unknown,
}

/// Qui peut joindre un port publié.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortScope {
    /// Tout le réseau du serveur.
    All,
    /// Ce serveur seulement (adresse locale).
    Local,
    /// Une portée que ce client ne connaît pas.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServicePort {
    pub host_port: u16,
    pub container_port: u16,
    pub protocol: PortProtocol,
    pub scope: PortScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MountKind {
    /// Un dossier du serveur.
    Folder,
    /// Un volume géré par le moteur.
    Volume,
    /// Un genre de montage que ce client ne connaît pas.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceMount {
    pub kind: MountKind,
    /// Dossier du serveur, ou nom du volume.
    pub source: String,
    /// Chemin dans le service.
    pub target: String,
    pub read_only: bool,
}

/// L'image d'un service (`BR-SVC-022`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageInfo {
    /// Nom de l'image sans la version ; absent pour une image sans nom (« image sans nom {id} »).
    #[serde(default, rename = "ref", skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// Identifiant court de l'image (12 caractères hexadécimaux).
    pub short_id: String,
}

/// Comment le service redémarre (le détail d'un service, tranche 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestartMode {
    UnlessStopped,
    Always,
    OnFailure,
    None,
    /// Le système du serveur (systemd) relance le service, pas le moteur (conception technique,
    /// section 7). Jamais produit par la tranche 1 ; déclaré pour qu'un client de cette tranche
    /// lise un agent de la tranche 2.
    Supervisor,
    /// Une politique que ce client ne connaît pas.
    #[serde(other)]
    Unknown,
}

/// Verrou d'une action en cours. Jamais rempli dans cette tranche (lecture seule).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BusyInfo {
    pub kind: String,
    pub by: String,
    pub since: String,
}

/// Dernière action sur le service. Jamais rempli dans cette tranche.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastAction {
    pub id: String,
    pub kind: String,
    pub by: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
    #[serde(default)]
    pub forced: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
}

/// Un service de la liste. AUCUNE valeur de variable, aucun nom de variable, aucune commande.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceItem {
    /// Identité stable, opaque pour le client (`c_…`, plus tard `h_…` et `u_…`).
    pub key: String,
    pub name: String,
    pub origin: ServiceOrigin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver: Option<ServiceDriver>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Le moteur supprime ce service à son arrêt (`AutoRemove`).
    pub removed_on_stop: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compose_project: Option<String>,
    pub state: ServiceState,
    /// Depuis quand le service est dans cet état.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Créé mais jamais démarré (« Jamais démarré »).
    pub never_started: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i64>,
    /// Démarrages non demandés des 10 dernières minutes (`BR-SVC-006`).
    pub restarts_10m: u32,
    pub image: ImageInfo,
    pub ports: Vec<ServicePort>,
    /// Le service utilise directement le réseau du serveur : aucun port individuel.
    pub host_network: bool,
    pub mounts: Vec<ServiceMount>,
    pub restart: RestartMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub busy: Option<BusyInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_action: Option<LastAction>,
}

/// Le contenu de `GET /services` et de `services_snapshot` : l'état du moteur et tous les services,
/// arrêtés compris, triés par nom sans tenir compte de la casse (`BR-SVC-012`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServicesState {
    /// Croît à chaque changement et repart de zéro au redémarrage de l'agent : un `rev` plus petit
    /// que le dernier vu vaut remise à zéro.
    pub rev: u64,
    pub engine: EngineView,
    pub services: Vec<ServiceItem>,
}

/// Changement d'un lot : services ajoutés ou modifiés, services disparus, état du moteur s'il a changé.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServicesDelta {
    pub rev: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<EngineView>,
    pub upserts: Vec<ServiceItem>,
    pub removed: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> EngineView {
        EngineView {
            state: EngineState::Ready,
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
        }
    }

    #[test]
    fn the_engine_states_use_the_documented_codes() {
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
                serde_json::to_value(state).expect("json"),
                serde_json::json!(code)
            );
        }
    }

    #[test]
    fn an_engine_view_without_optional_fields_stays_small() {
        let mut view = engine();
        view.name = None;
        view.version = None;
        let json = serde_json::to_value(&view).expect("json");
        assert!(json.get("name").is_none());
        assert!(json.get("version").is_none());
        let back: EngineView = serde_json::from_value(json).expect("back");
        assert_eq!(back, view);
    }

    #[test]
    fn the_list_freshness_fields_are_optional_both_ways() {
        // Un agent ancien ne les envoie pas : le client lit « à jour, date inconnue ».
        let mut json = serde_json::to_value(engine()).expect("json");
        assert!(json.get("list_read_at").is_none());
        assert!(json.get("list_degraded").is_none());
        let old: EngineView = serde_json::from_value(json.clone()).expect("old agent");
        assert!(!old.list_degraded && old.list_read_at.is_none());
        // Un agent récent les envoie ; un client ancien (qui ignore les champs inconnus) les ignore.
        json["list_read_at"] = "2026-10-13T09:59:00Z".into();
        json["list_degraded"] = true.into();
        let new: EngineView = serde_json::from_value(json).expect("new agent");
        assert!(new.list_degraded);
        assert_eq!(new.list_read_at.as_deref(), Some("2026-10-13T09:59:00Z"));
    }

    #[test]
    fn a_service_item_declares_no_variable_nor_command_field() {
        let item = ServiceItem {
            key: "c_0123".into(),
            name: "web".into(),
            origin: ServiceOrigin::External,
            driver: None,
            unit: None,
            removed_on_stop: false,
            compose_project: None,
            state: ServiceState::Running,
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
        };
        let json = serde_json::to_value(&item).expect("json");
        for forbidden in ["env", "env_names", "command", "cmd"] {
            assert!(
                json.get(forbidden).is_none(),
                "{forbidden} ne voyage pas dans cette tranche"
            );
        }
        // Les champs des tranches suivantes sont absents tant qu'ils ne sont pas remplis.
        for later in ["driver", "unit", "busy", "last_action"] {
            assert!(json.get(later).is_none(), "{later}");
        }
        assert_eq!(json["image"]["ref"], "nginx");
    }

    #[test]
    fn a_state_lacking_future_fields_still_reads() {
        // Un agent ancien ne connaît pas `busy` ni `last_action` ; un client récent les lit absents.
        let text = r#"{"key":"c_1","name":"a","origin":"external","removed_on_stop":false,
            "state":"stopped","never_started":true,"restarts_10m":0,
            "image":{"short_id":"abc"},"ports":[],"host_network":false,"mounts":[],
            "restart":"none"}"#;
        let item: ServiceItem = serde_json::from_str(text).expect("item");
        assert!(item.busy.is_none() && item.last_action.is_none());
    }

    #[test]
    fn every_enum_the_client_reads_tolerates_a_value_it_does_not_know() {
        fn read<T: serde::de::DeserializeOwned>(code: &str) -> T {
            serde_json::from_value(serde_json::json!(code)).expect("une valeur inconnue est lue")
        }
        assert_eq!(read::<EngineState>("quantum"), EngineState::Unknown);
        assert_eq!(read::<InitSystem>("quantum"), InitSystem::Other);
        assert_eq!(read::<ServiceState>("quantum"), ServiceState::Unknown);
        assert_eq!(read::<ServiceOrigin>("quantum"), ServiceOrigin::Unknown);
        assert_eq!(read::<ServiceDriver>("quantum"), ServiceDriver::Unknown);
        assert_eq!(read::<PortProtocol>("quantum"), PortProtocol::Unknown);
        assert_eq!(read::<PortScope>("quantum"), PortScope::Unknown);
        assert_eq!(read::<MountKind>("quantum"), MountKind::Unknown);
        assert_eq!(read::<RestartMode>("quantum"), RestartMode::Unknown);
        // La valeur que la tranche 2 produit pour un service relancé par le système du serveur.
        assert_eq!(read::<RestartMode>("supervisor"), RestartMode::Supervisor);
        assert_eq!(
            serde_json::to_value(RestartMode::Supervisor).expect("json"),
            serde_json::json!("supervisor")
        );
    }

    #[test]
    fn a_whole_state_from_a_newer_agent_still_reads() {
        // Un agent de la tranche suivante : un état de moteur, un état de service, une origine, un
        // pilote, un protocole, une portée, un genre de montage et une politique que ce client ne
        // connaît pas. Rien de cela ne fait perdre l'instantané.
        let text = r#"{"rev":7,
            "engine":{"state":"hibernating","min_version":"20.10","agent_account":"root",
              "agent_is_admin":true,"checked_at":"2026-10-13T10:00:00Z",
              "host":{"os_id":"debian","os_like":[],"init":"runit","declarative":false}},
            "services":[{"key":"c_1","name":"a","origin":"imported","driver":"swarm",
              "removed_on_stop":false,"state":"migrating","never_started":false,"restarts_10m":0,
              "image":{"short_id":"abc"},
              "ports":[{"host_port":1,"container_port":2,"protocol":"quic","scope":"vpn"}],
              "host_network":false,
              "mounts":[{"kind":"nfs","source":"s","target":"t","read_only":true}],
              "restart":"supervisor"}]}"#;
        let state: ServicesState = serde_json::from_str(text).expect("instantané");
        assert_eq!(state.engine.state, EngineState::Unknown);
        assert_eq!(state.engine.host.init, InitSystem::Other);
        let item = &state.services[0];
        assert_eq!(item.state, ServiceState::Unknown);
        assert_eq!(item.origin, ServiceOrigin::Unknown);
        assert_eq!(item.driver, Some(ServiceDriver::Unknown));
        assert_eq!(item.ports[0].protocol, PortProtocol::Unknown);
        assert_eq!(item.ports[0].scope, PortScope::Unknown);
        assert_eq!(item.mounts[0].kind, MountKind::Unknown);
        assert_eq!(item.restart, RestartMode::Supervisor);
    }
}
