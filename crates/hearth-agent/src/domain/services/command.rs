//! Découper la ligne de commande d'un déploiement en arguments, SANS interpréteur (HRT-68, BR-DEPLOY-014).
//!
//! Les arguments sont séparés par des espaces ; une paire de guillemets simples ou doubles regroupe ce
//! qu'elle contient en un argument (espaces compris) ; des morceaux accolés forment un seul argument
//! (`a"b c"d` donne `ab cd`). Rien d'autre n'a de sens : ni redirection, ni enchaînement, ni variable, ni
//! échappement (`\` est un caractère comme un autre). La commande est passée au moteur en tableau, jamais à
//! un interpréteur de commandes.

use thiserror::Error;

use super::limits::{MAX_COMMAND_ARG_CHARS, MAX_COMMAND_ARGS};
use hearth_proto::api::deploy::MAX_COMMAND_CHARS;

/// Pourquoi une ligne de commande est refusée. Aucune variante ne porte un morceau de la commande (c'est un
/// secret pour l'agent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CommandError {
    #[error("la commande dépasse {MAX_COMMAND_CHARS} caractères")]
    TooLong,
    #[error("un guillemet n'est pas fermé")]
    UnclosedQuote,
    #[error("octet nul ou caractère de contrôle dans la commande")]
    Control,
    #[error("trop d'arguments, ou un argument trop long")]
    TooManyArguments,
    #[error("la commande est vide")]
    Empty,
}

/// Découpe `line` en arguments. Une ligne vide ou faite d'espaces est refusée : une commande facultative
/// absente se dit `None` en amont, pas par une chaîne vide.
pub fn split_command(line: &str) -> Result<Vec<String>, CommandError> {
    if line.chars().count() > MAX_COMMAND_CHARS {
        return Err(CommandError::TooLong);
    }
    // Un retour à la ligne sépare comme une espace ; un autre caractère de contrôle (nul, échappement) est refusé.
    if line
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\t' | '\n' | '\r'))
    {
        return Err(CommandError::Control);
    }
    let mut args: Vec<String> = Vec::new();
    let mut current = String::new();
    // `started` : un argument est en cours, même vide (`""` est un argument vide).
    let mut started = false;
    let mut quote: Option<char> = None;
    for c in line.chars() {
        match quote {
            Some(open) if c == open => quote = None,
            Some(_) => current.push(c),
            None => match c {
                '\'' | '"' => {
                    quote = Some(c);
                    started = true;
                }
                c if c.is_whitespace() => {
                    if started {
                        args.push(std::mem::take(&mut current));
                        started = false;
                    }
                }
                c => {
                    current.push(c);
                    started = true;
                }
            },
        }
    }
    if quote.is_some() {
        return Err(CommandError::UnclosedQuote);
    }
    if started {
        args.push(current);
    }
    if args.is_empty() {
        return Err(CommandError::Empty);
    }
    if args.len() > MAX_COMMAND_ARGS
        || args
            .iter()
            .any(|arg| arg.chars().count() > MAX_COMMAND_ARG_CHARS)
    {
        return Err(CommandError::TooManyArguments);
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(line: &str) -> Vec<String> {
        split_command(line).expect("découpée")
    }

    #[test]
    fn spaces_separate_and_quotes_group() {
        assert_eq!(split("run --port 8080"), ["run", "--port", "8080"]);
        assert_eq!(split("  a   b  "), ["a", "b"]);
        assert_eq!(
            split(r#"sh -c "echo hi there""#),
            ["sh", "-c", "echo hi there"]
        );
        assert_eq!(split("say 'a \"b\" c'"), ["say", "a \"b\" c"]);
        // Des morceaux accolés ne font qu'un argument ; des guillemets vides font un argument vide.
        assert_eq!(split(r#"a"b c"d"#), ["ab cd"]);
        assert_eq!(split(r#"x "" y"#), ["x", "", "y"]);
    }

    #[test]
    fn no_interpreter_means_shell_syntax_is_plain_text() {
        assert_eq!(
            split("a | b > c && d ; e $HOME `x` \\n"),
            [
                "a", "|", "b", ">", "c", "&&", "d", ";", "e", "$HOME", "`x`", "\\n"
            ]
        );
    }

    #[test]
    fn an_open_quote_a_control_character_or_nothing_is_refused() {
        assert_eq!(split_command("a \"b"), Err(CommandError::UnclosedQuote));
        assert_eq!(split_command("a 'b"), Err(CommandError::UnclosedQuote));
        assert_eq!(split_command("a\0b"), Err(CommandError::Control));
        assert_eq!(split_command("a\u{1b}[31m"), Err(CommandError::Control));
        assert_eq!(split_command("   "), Err(CommandError::Empty));
        assert_eq!(split_command(""), Err(CommandError::Empty));
    }

    #[test]
    fn the_bounds_are_enforced() {
        assert_eq!(
            split_command(&"a".repeat(MAX_COMMAND_CHARS + 1)),
            Err(CommandError::TooLong)
        );
        let many = vec!["x"; MAX_COMMAND_ARGS + 1].join(" ");
        assert_eq!(split_command(&many), Err(CommandError::TooManyArguments));
        assert_eq!(
            split(&vec!["x"; MAX_COMMAND_ARGS].join(" ")).len(),
            MAX_COMMAND_ARGS
        );
        let long_arg = "y".repeat(MAX_COMMAND_ARG_CHARS + 1);
        assert_eq!(
            split_command(&long_arg),
            Err(CommandError::TooManyArguments)
        );
    }

    #[test]
    fn an_error_never_carries_a_piece_of_the_command() {
        let shown = format!(
            "{:?} {}",
            CommandError::UnclosedQuote,
            CommandError::Control
        );
        assert!(!shown.contains("SENTINEL"));
        let err = split_command("SENTINEL \"x").unwrap_err();
        assert!(!format!("{err:?} {err}").contains("SENTINEL"));
    }
}
