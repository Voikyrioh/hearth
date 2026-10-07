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
    /// Le nouvel agent n'a pas répondu : l'ancien binaire est revenu (BR-UPDATE-015).
    RolledBack,
    /// Entrée de synthèse de débordement (`repeat::RepeatFilter`) : trop de groupes différents.
    TooVaried,
    /// Mode attaque : le poste n'est pas reconnu (session présentée seule, activation sans clé
    /// prouvée). Jamais écrite pour une connexion : un refus de connexion garde la raison d'un mot de
    /// passe faux, pour que rien ne distingue les états (HRT-25).
    NotRecognized,
}

impl Reason {
    /// Texte de la raison **sans donnée variable**, pour la clé de regroupement des refus
    /// (`repeat::RepeatFilter`) : la durée d'une attente change à chaque seconde, elle ne doit
    /// pas faire un groupe neuf (donc une entrée neuve) à chaque tentative.
    pub fn group_text(self) -> String {
        match self {
            Self::TooManyAttempts { .. } => "trop de tentatives".to_owned(),
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
            Self::RolledBack => {
                "le nouvel agent n'a pas répondu, retour à la version précédente. Un changement du mode attaque fait depuis le début de la mise à jour a pu être annulé".to_owned()
            }
            Self::TooVaried => "activité trop variée".to_owned(),
            Self::NotRecognized => "mode attaque : poste non reconnu".to_owned(),
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
