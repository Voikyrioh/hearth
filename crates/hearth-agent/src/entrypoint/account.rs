//! Sous-commandes `account …` : accès direct à la base du serveur, sans réseau (BR-ACCT-015).
//!
//! Mêmes cas d'usage, mêmes validations et mêmes protections que l'interface : la ligne de
//! commande ne contourne rien (dernier administrateur, règles de mot de passe, fermeture des
//! sessions). Les messages sont ceux de la spécification fonctionnelle.

use std::io::{self, Write};

use thiserror::Error;
use time::OffsetDateTime;

use super::cli::AccountAction;
use crate::application::accounts::{AccountError, AccountService, AccountSummary};
use crate::application::ports::PasswordHasher;
use crate::application::services_right::{RightChange, RightError, ServicesRightService};
use crate::domain::accounts::{Authority, PlainPassword, ServicesRight, Username};
use crate::domain::audit::Actor;
use crate::domain::secret::Secret;

#[derive(Debug, Error)]
pub enum AccountCliError {
    #[error(transparent)]
    Account(#[from] AccountError),
    #[error("Les deux mots de passe ne correspondent pas")]
    PasswordMismatch,
    #[error(
        "Saisie du mot de passe impossible : {0}. Définis HEARTH_ACCOUNT_PASSWORD pour fournir le mot de passe sans terminal."
    )]
    PasswordInput(String),
    #[error("Écriture du résultat impossible : {0}")]
    Output(#[from] io::Error),
    /// BR-GRANT-008 : le droit ne se change qu'en root sur le serveur.
    #[error("{MSG_NOT_ROOT}")]
    NotRoot,
    #[error("{MSG_NO_OPTION}")]
    NoOption,
    /// Le nom, déjà nettoyé pour l'affichage (voir `printable`).
    #[error("Aucun compte « {0} » sur ce serveur.")]
    UnknownAccount(String),
    #[error("Le droit n'a pas pu être changé : {0}")]
    Right(String),
}

/// Source du mot de passe d'un compte : jamais la ligne de commande.
pub trait PasswordInput {
    /// Un nouveau mot de passe, saisi et confirmé (ou fourni par l'environnement).
    fn new_password(&self) -> Result<Secret, AccountCliError>;
}

pub const MSG_NO_ACCOUNT: &str = "Aucun compte n'existe pour le moment. Crée-en un pour commencer.";

// Messages de la commande `account services` (spec du droit de gérer les services, section 4).
pub const MSG_NOT_ROOT: &str = "Cette commande doit être lancée en root sur le serveur.";
pub const MSG_NO_OPTION: &str = "Précise --grant ou --revoke.";

pub fn msg_granted(account: &str) -> String {
    format!("Le compte {account} peut maintenant gérer les services de ce serveur.")
}

pub fn msg_revoked(account: &str) -> String {
    format!("Le compte {account} ne peut plus gérer les services de ce serveur.")
}

pub fn msg_already_granted(account: &str) -> String {
    format!("Le compte {account} peut déjà gérer les services de ce serveur.")
}

pub fn msg_already_revoked(account: &str) -> String {
    format!("Le compte {account} ne pouvait pas gérer les services de ce serveur.")
}

/// BR-GRANT-008 : refuse hors root, AVANT toute lecture de la base. `privileged` vient de la machine
/// (utilisateur effectif), jamais d'une option ni d'une variable.
pub fn require_root(privileged: bool) -> Result<(), AccountCliError> {
    if privileged {
        Ok(())
    } else {
        Err(AccountCliError::NotRoot)
    }
}

/// Le nom tapé, rendu affichable : jamais interprété, jamais rejoué tel quel sur le terminal (un
/// caractère de contrôle, dont ESC, devient « ? »), borné à 64 caractères.
fn printable(name: &str) -> String {
    name.chars()
        .take(64)
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

pub async fn execute(
    action: &AccountAction,
    service: &AccountService,
    rights: &ServicesRightService,
    passwords: &dyn PasswordInput,
    privileged: bool,
    out: &mut dyn Write,
) -> Result<(), AccountCliError> {
    // Origine « ligne de commande du serveur » : pas de compte, pas d'adresse (BR-AUDIT-002).
    let by = Actor::command_line();
    match action {
        AccountAction::Add { username, role } => {
            // Contrôle de l'identifiant avant de demander un mot de passe.
            AccountService::validate_username(username)?;
            let password = passwords.new_password()?;
            let account = service.create(username, password, *role, &by).await?;
            writeln!(out, "Compte {} créé", account.username)?;
        }
        AccountAction::List => {
            let summaries = service.list().await?;
            write_table(out, &summaries)?;
        }
        AccountAction::Passwd { username } => {
            let account = service.find(username).await?;
            let password = passwords.new_password()?;
            service
                .set_password(&account.id, password, &Authority::Server, &by)
                .await?;
            writeln!(out, "Mot de passe changé")?;
        }
        AccountAction::Role { username, role } => {
            let account = service.find(username).await?;
            service.change_role(&account.id, *role, &by).await?;
            writeln!(out, "{} est maintenant {}", account.username, role.label())?;
        }
        AccountAction::Remove { username } => {
            let account = service.find(username).await?;
            service
                .delete(&account.id, &Authority::Server, None, &by)
                .await?;
            writeln!(out, "Compte {} supprimé", account.username)?;
        }
        AccountAction::Revoke { username } => {
            let account = service.find(username).await?;
            service.revoke_sessions(&account.id, &by).await?;
            writeln!(out, "Sessions de {} fermées", account.username)?;
        }
        AccountAction::Services {
            username,
            grant,
            revoke,
        } => {
            // Dans cet ordre : root, option, compte. Rien n'est lu avant que l'on sache que
            // l'exécutant est root ; rien n'est écrit sans une option claire.
            require_root(privileged)?;
            let wanted = match (grant, revoke) {
                (true, false) => ServicesRight::Manage,
                (false, true) => ServicesRight::None,
                _ => return Err(AccountCliError::NoOption),
            };
            change_services_right(username, wanted, rights, &by, out).await?;
        }
    }
    Ok(())
}

/// BR-GRANT-008 et BR-GRANT-009 : change le droit d'un compte et le dit. Le message de réussite n'est
/// écrit qu'APRÈS la validation du changement et de son entrée au journal (une seule transaction).
async fn change_services_right(
    username: &str,
    wanted: ServicesRight,
    rights: &ServicesRightService,
    by: &Actor,
    out: &mut dyn Write,
) -> Result<(), AccountCliError> {
    let fail = |error: RightError| match error {
        RightError::NotFound => AccountCliError::UnknownAccount(printable(username)),
        RightError::Store(error) => AccountCliError::Right(error.to_string()),
    };
    let existing = rights.existing(username).await.map_err(fail)?;
    let change = rights
        .set(existing.as_str(), wanted, by)
        .await
        .map_err(fail)?;
    let name = existing.as_str();
    let line = match (change, wanted.can_manage_services()) {
        (RightChange::Changed, true) => msg_granted(name),
        (RightChange::Changed, false) => msg_revoked(name),
        (RightChange::Unchanged, true) => msg_already_granted(name),
        (RightChange::Unchanged, false) => msg_already_revoked(name),
    };
    writeln!(out, "{line}")?;
    Ok(())
}

/// `hearth-agent hash-password --user NOM` : le haché PHC Argon2id d'un mot de passe saisi sans
/// écho (mêmes règles que pour un compte), écrit seul sur la sortie standard.
pub async fn hash_password(
    user: &str,
    passwords: &dyn PasswordInput,
    hasher: &dyn PasswordHasher,
    out: &mut dyn Write,
) -> Result<(), AccountCliError> {
    let username = Username::parse(user).map_err(AccountError::from)?;
    let password = passwords.new_password()?;
    let plain = PlainPassword::new(password, &username).map_err(AccountError::WeakPassword)?;
    let hash = hasher.hash(&plain).await.map_err(AccountError::from)?;
    writeln!(out, "{}", hash.expose())?;
    Ok(())
}

const NEVER: &str = "jamais";

fn write_table(out: &mut dyn Write, summaries: &[AccountSummary]) -> io::Result<()> {
    if summaries.is_empty() {
        return writeln!(out, "{MSG_NO_ACCOUNT}");
    }
    let header = [
        "Identifiant",
        "Rôle",
        "Créé le",
        "Dernière connexion",
        "Sessions ouvertes",
    ];
    let mut rows: Vec<[String; 5]> = vec![header.map(String::from)];
    for summary in summaries {
        let account = &summary.account;
        rows.push([
            account.username.to_string(),
            account.role.label().to_owned(),
            short_date(account.created_at),
            account
                .last_login_at
                .map_or_else(|| NEVER.to_owned(), short_date),
            summary.sessions_open.to_string(),
        ]);
    }
    let widths: Vec<usize> = (0..header.len())
        .map(|column| {
            rows.iter()
                .map(|row| row[column].chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    for row in &rows {
        let line: Vec<String> = row
            .iter()
            .zip(&widths)
            .map(|(cell, width)| format!("{cell:<width$}"))
            .collect();
        writeln!(out, "{}", line.join("  ").trim_end())?;
    }
    Ok(())
}

/// Date affichée en UTC : `2026-10-04 12:30`.
fn short_date(date: OffsetDateTime) -> String {
    let date = date.to_offset(time::UtcOffset::UTC);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        date.year(),
        u8::from(date.month()),
        date.day(),
        date.hour(),
        date.minute()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Duration;

    #[test]
    fn the_messages_of_the_spec_are_word_for_word() {
        assert_eq!(
            MSG_NOT_ROOT,
            "Cette commande doit être lancée en root sur le serveur."
        );
        assert_eq!(MSG_NO_OPTION, "Précise --grant ou --revoke.");
        assert_eq!(
            AccountCliError::UnknownAccount("marie".into()).to_string(),
            "Aucun compte « marie » sur ce serveur."
        );
        assert_eq!(
            msg_granted("marie"),
            "Le compte marie peut maintenant gérer les services de ce serveur."
        );
        assert_eq!(
            msg_revoked("marie"),
            "Le compte marie ne peut plus gérer les services de ce serveur."
        );
        assert_eq!(
            msg_already_granted("marie"),
            "Le compte marie peut déjà gérer les services de ce serveur."
        );
        assert_eq!(
            msg_already_revoked("marie"),
            "Le compte marie ne pouvait pas gérer les services de ce serveur."
        );
    }

    #[test]
    fn a_name_is_never_replayed_with_control_characters_nor_unbounded() {
        assert_eq!(printable("ma\u{1b}[31mrie"), "ma?[31mrie");
        assert_eq!(printable("a\nb"), "a?b");
        assert_eq!(printable(&"x".repeat(500)).chars().count(), 64);
    }

    #[test]
    fn only_root_passes_the_gate() {
        assert!(require_root(true).is_ok());
        assert!(matches!(require_root(false), Err(AccountCliError::NotRoot)));
    }

    #[test]
    fn dates_are_shown_in_utc_to_the_minute() {
        let date = OffsetDateTime::UNIX_EPOCH + Duration::seconds(1_790_000_000);
        assert_eq!(short_date(date), "2026-09-21 14:13");
    }
}
