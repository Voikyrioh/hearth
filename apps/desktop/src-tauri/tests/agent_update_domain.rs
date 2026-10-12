//! Règles pures de la mise à jour de l'agent depuis le client (HRT-17, ADR-0021) : ce que le client
//! accepte comme cible, quand une version est « disponible », ce qu'il ne propose ni n'envoie
//! JAMAIS (rétrogradation, adresse non HTTPS, locale ou hors du dépôt prévu), et le fichier de
//! cibles. La sécurité reste celle de l'agent (signature, somme) : ces tests prouvent que le client
//! ne lui envoie rien qu'il sait mauvais.
#![allow(clippy::unwrap_used, clippy::expect_used)] // tests : les helpers peuvent paniquer

use hearth_desktop_lib::agent_update::domain::{
    AGENT_PLATFORM, AgentCandidate, AgentTarget, AgentTargetRecord, MANIFEST_MAX_BYTES,
    ManifestError, TargetRejection, host_is_local, is_local_ip, is_newer, parse_agent_section,
    validate_target,
};
use hearth_desktop_lib::update::domain::DownloadPolicy;
use serde_json::json;
use url::Url;

const URL: &str = "https://hearth.voikyrioh.fr/api/releases/0.2.0/hearth-agent-linux-x86_64";
const SIGNATURE: &str =
    "untrusted comment: signature from minisign secret key\nRUQabc\ntrusted comment: x\nabc\n";

fn sha() -> String {
    "ab".repeat(32)
}

fn candidate() -> AgentCandidate {
    AgentCandidate {
        version: "0.2.0".into(),
        url: URL.into(),
        signature: SIGNATURE.into(),
        sha256: sha(),
    }
}

fn policy() -> DownloadPolicy {
    DownloadPolicy::official_releases()
}

fn target(version: &str) -> AgentTarget {
    validate_target(
        &AgentCandidate {
            version: version.into(),
            ..candidate()
        },
        &policy(),
    )
    .unwrap()
}

#[test]
fn a_well_formed_target_from_the_release_of_the_repository_is_retained() {
    let target = validate_target(&candidate(), &policy()).unwrap();
    assert_eq!(target.version().to_string(), "0.2.0");
    assert_eq!(target.url().as_str(), URL);
    assert_eq!(target.sha256(), sha());
    assert!(target.signature().starts_with("untrusted comment:"));
}

#[test]
fn the_checksum_is_normalised_to_lowercase_and_the_signature_may_be_base64() {
    let mut c = candidate();
    c.sha256 = "AB".repeat(32);
    c.signature = "dW50cnVzdGVkIGNvbW1lbnQ6IHg=".into();
    let target = validate_target(&c, &policy()).unwrap();
    assert_eq!(target.sha256(), sha());
}

#[test]
fn an_unreadable_version_or_a_prerelease_is_never_retained() {
    for bad in [
        "",
        "0.2",
        "v0.2.0",
        "0.2.0-beta.1",
        "0.2.0+1",
        "a.b.c",
        "0.2.0 ",
    ] {
        let mut c = candidate();
        c.version = bad.into();
        let result = validate_target(&c, &policy());
        // « 0.2.0 » avec une espace de bord est rogné : seul ce cas est accepté.
        if bad == "0.2.0 " {
            assert!(result.is_ok());
        } else {
            assert_eq!(
                result.unwrap_err(),
                TargetRejection::Malformed(match bad {
                    "0.2.0-beta.1" | "0.2.0+1" => "préversion ou métadonnées de construction",
                    _ => "numéro de version",
                }),
                "{bad}"
            );
        }
    }
}

#[test]
fn a_bad_checksum_or_signature_is_never_retained() {
    for bad in ["", "ab", &"zz".repeat(32), &"ab".repeat(33)] {
        let mut c = candidate();
        c.sha256 = bad.into();
        assert_eq!(
            validate_target(&c, &policy()).unwrap_err(),
            TargetRejection::Malformed("somme SHA-256"),
            "{bad}"
        );
    }
    for bad in ["", "   ", "x\u{0}y", &"a".repeat(5000)] {
        let mut c = candidate();
        c.signature = bad.into();
        assert_eq!(
            validate_target(&c, &policy()).unwrap_err(),
            TargetRejection::Malformed("signature"),
            "{bad:?}"
        );
    }
}

#[test]
fn an_address_that_is_not_https_is_never_sent() {
    for bad in [
        "http://hearth.voikyrioh.fr/api/releases/0.2.0/a",
        "ftp://hearth.voikyrioh.fr/api/releases/0.2.0/a",
        "file:///etc/passwd",
    ] {
        let mut c = candidate();
        c.url = bad.into();
        let error = validate_target(&c, &policy()).unwrap_err();
        assert!(
            matches!(
                error,
                TargetRejection::NotHttps | TargetRejection::Malformed(_)
            ),
            "{bad} : {error:?}"
        );
    }
}

#[test]
fn a_local_or_private_address_is_never_sent() {
    for host in [
        "127.0.0.1",
        "10.0.0.5",
        "192.168.1.20",
        "172.16.0.1",
        "169.254.1.1",
        "100.64.0.1",
        "0.0.0.0",
        "[::1]",
        "[fd00::1]",
        "[fe80::1]",
        "[::ffff:127.0.0.1]",
        "localhost",
        "forge.local",
        "serveur",
        "nas.internal",
        "2130706433",
        "0x7f.1",
    ] {
        let mut c = candidate();
        c.url = format!("https://{host}/api/releases/0.2.0/a");
        let error = validate_target(&c, &policy()).unwrap_err();
        assert!(
            matches!(
                error,
                TargetRejection::LocalAddress
                    | TargetRejection::UntrustedSource
                    | TargetRejection::Malformed(_)
            ),
            "{host} : {error:?}"
        );
    }
}

#[test]
fn the_local_table_says_what_it_means() {
    for ip in [
        "127.0.0.1",
        "10.1.2.3",
        "192.168.0.1",
        "100.100.0.1",
        "::1",
        "fc00::1",
    ] {
        assert!(is_local_ip(ip.parse().unwrap()), "{ip}");
    }
    for ip in [
        "8.8.8.8",
        "140.82.121.4",
        "2606:4700::1111",
        "100.63.0.1",
        "100.128.0.1",
    ] {
        assert!(!is_local_ip(ip.parse().unwrap()), "{ip}");
    }
    assert!(host_is_local(&Url::parse("https://localhost./x").unwrap()));
    assert!(!host_is_local(
        &Url::parse("https://hearth.voikyrioh.fr/x").unwrap()
    ));
}

#[test]
fn an_address_outside_the_release_server_is_never_sent() {
    for bad in [
        // Autre hôte, hôte en suffixe, sous-domaine, ancienne source (GitHub).
        "https://exemple.org/api/releases/0.2.0/a",
        "https://hearth.voikyrioh.fr.evil.example/api/releases/0.2.0/a",
        "https://evil.hearth.voikyrioh.fr/api/releases/0.2.0/a",
        "https://github.com/Voikyrioh/hearth/releases/download/v0.2.0/a",
        // Chemin hors de `/api/releases/`.
        "https://hearth.voikyrioh.fr/",
        "https://hearth.voikyrioh.fr/api/install.sh",
        "https://hearth.voikyrioh.fr/api/releases-evil/0.2.0/a",
        "https://hearth.voikyrioh.fr/api/releases/../install.sh",
        // Identifiant, port, fragment.
        "https://user:pass@hearth.voikyrioh.fr/api/releases/0.2.0/a",
        "https://hearth.voikyrioh.fr:8443/api/releases/0.2.0/a",
        "https://hearth.voikyrioh.fr/api/releases/0.2.0/a#fragment",
    ] {
        let mut c = candidate();
        c.url = bad.into();
        assert!(validate_target(&c, &policy()).is_err(), "{bad}");
    }
    let mut long = candidate();
    long.url = format!("{URL}/{}", "a".repeat(2100));
    assert_eq!(
        validate_target(&long, &policy()).unwrap_err(),
        TargetRejection::Malformed("adresse trop longue")
    );
}

#[test]
fn only_a_strictly_newer_version_is_available_never_a_downgrade_or_the_same() {
    let target = target("0.2.0");
    assert!(is_newer("0.1.0", &target));
    assert!(is_newer("0.1.9", &target));
    assert!(!is_newer("0.2.0", &target), "la même version");
    assert!(!is_newer("0.2.1", &target), "rétrogradation");
    assert!(!is_newer("1.0.0", &target), "rétrogradation majeure");
    // Une version d'agent illisible ou en préversion ne se compare pas : rien n'est proposé.
    assert!(!is_newer("inconnue", &target));
    assert!(!is_newer("", &target));
    assert!(!is_newer("0.1.0-dev", &target));
}

#[test]
fn a_retained_target_is_validated_again_when_read_back() {
    let valid = validate_target(&candidate(), &policy()).unwrap();
    let record = AgentTargetRecord::from(&valid);
    assert_eq!(AgentTarget::from_record(&record, &policy()).unwrap(), valid);
    // Le fichier d'état modifié à la main : la cible modifiée n'est plus rendue.
    for tamper in [
        |r: &mut AgentTargetRecord| r.url = "https://exemple.org/hearth-agent".into(),
        |r: &mut AgentTargetRecord| {
            r.url = "http://hearth.voikyrioh.fr/api/releases/0.2.0/a".into()
        },
        |r: &mut AgentTargetRecord| r.sha256 = "00".into(),
        |r: &mut AgentTargetRecord| r.version = "0.2.0-evil".into(),
    ] {
        let mut changed = record.clone();
        tamper(&mut changed);
        assert!(AgentTarget::from_record(&changed, &policy()).is_err());
    }
}

#[test]
fn the_record_uses_camel_case_and_old_files_without_a_target_still_read() {
    let record = AgentTargetRecord::from(&validate_target(&candidate(), &policy()).unwrap());
    let value = serde_json::to_value(&record).unwrap();
    assert_eq!(value["sha256"], sha());
    assert!(value.get("signature").is_some());
    // Un update.json d'avant ce lot n'a pas de cible : il se relit tel quel.
    let old: hearth_desktop_lib::update::domain::UpdateRecord =
        serde_json::from_value(json!({ "lastAttemptAt": 1 })).unwrap();
    assert!(old.agent.is_none());
}

/// Un `latest.json` du client portant une section `agent`.
fn manifest(version: &str, entry: serde_json::Value) -> serde_json::Value {
    json!({
        "version": "9.9.9",
        "notes": "Notes du client.",
        "platforms": { "windows-x86_64": { "url": "https://x", "signature": "s" } },
        "agent": { "version": version, "platforms": { AGENT_PLATFORM: entry } }
    })
}

#[test]
fn the_agent_section_gives_one_candidate_for_the_platform_of_the_agent() {
    let value = manifest(
        "0.2.0",
        json!({ "url": URL, "signature": SIGNATURE, "sha256": sha() }),
    );
    assert_eq!(parse_agent_section(&value).unwrap(), Some(candidate()));
}

#[test]
fn a_manifest_without_an_agent_section_offers_nothing_and_a_broken_one_is_an_error() {
    // Le manifeste du client seul : pas de section de l'agent, ce n'est pas une panne.
    let client_only = json!({
        "version": "9.9.9",
        "platforms": { "windows-x86_64": { "url": "https://x", "signature": "s" } }
    });
    assert_eq!(parse_agent_section(&client_only).unwrap(), None);
    assert_eq!(parse_agent_section(&json!({})).unwrap(), None);
    // Une section sans entrée pour la plateforme de l'agent : rien à proposer.
    assert_eq!(
        parse_agent_section(&json!({ "agent": { "version": "0.2.0", "platforms": {} } })).unwrap(),
        None
    );
    // L'entrée existe mais il manque des champs : illisible.
    assert_eq!(
        parse_agent_section(&manifest("0.2.0", json!({ "url": URL }))).unwrap_err(),
        ManifestError::Unreadable
    );
    let no_version = json!({ "agent": { "platforms": { AGENT_PLATFORM: { "url": URL, "signature": SIGNATURE, "sha256": sha() } } } });
    assert_eq!(
        parse_agent_section(&no_version).unwrap_err(),
        ManifestError::Unreadable
    );
    let huge =
        json!({ "agent": { "version": "0.2.0", "padding": "x".repeat(MANIFEST_MAX_BYTES + 1) } });
    assert_eq!(
        parse_agent_section(&huge).unwrap_err(),
        ManifestError::TooLarge
    );
}

/// Cas limites de l'analyse d'adresse (voir `update_domain.rs`) : relevés en exécutant la bibliothèque
/// `url`, fixés ici pour que sa montée de version ne les change pas sans bruit.
#[test]
fn address_edge_cases_keep_their_recorded_meaning() {
    let refused = [
        "https://hearth.voikyrioh.fr./api/releases/0.2.0/a",
        "https://hearth.voikyrioh.fr\u{3002}/api/releases/0.2.0/a",
        r"https://hearth.voikyrioh.fr\@evil.test/api/releases/0.2.0/a",
        r"https://hearth.voikyrioh.fr\.evil.test/api/releases/0.2.0/a",
        r"https://hearth.voikyrioh.fr/api/releases\..\install.sh",
        "https://h\u{435}arth.voikyrioh.fr/api/releases/0.2.0/a",
        "https://hearth.voikyrioh.fr\u{ff0e}evil.test/api/releases/0.2.0/a",
        "https://hearth.voikyrioh.fr%2eevil.test/api/releases/0.2.0/a",
    ];
    for bad in refused {
        let mut c = candidate();
        c.url = bad.into();
        assert!(validate_target(&c, &policy()).is_err(), "{bad}");
    }
    // Acceptés : la même source officielle écrite autrement ; l'adresse retenue, donc envoyée à l'agent,
    // est celle que l'analyse a normalisée.
    for ok in [
        "HTTPS://HEARTH.VOIKYRIOH.FR/api/releases/0.2.0/a",
        "https://hearth.voikyrioh.fr:443/api/releases/0.2.0/a",
        "https://hearth\u{3002}voikyrioh\u{3002}fr/api/releases/0.2.0/a",
        r"https://hearth.voikyrioh.fr/api\releases/0.2.0/a",
    ] {
        let mut c = candidate();
        c.url = ok.into();
        let target = validate_target(&c, &policy()).unwrap_or_else(|e| panic!("{ok}: {e:?}"));
        assert_eq!(
            target.url().to_string(),
            "https://hearth.voikyrioh.fr/api/releases/0.2.0/a",
            "{ok}"
        );
    }
}
