//! Règles pures de la mise à jour de l'agent depuis le client (HRT-17, ADR-0021) : ce que le client
//! accepte comme cible, quand une version est « disponible », ce qu'il ne propose ni n'envoie
//! JAMAIS (rétrogradation, adresse non HTTPS, locale ou hors du dépôt prévu), et le fichier de
//! cibles. La sécurité reste celle de l'agent (signature, somme) : ces tests prouvent que le client
//! ne lui envoie rien qu'il sait mauvais.
#![allow(clippy::unwrap_used, clippy::expect_used)] // tests : les helpers peuvent paniquer

use hearth_desktop_lib::agent_update::domain::{
    AGENT_PLATFORM, AgentCandidate, AgentTarget, AgentTargetRecord, MANIFEST_MAX_BYTES,
    ManifestError, TargetRejection, host_is_local, is_local_ip, is_newer, parse_manifest,
    validate_target,
};
use hearth_desktop_lib::update::domain::DownloadPolicy;
use serde_json::json;
use url::Url;

const URL: &str =
    "https://github.com/Voikyrioh/hearth/releases/download/v0.2.0/hearth-agent-linux-x86_64";
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
    DownloadPolicy::github_releases()
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
        "http://github.com/Voikyrioh/hearth/releases/download/v0.2.0/a",
        "ftp://github.com/Voikyrioh/hearth/releases/download/v0.2.0/a",
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
        c.url = format!("https://{host}/Voikyrioh/hearth/releases/download/v0.2.0/a");
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
    assert!(!host_is_local(&Url::parse("https://github.com/x").unwrap()));
}

#[test]
fn an_address_outside_the_releases_of_the_repository_is_never_sent() {
    for bad in [
        "https://exemple.org/Voikyrioh/hearth/releases/download/v0.2.0/a",
        "https://github.com/autre/hearth/releases/download/v0.2.0/a",
        "https://github.com/Voikyrioh/hearth/archive/main.zip",
        "https://github.com.evil.example/Voikyrioh/hearth/releases/download/v0.2.0/a",
        "https://user:pass@github.com/Voikyrioh/hearth/releases/download/v0.2.0/a",
        "https://github.com:8443/Voikyrioh/hearth/releases/download/v0.2.0/a",
        "https://github.com/Voikyrioh/hearth/releases/download/v0.2.0/a#fragment",
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
            r.url = "http://github.com/Voikyrioh/hearth/releases/download/v0.2.0/a".into()
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

fn manifest(version: &str, entry: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "version": version,
        "pub_date": "2026-10-06T10:00:00Z",
        "platforms": { AGENT_PLATFORM: entry }
    }))
    .unwrap()
}

#[test]
fn the_manifest_gives_one_candidate_for_the_platform_of_the_agent() {
    let bytes = manifest(
        "0.2.0",
        json!({ "url": URL, "signature": SIGNATURE, "sha256": sha() }),
    );
    assert_eq!(parse_manifest(&bytes).unwrap(), Some(candidate()));
}

#[test]
fn a_manifest_without_an_entry_for_the_agent_offers_nothing_and_a_broken_one_is_an_error() {
    // Le manifeste du client seul : pas d'entrée de l'agent, ce n'est pas une panne.
    let client_only = serde_json::to_vec(&json!({
        "version": "0.2.0",
        "platforms": { "windows-x86_64": { "url": "https://x", "signature": "s" } }
    }))
    .unwrap();
    assert_eq!(parse_manifest(&client_only).unwrap(), None);
    assert_eq!(parse_manifest(b"{}").unwrap(), None);
    // L'entrée existe mais il manque des champs : illisible.
    assert_eq!(
        parse_manifest(&manifest("0.2.0", json!({ "url": URL }))).unwrap_err(),
        ManifestError::Unreadable
    );
    assert_eq!(
        parse_manifest(b"pas du json").unwrap_err(),
        ManifestError::Unreadable
    );
    assert_eq!(
        parse_manifest(&vec![b' '; MANIFEST_MAX_BYTES + 1]).unwrap_err(),
        ManifestError::TooLarge
    );
}
