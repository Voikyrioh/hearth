//! Ce que la coquille dit à l'interface sur les comptes : types sérialisés (typés par
//! tauri-specta). Aucun mot de passe n'apparaît ici, ni en entrée ni en sortie : les mots de passe
//! ne traversent que les paramètres des commandes, jamais un type qui dérive `Debug`.

use hearth_proto::account_rules::{InputCheck, PasswordRule, UsernameProblem};
use hearth_proto::api::accounts::AccountItem;
use serde::Serialize;
use specta::Type;

use crate::link_dto::RoleDto;

/// Un compte de la liste d'administration. Les dates sont celles de l'agent (RFC 3339, UTC) ;
/// l'interface les met en forme.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountDto {
    pub id: String,
    pub username: String,
    pub role: RoleDto,
    pub created_at: String,
    pub last_login_at: Option<String>,
    pub sessions_open: u32,
}

impl From<AccountItem> for AccountDto {
    fn from(item: AccountItem) -> Self {
        Self {
            id: item.id,
            username: item.username,
            role: item.role.into(),
            created_at: item.created_at,
            last_login_at: item.last_login_at,
            sessions_open: u32::try_from(item.sessions_open).unwrap_or(u32::MAX),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum UsernameProblemDto {
    Empty,
    TooShort,
    TooLong,
    InvalidChars,
}

impl From<UsernameProblem> for UsernameProblemDto {
    fn from(problem: UsernameProblem) -> Self {
        match problem {
            UsernameProblem::Empty => Self::Empty,
            UsernameProblem::TooShort => Self::TooShort,
            UsernameProblem::TooLong => Self::TooLong,
            UsernameProblem::InvalidChars => Self::InvalidChars,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PasswordRuleDto {
    Required,
    MinLength,
    Digit,
    Lowercase,
    Uppercase,
    ContainsUsername,
}

impl From<PasswordRule> for PasswordRuleDto {
    fn from(rule: PasswordRule) -> Self {
        match rule {
            PasswordRule::Required => Self::Required,
            PasswordRule::MinLength => Self::MinLength,
            PasswordRule::Digit => Self::Digit,
            PasswordRule::Lowercase => Self::Lowercase,
            PasswordRule::Uppercase => Self::Uppercase,
            PasswordRule::ContainsUsername => Self::ContainsUsername,
        }
    }
}

/// Verdict de la saisie en direct (`check_account_input`) : la règle est celle de `hearth-proto`,
/// la même que celle de l'agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountInputCheck {
    pub username: Option<UsernameProblemDto>,
    pub password: Vec<PasswordRuleDto>,
}

impl From<InputCheck> for AccountInputCheck {
    fn from(check: InputCheck) -> Self {
        Self {
            username: check.username.map(Into::into),
            password: check.password.into_iter().map(Into::into).collect(),
        }
    }
}

/// Pourquoi l'agent (ou la validation locale, avant tout envoi) a refusé. Sans texte : l'interface
/// choisit le message d'après `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AccountRefusal {
    /// Rôle insuffisant (BR-ACCT-013, 014) : l'agent est l'arbitre, même si le client est contourné.
    Forbidden,
    /// Identifiant au mauvais format : `problem` renseigné quand la validation locale l'a vu.
    InvalidUsername {
        problem: Option<UsernameProblemDto>,
    },
    /// Mot de passe refusé : toutes les règles non respectées.
    WeakPassword {
        rules: Vec<PasswordRuleDto>,
    },
    UsernameTaken,
    /// L'ancien mot de passe est incorrect.
    WrongPassword,
    /// Il doit toujours rester au moins un administrateur (BR-ACCT-007).
    LastAdmin,
    NotFound,
    /// L'identifiant retapé pour supprimer son propre compte ne correspond pas (BR-ACCT-012).
    ConfirmationMismatch,
    /// Le mot de passe a changé entre la vérification et l'écriture : réessayer.
    Conflict,
    /// L'agent est saturé : réessayer dans un instant.
    Busy,
    /// La session a pris fin pendant l'action : l'état du lien le dit.
    SessionEnded,
    Other,
}

/// Issue d'une action de compte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AccountOutcome {
    /// L'agent a exécuté l'action. `account` : le compte créé ; `sessions_closed` : sessions
    /// fermées (0 si l'action n'en ferme pas).
    Done {
        account: Option<AccountDto>,
        sessions_closed: u32,
    },
    Refused {
        refusal: AccountRefusal,
    },
    /// Le lien est tombé avant la réponse : on ne sait pas, l'action n'est JAMAIS rejouée.
    /// L'issue arrive par `link://operation` sous cet identifiant ; la liste se relit au retour du
    /// lien.
    Unknown {
        op_id: String,
    },
}

/// Résultat de la lecture de la liste.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AccountListDto {
    Listed { accounts: Vec<AccountDto> },
    Refused { refusal: AccountRefusal },
}
