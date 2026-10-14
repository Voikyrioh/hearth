//! Format d'erreur commun à toutes les routes : `{ "error": { "code", "message", "details" } }`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Code d'erreur stable, sérialisé en `SCREAMING_SNAKE_CASE`.
///
/// Les textes affichés à l'utilisateur sont indexés par ce code côté interface ;
/// le `message` du corps ne sert qu'au diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    Unauthenticated,
    /// Identifiant ou mot de passe refusé, sans préciser lequel (BR-CONN-013).
    InvalidCredentials,
    SessionExpired,
    SessionRevoked,
    ForbiddenRole,
    OperationInProgress,
    ValidationError,
    IncompatibleVersion,
    TooManyAttempts,
    /// Identifiant déjà pris par un autre compte.
    UsernameTaken,
    /// Mot de passe refusé ; `details.rules` liste les règles non respectées.
    WeakPassword,
    /// Mot de passe actuel incorrect lors d'un changement de son propre mot de passe.
    WrongPassword,
    /// L'opération laisserait le serveur sans administrateur.
    LastAdmin,
    /// Conflit avec un changement concurrent : réessayer.
    Conflict,
    /// Corps de requête trop volumineux.
    PayloadTooLarge,
    /// Cette clé d'opération a déjà servi pour une autre requête.
    IdempotencyKeyReused,
    /// L'agent est saturé (calculs de mots de passe) : réessayer après `Retry-After`.
    Busy,
    InternalError,
    /// Route ou ressource inconnue.
    NotFound,
    /// Méthode HTTP non prise en charge par la route.
    MethodNotAllowed,
    /// L'agent ne se met pas à jour à distance : installation gérée par le système, ou sans
    /// systemd (HRT-17).
    ManagedInstall,
    /// La signature minisign de la mise à jour est refusée (HRT-17).
    BadSignature,
    /// Activer ou désactiver le mode attaque exige la preuve d'une clé inscrite du compte appelant
    /// (HRT-25) ; `details.reason` dit ce qui manque. Un utilisateur déjà authentifié : ce n'est pas
    /// un oracle.
    PostNotRecognized,
    // --- Déploiements, lots suivants (HRT-83 déclare d'un coup les codes de tout le lot) ---
    /// La description a changé depuis que le formulaire a été ouvert ; `details.by` et `details.at` disent par qui et quand (BR-EDIT-024).
    SpecConflict,
    /// Le geste ne s'applique pas à un service adopté : sa configuration reste celle de son créateur.
    ServiceAdopted,
    /// Un réglage du plan de conteneur est refusé par la liste fermée de l'agent.
    PlanSettingRefused,
    /// Le nom est réservé : un service de ce nom est en cours de suppression (BR-DEL-019).
    NameBeingDeleted,
    /// La limite ne peut pas être posée à chaud ; `details.needs_recreate` propose de recréer le service.
    LimitRefusedLive,
    /// Les limites ont changé depuis la lecture ; `details.current`.
    LimitsChanged,
    /// Le total engagé a changé depuis la lecture ; `details.excess_bytes`.
    OvercommitChanged,
    /// Le moteur ne sait pas limiter cette ressource.
    ResourceNotLimitable,
    /// Chemin refusé ; `details.reason` : `symlink`, `outside`, `special` ou `name`.
    PathRefused,
    /// Le fichier a changé depuis la lecture ; `details.by` et `details.at` si connus.
    FileConflict,
    /// Le fichier ne s'édite pas ; `details.reason`.
    FileNotEditable,
    /// Le fichier a plusieurs liens durs : il n'est pas modifié sur place.
    Hardlinked,
    /// Pas assez d'espace ; `details.missing_bytes` et `details.free_after_bytes`.
    NoSpace,
    /// Écriture refusée ; `details.reason` : `permission`, `read_only_fs`, `agent_confined`, `mount_read_only` ou `root_read_only`.
    WriteDenied,
    /// Une sauvegarde, une restauration ou une reprise de monde est déjà en cours ; `details.service`.
    HeavyIoBusy,
    /// La sauvegarde est corrompue.
    BackupCorrupt,
    /// La sauvegarde est utilisée par une opération en cours.
    BackupInUse,
    /// Le plan de restauration a changé depuis sa prévisualisation.
    RestorePlanChanged,
    /// Le transfert a expiré.
    TransferExpired,
    /// Trop de transferts à la fois.
    TransferLimit,
    /// Fiche de serveur de jeu inconnue.
    FicheUnknown,
    /// La machine est trop petite pour ce service.
    HostTooSmall,
    /// Le monde à reprendre n'existe plus à son emplacement.
    WorldGone,
    /// L'opération a passé son point de non-retour : elle ne s'annule plus (ADR-0045).
    TooLateToCancel,
    // --- Codes des tranches L1 à L3, déclarés d'un coup par HRT-144 : les lots suivants les utilisent sans
    // --- toucher à ce fichier. Les détails (`details.*`) sont ceux de la conception technique. ---
    /// Le moteur de conteneurs n'est pas utilisable (`details.engine`).
    EngineUnavailable,
    /// Aucun service de cette clé, ou un service que ce compte ne peut pas voir.
    ServiceNotFound,
    /// Une opération tient déjà ce service (`details.by`, `details.kind`).
    ServiceBusy,
    /// Le service est déjà dans l'état demandé (`details.state`).
    ServiceAlreadyInState,
    /// Le service ne se pilote pas ainsi : en pause, ou dans un état non reconnu.
    ServiceNotDrivable,
    /// Le nom est pris (`details.holder` : `hearth`, `external` ou `deployment`).
    NameTaken,
    /// Le port est tenu (`details.port`, `details.protocol`, `details.holder`, `details.suggested`).
    PortInUse,
    /// Dossier refusé (`details.field`, `details.reason`).
    FolderRefused,
    /// Un service ne se crée pas sans limite de mémoire et de processeur.
    LimitsRequired,
    /// Trop de déploiements à la fois.
    DeployLimit,
    /// Aucune opération de cet identifiant, ou une opération sur un service que ce compte ne peut pas voir.
    OperationNotFound,
    /// Ce choix n'est pas parmi ceux que l'opération offre.
    DecisionNotOffered,
    /// Un contrat de la fiche n'est pas accepté (`details.ids`).
    AgreementRequired,
    /// La fiche n'est plus celle que le client a lue (`details.version`, `details.digest`).
    FicheChanged,
    /// Une valeur de réglage est refusée (`details.id`, `details.reason`, `details.char?`).
    SettingRefused,
    /// Les réglages ont changé depuis la lecture (`details.by`, `details.at`).
    SettingsConflict,
    /// Ce réglage exige de recréer le service, ce que cet agent ne sait pas encore faire.
    RecreateNotAvailable,
    /// Aucune sauvegarde de cet identifiant.
    BackupNotFound,
    /// Une valeur que cette version ne connaît pas (ajoutée par l'autre côté, plus récent). Tolérée à la
    /// lecture ; jamais prise pour une valeur connue (FIX:01M4VR6Y4Q4AVQVT9FCME7Z2SB).
    #[serde(other)]
    Unknown,
}

impl ErrorCode {
    /// Statut HTTP associé au code.
    pub const fn http_status(self) -> u16 {
        match self {
            Self::Unauthenticated
            | Self::InvalidCredentials
            | Self::SessionExpired
            | Self::SessionRevoked => 401,
            Self::ForbiddenRole | Self::WriteDenied => 403,
            Self::NotFound
            | Self::FicheUnknown
            | Self::ServiceNotFound
            | Self::OperationNotFound
            | Self::BackupNotFound => 404,
            Self::MethodNotAllowed => 405,
            Self::PayloadTooLarge => 413,
            Self::OperationInProgress
            | Self::UsernameTaken
            | Self::LastAdmin
            | Self::Conflict
            | Self::ManagedInstall
            | Self::PostNotRecognized
            | Self::SpecConflict
            | Self::ServiceAdopted
            | Self::NameBeingDeleted
            | Self::LimitRefusedLive
            | Self::LimitsChanged
            | Self::OvercommitChanged
            | Self::ResourceNotLimitable
            | Self::FileConflict
            | Self::FileNotEditable
            | Self::Hardlinked
            | Self::HeavyIoBusy
            | Self::BackupCorrupt
            | Self::BackupInUse
            | Self::RestorePlanChanged
            | Self::HostTooSmall
            | Self::WorldGone
            | Self::TooLateToCancel
            | Self::ServiceBusy
            | Self::ServiceAlreadyInState
            | Self::ServiceNotDrivable
            | Self::NameTaken
            | Self::PortInUse
            | Self::DeployLimit
            | Self::DecisionNotOffered
            | Self::FicheChanged
            | Self::SettingsConflict
            | Self::RecreateNotAvailable => 409,
            Self::ValidationError
            | Self::BadSignature
            | Self::WeakPassword
            | Self::WrongPassword
            | Self::IdempotencyKeyReused
            | Self::PlanSettingRefused
            | Self::PathRefused
            | Self::FolderRefused
            | Self::LimitsRequired
            | Self::AgreementRequired
            | Self::SettingRefused => 422,
            Self::Busy | Self::EngineUnavailable => 503,
            Self::IncompatibleVersion => 426,
            Self::TooManyAttempts | Self::TransferLimit => 429,
            Self::TransferExpired => 410,
            Self::NoSpace => 507,
            // `Unknown` n'existe qu'à la lecture : l'agent ne l'émet jamais, un statut 500 si on le demande.
            Self::InternalError | Self::Unknown => 500,
        }
    }

    /// Le sens d'un statut HTTP quand le code de l'erreur est celui d'une version plus récente
    /// (`Unknown`) : le code le plus générique de ce statut. Un statut que le protocole n'emploie pas
    /// reste `Unknown`. Jamais un succès, jamais « réessayer » de soi-même (BR-COMPAT-001 point 8).
    pub const fn from_status(status: u16) -> Self {
        match status {
            401 => Self::Unauthenticated,
            403 => Self::ForbiddenRole,
            404 => Self::NotFound,
            409 => Self::Conflict,
            422 => Self::ValidationError,
            426 => Self::IncompatibleVersion,
            429 => Self::TooManyAttempts,
            503 => Self::Busy,
            500..=599 => Self::InternalError,
            _ => Self::Unknown,
        }
    }

    /// Le code à interpréter : lui-même, ou, s'il est inconnu de cette version, le sens du statut HTTP
    /// de la réponse (le repli par statut d'avant la tolérance aux valeurs inconnues).
    pub const fn or_from_status(self, status: u16) -> Self {
        match self {
            Self::Unknown => Self::from_status(status),
            known => known,
        }
    }
}

/// Contenu de l'enveloppe `error`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorDetail {
    pub code: ErrorCode,
    pub message: String,
    /// Données complémentaires propres au code (objet vide par défaut).
    #[serde(default = "empty_details")]
    pub details: Value,
}

impl ErrorDetail {
    /// Le droit qui manque à un compte dont le rôle suffit : `details.missing` d'un `403 FORBIDDEN_ROLE`
    /// (BR-GRANT-004), un texte court de l'agent. Rien pour un refus de rôle ordinaire, ni pour une valeur
    /// vide, qui n'est pas un texte ou qui dépasse 32 caractères.
    pub fn missing_right(&self) -> Option<&str> {
        if self.code != ErrorCode::ForbiddenRole {
            return None;
        }
        let right = self.details.get("missing")?.as_str()?;
        (!right.is_empty() && right.chars().count() <= 32).then_some(right)
    }
}

/// Corps de toute réponse d'erreur.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

impl ErrorBody {
    /// Erreur sans détails.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::with_details(code, message, empty_details())
    }

    pub fn with_details(code: ErrorCode, message: impl Into<String>, details: Value) -> Self {
        Self {
            error: ErrorDetail {
                code,
                message: message.into(),
                details,
            },
        }
    }
}

/// Qui doit se mettre à jour quand les versions d'interface sont incompatibles (`details.upgrade`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpgradeTarget {
    Client,
    Agent,
    /// Une valeur que cette version ne connaît pas (ajoutée par l'autre côté, plus récent). Tolérée à la
    /// lecture ; jamais prise pour une valeur connue (FIX:01M4VR6Y4Q4AVQVT9FCME7Z2SB).
    #[serde(other)]
    Unknown,
}

impl ErrorBody {
    /// `426 INCOMPATIBLE_VERSION` avec `details.upgrade`.
    pub fn incompatible_version(upgrade: UpgradeTarget, message: impl Into<String>) -> Self {
        Self::with_details(
            ErrorCode::IncompatibleVersion,
            message,
            serde_json::json!({ "upgrade": upgrade }),
        )
    }

    /// `429 TOO_MANY_ATTEMPTS` avec `details.retry_after_s`.
    pub fn too_many_attempts(retry_after_s: u64, message: impl Into<String>) -> Self {
        Self::with_details(
            ErrorCode::TooManyAttempts,
            message,
            serde_json::json!({ "retry_after_s": retry_after_s }),
        )
    }

    /// `403 FORBIDDEN_ROLE` avec `details.missing` : le droit qui manque (`api::accounts::RIGHT_SERVICES`),
    /// pour un compte à qui son rôle ne manque pas (BR-GRANT-004). Même code que le refus de rôle :
    /// un client ancien le traite comme un refus ; un client récent lit `details.missing`.
    pub fn missing_right(right: &str, message: impl Into<String>) -> Self {
        Self::with_details(
            ErrorCode::ForbiddenRole,
            message,
            serde_json::json!({ "missing": right }),
        )
    }

    /// `422 WEAK_PASSWORD` avec `details.rules` : les règles non respectées, par code stable.
    pub fn weak_password(rules: &[&str], message: impl Into<String>) -> Self {
        Self::with_details(
            ErrorCode::WeakPassword,
            message,
            serde_json::json!({ "rules": rules }),
        )
    }
}

fn empty_details() -> Value {
    Value::Object(serde_json::Map::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// BR-COMPAT-001 point 8 : un code inconnu retombe sur le sens du statut HTTP, jamais sur un succès.
    #[test]
    fn an_unknown_code_falls_back_to_the_meaning_of_its_http_status() {
        for (status, meaning) in [
            (401, ErrorCode::Unauthenticated),
            (403, ErrorCode::ForbiddenRole),
            (404, ErrorCode::NotFound),
            (409, ErrorCode::Conflict),
            (422, ErrorCode::ValidationError),
            (429, ErrorCode::TooManyAttempts),
            (500, ErrorCode::InternalError),
            (502, ErrorCode::InternalError),
            (503, ErrorCode::Busy),
        ] {
            assert_eq!(
                ErrorCode::Unknown.or_from_status(status),
                meaning,
                "{status}"
            );
            // Un code connu garde son sens, quel que soit le statut.
            assert_eq!(
                ErrorCode::Conflict.or_from_status(status),
                ErrorCode::Conflict
            );
        }
        // Un statut que le protocole n'emploie pas ne dit rien de plus.
        assert_eq!(ErrorCode::Unknown.or_from_status(418), ErrorCode::Unknown);
        assert_eq!(ErrorCode::Unknown.or_from_status(200), ErrorCode::Unknown);
    }

    const ALL: [(ErrorCode, &str, u16); 66] = [
        (ErrorCode::Unauthenticated, "UNAUTHENTICATED", 401),
        (ErrorCode::InvalidCredentials, "INVALID_CREDENTIALS", 401),
        (ErrorCode::UsernameTaken, "USERNAME_TAKEN", 409),
        (ErrorCode::WeakPassword, "WEAK_PASSWORD", 422),
        (ErrorCode::WrongPassword, "WRONG_PASSWORD", 422),
        (ErrorCode::LastAdmin, "LAST_ADMIN", 409),
        (ErrorCode::Conflict, "CONFLICT", 409),
        (ErrorCode::Busy, "BUSY", 503),
        (ErrorCode::PayloadTooLarge, "PAYLOAD_TOO_LARGE", 413),
        (
            ErrorCode::IdempotencyKeyReused,
            "IDEMPOTENCY_KEY_REUSED",
            422,
        ),
        (ErrorCode::SessionExpired, "SESSION_EXPIRED", 401),
        (ErrorCode::SessionRevoked, "SESSION_REVOKED", 401),
        (ErrorCode::ForbiddenRole, "FORBIDDEN_ROLE", 403),
        (ErrorCode::OperationInProgress, "OPERATION_IN_PROGRESS", 409),
        (ErrorCode::ValidationError, "VALIDATION_ERROR", 422),
        (ErrorCode::IncompatibleVersion, "INCOMPATIBLE_VERSION", 426),
        (ErrorCode::TooManyAttempts, "TOO_MANY_ATTEMPTS", 429),
        (ErrorCode::InternalError, "INTERNAL_ERROR", 500),
        (ErrorCode::NotFound, "NOT_FOUND", 404),
        (ErrorCode::MethodNotAllowed, "METHOD_NOT_ALLOWED", 405),
        (ErrorCode::ManagedInstall, "MANAGED_INSTALL", 409),
        (ErrorCode::BadSignature, "BAD_SIGNATURE", 422),
        (ErrorCode::PostNotRecognized, "POST_NOT_RECOGNIZED", 409),
        (ErrorCode::SpecConflict, "SPEC_CONFLICT", 409),
        (ErrorCode::ServiceAdopted, "SERVICE_ADOPTED", 409),
        (ErrorCode::PlanSettingRefused, "PLAN_SETTING_REFUSED", 422),
        (ErrorCode::NameBeingDeleted, "NAME_BEING_DELETED", 409),
        (ErrorCode::LimitRefusedLive, "LIMIT_REFUSED_LIVE", 409),
        (ErrorCode::LimitsChanged, "LIMITS_CHANGED", 409),
        (ErrorCode::OvercommitChanged, "OVERCOMMIT_CHANGED", 409),
        (
            ErrorCode::ResourceNotLimitable,
            "RESOURCE_NOT_LIMITABLE",
            409,
        ),
        (ErrorCode::PathRefused, "PATH_REFUSED", 422),
        (ErrorCode::FileConflict, "FILE_CONFLICT", 409),
        (ErrorCode::FileNotEditable, "FILE_NOT_EDITABLE", 409),
        (ErrorCode::Hardlinked, "HARDLINKED", 409),
        (ErrorCode::NoSpace, "NO_SPACE", 507),
        (ErrorCode::WriteDenied, "WRITE_DENIED", 403),
        (ErrorCode::HeavyIoBusy, "HEAVY_IO_BUSY", 409),
        (ErrorCode::BackupCorrupt, "BACKUP_CORRUPT", 409),
        (ErrorCode::BackupInUse, "BACKUP_IN_USE", 409),
        (ErrorCode::RestorePlanChanged, "RESTORE_PLAN_CHANGED", 409),
        (ErrorCode::TransferExpired, "TRANSFER_EXPIRED", 410),
        (ErrorCode::TransferLimit, "TRANSFER_LIMIT", 429),
        (ErrorCode::FicheUnknown, "FICHE_UNKNOWN", 404),
        (ErrorCode::HostTooSmall, "HOST_TOO_SMALL", 409),
        (ErrorCode::WorldGone, "WORLD_GONE", 409),
        (ErrorCode::TooLateToCancel, "TOO_LATE_TO_CANCEL", 409),
        (ErrorCode::EngineUnavailable, "ENGINE_UNAVAILABLE", 503),
        (ErrorCode::ServiceNotFound, "SERVICE_NOT_FOUND", 404),
        (ErrorCode::ServiceBusy, "SERVICE_BUSY", 409),
        (
            ErrorCode::ServiceAlreadyInState,
            "SERVICE_ALREADY_IN_STATE",
            409,
        ),
        (ErrorCode::ServiceNotDrivable, "SERVICE_NOT_DRIVABLE", 409),
        (ErrorCode::NameTaken, "NAME_TAKEN", 409),
        (ErrorCode::PortInUse, "PORT_IN_USE", 409),
        (ErrorCode::FolderRefused, "FOLDER_REFUSED", 422),
        (ErrorCode::LimitsRequired, "LIMITS_REQUIRED", 422),
        (ErrorCode::DeployLimit, "DEPLOY_LIMIT", 409),
        (ErrorCode::OperationNotFound, "OPERATION_NOT_FOUND", 404),
        (ErrorCode::DecisionNotOffered, "DECISION_NOT_OFFERED", 409),
        (ErrorCode::AgreementRequired, "AGREEMENT_REQUIRED", 422),
        (ErrorCode::FicheChanged, "FICHE_CHANGED", 409),
        (ErrorCode::SettingRefused, "SETTING_REFUSED", 422),
        (ErrorCode::SettingsConflict, "SETTINGS_CONFLICT", 409),
        (
            ErrorCode::RecreateNotAvailable,
            "RECREATE_NOT_AVAILABLE",
            409,
        ),
        (ErrorCode::BackupNotFound, "BACKUP_NOT_FOUND", 404),
        (ErrorCode::Unknown, "UNKNOWN", 500),
    ];

    #[test]
    fn codes_serialize_in_screaming_snake_case() {
        for (code, name, _) in ALL {
            assert_eq!(
                serde_json::to_value(code).expect("serialization"),
                json!(name)
            );
        }
    }

    #[test]
    fn codes_map_to_their_http_status() {
        for (code, _, status) in ALL {
            assert_eq!(code.http_status(), status);
        }
    }

    #[test]
    fn body_has_the_documented_envelope() {
        let body = ErrorBody::with_details(
            ErrorCode::IncompatibleVersion,
            "client trop ancien",
            json!({ "upgrade": "client" }),
        );
        assert_eq!(
            serde_json::to_value(&body).expect("serialization"),
            json!({
                "error": {
                    "code": "INCOMPATIBLE_VERSION",
                    "message": "client trop ancien",
                    "details": { "upgrade": "client" }
                }
            })
        );
    }

    #[test]
    fn helpers_build_the_documented_details() {
        let body = ErrorBody::incompatible_version(UpgradeTarget::Agent, "agent trop ancien");
        assert_eq!(body.error.details, json!({ "upgrade": "agent" }));
        let body = ErrorBody::too_many_attempts(60, "attends");
        assert_eq!(body.error.code, ErrorCode::TooManyAttempts);
        assert_eq!(body.error.details, json!({ "retry_after_s": 60 }));
        let body = ErrorBody::missing_right("services", "pas le droit");
        assert_eq!(body.error.code, ErrorCode::ForbiddenRole);
        assert_eq!(body.error.details, json!({ "missing": "services" }));
        assert_eq!(body.error.missing_right(), Some("services"));
        let body = ErrorBody::weak_password(&["min_length", "digit"], "faible");
        assert_eq!(
            body.error.details,
            json!({ "rules": ["min_length", "digit"] })
        );
    }

    #[test]
    fn details_default_to_an_empty_object() {
        let body = ErrorBody::new(ErrorCode::InternalError, "oups");
        assert_eq!(body.error.details, json!({}));
        let parsed: ErrorBody =
            serde_json::from_value(json!({ "error": { "code": "NOT_FOUND", "message": "x" } }))
                .expect("deserialization");
        assert_eq!(parsed.error.details, json!({}));
    }

    #[test]
    fn a_code_this_client_does_not_know_reads_as_unknown_and_never_breaks_the_envelope() {
        // Un agent plus récent : le corps d'erreur entier se lit, le code est `Unknown`, le message et les
        // détails restent. Avant, ce corps était rejeté au décodage et le client perdait la raison.
        let parsed: ErrorBody = serde_json::from_value(json!({
            "error": { "code": "SOMETHING_FROM_THE_FUTURE", "message": "x", "details": { "a": 1 } }
        }))
        .expect("lu");
        assert_eq!(parsed.error.code, ErrorCode::Unknown);
        assert_eq!(parsed.error.details, json!({ "a": 1 }));
        assert_eq!(parsed.error.message, "x");
    }

    #[test]
    fn every_code_is_listed_once_in_the_table_of_the_tests() {
        let mut names: Vec<&str> = ALL.iter().map(|(_, name, _)| *name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), ALL.len());
    }
}
