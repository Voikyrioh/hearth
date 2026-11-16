//! La CSP du client (HRT-239, ADR-0061) : la page ne peut joindre AUCUN hôte externe. Les images des jeux passent par
//! le protocole de la coquille (`hearth-illustration`), jamais par une adresse Steam dans la CSP, et `data:` n'est
//! admis qu'en développement.

use serde_json::Value;

fn security() -> (String, String) {
    let raw = include_str!("../tauri.conf.json");
    let conf: Value = serde_json::from_str(raw).expect("tauri.conf.json");
    let csp = |key: &str| {
        conf["app"]["security"][key]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    };
    (csp("csp"), csp("devCsp"))
}

fn directive<'a>(csp: &'a str, name: &str) -> &'a str {
    csp.split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix(name).map(str::trim))
        .unwrap_or_default()
}

#[test]
fn images_come_only_from_the_page_itself_and_the_shell_protocol() {
    let (production, development) = security();
    for csp in [&production, &development] {
        let images = directive(csp, "img-src");
        assert!(images.contains("'self'"));
        assert!(images.contains("hearth-illustration:"));
        assert!(images.contains("http://hearth-illustration.localhost"));
    }
    assert!(
        !directive(&production, "img-src").contains("data:"),
        "jamais data: en production"
    );
}

#[test]
fn no_external_host_appears_in_any_policy() {
    let (production, development) = security();
    for (name, csp) in [("production", &production), ("développement", &development)] {
        for token in csp.split(|c: char| c.is_whitespace() || c == ';') {
            if let Some(rest) = token
                .strip_prefix("http://")
                .or_else(|| token.strip_prefix("https://"))
            {
                let host = rest.split('/').next().unwrap_or_default();
                let allowed = [
                    "ipc.localhost",
                    "hearth-illustration.localhost",
                    "localhost:1420",
                ];
                assert!(allowed.contains(&host), "{name} : hôte externe {host}");
            }
        }
        assert!(
            !csp.contains("steamstatic") && !csp.contains("valve") && !csp.contains("akamai"),
            "{name}"
        );
    }
    assert!(!production.contains("ws://") && !production.contains("localhost:1420"));
}
