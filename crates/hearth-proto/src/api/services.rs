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

use crate::lenient::Items;

/// Capacité annoncée par `GET /hello` (`capabilities`) quand l'agent sert le sujet `services`.
/// Un client n'envoie le sujet `services` (ni aucune route des services) qu'à un agent qui l'annonce :
/// un sujet inconnu fait refuser TOUT le message `subscribe`, `metrics` compris.
pub const CAPABILITY_SERVICES: &str = "services";

/// Capacité annoncée par `GET /hello` quand l'agent connaît le droit « gérer les services » : `rights`
/// est porté par la description des comptes et le serveur refuse les actes de service sans lui
/// (HRT-76, ADR-0041, BR-GRANT-016). Un agent qui ne l'annonce pas n'a pas ce droit : le client le
/// traite comme non accordé pour tous et n'offre aucune action de service.
pub const CAPABILITY_SERVICE_RIGHTS: &str = "service-rights";

/// Capacité annoncée par `GET /hello` quand l'agent ne publie que les services créés par Hearth et les
/// services adoptés (HRT-79, ADR-0043, BR-ADOPT-001) : les autres conteneurs sont comptés
/// (`EngineView::external_count`), jamais détaillés. Un agent qui ne l'annonce pas publie tous les
/// conteneurs du serveur (versions d'avant l'adoption). L'annonce ne dit pas que l'adoption est
/// possible : l'adoption se désactive dans la configuration de l'agent (`[services] adoption`), la
/// réponse des candidats le dit alors par un refus `adoption_disabled`.
pub const CAPABILITY_SERVICE_ADOPTION: &str = "service-adoption";

/// `details.reason` des refus d'adoption (HRT-80, revue de la PR 89) : `409 CONFLICT` pour les deux premiers,
/// `422 VALIDATION_ERROR` (avec `details.field = "keys"`) pour les trois autres. Additif : un agent plus
/// ancien ne donne pas de raison pour la validation, un client qui lit une raison qu'il ne connaît pas la
/// traite comme inconnue.
pub mod adoption_refusal {
    /// `[services] adoption = false` (BR-ADOPT-020).
    pub const ADOPTION_DISABLED: &str = "adoption_disabled";
    /// Le moteur n'est pas prêt (BR-ADOPT-026) ; `details.engine` dit son état.
    pub const ENGINE_NOT_READY: &str = "engine_not_ready";
    /// Aucune clé dans la demande.
    pub const KEYS_EMPTY: &str = "keys_empty";
    /// Plus de `MAX_ADOPTIONS` clés dans la demande.
    pub const KEYS_TOO_MANY: &str = "keys_too_many";
    /// Une clé deux fois dans la demande.
    pub const KEYS_DUPLICATE: &str = "keys_duplicate";
}

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
    /// tant qu'aucune n'a réussi depuis que la liste a été vidée. Information seulement : elle ne
    /// dit PAS depuis quand la liste est périmée (une liste suivie en direct par les événements est
    /// à jour sans nouvelle lecture). Pour l'âge, utiliser `list_current_at` (HRT-72).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_read_at: Option<String>,
    /// `true` quand la liste affichée n'est peut-être pas à jour : la dernière relecture a échoué, des
    /// conteneurs sont illisibles (leur dernière description est gardée), le suivi en direct est
    /// suspendu, ou la liste n'a pas encore été relue depuis que le moteur a (re)démarré ou est
    /// revenu (HRT-72). Le client dit alors « Impossible de lire les services » et date la liste avec
    /// `list_current_at`. Absent (donc `false`) pour un agent qui ne le sait pas.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub list_degraded: bool,
    /// Heure de l'agent au moment où cette vue a été fabriquée (RFC 3339), HRT-72. Sa présence dit
    /// que l'agent connaît `list_current_at` : avec `at` présent, `list_current_at` absent veut dire
    /// « aucune liste n'a été lue » ; avec `at` absent (agent plus ancien), la date est inconnue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    /// Dernier moment où la liste était à jour (RFC 3339, HRT-72) : `at` quand elle l'est
    /// maintenant, sinon l'instant où l'agent a vu qu'elle cessait de l'être (lecture en échec,
    /// conteneur illisible, suivi suspendu, moteur plus joignable). Absent quand aucune liste n'a
    /// été lue. L'âge est `at - list_current_at` (deux dates de l'agent : l'écart d'horloge avec le
    /// client n'y entre pas, à ramener à zéro s'il est négatif), augmenté du temps écoulé chez le
    /// client depuis la réception du message, mesuré par SON horloge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_current_at: Option<String>,
    /// Nombre de conteneurs que le moteur connaît, arrêtés compris, qui ne sont ni créés par Hearth ni
    /// adoptés (HRT-79, BR-ADOPT-002) : un nombre, jamais un détail. Absent vaut 0 (agent d'avant
    /// l'adoption, qui publie tous les conteneurs, ou serveur sans conteneur non géré).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub external_count: u32,
}

fn is_zero(value: &u32) -> bool {
    *value == 0
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

/// Un texte qui ne s'affiche jamais : nom ou valeur d'une variable d'environnement, argument de la
/// commande de lancement. Sur le fil c'est un simple texte JSON ; dans le code, `Debug` ne montre
/// rien (`Concealed(***)`), il n'y a pas de `Display`, et la lecture passe par [`Concealed::expose`],
/// facile à repérer en revue. Un nom, une valeur ou une commande ne doivent jamais entrer dans une
/// trace, un message d'erreur, un journal ni la base (ADR-0037).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Concealed(String);

impl Concealed {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    /// Le texte en clair. À n'utiliser que pour le rendre à l'écran ou le comparer.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Concealed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Concealed(***)")
    }
}

/// Qui a adopté un service, et quand (HRT-79, ADR-0043). Absent d'un service créé par Hearth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdoptionInfo {
    /// Le compte qui a adopté le service, tel que l'agent l'a figé.
    pub by: String,
    /// Date de l'adoption (RFC 3339, UTC).
    pub at: String,
}

/// Un service de la liste, avec son détail. AUCUNE valeur de variable d'environnement.
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
    /// Ports publiés au-delà de la borne de 128, non listés (`BR-SVC-021`). Absent vaut 0.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub ports_omitted: u32,
    /// Le service utilise directement le réseau du serveur : aucun port individuel.
    pub host_network: bool,
    pub mounts: Vec<ServiceMount>,
    /// Montages au-delà de la borne de 64, non listés (`BR-SVC-021`). Absent vaut 0.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub mounts_omitted: u32,
    pub restart: RestartMode,
    /// Noms des variables d'environnement du service, y compris celles de l'image (BR-SVC-008). Jamais
    /// de valeur : elles ne se lisent que par `GET /services/{key}/env` (comptes qui ont le droit « gérer les services »). Plafonné
    /// (`env_names_omitted` compte le reste). Absent d'un agent de la tranche 1 : liste vide.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env_names: Vec<Concealed>,
    /// Variables au-delà de la borne, non listées. Absent vaut 0.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub env_names_omitted: u32,
    /// La commande lancée (entrée puis arguments) ; absente quand le moteur n'en annonce aucune
    /// (BR-SVC-007 : « Commande de l'image »).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<Vec<Concealed>>,
    /// Délai d'arrêt du service en secondes (BR-SVC-007) : celui du service (pilote `engine`) ou de son
    /// unité (pilote `supervisor`), 10 par défaut pour un conteneur qui n'en déclare pas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grace_s: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub busy: Option<BusyInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_action: Option<LastAction>,
    /// Présent pour un service adopté (HRT-79) : qui l'a adopté et quand. Absent pour un service créé
    /// par Hearth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adopted: Option<AdoptionInfo>,
    /// `true` : ce service est adopté et le compte qui lit n'a pas le droit « gérer les services » ;
    /// sa commande de lancement et les noms de ses variables sont retirés de ce message
    /// (`env_names`, `command` sont vides ou absents) et ne signifient donc pas « aucune » (HRT-79,
    /// BR-ADOPT-015). Absent vaut `false`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub details_reserved: bool,
}

/// Une variable d'environnement révélée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvVariable {
    pub name: Concealed,
    pub value: Concealed,
}

/// La réponse de `GET /services/{key}/env` (comptes qui ont le droit « gérer les services » seulement, quel que
/// soit leur rôle, BR-SVC-009, BR-GRANT-003) : les variables
/// du service avec leur valeur, dans l'ordre du moteur. Réponse jamais stockable
/// (`Cache-Control: no-store`) ; son contenu n'entre dans aucune trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvReveal {
    pub variables: Vec<EnvVariable>,
}

/// Au plus ce nombre de services par acte d'adoption (BR-ADOPT-007).
pub const MAX_ADOPTIONS: usize = 50;

/// Un conteneur non géré proposé à l'adoption (BR-ADOPT-006). C'est TOUT ce que la lecture des candidats
/// expose d'un conteneur : de quoi choisir, rien de plus. Ni variable, ni commande, ni port, ni dossier
/// monté, ni journal : avant l'adoption, aucun autre détail n'existe pour le client. La clé `c_` porte
/// l'identifiant COMPLET du conteneur (elle en est la preuve d'identité, nécessaire à l'acte) ; la clé
/// `u_` porte le nom de l'unité.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalContainer {
    /// La clé que l'acte d'adoption enverra (`u_<unité>` ou `c_<identifiant>`), opaque pour le client.
    pub key: String,
    pub name: String,
    pub image: ImageInfo,
    /// L'unité du système qui le lance, absente sans unité : l'adoption ne tient alors que pour la vie de
    /// ce conteneur (BR-ADOPT-017).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    pub state: ServiceState,
}

/// La réponse de `GET /services/external` (comptes qui ont le droit « gérer les services » seulement) :
/// les candidats triés par nom sans tenir compte de la casse, et l'état du moteur qui explique une liste
/// vide (`BR-ADOPT-026`). Jamais diffusée par le flux, jamais gardée par l'agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalContainers {
    pub engine: EngineState,
    pub containers: Vec<ExternalContainer>,
}

/// Corps de `POST /services/adopt` : les clés cochées, 50 au plus (`MAX_ADOPTIONS`), sans doublon. Le
/// membre `reauth` de la confirmation voyage à côté (voir `api::reauth`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdoptRequest {
    pub keys: Vec<String>,
}

/// Le résultat d'une clé d'adoption (`BR-ADOPT-010`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdoptionResult {
    /// Adopté.
    Adopted,
    /// Déjà adopté (par `AdoptionItem::by`), pas touché.
    AlreadyAdopted,
    /// Aucun conteneur de cette clé : disparu depuis la liste, ou jamais existé. L'agent ne distingue
    /// jamais l'un de l'autre (BR-ADOPT-004).
    NotFound,
    /// L'agent n'a pas pu écrire (base ou journal indisponibles) : rien n'a été adopté pour cette clé.
    Failed,
    /// Un résultat que ce client ne connaît pas (agent plus récent) : traité comme non adopté.
    #[serde(other)]
    Unknown,
}

/// Le résultat d'une clé de `POST /services/adopt`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdoptionItem {
    pub key: String,
    pub result: AdoptionResult,
    /// Le compte qui avait déjà adopté ce service (`already_adopted` seulement) : déjà publié à tous les
    /// comptes par `ServiceItem::adopted`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
}

/// La réponse de `POST /services/adopt` : un résultat par clé, dans l'ordre de la demande.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdoptResponse {
    /// Combien de services ont été adoptés par cet acte.
    pub adopted: u32,
    pub results: Vec<AdoptionItem>,
}

/// La réponse de `POST /services/{key}/release`. `released: false` : le service n'était déjà plus adopté
/// (aucun effet, aucune entrée de journal, `BR-ADOPT-013`) ; l'agent ne dit jamais pourquoi (inconnu,
/// jamais adopté ou déjà relâché se répondent pareil).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseResponse {
    pub released: bool,
}

/// Le contenu de `GET /services` et de `services_snapshot` : l'état du moteur et tous les services,
/// arrêtés compris, triés par nom sans tenir compte de la casse (`BR-SVC-012`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServicesState {
    /// Croît à chaque changement et repart de zéro au redémarrage de l'agent : un `rev` plus petit
    /// que le dernier vu vaut remise à zéro.
    pub rev: u64,
    pub engine: EngineView,
    /// Un service illisible par cette version est écarté et compté (BR-COMPAT-001 point 10).
    pub services: Items<ServiceItem>,
}

/// Changement d'un lot : services ajoutés ou modifiés, services disparus, état du moteur s'il a changé.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServicesDelta {
    pub rev: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<EngineView>,
    /// Un service illisible par cette version est écarté et compté (BR-COMPAT-001 point 10).
    pub upserts: Items<ServiceItem>,
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
            at: None,
            list_current_at: None,
            external_count: 0,
        }
    }

    fn sample_item() -> ServiceItem {
        ServiceItem {
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
            ports_omitted: 0,
            host_network: false,
            mounts: vec![],
            mounts_omitted: 0,
            restart: RestartMode::None,
            env_names: vec![],
            env_names_omitted: 0,
            command: None,
            grace_s: None,
            busy: None,
            last_action: None,
            adopted: None,
            details_reserved: false,
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
    fn the_list_age_fields_tell_a_never_read_list_from_an_old_agent() {
        // Agent ancien : ni `at` ni `list_current_at` (date inconnue).
        let json = serde_json::to_value(engine()).expect("json");
        assert!(json.get("at").is_none() && json.get("list_current_at").is_none());
        let old: EngineView = serde_json::from_value(json).expect("old agent");
        assert!(old.at.is_none() && old.list_current_at.is_none());
        // Agent récent, aucune liste lue : `at` présent, `list_current_at` absent.
        let mut never = engine();
        never.at = Some("2026-10-13T10:00:05Z".into());
        let json = serde_json::to_value(&never).expect("json");
        assert!(json.get("list_current_at").is_none());
        let back: EngineView = serde_json::from_value(json).expect("never read");
        assert_eq!(back.at.as_deref(), Some("2026-10-13T10:00:05Z"));
        assert!(back.list_current_at.is_none());
        // Agent récent, liste datée : les deux dates voyagent.
        never.list_current_at = Some("2026-10-13T10:00:00Z".into());
        let back: EngineView =
            serde_json::from_value(serde_json::to_value(&never).expect("json")).expect("dated");
        assert_eq!(back, never);
    }

    #[test]
    fn the_adoption_fields_are_absent_until_filled_and_old_clients_ignore_them() {
        // Un agent d'avant l'adoption (ou un service créé par Hearth) : ni compte de non gérés, ni
        // repère d'adoption, ni réserve ; le client lit le même JSON qu'avant (HRT-79).
        let engine_json = serde_json::to_value(engine()).expect("json");
        assert!(engine_json.get("external_count").is_none());
        let old: EngineView = serde_json::from_value(engine_json).expect("old agent");
        assert_eq!(old.external_count, 0);
        let item_json = serde_json::to_value(sample_item()).expect("json");
        assert!(item_json.get("adopted").is_none() && item_json.get("details_reserved").is_none());
        let old: ServiceItem = serde_json::from_value(item_json).expect("old agent");
        assert!(old.adopted.is_none() && !old.details_reserved);
        // Un agent récent les envoie ; elles font l'aller-retour.
        let mut view = engine();
        view.external_count = 3;
        let back: EngineView =
            serde_json::from_value(serde_json::to_value(&view).expect("json")).expect("new");
        assert_eq!(back.external_count, 3);
        let mut item = sample_item();
        item.adopted = Some(AdoptionInfo {
            by: "marie".into(),
            at: "2026-10-13T10:00:00Z".into(),
        });
        item.details_reserved = true;
        let back: ServiceItem =
            serde_json::from_value(serde_json::to_value(&item).expect("json")).expect("new");
        assert_eq!(back, item);
        assert_eq!(CAPABILITY_SERVICE_ADOPTION, "service-adoption");
    }

    #[test]
    fn the_omitted_counters_are_optional_both_ways() {
        let mut item = serde_json::to_value(sample_item()).expect("json");
        assert!(item.get("ports_omitted").is_none() && item.get("mounts_omitted").is_none());
        assert!(item.get("env_names_omitted").is_none());
        let old: ServiceItem = serde_json::from_value(item.clone()).expect("old agent");
        assert_eq!((old.ports_omitted, old.mounts_omitted), (0, 0));
        assert_eq!(old.env_names_omitted, 0);
        item["ports_omitted"] = 2872.into();
        item["mounts_omitted"] = 436.into();
        item["env_names_omitted"] = 9.into();
        let new: ServiceItem = serde_json::from_value(item).expect("new agent");
        assert_eq!((new.ports_omitted, new.mounts_omitted), (2872, 436));
        assert_eq!(new.env_names_omitted, 9);
    }

    #[test]
    fn the_detail_fields_are_absent_until_filled_and_never_show_in_debug() {
        // Un service sans détail ne porte aucun des champs de la tranche 2 : un client de la
        // tranche 1 lit le même JSON qu'avant.
        let json = serde_json::to_value(sample_item()).expect("json");
        for later in [
            "env_names",
            "env_names_omitted",
            "command",
            "grace_s",
            "driver",
            "unit",
            "busy",
            "last_action",
        ] {
            assert!(json.get(later).is_none(), "{later}");
        }
        assert_eq!(json["image"]["ref"], "nginx");
        // Avec détail : les noms et la commande voyagent (jamais une valeur), `Debug` ne les montre pas.
        let mut item = sample_item();
        item.env_names = vec![Concealed::new("SENTINEL_NAME")];
        item.command = Some(vec![
            Concealed::new("run"),
            Concealed::new("--token=SENTINEL"),
        ]);
        item.grace_s = Some(120);
        let json = serde_json::to_value(&item).expect("json");
        assert_eq!(json["env_names"], serde_json::json!(["SENTINEL_NAME"]));
        assert_eq!(
            json["command"],
            serde_json::json!(["run", "--token=SENTINEL"])
        );
        assert_eq!(json["grace_s"], 120);
        let back: ServiceItem = serde_json::from_value(json).expect("back");
        assert_eq!(back, item);
        let shown = format!("{item:?} {item:#?}");
        assert!(!shown.contains("SENTINEL"), "{shown}");
        assert!(shown.contains("Concealed(***)"));
    }

    #[test]
    fn a_revealed_value_never_shows_in_debug_but_travels_as_plain_json() {
        let reveal = EnvReveal {
            variables: vec![EnvVariable {
                name: Concealed::new("DB_PASSWORD"),
                value: Concealed::new("SENTINEL-VALUE"),
            }],
        };
        let shown = format!("{reveal:?}");
        assert!(
            !shown.contains("SENTINEL") && !shown.contains("DB_PASSWORD"),
            "{shown}"
        );
        let json = serde_json::to_value(&reveal).expect("json");
        assert_eq!(json["variables"][0]["value"], "SENTINEL-VALUE");
        let back: EnvReveal = serde_json::from_value(json).expect("back");
        assert_eq!(back, reveal);
        assert_eq!(back.variables[0].value.expose(), "SENTINEL-VALUE");
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

    #[test]
    fn the_adoption_messages_tolerate_what_a_newer_agent_adds() {
        // Un résultat inconnu est lu comme tel, jamais comme une erreur de tout le message.
        let response: AdoptResponse = serde_json::from_value(serde_json::json!({
            "adopted": 1,
            "results": [
                { "key": "c_a", "result": "adopted", "extra": true },
                { "key": "c_b", "result": "already_adopted", "by": "marie" },
                { "key": "c_c", "result": "something_new" },
            ],
        }))
        .expect("réponse");
        assert_eq!(response.results[0].result, AdoptionResult::Adopted);
        assert_eq!(response.results[1].by.as_deref(), Some("marie"));
        assert_eq!(response.results[2].result, AdoptionResult::Unknown);
        let candidates: ExternalContainers = serde_json::from_value(serde_json::json!({
            "engine": "ready",
            "containers": [{
                "key": "u_x.service", "name": "x", "image": { "short_id": "ab" },
                "state": "running", "later": 1
            }],
        }))
        .expect("candidats");
        assert_eq!(candidates.containers[0].unit, None);
        // Ce que la lecture des candidats expose d'un conteneur, et rien d'autre.
        let value = serde_json::to_value(&candidates.containers[0]).unwrap();
        let mut fields: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        fields.sort_unstable();
        assert_eq!(fields, ["image", "key", "name", "state"]);
    }
}
