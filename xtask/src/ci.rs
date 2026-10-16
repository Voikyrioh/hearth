//! Mode GitHub Actions des tâches de publication (HRT-172, conception « publier une version » 9.5) :
//! annotations (`::error title=…::…`), résumé de l'exécution (`$GITHUB_STEP_SUMMARY`) et sorties du
//! job (`$GITHUB_OUTPUT`). Hors GitHub (`GITHUB_ACTIONS` absent), rien de tout cela n'est écrit : la
//! procédure manuelle garde ses textes (`erreur : …`).
//!
//! Les fonctions qui fabriquent un texte (`error_line`, `escape_data`) et celles qui écrivent dans un
//! fichier donné (`append_output`, `append_summary`) sont pures ou prennent un chemin : les tests ne
//! touchent à aucune variable d'environnement.

use std::io::Write;
use std::path::Path;

/// Titre des annotations : le nom du flux.
pub const TITLE: &str = "Publier une version";

/// Sommes-nous dans un job GitHub Actions ?
pub fn is_github() -> bool {
    std::env::var("GITHUB_ACTIONS").is_ok_and(|value| value == "true")
}

/// Échappement du message d'une commande de flux de travail (`%`, retours à la ligne).
pub fn escape_data(text: &str) -> String {
    text.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

/// Échappement d'une propriété (le titre) : comme le message, plus `:` et `,`.
fn escape_property(text: &str) -> String {
    escape_data(text).replace(':', "%3A").replace(',', "%2C")
}

/// `::{level} title={TITLE}::{message}` : la ligne qui devient une annotation de l'exécution.
pub fn annotation(level: &str, message: &str) -> String {
    format!(
        "::{level} title={}::{}",
        escape_property(TITLE),
        escape_data(message)
    )
}

/// `::error …` (message de la spec en première ligne, détail technique ensuite).
pub fn error_line(message: &str) -> String {
    annotation("error", message)
}

/// Écrit `key=value` à la fin du fichier des sorties du job. Une valeur sur plusieurs lignes est
/// refusée (elle demanderait un délimiteur : aucune sortie de ces tâches n'en a besoin, et refuser
/// vaut mieux qu'écrire une sortie tronquée ou injectée).
pub fn append_output(path: &Path, key: &str, value: &str) -> Result<(), String> {
    let plain = |text: &str| !text.contains(['\n', '\r']);
    if key.is_empty() || key.contains('=') || !plain(key) || !plain(value) {
        return Err(format!("sortie « {key} » : clé ou valeur inutilisable"));
    }
    append(path, &format!("{key}={value}\n"))
}

/// Ajoute du texte au résumé de l'exécution.
pub fn append_summary(path: &Path, text: &str) -> Result<(), String> {
    append(path, &format!("{text}\n"))
}

fn append(path: &Path, text: &str) -> Result<(), String> {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("{} : {error}", path.display()))?;
    file.write_all(text.as_bytes())
        .map_err(|error| format!("{} : {error}", path.display()))
}

fn env_path(name: &str) -> Option<std::path::PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(Into::into)
}

/// Annotation d'erreur et ligne du résumé, en mode GitHub seulement. Rend `true` si l'erreur a été
/// annoncée (l'appelant n'a alors pas à la répéter).
pub fn fail(message: &str) -> bool {
    if !is_github() {
        return false;
    }
    println!("{}", error_line(message));
    summary(&format!("**Refusé.** {message}"));
    true
}

/// Information de l'exécution (annotation `notice`) et ligne du résumé, en mode GitHub seulement.
pub fn notice(message: &str) {
    if is_github() {
        println!("{}", annotation("notice", message));
        summary(message);
    }
}

/// Texte ajouté au résumé de l'exécution, en mode GitHub seulement. Le résumé est un confort : une
/// écriture impossible n'arrête jamais la publication.
pub fn summary(text: &str) {
    if is_github()
        && let Some(path) = env_path("GITHUB_STEP_SUMMARY")
    {
        let _ = append_summary(&path, text);
    }
}

/// Sortie du job (`version`, `commit`), en mode GitHub seulement. Contrairement au résumé, une sortie
/// manquante casserait les jobs suivants : l'erreur est rendue.
pub fn output(key: &str, value: &str) -> Result<(), String> {
    if !is_github() {
        return Ok(());
    }
    let path = env_path("GITHUB_OUTPUT").ok_or("GITHUB_OUTPUT est absent")?;
    append_output(&path, key, value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_escapes_percent_and_line_breaks_only() {
        assert_eq!(escape_data("a%b\r\nc\nd"), "a%25b%0D%0Ac%0Ad");
        assert_eq!(
            escape_data("La version 1.2.0 : ok, vraiment"),
            "La version 1.2.0 : ok, vraiment"
        );
    }

    #[test]
    fn the_error_annotation_carries_the_flow_title_and_the_escaped_message() {
        assert_eq!(
            error_line("La publication ne part que de main."),
            "::error title=Publier une version::La publication ne part que de main."
        );
        assert_eq!(
            error_line("premier\ndétail 100%"),
            "::error title=Publier une version::premier%0Adétail 100%25"
        );
        assert_eq!(
            annotation("notice", "x"),
            "::notice title=Publier une version::x"
        );
    }

    #[test]
    fn a_property_escapes_colon_and_comma_too() {
        assert_eq!(escape_property("a:b,c%\n"), "a%3Ab%2Cc%25%0A");
    }

    #[test]
    fn outputs_are_appended_as_key_value_lines() {
        let dir = crate::release_stage::fixtures::tmp_dir("ci-test");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("output");
        append_output(&file, "version", "1.2.0").unwrap();
        append_output(&file, "commit", "abc").unwrap();
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "version=1.2.0\ncommit=abc\n"
        );
        let summary = dir.join("summary");
        append_summary(&summary, "un").unwrap();
        append_summary(&summary, "deux").unwrap();
        assert_eq!(std::fs::read_to_string(&summary).unwrap(), "un\ndeux\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_output_that_could_inject_another_line_is_refused() {
        let nowhere = Path::new("hearth-ci-test-never-written");
        for (key, value) in [
            ("version", "1.2.0\nevil=1"),
            ("version", "1.2.0\r"),
            ("a=b", "x"),
            ("", "x"),
            ("a\nb", "x"),
        ] {
            assert!(
                append_output(nowhere, key, value).is_err(),
                "{key:?} {value:?}"
            );
        }
        assert!(!nowhere.exists());
    }
}
