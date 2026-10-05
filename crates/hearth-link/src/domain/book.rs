//! Règles du carnet de serveurs : nom unique, adresse valide, modification d'adresse
//! (BR-CONN-008, BR-CONN-009, BR-CONN-010 de la spec : formats). Pures : le gestionnaire les
//! appelle, il n'en porte aucune.

use std::net::{Ipv4Addr, Ipv6Addr};

use thiserror::Error;

use super::server::ServerRecord;

/// Longueur maximale du nom d'un serveur (spec : 1 à 255 caractères).
pub const NAME_MAX_CHARS: usize = 255;

/// Pourquoi une saisie du carnet est refusée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum BookError {
    #[error("le nom du serveur est requis")]
    NameRequired,
    #[error("le nom du serveur est trop long")]
    NameTooLong,
    #[error("un serveur porte déjà ce nom")]
    NameTaken,
    #[error("adresse invalide")]
    BadAddress,
    #[error("port invalide")]
    BadPort,
    #[error("identifiant invalide")]
    BadUsername,
}

/// Longueur maximale de l'identifiant d'un compte gardé au carnet.
pub const USERNAME_MAX_CHARS: usize = 64;
/// Nombre maximal d'adresses MAC gardées pour un serveur.
pub const MAC_MAX: usize = 16;

/// Identifiant nettoyé (espaces de bord retirés) : non vide, borné, sans caractère de contrôle.
pub fn check_username(username: &str) -> Result<String, BookError> {
    let username = username.trim();
    let bad = username.is_empty()
        || username.chars().count() > USERNAME_MAX_CHARS
        || username.chars().any(char::is_control);
    if bad {
        return Err(BookError::BadUsername);
    }
    Ok(username.to_owned())
}

/// Adresses MAC annoncées par l'agent : elles viennent de lui, pas de l'utilisateur, et une machine
/// avec Docker en annonce des dizaines. On garde les valides (six paires hexadécimales, rendues en
/// majuscules), dans l'ordre annoncé, au plus [`MAC_MAX`] ; le reste est écarté, jamais un refus.
pub fn check_mac_addresses(macs: &[String]) -> Vec<String> {
    macs.iter()
        .filter(|mac| is_mac(mac))
        .take(MAC_MAX)
        .map(|mac| mac.to_ascii_uppercase())
        .collect()
}

fn is_mac(mac: &str) -> bool {
    let groups: Vec<&str> = mac.split(':').collect();
    groups.len() == 6
        && groups
            .iter()
            .all(|g| g.len() == 2 && g.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Nom nettoyé (espaces de bord retirés) si valide, et unique parmi les autres serveurs
/// (sans tenir compte de la casse). `others` exclut le serveur modifié lui-même.
pub fn check_name<'a>(
    name: &str,
    others: impl IntoIterator<Item = &'a ServerRecord>,
) -> Result<String, BookError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(BookError::NameRequired);
    }
    if name.chars().count() > NAME_MAX_CHARS {
        return Err(BookError::NameTooLong);
    }
    let lowered = name.to_lowercase();
    if others
        .into_iter()
        .any(|other| other.name.trim().to_lowercase() == lowered)
    {
        return Err(BookError::NameTaken);
    }
    Ok(name.to_owned())
}

/// Adresse (IPv4, IPv6 ou nom : un seul mot comme `forge`, ou `forge.maison`) et port.
pub fn check_address(host: &str, port: u16) -> Result<(), BookError> {
    if !is_valid_host(host) {
        return Err(BookError::BadAddress);
    }
    if port == 0 {
        return Err(BookError::BadPort);
    }
    Ok(())
}

fn is_valid_host(host: &str) -> bool {
    if host.is_empty() || host.len() > 253 {
        return false;
    }
    let bare = host
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(host);
    if bare.parse::<Ipv6Addr>().is_ok() {
        return true;
    }
    // Un nom tout en chiffres et points qui n'est pas une IPv4 valide n'est pas un nom.
    let numeric = host.chars().all(|c| c.is_ascii_digit() || c == '.');
    if numeric {
        return host.parse::<Ipv4Addr>().is_ok();
    }
    host.trim_end_matches('.').split('.').all(is_valid_label)
}

fn is_valid_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 63
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// L'adresse (hôte ou port) diffère-t-elle de celle du carnet ? Si oui, l'empreinte doit être
/// relue et confirmée de nouveau (BR-CONN-009).
pub fn address_changed(record: &ServerRecord, host: &str, port: u16) -> bool {
    !record.host.eq_ignore_ascii_case(host) || record.port != port
}

#[cfg(test)]
mod tests {
    use hearth_proto::fingerprint::Fingerprint;

    use super::*;
    use crate::domain::server::ServerId;

    fn record(name: &str) -> ServerRecord {
        ServerRecord {
            id: ServerId::parse(&name.replace(' ', "-")).unwrap(),
            name: name.into(),
            color: "#fff".into(),
            host: "forge.lan".into(),
            port: 7341,
            fingerprint: Fingerprint::from_bytes([1; 32]),
            username: String::new(),
            remember: false,
            mac_addresses: vec![],
            last_contact_at: None,
            signed_out: false,
            role: None,
        }
    }

    #[test]
    fn a_name_is_trimmed_required_bounded_and_unique_ignoring_case() {
        let others = [record("Forge")];
        assert_eq!(check_name("  Salon ", &others), Ok("Salon".to_owned()));
        assert_eq!(check_name("   ", &others), Err(BookError::NameRequired));
        assert_eq!(
            check_name(&"x".repeat(256), &others),
            Err(BookError::NameTooLong)
        );
        assert_eq!(
            check_name(&"x".repeat(255), &others).map(|n| n.len()),
            Ok(255)
        );
        assert_eq!(check_name("forge", &others), Err(BookError::NameTaken));
        assert_eq!(check_name(" FORGE ", &others), Err(BookError::NameTaken));
        // Aucun autre serveur : le nom du serveur modifié lui-même reste libre.
        assert!(check_name("Forge", &[] as &[ServerRecord]).is_ok());
    }

    #[test]
    fn addresses_are_ipv4_ipv6_or_names() {
        for good in [
            "192.168.1.20",
            "::1",
            "[fe80::1]",
            "2001:db8::7",
            "forge",
            "forge.maison",
            "nas-salon.local",
            "FORGE.Maison.",
            "a_b.lan",
        ] {
            assert_eq!(check_address(good, 7341), Ok(()), "{good}");
        }
        for bad in [
            "",
            "   ",
            "for ge",
            "forge/",
            "http://forge",
            "forge:7341",
            "-forge",
            "forge-.lan",
            "a..b",
            "300.1.1.1",
            "1.2.3",
            ".",
            &"a".repeat(64),
            &format!("{}.{}", "a".repeat(63), "b".repeat(200)),
        ] {
            assert_eq!(
                check_address(bad, 7341),
                Err(BookError::BadAddress),
                "{bad}"
            );
        }
    }

    #[test]
    fn a_username_is_trimmed_and_bounded() {
        assert_eq!(check_username("  marie "), Ok("marie".to_owned()));
        for bad in ["", "   ", "a\nb", &"x".repeat(65)] {
            assert_eq!(check_username(bad), Err(BookError::BadUsername), "{bad:?}");
        }
        assert!(check_username(&"x".repeat(64)).is_ok());
    }

    #[test]
    fn mac_addresses_are_filtered_and_truncated_never_refused() {
        assert_eq!(
            check_mac_addresses(&["aa:bb:cc:dd:ee:0f".to_owned()]),
            vec!["AA:BB:CC:DD:EE:0F".to_owned()]
        );
        assert!(check_mac_addresses(&[]).is_empty());
        // Les entrées mal formées sont écartées, les bonnes gardées, dans l'ordre.
        let mixed: Vec<String> = [
            "",
            "aa:bb",
            "aa:bb:cc:dd:ee:gg",
            "11:22:33:44:55:66",
            "aabbccddeeff",
        ]
        .iter()
        .map(|m| (*m).to_owned())
        .collect();
        assert_eq!(
            check_mac_addresses(&mixed),
            vec!["11:22:33:44:55:66".to_owned()]
        );
        // Quarante interfaces : les seize premières valides.
        let many: Vec<String> = (0..40).map(|n| format!("02:00:00:00:00:{n:02x}")).collect();
        let kept = check_mac_addresses(&many);
        assert_eq!(kept.len(), MAC_MAX);
        assert_eq!(kept[0], "02:00:00:00:00:00");
        assert_eq!(
            kept[MAC_MAX - 1],
            format!("02:00:00:00:00:{:02X}", MAC_MAX - 1)
        );
    }

    #[test]
    fn port_zero_is_refused() {
        assert_eq!(check_address("forge", 0), Err(BookError::BadPort));
        assert_eq!(check_address("forge", 65535), Ok(()));
    }

    #[test]
    fn a_new_host_or_port_counts_as_a_changed_address() {
        let record = record("Forge");
        assert!(!address_changed(&record, "FORGE.lan", 7341));
        assert!(address_changed(&record, "forge.maison", 7341));
        assert!(address_changed(&record, "forge.lan", 7342));
    }
}
