//! BR-UPDATE-011, BR-UPDATE-012 : qui peut demander une mise à jour de l'agent, et laquelle. Le
//! rôle (administrateur) est contrôlé par la couche d'accès du routeur avant tout ; ici, l'état de
//! l'agent et la cible.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use thiserror::Error;

use crate::domain::install::Version;

/// Taille maximale du binaire téléchargé : au-delà, le téléchargement est refusé (le fichier est
/// gardé en mémoire le temps de la vérification, rien n'est écrit avant).
pub const MAX_BINARY_BYTES: u64 = 128 * 1024 * 1024;

const MAX_URL_LEN: usize = 2048;
const MAX_SIGNATURE_LEN: usize = 4096;

/// Pourquoi une demande de mise à jour est refusée avant de commencer. Rien n'a été téléchargé ni
/// écrit.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UpdateRefusal {
    /// Installation gérée par le système, ou sans systemd : pas de mise à jour à distance.
    #[error(
        "Cette installation est gérée par le système : l'agent ne se met pas à jour à distance. Mets-le à jour par la configuration du système."
    )]
    Managed,
    /// BR-UPDATE-012.
    #[error("Une mise à jour de l'agent est déjà en cours. Réessaye plus tard.")]
    InProgress,
    #[error("Le champ {field} est invalide : {reason}.")]
    Invalid {
        field: &'static str,
        reason: &'static str,
    },
    #[error(
        "La version {target} n'est pas plus récente que celle de l'agent ({current}). Rien n'a été modifié."
    )]
    NotNewer { current: Version, target: Version },
}

/// Ce que la demande dit, avant validation.
#[derive(Debug, Clone, Copy)]
pub struct UpdateInput<'a> {
    pub version: &'a str,
    pub url: &'a str,
    pub signature: &'a str,
    pub sha256: &'a str,
}

/// Une cible valable : version plus récente, adresse HTTPS, somme bien formée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateTarget {
    pub version: Version,
    pub url: String,
    pub signature: String,
    /// SHA-256 attendu, 64 caractères hexadécimaux en minuscules.
    pub sha256: String,
}

/// Une adresse que le serveur ne contacte jamais pour une mise à jour (BR-UPDATE-027) : bouclage,
/// non spécifiée, privée, lien-local, partagée (CGNAT), multicast, ou l'équivalent IPv4 d'une
/// adresse IPv6. Sinon, un jeton administrateur volé ferait sonder le réseau local par un agent
/// root (le résultat `unreachable` ou non dit si un port est ouvert).
pub fn is_local_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_local_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_local_v4(v4);
            }
            let seg = v6.segments();
            // `::a.b.c.d` (compatible IPv4, obsolète, jamais routée) : refusée ; `64:ff9b::a.b.c.d`
            // (NAT64) : l'IPv4 incorporée décide.
            let embedded = Ipv4Addr::new(
                (seg[6] >> 8) as u8,
                seg[6] as u8,
                (seg[7] >> 8) as u8,
                seg[7] as u8,
            );
            if seg[..6] == [0; 6] {
                return true;
            }
            if seg[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
                return is_local_v4(embedded);
            }
            let first = seg[0];
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (first & 0xfe00) == 0xfc00 // fc00::/7, adresses locales uniques
                || (first & 0xffc0) == 0xfe80 // fe80::/10, lien-local
                || v6 == Ipv6Addr::LOCALHOST
        }
    }
}

fn is_local_v4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_broadcast()
        || (a == 100 && (64..128).contains(&b)) // 100.64.0.0/10, partagée
        || a == 0
}

/// Décide si cette demande peut commencer. Ordre : installation gérée, mise à jour déjà en cours,
/// puis la validité de la cible. `allow_local` ouvre les adresses locales : les tests de bout en
/// bout seulement (jamais dans une construction de publication).
pub fn plan_update(
    current: Version,
    remote_update_allowed: bool,
    in_progress: bool,
    allow_local: bool,
    input: UpdateInput<'_>,
) -> Result<UpdateTarget, UpdateRefusal> {
    if !remote_update_allowed {
        return Err(UpdateRefusal::Managed);
    }
    if in_progress {
        return Err(UpdateRefusal::InProgress);
    }
    let version = Version::parse(input.version).map_err(|_| UpdateRefusal::Invalid {
        field: "version",
        reason: "attendu X.Y.Z",
    })?;
    if version <= current {
        return Err(UpdateRefusal::NotNewer {
            current,
            target: version,
        });
    }
    let url = check_url(input.url, allow_local)?;
    let signature = input.signature.trim();
    if signature.is_empty() || signature.len() > MAX_SIGNATURE_LEN || signature.contains('\0') {
        return Err(UpdateRefusal::Invalid {
            field: "signature",
            reason: "signature minisign attendue",
        });
    }
    let sha256 = input.sha256.trim().to_ascii_lowercase();
    if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(UpdateRefusal::Invalid {
            field: "sha256",
            reason: "64 caractères hexadécimaux attendus",
        });
    }
    Ok(UpdateTarget {
        version,
        url,
        signature: signature.to_owned(),
        sha256,
    })
}

/// Adresse HTTPS seulement (jamais en clair : un binaire lancé en root ne se télécharge pas en
/// clair), sans identifiant dans l'adresse (un secret ne passe pas dans une URL qui finit dans des
/// journaux), sans espace ni caractère de contrôle.
///
/// **Un seul parseur** : l'adresse est lue par la crate `url`, celle que `reqwest` utilisera
/// réellement. La décision porte sur l'hôte **normalisé** (`2130706433`, `127.1`, `0x7f.0.0.1`,
/// `%31%32%37.0.0.1` valent tous `127.0.0.1`), jamais sur la chaîne brute.
fn check_url(raw: &str, allow_local: bool) -> Result<String, UpdateRefusal> {
    let invalid = |reason| UpdateRefusal::Invalid {
        field: "url",
        reason,
    };
    let raw = raw.trim();
    if raw.len() > MAX_URL_LEN || raw.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(invalid("adresse illisible"));
    }
    let url = url::Url::parse(raw).map_err(|error| {
        if matches!(error, url::ParseError::EmptyHost) {
            invalid("adresse sans serveur")
        } else {
            invalid("adresse illisible")
        }
    })?;
    if url.scheme() != "https" {
        return Err(invalid("l'adresse doit être en HTTPS"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid("pas d'identifiant dans l'adresse"));
    }
    let Some(host) = url.host() else {
        return Err(invalid("adresse sans serveur"));
    };
    if !allow_local && host_is_local(host) {
        return Err(invalid("adresse locale ou privée refusée"));
    }
    Ok(url.to_string())
}

/// Le serveur de l'adresse est-il local ? Même filtre pour la demande, chaque redirection et
/// l'adresse lue par le téléchargeur ; les noms sont revérifiés **après résolution**
/// (`infrastructure/update/download.rs`). Un point final, les majuscules et les formes numériques
/// exotiques sont déjà normalisés par `url`.
pub fn host_is_local(host: url::Host<&str>) -> bool {
    match host {
        url::Host::Ipv4(ip) => is_local_address(IpAddr::V4(ip)),
        url::Host::Ipv6(ip) => is_local_address(IpAddr::V6(ip)),
        url::Host::Domain(name) => {
            let name = name.trim_end_matches('.').to_ascii_lowercase();
            name == "localhost" || name.ends_with(".localhost")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CURRENT: Version = Version::new(0, 1, 0);

    fn input<'a>() -> UpdateInput<'a> {
        UpdateInput {
            version: "0.2.0",
            url: "https://exemple.org/hearth-agent",
            signature: "untrusted comment: x\nRW...",
            sha256: "AB".repeat(32).leak(),
        }
    }

    fn plan(
        allowed: bool,
        running: bool,
        input: UpdateInput<'_>,
    ) -> Result<UpdateTarget, UpdateRefusal> {
        plan_update(CURRENT, allowed, running, false, input)
    }

    #[test]
    fn local_and_private_addresses_are_refused_by_literal_and_by_name() {
        for url in [
            "https://127.0.0.1/x",
            "https://127.0.0.1:8443/x",
            "https://10.0.0.5/x",
            "https://172.16.3.1/x",
            "https://192.168.1.10/x",
            "https://169.254.169.254/latest",
            "https://100.64.1.1/x",
            "https://0.0.0.0/x",
            "https://[::1]/x",
            "https://[::1]:7341/x",
            "https://[fe80::1]/x",
            "https://[fd00::1]/x",
            "https://[::ffff:192.168.0.1]/x",
            "https://localhost/x",
            "https://localhost./x",
            "https://LOCALHOST:8443/x",
            // Formes numériques exotiques : `url` les normalise, c'est cette lecture qui décide.
            "https://2130706433/",
            "https://127.1/",
            "https://127.0.1/",
            "https://0x7f.0.0.1/",
            "https://0x7f000001/",
            "https://0177.0.0.1/",
            "https://017700000001/",
            "https://%31%32%37.0.0.1/",
            "https://3232235777/",
            "https://192.168.1/",
            "https://0/",
            "https://[::127.0.0.1]/",
            "https://[::1.2.3.4]/",
            "https://[64:ff9b::7f00:1]/",
            "https://[0:0:0:0:0:ffff:7f00:1]/",
            "https://127.0.0.1./x",
            "https://127.0.0.1:443/x",
            "https://app.localhost/x",
        ] {
            let result = plan(true, false, UpdateInput { url, ..input() });
            assert_eq!(
                result,
                Err(UpdateRefusal::Invalid {
                    field: "url",
                    reason: "adresse locale ou privée refusée"
                }),
                "{url}"
            );
        }
        for url in [
            "https://8.8.8.8/x",
            "https://172.32.0.1/x",
            "https://exemple.org/x",
            "https://[2606:4700::1111]/x",
        ] {
            assert!(
                plan(true, false, UpdateInput { url, ..input() }).is_ok(),
                "{url}"
            );
        }
        // Les tests de bout en bout seulement ouvrent les adresses locales.
        let open = plan_update(
            CURRENT,
            true,
            false,
            true,
            UpdateInput {
                url: "https://127.0.0.1/x",
                ..input()
            },
        );
        assert!(open.is_ok());
    }

    #[test]
    fn a_newer_version_over_https_with_a_checksum_is_accepted_and_the_sum_is_lowercased() {
        let target = plan(true, false, input()).unwrap();
        assert_eq!(target.version, Version::new(0, 2, 0));
        assert_eq!(target.sha256, "ab".repeat(32));
        assert_eq!(target.url, "https://exemple.org/hearth-agent");
    }

    #[test]
    fn a_managed_installation_is_refused_first_even_when_an_update_runs() {
        assert_eq!(plan(false, true, input()), Err(UpdateRefusal::Managed));
        assert_eq!(plan(false, false, input()), Err(UpdateRefusal::Managed));
    }

    #[test]
    fn only_one_update_at_a_time_with_the_spec_message() {
        let error = plan(true, true, input()).unwrap_err();
        assert_eq!(error, UpdateRefusal::InProgress);
        assert_eq!(
            error.to_string(),
            "Une mise à jour de l'agent est déjà en cours. Réessaye plus tard."
        );
        // Refusée avant la validation : une demande invalide pendant une mise à jour dit « en cours ».
        let bad = UpdateInput {
            version: "x",
            ..input()
        };
        assert_eq!(plan(true, true, bad), Err(UpdateRefusal::InProgress));
    }

    #[test]
    fn the_same_or_an_older_version_is_refused() {
        for version in ["0.1.0", "0.0.9", "v0.1.0"] {
            let result = plan(true, false, UpdateInput { version, ..input() });
            assert!(
                matches!(result, Err(UpdateRefusal::NotNewer { .. })),
                "{version}"
            );
        }
    }

    #[test]
    fn an_unreadable_version_names_the_field() {
        let result = plan(
            true,
            false,
            UpdateInput {
                version: "latest",
                ..input()
            },
        );
        assert_eq!(
            result,
            Err(UpdateRefusal::Invalid {
                field: "version",
                reason: "attendu X.Y.Z"
            })
        );
    }

    #[test]
    fn the_address_must_be_https_without_credentials_or_spaces() {
        for url in [
            "http://exemple.org/a",
            "ftp://exemple.org/a",
            "exemple.org/a",
            "https://",
            "https://user:pass@exemple.org/a",
            "https://exemple.org/a b",
            "https://exemple.org/a\nb",
            "",
        ] {
            let result = plan(true, false, UpdateInput { url, ..input() });
            assert!(
                matches!(result, Err(UpdateRefusal::Invalid { field: "url", .. })),
                "{url:?} : {result:?}"
            );
        }
        let long = format!("https://exemple.org/{}", "a".repeat(3000));
        let result = plan(
            true,
            false,
            UpdateInput {
                url: &long,
                ..input()
            },
        );
        assert!(matches!(
            result,
            Err(UpdateRefusal::Invalid { field: "url", .. })
        ));
        // Le schéma s'écrit dans n'importe quelle casse.
        assert!(
            plan(
                true,
                false,
                UpdateInput {
                    url: "HTTPS://exemple.org/a",
                    ..input()
                }
            )
            .is_ok()
        );
    }

    #[test]
    fn the_signature_and_the_checksum_are_required_and_well_formed() {
        for signature in ["", "   ", "a\0b"] {
            let result = plan(
                true,
                false,
                UpdateInput {
                    signature,
                    ..input()
                },
            );
            assert!(matches!(
                result,
                Err(UpdateRefusal::Invalid {
                    field: "signature",
                    ..
                })
            ));
        }
        for sha256 in ["", "abc", &"g".repeat(64), &"a".repeat(63), &"a".repeat(65)] {
            let result = plan(true, false, UpdateInput { sha256, ..input() });
            assert!(
                matches!(
                    result,
                    Err(UpdateRefusal::Invalid {
                        field: "sha256",
                        ..
                    })
                ),
                "{sha256}"
            );
        }
    }

    #[test]
    fn no_message_has_an_em_dash() {
        for refusal in [
            UpdateRefusal::Managed,
            UpdateRefusal::InProgress,
            UpdateRefusal::Invalid {
                field: "url",
                reason: "x",
            },
            UpdateRefusal::NotNewer {
                current: CURRENT,
                target: CURRENT,
            },
        ] {
            assert!(!refusal.to_string().contains('\u{2014}'));
        }
    }
}
