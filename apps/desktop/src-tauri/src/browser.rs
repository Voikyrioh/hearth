//! Ouvrir une adresse dans le navigateur du PC (HRT-102). UN seul endroit pour toute la coquille : l'adresse ne part
//! à l'Explorateur de Windows qu'après la garde ci-dessous, jamais telle que la page ou l'agent l'a écrite.

use std::process::Command;

/// Une adresse que l'on peut faire ouvrir : `https`, un nom de domaine (un point), aucun identifiant dans l'adresse,
/// 2 048 caractères au plus, et seulement des caractères sans effet sur la ligne de commande de l'Explorateur (ni
/// espace, ni guillemet, ni virgule ni point-virgule, que l'Explorateur lit comme des séparateurs).
pub fn is_safe_https_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    url.len() <= 2048
        && !authority.is_empty()
        && !authority.contains('@')
        && authority.contains('.')
        && url
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~:/?#=&%+!".contains(&b))
}

/// Ouvre `url` dans le navigateur ; `false` : l'adresse est refusée par la garde ou le lancement a échoué.
pub fn open_in_browser(url: &str) -> bool {
    is_safe_https_url(url) && Command::new("explorer").arg(url).spawn().map(drop).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_plain_https_address_with_a_domain_ever_opens() {
        assert!(is_safe_https_url("https://example.org/licence"));
        assert!(is_safe_https_url(
            "https://store.example.org/jetons?lang=fr#haut"
        ));
        for bad in [
            "http://example.org/licence",
            "https://",
            "https:///x",
            "https://user:pass@example.org/",
            "https://localhost/x",
            "https://example.org/a b",
            "https://example.org/\"x",
            "https://example.org/^calc",
            "https://example.org/a,b",
            "https://example.org/a;b",
            "file:///c:/windows/system32/calc.exe",
            "ftp://example.org/x",
            "javascript:alert(1)",
        ] {
            assert!(!is_safe_https_url(bad), "{bad}");
        }
        assert!(!is_safe_https_url(&format!(
            "https://example.org/{}",
            "a".repeat(2100)
        )));
    }

    #[test]
    fn the_fixed_documentation_address_passes_the_guard() {
        assert!(is_safe_https_url(
            crate::services::commands::ENGINE_DOCS_URL
        ));
    }

    #[test]
    fn a_refused_address_is_never_launched() {
        assert!(!open_in_browser("file:///c:/windows/system32/calc.exe"));
    }
}
