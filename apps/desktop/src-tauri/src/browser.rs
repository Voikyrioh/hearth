//! Ouvrir une adresse dans le navigateur du PC (HRT-102). UN seul endroit pour toute la coquille : l'adresse ne part
//! à l'Explorateur de Windows qu'après la garde ci-dessous, jamais telle que la page ou l'agent l'a écrite.

use std::process::Command;

/// Une adresse que l'on peut faire ouvrir : `https`, un vrai nom de domaine (jamais une adresse IP, ni IPv4 ni IPv6), aucun identifiant dans l'adresse,
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
        && is_domain_name(host_of(authority))
        && url
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~:/?#=&%+!".contains(&b))
}

/// Le nom d'hôte de l'autorité (sans le port).
fn host_of(authority: &str) -> &str {
    if authority.starts_with('[') {
        return authority;
    }
    authority.split(':').next().unwrap_or("")
}

/// Un vrai nom de domaine : au moins deux étiquettes de lettres, chiffres et tirets, une dernière étiquette qui n'est
/// pas un nombre (ce qui écarte `192.168.1.1`) ; un crochet (IPv6) ou un deux-points restant le refuse.
fn is_domain_name(host: &str) -> bool {
    let labels: Vec<&str> = host.split('.').collect();
    labels.len() >= 2
        && labels.iter().all(|label| {
            !label.is_empty()
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
        && labels
            .last()
            .is_some_and(|tld| tld.bytes().any(|b| b.is_ascii_alphabetic()))
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
            "https://192.168.1.1/",
            "https://127.0.0.1:8080/x",
            "https://[::1]/",
            "https://[2001:db8::1]:443/",
            "https://1.2.3.4.5/",
            "https://-a.example.org/",
            "https://example..org/",
            "https://exa_mple.org/",
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
