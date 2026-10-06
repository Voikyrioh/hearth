//! Adresse du client et origine de ses échecs (ADR-0022).
//!
//! Une **adresse** est celle de la connexion TCP, en forme canonique (une IPv4 reçue sur une
//! socket double pile est ramenée à IPv4). L'**origine** regroupe les adresses qu'un même appareil
//! change à volonté : en IPv6, le préfixe /64 (adresses temporaires, SLAAC) ; en IPv4, l'adresse.
//! Fonctions pures, sans E/S.

use std::net::{IpAddr, Ipv6Addr};

/// Longueur maximale (en caractères) retenue d'une adresse qui n'est pas une adresse IP : borne la
/// taille des clés si une valeur inattendue arrivait.
const MAX_RAW_LEN: usize = 64;

/// Forme canonique de l'adresse. Une valeur qui n'est pas une adresse IP est gardée telle quelle,
/// tronquée.
pub fn canonical(addr: &str) -> String {
    match addr.trim().parse::<IpAddr>() {
        Ok(ip) => ip.to_canonical().to_string(),
        Err(_) => addr.chars().take(MAX_RAW_LEN).collect(),
    }
}

/// Origine d'une adresse : l'IPv4 elle-même, ou le préfixe /64 d'une IPv6 (`2001:db8:0:1::/64`).
pub fn origin(addr: &str) -> String {
    match addr.trim().parse::<IpAddr>() {
        Ok(ip) => match ip.to_canonical() {
            IpAddr::V4(v4) => v4.to_string(),
            IpAddr::V6(v6) => {
                let [a, b, c, d, ..] = v6.segments();
                format!("{}/64", Ipv6Addr::new(a, b, c, d, 0, 0, 0, 0))
            }
        },
        Err(_) => addr.chars().take(MAX_RAW_LEN).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ipv4_address_is_its_own_origin() {
        assert_eq!(origin("10.0.0.7"), "10.0.0.7");
        assert_ne!(origin("10.0.0.7"), origin("10.0.0.8"));
    }

    #[test]
    fn every_ipv6_address_of_a_slash_64_is_one_origin() {
        let a = origin("2001:db8:aaaa:1:1111:2222:3333:4444");
        let b = origin("2001:db8:aaaa:1:ffff:eeee:dddd:cccc");
        assert_eq!(a, b);
        assert_eq!(a, "2001:db8:aaaa:1::/64");
        assert_ne!(a, origin("2001:db8:aaaa:2::1"), "un autre /64");
    }

    #[test]
    fn ipv6_spellings_give_the_same_origin_and_the_same_canonical_address() {
        assert_eq!(
            origin("2001:DB8:0:1::5"),
            origin("2001:db8:0000:0001::0005")
        );
        assert_eq!(
            canonical("2001:DB8:0:1:0:0:0:5"),
            canonical("2001:db8:0:1::5")
        );
    }

    #[test]
    fn an_ipv4_mapped_address_is_the_ipv4_address() {
        assert_eq!(canonical("::ffff:10.0.0.7"), "10.0.0.7");
        assert_eq!(origin("::ffff:10.0.0.7"), "10.0.0.7");
    }

    #[test]
    fn the_canonical_ipv6_address_stays_complete_not_a_prefix() {
        assert_ne!(canonical("2001:db8:0:1::5"), canonical("2001:db8:0:1::6"));
    }

    #[test]
    fn a_value_that_is_not_an_address_stays_bounded() {
        let long = "x".repeat(10_000);
        assert_eq!(origin(&long).chars().count(), MAX_RAW_LEN);
        assert_eq!(canonical(&long).chars().count(), MAX_RAW_LEN);
        assert_eq!(origin("pas-une-adresse"), "pas-une-adresse");
    }
}
