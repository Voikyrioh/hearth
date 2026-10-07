//! Le défi de la preuve de clé (BR-TRUST-005, ADR-0023) : sans état, valable 60 secondes, à usage
//! unique.
//!
//! Un défi fait 56 octets : `nonce (16) || émission (8, millisecondes d'horloge monotone) || code
//! (32)`. Le code est un HMAC-SHA256, calculé par un port avec une clé tirée au démarrage du
//! service ; il lie le défi à l'usage, à l'identifiant normalisé et à l'adresse de celui qui l'a
//! demandé. L'agent n'en garde rien : il le recalcule. Un défi n'est retenu qu'au moment où sa preuve
//! **sert** (mot de passe juste ET signature valide, ou session valide ET signature valide sous une
//! clé inscrite du compte), jamais avant : une requête non authentifiée n'écrit aucun état.
//!
//! Ce module ne fait aucun calcul cryptographique : il assemble les octets, juge la fraîcheur et
//! tient l'ensemble des défis consommés. La comparaison du code se fait à temps constant.

use std::collections::HashMap;

use hearth_proto::device_proof::{CHALLENGE_LEN, normalize_identifier};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::domain::known_address::canonical;

/// Durée de vie d'un défi, en millisecondes.
pub const TTL_MS: u64 = 60_000;

/// Défis consommés retenus au plus, tous comptes confondus.
pub const MAX_CONSUMED: usize = 4_096;

/// Défis consommés retenus au plus pour un même compte : un compte authentifié ne peut occuper que
/// sa part (un poste en consomme un à chaque connexion ou ouverture du flux, 8 postes au plus).
pub const MAX_CONSUMED_PER_OWNER: usize = 64;

const NONCE_LEN: usize = 16;
const MAC_LEN: usize = 32;
/// Préfixe de domaine du code d'authentification : sépare ce HMAC de tout autre emploi de la clé.
const MAC_DOMAIN: &[u8] = b"hearth-challenge/1";

/// Un défi décomposé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Challenge {
    pub nonce: [u8; NONCE_LEN],
    /// Émission, en millisecondes sur l'horloge monotone de l'agent.
    pub emitted_ms: u64,
    pub mac: [u8; MAC_LEN],
}

impl Challenge {
    /// Décompose 56 octets ; toute autre longueur est refusée (jamais de panique sur une entrée
    /// mal formée).
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let bytes: &[u8; CHALLENGE_LEN] = bytes.try_into().ok()?;
        let (nonce, rest) = bytes.split_at(NONCE_LEN);
        let (emitted, mac) = rest.split_at(8);
        Some(Self {
            nonce: nonce.try_into().ok()?,
            emitted_ms: u64::from_be_bytes(emitted.try_into().ok()?),
            mac: mac.try_into().ok()?,
        })
    }

    pub fn to_bytes(&self) -> [u8; CHALLENGE_LEN] {
        let mut bytes = [0_u8; CHALLENGE_LEN];
        bytes[..NONCE_LEN].copy_from_slice(&self.nonce);
        bytes[NONCE_LEN..NONCE_LEN + 8].copy_from_slice(&self.emitted_ms.to_be_bytes());
        bytes[NONCE_LEN + 8..].copy_from_slice(&self.mac);
        bytes
    }
}

/// Les octets que le port d'authentification signe : l'usage (octet du message signé), le nonce,
/// l'émission, l'empreinte de l'identifiant normalisé et l'adresse canonique du demandeur. Tous les
/// champs ont une longueur fixe ou préfixée : deux jeux d'entrées différents ne donnent jamais les
/// mêmes octets.
pub fn mac_input(
    usage: u8,
    nonce: &[u8; NONCE_LEN],
    emitted_ms: u64,
    identifier: &str,
    addr: &str,
) -> Vec<u8> {
    let identifier_digest = Sha256::digest(normalize_identifier(identifier).as_bytes());
    let addr = canonical(addr);
    let addr = addr.as_bytes();
    let addr_len = u16::try_from(addr.len()).unwrap_or(u16::MAX);
    let mut bytes = Vec::with_capacity(MAC_DOMAIN.len() + 1 + NONCE_LEN + 8 + 32 + 2 + addr.len());
    bytes.extend_from_slice(MAC_DOMAIN);
    bytes.push(usage);
    bytes.extend_from_slice(nonce);
    bytes.extend_from_slice(&emitted_ms.to_be_bytes());
    bytes.extend_from_slice(&identifier_digest);
    bytes.extend_from_slice(&addr_len.to_be_bytes());
    bytes.extend_from_slice(addr);
    bytes
}

/// Le défi est-il authentique, frais et pas déjà consommé ? `expected_mac` est le code que l'agent
/// recalcule pour cet usage, cet identifiant et cette adresse. Le code se compare à temps constant ;
/// un défi émis « dans le futur » (horloge qui ne devrait pas pouvoir reculer) est refusé.
pub fn check(
    challenge: &Challenge,
    expected_mac: &[u8; MAC_LEN],
    now_ms: u64,
    consumed: &ConsumedChallenges,
) -> bool {
    let authentic = bool::from(challenge.mac.ct_eq(expected_mac));
    let fresh = now_ms >= challenge.emitted_ms && now_ms - challenge.emitted_ms <= TTL_MS;
    authentic && fresh && !consumed.contains(&challenge.nonce)
}

/// Les défis dont la preuve a servi, retenus le temps de leur validité, chacun au nom du compte qui
/// s'en est servi. **Seuls des comptes authentifiés y écrivent** : la borne ne peut être atteinte que
/// par eux, jamais par un appareil anonyme.
#[derive(Debug, Default)]
pub struct ConsumedChallenges {
    /// Nonce, (compte, fin de validité en millisecondes monotones).
    seen: HashMap<[u8; NONCE_LEN], (String, u64)>,
}

impl ConsumedChallenges {
    pub fn contains(&self, nonce: &[u8; NONCE_LEN]) -> bool {
        self.seen.contains_key(nonce)
    }

    /// Retient ce défi au nom de `owner`. `false` si déjà retenu (rejeu perdu), ou si `owner` a déjà
    /// `MAX_CONSUMED_PER_OWNER` défis valides (il ne se prive que lui-même).
    ///
    /// Les défis expirés sont oubliés d'abord. Ensemble plein malgré cela : on oublie le plus ancien
    /// défi du compte qui en retient le plus, jamais on ne refuse pour tout le monde à cause d'un seul
    /// compte (un défi oublié ne peut resservir qu'à qui a aussi le mot de passe ou une session
    /// valide du compte concerné, dans sa minute de validité).
    pub fn consume(&mut self, owner: &str, nonce: [u8; NONCE_LEN], now_ms: u64) -> bool {
        self.seen.retain(|_, (_, until)| *until >= now_ms);
        if self.seen.contains_key(&nonce) {
            return false;
        }
        let mut per_owner: HashMap<&str, usize> = HashMap::new();
        for (name, _) in self.seen.values() {
            *per_owner.entry(name.as_str()).or_insert(0) += 1;
        }
        if per_owner.get(owner).copied().unwrap_or(0) >= MAX_CONSUMED_PER_OWNER {
            return false;
        }
        if self.seen.len() >= MAX_CONSUMED {
            let heaviest = per_owner
                .iter()
                .max_by_key(|(name, count)| (**count, std::cmp::Reverse(**name)))
                .map(|(name, _)| (*name).to_owned());
            let oldest = heaviest.and_then(|heaviest| {
                self.seen
                    .iter()
                    .filter(|(_, (name, _))| *name == heaviest)
                    .min_by_key(|(_, (_, until))| *until)
                    .map(|(nonce, _)| *nonce)
            });
            match oldest {
                Some(oldest) => {
                    self.seen.remove(&oldest);
                }
                None => return false,
            }
        }
        self.seen
            .insert(nonce, (owner.to_owned(), now_ms.saturating_add(TTL_MS)));
        true
    }

    pub fn len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn challenge(emitted_ms: u64) -> Challenge {
        Challenge {
            nonce: [7; NONCE_LEN],
            emitted_ms,
            mac: [9; MAC_LEN],
        }
    }

    #[test]
    fn a_challenge_round_trips_through_its_fifty_six_bytes() {
        let original = challenge(123_456_789);
        let bytes = original.to_bytes();
        assert_eq!(bytes.len(), 56);
        assert_eq!(Challenge::parse(&bytes), Some(original));
    }

    #[test]
    fn any_other_length_is_refused_without_panicking() {
        for len in [0, 1, 55, 57, 64, 4096] {
            assert_eq!(Challenge::parse(&vec![1; len]), None, "{len} octets");
        }
    }

    #[test]
    fn a_fresh_authentic_unused_challenge_passes() {
        let c = challenge(1_000);
        let consumed = ConsumedChallenges::default();
        assert!(check(&c, &[9; 32], 1_000, &consumed));
        assert!(
            check(&c, &[9; 32], 1_000 + TTL_MS, &consumed),
            "borne incluse"
        );
    }

    #[test]
    fn an_expired_challenge_is_refused() {
        let c = challenge(1_000);
        assert!(!check(
            &c,
            &[9; 32],
            1_000 + TTL_MS + 1,
            &ConsumedChallenges::default()
        ));
    }

    #[test]
    fn a_challenge_from_the_future_is_refused() {
        let c = challenge(5_000);
        assert!(!check(&c, &[9; 32], 4_999, &ConsumedChallenges::default()));
    }

    #[test]
    fn a_forged_code_is_refused_even_by_one_bit() {
        let c = challenge(1_000);
        let mut wrong = [9; 32];
        wrong[31] ^= 1;
        assert!(!check(&c, &wrong, 1_000, &ConsumedChallenges::default()));
        assert!(!check(&c, &[0; 32], 1_000, &ConsumedChallenges::default()));
    }

    #[test]
    fn a_consumed_challenge_is_refused_the_second_time() {
        let c = challenge(1_000);
        let mut consumed = ConsumedChallenges::default();
        assert!(check(&c, &[9; 32], 1_000, &consumed));
        assert!(consumed.consume("marie", c.nonce, 1_000));
        assert!(!check(&c, &[9; 32], 1_001, &consumed));
        assert!(!consumed.consume("marie", c.nonce, 1_001), "rejeu");
    }

    fn nonce_of(index: usize) -> [u8; NONCE_LEN] {
        let mut nonce = [0; NONCE_LEN];
        nonce[..8].copy_from_slice(&(index as u64).to_be_bytes());
        nonce
    }

    #[test]
    fn an_owner_holds_at_most_its_share_and_only_blocks_itself() {
        let mut consumed = ConsumedChallenges::default();
        for index in 0..MAX_CONSUMED_PER_OWNER {
            assert!(consumed.consume("marie", nonce_of(index), 0));
        }
        assert!(
            !consumed.consume("marie", nonce_of(10_000), 0),
            "sa part est pleine"
        );
        assert!(
            consumed.consume("paul", nonce_of(10_001), 0),
            "un autre compte passe"
        );
        // Sa part se libère avec l'expiration de ses défis.
        assert!(consumed.consume("marie", nonce_of(10_002), TTL_MS + 1));
    }

    #[test]
    fn a_full_set_never_refuses_everybody_the_heaviest_owner_gives_up_its_oldest() {
        let mut consumed = ConsumedChallenges::default();
        // 64 comptes de 64 défis : 4 096, l'ensemble est plein.
        for owner in 0..64 {
            for slot in 0..MAX_CONSUMED_PER_OWNER {
                assert!(consumed.consume(
                    &format!("compte{owner}"),
                    nonce_of(owner * 100 + slot),
                    slot as u64
                ));
            }
        }
        assert_eq!(consumed.len(), MAX_CONSUMED);
        // Un 65e compte (authentifié) n'est pas privé de son usage unique.
        assert!(consumed.consume("nouveau", nonce_of(900_000), 100));
        assert_eq!(consumed.len(), MAX_CONSUMED);
        // Une fois les anciens expirés, tout se libère.
        assert!(consumed.consume("nouveau", nonce_of(900_001), TTL_MS + 1_000));
        assert!(consumed.len() < 10);
    }

    #[test]
    fn the_code_input_changes_with_every_bound_element() {
        let base = mac_input(1, &[1; 16], 10, "marie", "10.0.0.7");
        let variants = [
            mac_input(2, &[1; 16], 10, "marie", "10.0.0.7"),
            mac_input(1, &[2; 16], 10, "marie", "10.0.0.7"),
            mac_input(1, &[1; 16], 11, "marie", "10.0.0.7"),
            mac_input(1, &[1; 16], 10, "paul", "10.0.0.7"),
            mac_input(1, &[1; 16], 10, "marie", "10.0.0.8"),
        ];
        for variant in &variants {
            assert_ne!(variant, &base);
        }
    }

    #[test]
    fn the_code_input_normalizes_the_identifier_and_the_address() {
        assert_eq!(
            mac_input(1, &[1; 16], 10, "  MARIE ", "::ffff:10.0.0.7"),
            mac_input(1, &[1; 16], 10, "marie", "10.0.0.7")
        );
    }
}
