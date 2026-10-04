//! Règles des comptes (BR-ACCT-*) : identifiant, mot de passe, rôle, dernier administrateur,
//! confirmation de suppression de son propre compte.

pub mod account;
pub mod admin_guard;
pub mod password;
pub mod role;
pub mod self_deletion;
pub mod username;

pub use account::{Account, AccountId};
pub use admin_guard::{LastAdminError, check_removal, check_role_change};
pub use password::{PasswordRejected, PasswordRule, PlainPassword};
pub use role::{Role, UnknownRole};
pub use self_deletion::{ConfirmationMismatch, confirm_self_deletion};
pub use username::{Username, UsernameError};
