//! Adresses connues d'un compte (ADR-0022, BR-CONN-019).
//!
//! Une adresse devient connue d'un compte par une connexion **réussie** de ce compte depuis cette
//! adresse, jamais autrement. Elle ne donne **aucun droit** : le mot de passe reste exigé. Elle
//! évite seulement d'être ralenti par les échecs des autres. Fonctions pures : le stockage lit la
//! liste du compte, appelle `learn`, réécrit.

use time::{Duration, OffsetDateTime};

use super::login_origin::canonical;

/// Adresses connues au plus par compte ; au-delà, la moins récemment réussie est oubliée.
pub const MAX_PER_ACCOUNT: usize = 8;

/// Durée de validité d'une adresse connue, comptée depuis sa dernière connexion réussie : celle
/// de la session glissante (BR-RESIL-012).
pub const VALIDITY: Duration = Duration::days(30);

/// Une adresse connue d'un compte et sa dernière connexion réussie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownAddress {
    /// Adresse **exacte**, canonique (en IPv6 l'adresse complète, jamais le préfixe).
    pub address: String,
    pub last_success_at: OffsetDateTime,
}

/// L'adresse est-elle valide à `now` ? Une date dans le futur (horloge reculée) reste valide.
fn is_valid(entry: &KnownAddress, now: OffsetDateTime) -> bool {
    now - entry.last_success_at < VALIDITY
}

/// Cette adresse est-elle connue de ce compte, d'après sa liste ?
pub fn is_known(list: &[KnownAddress], addr: &str, now: OffsetDateTime) -> bool {
    let addr = canonical(addr);
    list.iter()
        .any(|entry| entry.address == addr && is_valid(entry, now))
}

/// Instant avant lequel une connexion réussie n'est plus une adresse connue (purge, requêtes).
pub fn cutoff(now: OffsetDateTime) -> OffsetDateTime {
    now - VALIDITY
}

/// La liste du compte après une connexion réussie depuis `addr` : l'adresse est ajoutée ou
/// rafraîchie, les adresses expirées sont oubliées, et on garde les `MAX_PER_ACCOUNT` plus
/// récentes. Une date dans le futur (horloge reculée) est ramenée à `now` : l'adresse qu'on vient
/// d'apprendre n'est jamais évincée par elle.
pub fn learn(list: Vec<KnownAddress>, addr: &str, now: OffsetDateTime) -> Vec<KnownAddress> {
    let addr = canonical(addr);
    let mut next = vec![KnownAddress {
        address: addr.clone(),
        last_success_at: now,
    }];
    next.extend(
        list.into_iter()
            .filter(|entry| entry.address != addr && is_valid(entry, now))
            .map(|entry| KnownAddress {
                last_success_at: entry.last_success_at.min(now),
                ..entry
            }),
    );
    // Tri stable : à date égale, l'adresse apprise (en tête) passe avant les autres.
    next.sort_by_key(|entry| std::cmp::Reverse(entry.last_success_at));
    next.truncate(MAX_PER_ACCOUNT);
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH + Duration::days(20_000)
    }

    fn entry(addr: &str, at: OffsetDateTime) -> KnownAddress {
        KnownAddress {
            address: addr.to_owned(),
            last_success_at: at,
        }
    }

    #[test]
    fn nothing_is_known_before_a_success() {
        assert!(!is_known(&[], "10.0.0.7", t0()));
    }

    #[test]
    fn a_success_makes_the_exact_address_known_and_no_other() {
        let list = learn(vec![], "10.0.0.7", t0());
        assert!(is_known(&list, "10.0.0.7", t0()));
        assert!(!is_known(&list, "10.0.0.8", t0()));
    }

    #[test]
    fn an_ipv6_address_is_known_alone_not_its_prefix() {
        let list = learn(vec![], "2001:db8:0:1::5", t0());
        assert!(
            is_known(&list, "2001:DB8:0:1:0:0:0:5", t0()),
            "autre graphie"
        );
        assert!(
            !is_known(&list, "2001:db8:0:1::6", t0()),
            "même /64, autre adresse"
        );
    }

    #[test]
    fn an_address_is_known_for_thirty_days_after_its_last_success() {
        let list = learn(vec![], "10.0.0.7", t0());
        assert!(is_known(
            &list,
            "10.0.0.7",
            t0() + VALIDITY - Duration::seconds(1)
        ));
        assert!(!is_known(&list, "10.0.0.7", t0() + VALIDITY));
    }

    #[test]
    fn a_new_success_refreshes_the_address_without_duplicating_it() {
        let list = learn(vec![], "10.0.0.7", t0());
        let later = t0() + Duration::days(20);
        let list = learn(list, "10.0.0.7", later);
        assert_eq!(list, vec![entry("10.0.0.7", later)]);
        assert!(is_known(&list, "10.0.0.7", later + Duration::days(29)));
    }

    #[test]
    fn at_most_eight_addresses_the_least_recent_is_forgotten() {
        let mut list = vec![];
        for index in 1..=9 {
            list = learn(
                list,
                &format!("10.0.0.{index}"),
                t0() + Duration::minutes(index),
            );
        }
        assert_eq!(list.len(), MAX_PER_ACCOUNT);
        let now = t0() + Duration::minutes(10);
        assert!(
            !is_known(&list, "10.0.0.1", now),
            "la plus ancienne est oubliée"
        );
        assert!(is_known(&list, "10.0.0.2", now));
        assert!(is_known(&list, "10.0.0.9", now));
    }

    #[test]
    fn refreshing_an_old_address_protects_it_from_eviction() {
        let mut list = vec![];
        for index in 1..=8 {
            list = learn(
                list,
                &format!("10.0.0.{index}"),
                t0() + Duration::minutes(index),
            );
        }
        list = learn(list, "10.0.0.1", t0() + Duration::minutes(20));
        list = learn(list, "10.0.0.99", t0() + Duration::minutes(21));
        let now = t0() + Duration::minutes(22);
        assert!(is_known(&list, "10.0.0.1", now));
        assert!(!is_known(&list, "10.0.0.2", now), "la moins récente part");
    }

    #[test]
    fn expired_addresses_are_dropped_on_learning() {
        let list = vec![entry("10.0.0.1", t0() - VALIDITY - Duration::seconds(1))];
        let list = learn(list, "10.0.0.2", t0());
        assert_eq!(list, vec![entry("10.0.0.2", t0())]);
    }

    #[test]
    fn a_clock_set_back_never_evicts_the_address_just_learned() {
        let future = t0() + Duration::days(5);
        let full: Vec<_> = (1..=8)
            .map(|index| entry(&format!("10.0.0.{index}"), future))
            .collect();
        let list = learn(full, "10.0.1.1", t0());
        assert!(is_known(&list, "10.0.1.1", t0()));
        assert_eq!(list.len(), MAX_PER_ACCOUNT);
    }

    #[test]
    fn the_cutoff_is_the_validity_before_now() {
        assert_eq!(cutoff(t0()), t0() - VALIDITY);
    }
}
