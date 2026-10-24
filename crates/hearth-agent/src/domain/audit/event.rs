//! Une entrée du journal (BR-AUDIT-002) et ce qui peut y entrer.
//!
//! **Aucun secret ne peut entrer dans un événement (BR-AUDIT-005)** : ce n'est pas une
//! convention, c'est la forme des types. Un événement n'a aucun champ de texte libre :
//! - le compte est un `Username` (jamais l'identifiant saisi à la connexion, qui peut être un mot
//!   de passe tapé au mauvais endroit : il n'est pas retenu) ;
//! - l'action, le résultat et la raison sont des énumérations dont chaque variante a un texte fixe ;
//! - la cible est un compte (`Username`) ou le motif d'une route (`&'static str`) ;
//! - l'origine ne porte que l'adresse vue par l'agent et le nom du poste annoncé par le client,
//!   nettoyés et bornés (`ClientName`, `Origin::client`).
//!
//! Les types du domaine qui portent un secret (`Secret`, `PlainPassword`, `SessionToken`) n'ont
//! aucun chemin vers ces champs.

use time::OffsetDateTime;

use super::action::AuditAction;
use crate::domain::accounts::{Role, Username};
use crate::domain::text::strip_unsafe;
use crate::domain::trust::TrialKind;

/// Longueur maximale (en caractères) du nom du poste retenu.
pub const MAX_CLIENT_NAME: usize = 128;
/// Longueur maximale d'une adresse (une adresse IPv6 écrite tient en 45 caractères).
const MAX_ADDR: usize = 64;

/// Nom du poste annoncé par le client (`X-Hearth-Client`), nettoyé : sans caractère de contrôle,
/// de format ni séparateur de ligne, sans espace autour, 128 caractères au plus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientName(String);

impl ClientName {
    /// `None` si rien ne reste une fois nettoyé : le poste n'est pas identifié.
    pub fn parse(raw: &str) -> Option<Self> {
        let cleaned: String = strip_unsafe(raw).chars().take(MAX_CLIENT_NAME).collect();
        let cleaned = cleaned.trim();
        (!cleaned.is_empty()).then(|| Self(cleaned.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// D'où vient l'action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OriginKind {
    Client,
    CommandLine,
    Assistant,
    /// L'agent lui-même, sans appelant : fin d'une alerte, sortie automatique, suspension et reprise
    /// du mode attaque (HRT-25).
    System,
}

impl OriginKind {
    pub fn code(self) -> &'static str {
        match self {
            Self::Client => "client",
            Self::CommandLine => "cli",
            Self::Assistant => "assistant",
            Self::System => "system",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        [
            Self::Client,
            Self::CommandLine,
            Self::Assistant,
            Self::System,
        ]
        .into_iter()
        .find(|kind| kind.code() == code)
    }
}

/// Texte d'une origine (BR-AUDIT-002) : « {adresse} ({poste}) », « {adresse} (inconnu) » si le
/// poste n'est pas identifié, « ligne de commande du serveur », « assistant ».
fn describe(kind: OriginKind, name: Option<&str>, addr: Option<&str>) -> String {
    match kind {
        OriginKind::CommandLine => "ligne de commande du serveur".to_owned(),
        OriginKind::Assistant => "assistant".to_owned(),
        OriginKind::System => "agent (automatique)".to_owned(),
        OriginKind::Client => format!(
            "{} ({})",
            addr.filter(|addr| !addr.is_empty())
                .unwrap_or("adresse inconnue"),
            name.unwrap_or("inconnu")
        ),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// Un client sur le réseau : l'adresse de la connexion, jamais un en-tête de mandataire.
    Client {
        name: Option<ClientName>,
        addr: String,
    },
    /// Une sous-commande lancée sur le serveur.
    CommandLine,
    /// L'assistant.
    Assistant,
    /// L'agent lui-même (HRT-25) : aucun appelant, aucune adresse.
    System,
}

impl Origin {
    /// Origine d'une requête reçue : nom du poste (en-tête) et adresse de la connexion.
    pub fn client(name: Option<&str>, addr: &str) -> Self {
        Self::Client {
            name: name.and_then(ClientName::parse),
            addr: strip_unsafe(addr).chars().take(MAX_ADDR).collect(),
        }
    }

    pub fn kind(&self) -> OriginKind {
        match self {
            Self::Client { .. } => OriginKind::Client,
            Self::CommandLine => OriginKind::CommandLine,
            Self::Assistant => OriginKind::Assistant,
            Self::System => OriginKind::System,
        }
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Client { name, .. } => name.as_ref().map(ClientName::as_str),
            Self::CommandLine | Self::Assistant | Self::System => None,
        }
    }

    pub fn addr(&self) -> Option<&str> {
        match self {
            Self::Client { addr, .. } => Some(addr),
            Self::CommandLine | Self::Assistant | Self::System => None,
        }
    }

    /// Le texte lu par l'administrateur.
    pub fn describe(&self) -> String {
        describe(self.kind(), self.name(), self.addr())
    }
}

/// Qui agit : un compte (absent pour la ligne de commande et pour une connexion refusée) et
/// l'origine de la demande.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    pub account: Option<Username>,
    pub origin: Origin,
}

impl Actor {
    pub fn new(account: Option<Username>, origin: Origin) -> Self {
        Self { account, origin }
    }

    /// Une sous-commande du serveur : pas de compte, origine « ligne de commande ».
    pub fn command_line() -> Self {
        Self::new(None, Origin::CommandLine)
    }

    /// L'agent lui-même : pas de compte, origine « système » (HRT-25).
    pub fn system() -> Self {
        Self::new(None, Origin::System)
    }
}

/// Sur quoi porte l'action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    None,
    /// Un compte, par son identifiant.
    Account(Username),
    /// Un compte dont le rôle devient celui-ci.
    AccountRole(Username, Role),
    /// Une route de l'API, par son motif (`/accounts/{id}`) : quand la cible n'est pas connue.
    Route(&'static str),
    /// La version visée d'une mise à jour de l'agent (`0.2.0`).
    AgentVersion(String),
    /// Un poste de confiance, par le nom qu'il a annoncé (nettoyé et borné comme l'origine).
    Device(ClientName),
    /// Début ou fin d'un épisode d'alerte sur un identifiant (HRT-24).
    Alert(AlertPhase),
    /// L'essai unique d'un critère, en mode attaque (HRT-25) : l'adresse ou la clé, jamais leur valeur.
    Trial(TrialKind),
    /// Un service de conteneurs, par son nom (nettoyé et borné comme un nom de poste) : jamais le nom
    /// ni la valeur d'une de ses variables (HRT-63).
    Service(ClientName),
    /// Un service adopté ou rendu, par son nom et l'unité du système qui le lance s'il en a une (HRT-80,
    /// BR-ADOPT-019) : ce sont les SEULS détails d'un conteneur que le journal écrit, et seulement au
    /// moment où l'acte réussit (le nom devient visible de tous à l'adoption, pas avant).
    ServiceUnit(ClientName, Option<ClientName>),
    /// Une adoption qui change d'état sans qu'aucun compte agisse (HRT-80, BR-ADOPT-027, BR-ADOPT-019) :
    /// elle devient « à confirmer » ou elle tombe. Le service est désigné par sa CLÉ (`u_...`, `c_...`),
    /// déjà publiée à tous les comptes tant qu'il était adopté, jamais par le nom ni l'image du
    /// conteneur : le journal se lit sans le droit de gérer les services.
    ServiceAdoption(ClientName, AdoptionCause),
    /// Les sessions d'un compte ont été fermées par l'administration (HRT-133, BR-ACCT-019) : le compte
    /// visé, le nombre de sessions, si le compte visé avait le droit de gérer les services et si l'auteur
    /// l'avait (`None` : la ligne de commande du serveur, qui n'a pas de compte). Jamais d'adresse.
    SessionsClosed {
        account: Username,
        closed: u64,
        target_holder: bool,
        author_holder: Option<bool>,
    },
    /// Un compte a annulé une opération de service ou tranché ce qu'elle attendait (HRT-144, ADR-0045) : le
    /// nom du service et le geste. L'entrée est écrite sous l'action du GENRE de l'opération. Jamais un
    /// identifiant de conteneur, un chemin du serveur, une valeur ni un nom de variable.
    ServiceOperation {
        service: ClientName,
        gesture: OperationGesture,
    },
    /// Un service démarré, arrêté ou redémarré (HRT-64, BR-LIFE-018), avec ce que l'issue a de particulier
    /// (liste fermée). Le nom du service est le seul détail d'un conteneur que l'entrée écrit.
    ServiceLifecycle {
        service: ClientName,
        note: LifecycleNote,
    },
    /// Un déploiement terminé, réussi ou non (HRT-68, BR-DEPLOY-025) : le nom du service, l'image et sa version,
    /// les ports du SERVEUR, le NOMBRE de variables et si une commande est fixée. Jamais le nom ni la valeur d'une
    /// variable, jamais la commande, jamais un chemin (les dossiers gérés par Hearth n'y figurent pas).
    ServiceDeploy {
        service: ClientName,
        image: ClientName,
        host_ports: Vec<u16>,
        env_count: u32,
        custom_command: bool,
        /// Le service s'est arrêté dans les 10 secondes qui ont suivi son lancement : déploiement réussi, service
        /// conservé (BR-DEPLOY-020).
        stopped_right_after_start: bool,
    },
    /// Un serveur de jeu installé depuis une fiche, réussi ou non (HRT-101, BR-FICHE-032) : la fiche et sa
    /// version, l'image (et son empreinte), les ports du SERVEUR, le NOMBRE de réponses, les ressources choisies
    /// et le titre des contrats acceptés. **Aucune valeur de réponse, aucun nom de variable, aucun secret
    /// tiré, aucun chemin** : le type n'a pas de champ pour eux.
    GameInstall {
        service: ClientName,
        fiche: ClientName,
        version: u32,
        image: ClientName,
        host_ports: Vec<u16>,
        answers: u32,
        cpu_millis: u32,
        memory_bytes: u64,
        agreements: Vec<ClientName>,
    },
    /// Une fiche visée par une demande refusée avant de connaître le service (fiche inconnue, empreinte
    /// changée, contrat non accepté...) : son identifiant, nettoyé et borné.
    Fiche(ClientName),
    /// Une fiche écartée au chargement de l'agent : son dossier et l'invariant violé (jamais un contenu).
    FicheRefused {
        dir: ClientName,
        invariant: ClientName,
    },
}

/// Ce qu'une issue de démarrage, d'arrêt ou de redémarrage a de particulier (texte fixe).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleNote {
    /// Rien à signaler.
    Plain,
    /// Le service ne s'est pas arrêté dans son délai : il a été coupé de force (BR-LIFE-009).
    Forced,
}

/// Ce qu'un compte a fait d'une opération de service (liste fermée : le texte de l'entrée est fixe).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationGesture {
    /// L'opération a été annulée par le compte.
    Cancelled,
    /// Le compte a pris cette décision (le libellé du choix, en français).
    Decided(&'static str),
}

/// Pourquoi une adoption devient « à confirmer » ou tombe sans qu'aucun compte agisse (texte fixe).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdoptionCause {
    /// L'unité n'a pas été relue à un passage (au-delà de la borne) : l'adoption est « à confirmer ».
    UnitNotRead,
    /// Le délai de confirmation (24 heures) est écoulé.
    Expired,
    /// Le système ne connaît plus l'unité et aucun conteneur ne la porte.
    UnitGone,
    /// Le conteneur d'un service adopté sans unité n'existe plus.
    ContainerGone,
}

impl AdoptionCause {
    fn text(self) -> &'static str {
        match self {
            Self::UnitNotRead => "unité plus relue, à confirmer sous 24 h",
            Self::Expired => "non confirmée sous 24 h",
            Self::UnitGone => "unité disparue",
            Self::ContainerGone => "conteneur disparu",
        }
    }
}

/// Le moment de l'épisode d'alerte que l'entrée consigne.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertPhase {
    Started,
    Ended,
}

impl Target {
    pub fn text(&self) -> Option<String> {
        match self {
            Self::None => None,
            Self::Account(username) => Some(username.to_string()),
            Self::AccountRole(username, role) => Some(format!("{username} ({})", role.label())),
            Self::Route(pattern) => Some((*pattern).to_owned()),
            Self::AgentVersion(version) => Some(format!("version {version}")),
            Self::Device(name) => Some(format!("poste {}", name.as_str())),
            Self::Alert(AlertPhase::Started) => Some("début de l'alerte".to_owned()),
            Self::Alert(AlertPhase::Ended) => {
                Some("fin de l'alerte (levée par l'agent)".to_owned())
            }
            Self::Trial(TrialKind::Address) => Some("essai sur l'adresse retenue".to_owned()),
            Self::Trial(TrialKind::Key) => Some("essai sur la clé du poste".to_owned()),
            Self::Service(name) => Some(format!("service {}", name.as_str())),
            Self::ServiceUnit(name, None) => Some(format!("service {}", name.as_str())),
            Self::ServiceUnit(name, Some(unit)) => {
                Some(format!("service {} ({})", name.as_str(), unit.as_str()))
            }
            Self::ServiceOperation {
                service,
                gesture: OperationGesture::Cancelled,
            } => Some(format!("service {} : opération annulée", service.as_str())),
            Self::ServiceOperation {
                service,
                gesture: OperationGesture::Decided(choice),
            } => Some(format!(
                "service {} : décision « {choice} »",
                service.as_str()
            )),
            Self::ServiceLifecycle {
                service,
                note: LifecycleNote::Plain,
            } => Some(format!("service {}", service.as_str())),
            Self::ServiceLifecycle {
                service,
                note: LifecycleNote::Forced,
            } => Some(format!("service {} (arrêt forcé)", service.as_str())),
            Self::ServiceDeploy {
                service,
                image,
                host_ports,
                env_count,
                custom_command,
                stopped_right_after_start,
            } => {
                let mut text = format!("service {} : image {}", service.as_str(), image.as_str());
                if !host_ports.is_empty() {
                    let ports: Vec<String> = host_ports.iter().map(u16::to_string).collect();
                    text.push_str(&format!(", ports {}", ports.join(", ")));
                }
                match env_count {
                    0 => {}
                    1 => text.push_str(", 1 variable"),
                    n => text.push_str(&format!(", {n} variables")),
                }
                if *custom_command {
                    text.push_str(", commande personnalisée");
                }
                if *stopped_right_after_start {
                    text.push_str(", arrêté juste après le démarrage");
                }
                Some(text)
            }
            Self::GameInstall {
                service,
                fiche,
                version,
                image,
                host_ports,
                answers,
                cpu_millis,
                memory_bytes,
                agreements,
            } => {
                let mut text = format!(
                    "service {} : fiche {} (version {version}), image {}",
                    service.as_str(),
                    fiche.as_str(),
                    image.as_str()
                );
                if !host_ports.is_empty() {
                    let ports: Vec<String> = host_ports.iter().map(u16::to_string).collect();
                    text.push_str(&format!(", ports {}", ports.join(", ")));
                }
                match answers {
                    0 => {}
                    1 => text.push_str(", 1 réponse"),
                    n => text.push_str(&format!(", {n} réponses")),
                }
                text.push_str(&format!(
                    ", {cpu_millis} millicœurs, {} Mio de mémoire",
                    memory_bytes / (1024 * 1024)
                ));
                if !agreements.is_empty() {
                    let titles: Vec<&str> = agreements.iter().map(ClientName::as_str).collect();
                    text.push_str(&format!(", contrats acceptés : {}", titles.join(", ")));
                }
                Some(text)
            }
            Self::Fiche(fiche) => Some(format!("fiche {}", fiche.as_str())),
            Self::FicheRefused { dir, invariant } => {
                Some(format!("fiche {} ({})", dir.as_str(), invariant.as_str()))
            }
            Self::ServiceAdoption(key, cause) => {
                Some(format!("service {} ({})", key.as_str(), cause.text()))
            }
            Self::SessionsClosed {
                account,
                closed,
                target_holder,
                author_holder,
            } => {
                let count = if *closed == 1 {
                    "1 session fermée".to_owned()
                } else {
                    format!("{closed} sessions fermées")
                };
                let right = |holder: bool| {
                    if holder {
                        "peut gérer les services"
                    } else {
                        "ne peut pas gérer les services"
                    }
                };
                Some(match author_holder {
                    Some(author) => format!(
                        "{account} : {count} (compte visé : {} ; auteur : {})",
                        right(*target_holder),
                        right(*author)
                    ),
                    None => format!(
                        "{account} : {count} (compte visé : {})",
                        right(*target_holder)
                    ),
                })
            }
        }
    }
}

/// Pourquoi une action est refusée ou échoue. Un texte fixe par variante : jamais de texte libre,
/// donc jamais de secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// Identifiant ou mot de passe faux : le même texte que l'identifiant existe ou non
    /// (BR-AUDIT-006).
    InvalidCredentials,
    /// Identifiant saisi dans un format impossible (par exemple un mot de passe tapé à la place) :
    /// la valeur n'est jamais retenue.
    InvalidIdentifier,
    /// Les échecs ont déclenché une attente de cette durée.
    TooManyAttempts {
        retry_after_s: u64,
    },
    /// Un compte en lecture seule demande ce qui est réservé aux administrateurs.
    ReadOnly,
    /// Un acte de service est demandé par un compte qui n'a pas le droit de gérer les services,
    /// quel que soit son rôle (BR-GRANT-004, cause « droits insuffisants » du cycle de vie des services).
    InsufficientRights,
    Validation,
    UsernameTaken,
    WrongPassword,
    LastAdmin,
    Conflict,
    NotFound,
    Busy,
    Internal,
    /// Une mise à jour de l'agent est déjà en cours (BR-UPDATE-012).
    UpdateInProgress,
    /// Installation gérée par le système : pas de mise à jour à distance.
    ManagedInstall,
    /// La signature de la mise à jour est refusée.
    BadSignature,
    /// Le téléchargement de la mise à jour n'a pas abouti.
    DownloadFailed,
    /// La mise à jour a échoué avant tout échange de binaire.
    UpdateFailed,
    /// Le résultat d'une mise à jour a été écrit par une autre version (un agent plus récent, avant un retour
    /// arrière) et cet agent ne le comprend pas : il n'est ni une réussite ni un échec établi (BR-COMPAT-001).
    /// Le fichier garde la valeur d'origine.
    UpdateOutcomeUnknown,
    /// Le nouvel agent n'a pas répondu : l'ancien binaire est revenu (BR-UPDATE-015).
    RolledBack,
    /// Entrée de synthèse de débordement (`repeat::RepeatFilter`) : trop de groupes différents.
    TooVaried,
    /// Mode attaque : le poste n'est pas reconnu (session présentée seule, activation sans clé
    /// prouvée). Jamais écrite pour une connexion : un refus de connexion garde la raison d'un mot de
    /// passe faux, pour que rien ne distingue les états (HRT-25).
    NotRecognized,
    /// Un acte d'administration arrive sans confirmation : le client est trop ancien (HRT-28).
    ReauthMissing,
    /// La confirmation d'un acte n'a pas de preuve de clé (HRT-28).
    ProofMissing,
    /// La preuve de clé d'un acte est invalide : autre acte, autre cible, autre compte, autre jeton,
    /// périmée, rejouée, clé non inscrite pour le compte (HRT-28).
    ProofInvalid,
    /// Le mot de passe manque et l'élévation ne couvre pas l'acte (HRT-28).
    PasswordRequired,
    /// Le service est déjà adopté (HRT-80, BR-ADOPT-019).
    AlreadyAdopted,
    /// Aucun conteneur de cette clé : disparu depuis la liste, ou jamais existé (HRT-80, BR-ADOPT-019). Le
    /// journal ne distingue pas non plus l'un de l'autre.
    ServiceNotFound,
    /// Un compte a été refusé PLUS DE `over` fois en 10 minutes en ouvrant des lectures de journaux trop
    /// vite (HRT-107, BR-SLOG-027). Le nombre exact n'est pas connu au moment d'écrire (l'entrée est
    /// unique par fenêtre) : le texte dit « plus de », jamais un total qui serait faux.
    OpensThrottled {
        over: u32,
    },
    /// Une autre action tient déjà ce service (HRT-64, BR-LIFE-013, BR-LIFE-018).
    ActionInProgress,
    /// Le service est déjà dans l'état demandé (HRT-64, BR-LIFE-014).
    AlreadyInState,
    /// Le service est dans un état où l'action n'a pas de sens : en pause, en arrêt, état non reconnu, ou, pour
    /// redémarrer, pas en marche (HRT-64, BR-LIFE-003, BR-LIFE-014).
    NotDrivable,
    /// Un port que le service demande est déjà pris : il reste arrêté (HRT-64, BR-LIFE-006).
    PortInUse,
    /// Le programme s'est arrêté dans les 10 secondes qui ont suivi son lancement (HRT-64, BR-LIFE-005).
    StoppedRightAfterStart,
    /// Le moteur de conteneurs est inaccessible ou pas prêt (HRT-64, BR-LIFE-019).
    EngineUnavailable,
    /// Le moteur ou le système du serveur a refusé l'ordre (HRT-64, BR-LIFE-019).
    EngineRefused,
    /// Arrêté depuis Hearth, le service est reparti tout seul : une règle du serveur le relance (HRT-64,
    /// BR-LIFE-011).
    RestartedByHost,
    /// Le moteur n'a pas répondu dans le délai d'arrêt du service plus 30 secondes (HRT-64, BR-LIFE-021).
    Timeout,
    /// L'agent s'est arrêté pendant l'action (HRT-64, BR-LIFE-020).
    Interrupted,
    /// Déployer (HRT-68, BR-DEPLOY-025) : image introuvable.
    ImageNotFound,
    /// Version introuvable pour cette image.
    TagNotFound,
    /// Image privée ou introuvable (le registre ne distingue pas).
    ImagePrivateOrMissing,
    /// Registre injoignable depuis le serveur.
    RegistryUnreachable,
    /// Limite de téléchargements du registre.
    RegistryRateLimited,
    /// Architecture incompatible.
    PlatformMismatch,
    /// Disque plein.
    DiskFull,
    /// Nom déjà pris.
    NameTaken,
    /// Dossier refusé.
    FolderRefused,
    /// Réglage hors du plan fermé d'un service créé par Hearth (BR-DEPLOY-035).
    SettingNotAllowed,
    /// Limites de mémoire et de processeur absentes ou hors bornes (BR-DEPLOY-038).
    LimitsRequired,
    /// Deux déploiements sont déjà en cours (BR-DEPLOY-023).
    DeployLimit,
    /// Annulé par l'utilisateur (BR-DEPLOY-025).
    CancelledByAccount,
    /// Installer depuis une fiche (HRT-101, BR-FICHE-021) : la machine est plus petite que le minimum mesuré.
    HostTooSmall,
    /// Installer depuis une fiche (BR-FICHE-023) : l'espace libre est sous le besoin mesuré de l'installation.
    NotEnoughSpace,
    /// Installer depuis une fiche : la fiche n'est pas proposée par cet agent (inconnue, ou d'essai).
    FicheUnknown,
    /// Installer depuis une fiche : la fiche de l'agent n'est plus celle que le client a vue.
    FicheChanged,
    /// Installer depuis une fiche : un contrat n'est pas accepté (BR-FICHE-058).
    AgreementRequired,
    /// Installer depuis une fiche : la fiche ne se traduit pas en description valable (fiche refusée).
    FicheRefused,
}

impl Reason {
    /// Texte de la raison **sans donnée variable**, pour la clé de regroupement des refus
    /// (`repeat::RepeatFilter`) : la durée d'une attente change à chaque seconde, elle ne doit
    /// pas faire un groupe neuf (donc une entrée neuve) à chaque tentative.
    pub fn group_text(self) -> String {
        match self {
            Self::TooManyAttempts { .. } => "trop de tentatives".to_owned(),
            Self::OpensThrottled { .. } => "ouvertures de journaux freinées".to_owned(),
            other => other.text(),
        }
    }

    /// Texte lu après le résultat : « Refusé : lecture seule ».
    pub fn text(self) -> String {
        match self {
            Self::InvalidCredentials => "identifiants incorrects".to_owned(),
            Self::InvalidIdentifier => "identifiant invalide".to_owned(),
            Self::TooManyAttempts { retry_after_s } => {
                format!("trop de tentatives, attente de {retry_after_s} s")
            }
            Self::ReadOnly => "lecture seule".to_owned(),
            Self::InsufficientRights => "droits insuffisants".to_owned(),
            Self::Validation => "données invalides".to_owned(),
            Self::UsernameTaken => "identifiant déjà utilisé".to_owned(),
            Self::WrongPassword => "mot de passe actuel incorrect".to_owned(),
            Self::LastAdmin => "dernier administrateur".to_owned(),
            Self::Conflict => "conflit avec l'état du serveur".to_owned(),
            Self::NotFound => "cible introuvable".to_owned(),
            Self::Busy => "agent occupé".to_owned(),
            Self::Internal => "erreur interne".to_owned(),
            Self::UpdateInProgress => "mise à jour déjà en cours".to_owned(),
            Self::ManagedInstall => "installation gérée par le système".to_owned(),
            Self::BadSignature => "signature invalide".to_owned(),
            Self::DownloadFailed => "téléchargement impossible".to_owned(),
            Self::UpdateFailed => "mise à jour échouée".to_owned(),
            Self::UpdateOutcomeUnknown => {
                "résultat de la mise à jour inconnu de cette version (écrit par une version plus récente), ni réussite ni échec établis".to_owned()
            }
            Self::RolledBack => {
                "le nouvel agent n'a pas répondu, retour à la version précédente. Un changement du mode attaque fait depuis le début de la mise à jour a pu être annulé".to_owned()
            }
            Self::TooVaried => "activité trop variée".to_owned(),
            Self::NotRecognized => "mode attaque : poste non reconnu".to_owned(),
            Self::ReauthMissing => "confirmation absente : client trop ancien".to_owned(),
            Self::ProofMissing => "preuve de clé absente".to_owned(),
            Self::ProofInvalid => "preuve de clé invalide".to_owned(),
            Self::PasswordRequired => "mot de passe requis".to_owned(),
            Self::AlreadyAdopted => "déjà adopté".to_owned(),
            Self::ServiceNotFound => "service introuvable".to_owned(),
            Self::OpensThrottled { over } => {
                format!("plus de {over} ouvertures de journaux refusées en 10 minutes")
            }
            Self::ActionInProgress => "action déjà en cours".to_owned(),
            Self::AlreadyInState => "déjà dans l'état demandé".to_owned(),
            Self::NotDrivable => "état du service incompatible avec l'action".to_owned(),
            Self::PortInUse => "port occupé".to_owned(),
            Self::StoppedRightAfterStart => "arrêté juste après le démarrage".to_owned(),
            Self::EngineUnavailable => "moteur inaccessible".to_owned(),
            Self::EngineRefused => "refusé par le moteur".to_owned(),
            Self::RestartedByHost => "repart tout seul".to_owned(),
            Self::Timeout => "délai dépassé".to_owned(),
            Self::Interrupted => "interrompu par un redémarrage de l'agent".to_owned(),
            Self::ImageNotFound => "image introuvable".to_owned(),
            Self::TagNotFound => "version introuvable".to_owned(),
            Self::ImagePrivateOrMissing => "image privée ou introuvable".to_owned(),
            Self::RegistryUnreachable => "registre injoignable".to_owned(),
            Self::RegistryRateLimited => "limite du registre".to_owned(),
            Self::PlatformMismatch => "architecture incompatible".to_owned(),
            Self::DiskFull => "disque plein".to_owned(),
            Self::NameTaken => "nom déjà pris".to_owned(),
            Self::FolderRefused => "dossier refusé".to_owned(),
            Self::SettingNotAllowed => "réglage non permis".to_owned(),
            Self::LimitsRequired => "limites de ressources requises".to_owned(),
            Self::DeployLimit => "deux déploiements déjà en cours".to_owned(),
            Self::CancelledByAccount => "annulé par l'utilisateur".to_owned(),
            Self::HostTooSmall => "machine trop petite".to_owned(),
            Self::NotEnoughSpace => "espace insuffisant pour l'installation".to_owned(),
            Self::FicheUnknown => "fiche inconnue".to_owned(),
            Self::FicheChanged => "fiche changée depuis l'affichage".to_owned(),
            Self::AgreementRequired => "contrat non accepté".to_owned(),
            Self::FicheRefused => "fiche refusée".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutcomeKind {
    Ok,
    Denied,
    Failed,
}

impl OutcomeKind {
    pub const ALL: [OutcomeKind; 3] = [Self::Ok, Self::Denied, Self::Failed];

    pub fn code(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Denied => "denied",
            Self::Failed => "failed",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Ok => "Réussi",
            Self::Denied => "Refusé",
            Self::Failed => "Échoué",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.code() == code)
    }
}

/// Résultat d'une action : un refus ou un échec dit toujours pourquoi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Succeeded,
    Denied(Reason),
    Failed(Reason),
}

impl Outcome {
    pub fn kind(self) -> OutcomeKind {
        match self {
            Self::Succeeded => OutcomeKind::Ok,
            Self::Denied(_) => OutcomeKind::Denied,
            Self::Failed(_) => OutcomeKind::Failed,
        }
    }

    pub fn reason(self) -> Option<Reason> {
        match self {
            Self::Succeeded => None,
            Self::Denied(reason) | Self::Failed(reason) => Some(reason),
        }
    }
}

/// Une entrée à écrire. L'identifiant (`id`) est donné par le stockage (`AuditRecord`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    pub at: OffsetDateTime,
    pub actor: Actor,
    pub action: AuditAction,
    pub target: Target,
    pub outcome: Outcome,
    /// Pour une entrée de synthèse (`repeat::RepeatFilter`) : combien d'autres fois le même
    /// événement s'est produit dans la fenêtre ; 0 pour une entrée ordinaire.
    pub repeat_count: u32,
    /// Pour une synthèse « N tentatives depuis M adresses » (`repeat::RepeatFilter`, plafond par clé
    /// sans adresse) : combien d'adresses distinctes ont fait ces `repeat_count` tentatives ; 0 sinon.
    pub addresses: u32,
}

impl AuditEvent {
    pub fn new(
        at: OffsetDateTime,
        actor: Actor,
        action: AuditAction,
        target: Target,
        outcome: Outcome,
    ) -> Self {
        Self {
            at,
            actor,
            action,
            target,
            outcome,
            repeat_count: 0,
            addresses: 0,
        }
    }

    /// La synthèse de `attempts` tentatives venues de `addresses` adresses distinctes qui n'ont pas eu
    /// leur entrée par adresse (`repeat::MAX_ADDRESSES` par fenêtre). **Aucune adresse n'y est
    /// écrite** : ni liste, ni celle de la dernière occurrence ; l'origine reste sans adresse.
    pub fn with_addresses(mut self, attempts: u32, addresses: u32) -> Self {
        self.actor.origin = Origin::client(None, "");
        self.repeat_count = attempts;
        self.addresses = addresses;
        self
    }

    /// L'entrée de synthèse de `count` autres occurrences.
    pub fn with_repeats(mut self, count: u32) -> Self {
        self.repeat_count = count;
        self
    }

    /// L'entrée de synthèse de débordement : `count` événements de natures trop variées, regroupés
    /// en une entrée. Ils viennent de comptes et visent des cibles différents : **la synthèse n'a
    /// ni compte ni cible** (ceux de la dernière occurrence ne diraient rien des autres) ; elle
    /// garde l'action, l'origine et la date de la dernière occurrence.
    pub fn as_overflow(mut self, count: u32) -> Self {
        self.actor.account = None;
        self.target = Target::None;
        self.outcome = match self.outcome {
            Outcome::Denied(_) => Outcome::Denied(Reason::TooVaried),
            _ => Outcome::Failed(Reason::TooVaried),
        };
        self.repeat_count = count;
        self
    }

    /// L'entrée telle qu'elle est écrite et relue : textes figés.
    pub fn into_record(self, id: i64) -> AuditRecord {
        AuditRecord {
            id,
            at: self.at,
            account: self.actor.account.as_ref().map(ToString::to_string),
            origin_kind: self.actor.origin.kind(),
            origin_name: self.actor.origin.name().map(str::to_owned),
            origin_addr: self.actor.origin.addr().map(str::to_owned),
            action: self.action.code().to_owned(),
            action_label: self.action.label().to_owned(),
            target: self.target.text(),
            outcome: self.outcome.kind(),
            reason: self
                .outcome
                .reason()
                .map(|reason| match (reason, self.repeat_count) {
                    (_, 0) => reason.text(),
                    (Reason::TooVaried, n) => {
                        format!("{}, {n} événements regroupés", reason.text())
                    }
                    (_, n) if self.addresses > 0 => format!(
                        "{} ({n} tentatives depuis {} adresses)",
                        reason.text(),
                        self.addresses
                    ),
                    (_, n) => format!("{} ({n} autres fois en 1 min)", reason.text()),
                }),
            repeat_count: self.repeat_count,
            repeat_addresses: self.addresses,
        }
    }
}

/// Une entrée écrite : ce que le stockage rend, ce que le flux diffuse, ce que l'export écrit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRecord {
    /// Croissant : sert de curseur.
    pub id: i64,
    pub at: OffsetDateTime,
    /// Identifiant du compte, figé à l'écriture (le compte peut disparaître ensuite).
    pub account: Option<String>,
    pub origin_kind: OriginKind,
    pub origin_name: Option<String>,
    pub origin_addr: Option<String>,
    /// Code stable de l'action (`AuditAction::code`) ; texte tel qu'écrit, même si le catalogue
    /// a changé depuis.
    pub action: String,
    pub action_label: String,
    pub target: Option<String>,
    pub outcome: OutcomeKind,
    /// La raison dit aussi « (n autres fois en 1 min) » pour une synthèse.
    pub reason: Option<String>,
    /// Autres occurrences regroupées dans cette entrée de synthèse ; 0 pour une entrée ordinaire.
    pub repeat_count: u32,
    /// Pour une synthèse « N tentatives depuis M adresses » : M, typé (jamais relu dans le texte de la
    /// raison) ; 0 pour toute autre entrée.
    pub repeat_addresses: u32,
}

impl AuditRecord {
    /// Le texte de l'origine lu par l'administrateur.
    pub fn origin_text(&self) -> String {
        describe(
            self.origin_kind,
            self.origin_name.as_deref(),
            self.origin_addr.as_deref(),
        )
    }
}

#[cfg(test)]
mod tests {
    use time::Duration;

    use super::*;

    fn at() -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH + Duration::seconds(1_790_000_000)
    }

    fn marie() -> Username {
        Username::parse("marie").unwrap()
    }

    #[test]
    fn a_lifecycle_entry_names_the_service_and_says_when_the_stop_was_forced() {
        let name = ClientName::parse("terraria").unwrap();
        let plain = Target::ServiceLifecycle {
            service: name.clone(),
            note: LifecycleNote::Plain,
        };
        let forced = Target::ServiceLifecycle {
            service: name,
            note: LifecycleNote::Forced,
        };
        assert_eq!(plain.text().unwrap(), "service terraria");
        assert_eq!(forced.text().unwrap(), "service terraria (arrêt forcé)");
    }

    #[test]
    fn the_lifecycle_reasons_are_the_closed_list_of_the_rule_in_french() {
        // BR-LIFE-018 : « port occupé », « arrêté juste après le démarrage », « moteur inaccessible », « service
        // introuvable », « droits insuffisants », « action déjà en cours », « déjà dans l'état demandé », « repart
        // tout seul », « délai dépassé », « interrompu par un redémarrage de l'agent », « refusé par le moteur ».
        for (reason, text) in [
            (Reason::PortInUse, "port occupé"),
            (
                Reason::StoppedRightAfterStart,
                "arrêté juste après le démarrage",
            ),
            (Reason::EngineUnavailable, "moteur inaccessible"),
            (Reason::ServiceNotFound, "service introuvable"),
            (Reason::InsufficientRights, "droits insuffisants"),
            (Reason::ActionInProgress, "action déjà en cours"),
            (Reason::AlreadyInState, "déjà dans l'état demandé"),
            (Reason::RestartedByHost, "repart tout seul"),
            (Reason::Timeout, "délai dépassé"),
            (
                Reason::Interrupted,
                "interrompu par un redémarrage de l'agent",
            ),
            (Reason::EngineRefused, "refusé par le moteur"),
        ] {
            assert_eq!(reason.text(), text);
            assert_eq!(reason.group_text(), text);
        }
    }

    #[test]
    fn the_closure_of_sessions_says_who_could_manage_services_and_never_an_address() {
        let closed = |closed, author_holder| {
            Target::SessionsClosed {
                account: marie(),
                closed,
                target_holder: true,
                author_holder,
            }
            .text()
            .unwrap()
        };
        assert_eq!(
            closed(1, Some(false)),
            "marie : 1 session fermée (compte visé : peut gérer les services ; auteur : ne peut pas gérer les services)"
        );
        assert_eq!(
            closed(3, Some(true)),
            "marie : 3 sessions fermées (compte visé : peut gérer les services ; auteur : peut gérer les services)"
        );
        assert_eq!(
            closed(0, None),
            "marie : 0 sessions fermées (compte visé : peut gérer les services)"
        );
    }

    #[test]
    fn a_client_origin_reads_address_then_host() {
        let origin = Origin::client(Some("poste-de-marie/1.2"), "10.0.0.7");
        assert_eq!(origin.describe(), "10.0.0.7 (poste-de-marie/1.2)");
        assert_eq!(origin.kind(), OriginKind::Client);
        assert_eq!(origin.addr(), Some("10.0.0.7"));
    }

    #[test]
    fn an_unidentified_host_reads_unknown() {
        for name in [None, Some(""), Some("   "), Some("\u{202E}\n")] {
            assert_eq!(
                Origin::client(name, "10.0.0.7").describe(),
                "10.0.0.7 (inconnu)",
                "{name:?}"
            );
        }
    }

    #[test]
    fn the_command_line_and_the_assistant_have_their_own_text() {
        assert_eq!(
            Origin::CommandLine.describe(),
            "ligne de commande du serveur"
        );
        assert_eq!(Origin::Assistant.describe(), "assistant");
        assert_eq!(Origin::CommandLine.addr(), None);
        assert_eq!(Origin::Assistant.name(), None);
    }

    #[test]
    fn the_host_name_is_cleaned_and_bounded() {
        let name = ClientName::parse("  po\nste\u{202E}\u{2028}-1  ").unwrap();
        assert_eq!(name.as_str(), "poste-1");
        let long = ClientName::parse(&"x".repeat(1000)).unwrap();
        assert_eq!(long.as_str().chars().count(), MAX_CLIENT_NAME);
        let origin = Origin::client(Some("a"), &format!("1.2.3.4\n{}", "9".repeat(200)));
        assert!(origin.addr().unwrap().chars().count() <= 64);
        assert!(!origin.addr().unwrap().contains('\n'));
    }

    #[test]
    fn origin_kinds_round_trip_by_code() {
        for kind in [
            OriginKind::Client,
            OriginKind::CommandLine,
            OriginKind::Assistant,
        ] {
            assert_eq!(OriginKind::from_code(kind.code()), Some(kind));
        }
        assert_eq!(OriginKind::from_code("autre"), None);
    }

    #[test]
    fn targets_read_as_an_account_a_role_or_a_route() {
        assert_eq!(Target::None.text(), None);
        assert_eq!(Target::Account(marie()).text().as_deref(), Some("marie"));
        assert_eq!(
            Target::AccountRole(marie(), Role::ReadOnly)
                .text()
                .as_deref(),
            Some("marie (Lecture seule)")
        );
        assert_eq!(
            Target::Route("/accounts/{id}").text().as_deref(),
            Some("/accounts/{id}")
        );
    }

    #[test]
    fn outcomes_say_their_kind_and_reason() {
        assert_eq!(Outcome::Succeeded.kind(), OutcomeKind::Ok);
        assert_eq!(Outcome::Succeeded.reason(), None);
        let denied = Outcome::Denied(Reason::ReadOnly);
        assert_eq!(denied.kind(), OutcomeKind::Denied);
        assert_eq!(denied.reason(), Some(Reason::ReadOnly));
        assert_eq!(Outcome::Failed(Reason::Busy).kind(), OutcomeKind::Failed);
        for kind in OutcomeKind::ALL {
            assert_eq!(OutcomeKind::from_code(kind.code()), Some(kind));
        }
        assert_eq!(OutcomeKind::Ok.label(), "Réussi");
        assert_eq!(OutcomeKind::Denied.label(), "Refusé");
        assert_eq!(OutcomeKind::Failed.label(), "Échoué");
    }

    #[test]
    fn a_failed_login_never_says_whether_the_account_exists() {
        // BR-AUDIT-006 : une seule raison pour un identifiant inconnu et un mot de passe faux.
        assert_eq!(Reason::InvalidCredentials.text(), "identifiants incorrects");
        assert_eq!(Reason::InvalidIdentifier.text(), "identifiant invalide");
        for reason in [Reason::InvalidCredentials, Reason::InvalidIdentifier] {
            let text = reason.text().to_lowercase();
            assert!(
                !text.contains("inconnu") && !text.contains("existe"),
                "{text}"
            );
        }
    }

    #[test]
    fn the_group_text_of_a_lock_does_not_carry_the_wait() {
        assert_eq!(
            Reason::TooManyAttempts { retry_after_s: 60 }.group_text(),
            Reason::TooManyAttempts { retry_after_s: 7 }.group_text()
        );
        assert_eq!(
            Reason::ReadOnly.group_text(),
            Reason::ReadOnly.text(),
            "les autres raisons sont inchangées"
        );
    }

    #[test]
    fn the_lock_reason_carries_only_the_wait() {
        assert_eq!(
            Reason::TooManyAttempts { retry_after_s: 60 }.text(),
            "trop de tentatives, attente de 60 s"
        );
    }

    #[test]
    fn a_game_install_entry_carries_no_answer_value() {
        let name = |text: &str| ClientName::parse(text).unwrap();
        let entry = Target::GameInstall {
            service: name("mon-terraria"),
            fiche: name("terraria"),
            version: 1,
            image: name("ryshe/terraria:vanilla-1.4.5.8"),
            host_ports: vec![7777],
            answers: 3,
            cpu_millis: 1000,
            memory_bytes: 2 << 30,
            agreements: vec![name("Contrat de licence")],
        };
        let text = entry.text().unwrap();
        assert_eq!(
            text,
            "service mon-terraria : fiche terraria (version 1), image ryshe/terraria:vanilla-1.4.5.8, ports 7777, 3 réponses, 1000 millicœurs, 2048 Mio de mémoire, contrats acceptés : Contrat de licence"
        );
        // Le type n'a pas de champ pour une valeur : seul le NOMBRE de réponses y entre.
        assert!(!text.contains("SENTINEL"));
        assert_eq!(
            Target::FicheRefused {
                dir: name("bad"),
                invariant: name("I2")
            }
            .text()
            .unwrap(),
            "fiche bad (I2)"
        );
        assert_eq!(
            Target::Fiche(name("terraria")).text().unwrap(),
            "fiche terraria"
        );
        for reason in [
            Reason::HostTooSmall,
            Reason::NotEnoughSpace,
            Reason::FicheUnknown,
            Reason::FicheChanged,
            Reason::AgreementRequired,
            Reason::FicheRefused,
        ] {
            let text = reason.text();
            assert!(!text.contains('\u{2014}') && !text.contains('{'), "{text}");
        }
        assert_eq!(
            Reason::NotEnoughSpace.text(),
            "espace insuffisant pour l'installation"
        );
        let record = AuditEvent::new(
            at(),
            Actor::system(),
            AuditAction::CatalogFicheRefused,
            Target::FicheRefused {
                dir: name("bad"),
                invariant: name("I2"),
            },
            Outcome::Failed(Reason::FicheRefused),
        )
        .into_record(1);
        assert_eq!(record.action, "catalog.fiche.refused");
        assert_eq!(record.action_label, "Fiche écartée");
    }

    #[test]
    fn every_reason_is_a_fixed_text_without_dash_or_placeholder() {
        for reason in [
            Reason::InvalidCredentials,
            Reason::InvalidIdentifier,
            Reason::ReadOnly,
            Reason::Validation,
            Reason::UsernameTaken,
            Reason::WrongPassword,
            Reason::LastAdmin,
            Reason::Conflict,
            Reason::NotFound,
            Reason::Busy,
            Reason::Internal,
            Reason::TooVaried,
            Reason::NotRecognized,
            Reason::ReauthMissing,
            Reason::ProofMissing,
            Reason::ProofInvalid,
            Reason::PasswordRequired,
        ] {
            let text = reason.text();
            assert!(!text.is_empty());
            assert!(!text.contains('\u{2014}') && !text.contains('{'), "{text}");
        }
    }

    #[test]
    fn an_event_becomes_a_record_with_frozen_texts() {
        let event = AuditEvent::new(
            at(),
            Actor::new(Some(marie()), Origin::client(Some("poste"), "10.0.0.7")),
            AuditAction::AuditRead,
            Target::Route("/audit"),
            Outcome::Denied(Reason::ReadOnly),
        );
        let record = event.into_record(42);
        assert_eq!(record.id, 42);
        assert_eq!(record.account.as_deref(), Some("marie"));
        assert_eq!(record.origin_kind, OriginKind::Client);
        assert_eq!(record.origin_name.as_deref(), Some("poste"));
        assert_eq!(record.origin_addr.as_deref(), Some("10.0.0.7"));
        assert_eq!(record.action, "audit.read");
        assert_eq!(record.action_label, "Tentative de lecture du journal");
        assert_eq!(record.target.as_deref(), Some("/audit"));
        assert_eq!(record.outcome, OutcomeKind::Denied);
        assert_eq!(record.reason.as_deref(), Some("lecture seule"));
        assert_eq!(record.origin_text(), "10.0.0.7 (poste)");
    }

    #[test]
    fn a_summary_entry_says_how_many_other_times() {
        let record = AuditEvent::new(
            at(),
            Actor::command_line(),
            AuditAction::AuditRead,
            Target::Route("/audit"),
            Outcome::Denied(Reason::ReadOnly),
        )
        .with_repeats(999)
        .into_record(3);
        assert_eq!(record.repeat_count, 999);
        assert_eq!(
            record.reason.as_deref(),
            Some("lecture seule (999 autres fois en 1 min)")
        );
    }

    #[test]
    fn a_command_line_event_has_no_account_and_no_address() {
        let record = AuditEvent::new(
            at(),
            Actor::command_line(),
            AuditAction::AccountCreate,
            Target::Account(marie()),
            Outcome::Succeeded,
        )
        .into_record(1);
        assert_eq!(record.account, None);
        assert_eq!(record.origin_kind, OriginKind::CommandLine);
        assert_eq!(record.origin_name, None);
        assert_eq!(record.origin_addr, None);
        assert_eq!(record.reason, None);
        assert_eq!(record.origin_text(), "ligne de commande du serveur");
    }
}
