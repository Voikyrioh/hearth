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
use crate::domain::accounts::{PlainPassword, Username};
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
}

/// Source du mot de passe d'un compte : jamais la ligne de commande.
pub trait PasswordInput {
    /// Un nouveau mot de passe, saisi et confirmé (ou fourni par l'environnement).
    fn new_password(&self) -> Result<Secret, AccountCliError>;
}

pub const MSG_NO_ACCOUNT: &str = "Aucun compte n'existe pour le moment. Crée-en un pour commencer.";

pub async fn execute(
    action: &AccountAction,
    service: &AccountService,
    passwords: &dyn PasswordInput,
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
            service.set_password(&account.id, password, &by).await?;
            writeln!(out, "Mot de passe changé")?;
        }
        AccountAction::Role { username, role } => {
            let account = service.find(username).await?;
            service.change_role(&account.id, *role, &by).await?;
            writeln!(out, "{} est maintenant {}", account.username, role.label())?;
        }
        AccountAction::Remove { username } => {
            let account = service.find(username).await?;
            service.delete(&account.id, None, None, &by).await?;
            writeln!(out, "Compte {} supprimé", account.username)?;
        }
        AccountAction::Revoke { username } => {
            let account = service.find(username).await?;
            service.revoke_sessions(&account.id, &by).await?;
            writeln!(out, "Sessions de {} fermées", account.username)?;
        }
    }
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
    fn dates_are_shown_in_utc_to_the_minute() {
        let date = OffsetDateTime::UNIX_EPOCH + Duration::seconds(1_790_000_000);
        assert_eq!(short_date(date), "2026-09-21 14:13");
    }
}
