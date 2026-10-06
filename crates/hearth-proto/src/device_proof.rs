//! Preuve de possession de la clé d'appareil (HRT-22, ADR-0023, BR-TRUST-005).
//!
//! Ce module est **la source unique** de la disposition en octets du message que le client signe
//! et que l'agent vérifie : les deux côtés l'appellent, aucun ne la recopie. Aucune E/S, aucune
//! cryptographie ici (la signature et sa vérification vivent dans `ring`, côté agent et côté
//! liaison).
//!
//! # Disposition du message signé
//!
//! ```text
//! "hearth-device-proof/1" || 0x00
//! || usage (1 octet : 0x01 connexion, 0x02 session, 0x03 mode attaque)
//! || empreinte SHA-256 du certificat du serveur épinglé (32 octets)
//! || longueur de l'identifiant normalisé (2 octets, grand-boutiste) || identifiant normalisé (UTF-8)
//! || défi (56 octets)
//! || usages 0x02 et 0x03 : SHA-256 du jeton de session (32 octets)
//! || usage 0x03 : 0x01 pour activer, 0x00 pour désactiver
//! ```
//!
//! La signature lie ainsi la preuve au serveur (une preuve obtenue par un faux serveur ne vaut
//! rien sur le vrai), à l'identifiant, à l'usage et, pour les usages qui en ont un, au jeton : une
//! preuve de connexion ne sert pas à authentifier un flux, ni l'inverse.

use sha2::{Digest, Sha256};

use crate::fingerprint::Fingerprint;

/// Algorithme de la clé d'appareil : seul `ed25519` existe (la colonne et le champ permettent
/// d'en ajouter un autre sans migration destructive).
pub const ALGORITHM_ED25519: &str = "ed25519";

/// Préfixe de domaine du message signé : il sépare cette signature de toute autre.
pub const DOMAIN: &[u8] = b"hearth-device-proof/1";

/// Taille d'un défi : nonce (16) || émission en millisecondes d'horloge monotone (8) || code (32).
pub const CHALLENGE_LEN: usize = 56;
/// Taille de la clé publique Ed25519.
pub const PUBLIC_KEY_LEN: usize = 32;
/// Taille d'une signature Ed25519.
pub const SIGNATURE_LEN: usize = 64;
/// Taille d'une empreinte de jeton de session (SHA-256).
pub const TOKEN_HASH_LEN: usize = 32;
/// Durée de validité d'un défi, en secondes.
pub const CHALLENGE_TTL_S: u64 = 60;
/// Longueur maximale, en caractères, de l'identifiant normalisé qui entre dans le message : plus
/// long que tout identifiant valide (32), il borne la taille du message sans rien tronquer d'utile.
pub const IDENTIFIER_MAX_CHARS: usize = 64;

/// À quoi sert la preuve, avec ce qu'elle lie en plus du défi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Binding<'a> {
    /// Connexion par mot de passe (usage `0x01`).
    Login,
    /// Ouverture du flux d'une session (usage `0x02`), liée au jeton.
    Session {
        token_hash: &'a [u8; TOKEN_HASH_LEN],
    },
    /// Activation ou désactivation du mode attaque (usage `0x03`), liée au jeton et au geste.
    AttackMode {
        token_hash: &'a [u8; TOKEN_HASH_LEN],
        activate: bool,
    },
}

impl Binding<'_> {
    /// Octet d'usage du message signé.
    pub const fn usage(&self) -> u8 {
        match self {
            Self::Login => 0x01,
            Self::Session { .. } => 0x02,
            Self::AttackMode { .. } => 0x03,
        }
    }
}

/// L'identifiant tel qu'il entre dans le message et dans le code du défi : sans espace autour, en
/// minuscules, 64 caractères au plus. Pour un identifiant valide c'est exactement l'identifiant du
/// compte ; pour une saisie quelconque c'est une valeur stable, que le client et l'agent calculent
/// de la même façon (l'agent émet un défi pour toute saisie, existante ou non).
pub fn normalize_identifier(raw: &str) -> String {
    raw.trim()
        .to_lowercase()
        .chars()
        .take(IDENTIFIER_MAX_CHARS)
        .collect()
}

/// Empreinte d'une clé publique : les 16 premiers octets de son SHA-256, en hexadécimal minuscule
/// (32 caractères). Sert d'identifiant de clé : unique par compte, jamais la clé elle-même.
pub fn key_id(public_key: &[u8; PUBLIC_KEY_LEN]) -> String {
    let digest = Sha256::digest(public_key);
    digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Octets à signer (voir la disposition en tête de module). `identifier` est normalisé ici : un
/// appelant ne peut pas signer ou vérifier une autre forme que l'autre côté.
pub fn signing_bytes(
    binding: Binding<'_>,
    fingerprint: &Fingerprint,
    identifier: &str,
    challenge: &[u8; CHALLENGE_LEN],
) -> Vec<u8> {
    let identifier = normalize_identifier(identifier);
    let identifier = identifier.as_bytes();
    // Au plus 64 caractères, donc 256 octets : la conversion ne sature jamais.
    let length = u16::try_from(identifier.len()).unwrap_or(u16::MAX);
    let mut bytes = Vec::with_capacity(DOMAIN.len() + 1 + 1 + 32 + 2 + identifier.len() + 56 + 33);
    bytes.extend_from_slice(DOMAIN);
    bytes.push(0x00);
    bytes.push(binding.usage());
    bytes.extend_from_slice(fingerprint.as_bytes());
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(identifier);
    bytes.extend_from_slice(challenge);
    match binding {
        Binding::Login => {}
        Binding::Session { token_hash } => bytes.extend_from_slice(token_hash),
        Binding::AttackMode {
            token_hash,
            activate,
        } => {
            bytes.extend_from_slice(token_hash);
            bytes.push(u8::from(activate));
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint() -> Fingerprint {
        Fingerprint::from_bytes([0xaa; 32])
    }

    fn challenge() -> [u8; CHALLENGE_LEN] {
        let mut bytes = [0_u8; CHALLENGE_LEN];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::try_from(index).unwrap_or(0);
        }
        bytes
    }

    /// Vecteur de référence, assemblé à la main : si la disposition change, ce test casse, et le
    /// client avec elle.
    #[test]
    fn the_login_layout_is_the_documented_one() {
        let message = signing_bytes(Binding::Login, &fingerprint(), "marie", &challenge());
        let mut expected = b"hearth-device-proof/1".to_vec();
        expected.push(0x00);
        expected.push(0x01);
        expected.extend_from_slice(&[0xaa; 32]);
        expected.extend_from_slice(&[0x00, 0x05]);
        expected.extend_from_slice(b"marie");
        expected.extend_from_slice(&challenge());
        assert_eq!(message, expected);
        assert_eq!(message.len(), 21 + 1 + 1 + 32 + 2 + 5 + 56);
    }

    #[test]
    fn the_session_layout_adds_the_token_hash() {
        let hash = [0x11; 32];
        let message = signing_bytes(
            Binding::Session { token_hash: &hash },
            &fingerprint(),
            "marie",
            &challenge(),
        );
        let login = signing_bytes(Binding::Login, &fingerprint(), "marie", &challenge());
        assert_eq!(message[21 + 1], 0x02, "octet d'usage");
        assert_eq!(&message[..21 + 1], &login[..21 + 1]);
        assert_eq!(&message[message.len() - 32..], &hash);
        assert_eq!(message.len(), login.len() + 32);
    }

    #[test]
    fn the_attack_mode_layout_adds_the_token_hash_and_the_gesture() {
        let hash = [0x22; 32];
        let on = signing_bytes(
            Binding::AttackMode {
                token_hash: &hash,
                activate: true,
            },
            &fingerprint(),
            "marie",
            &challenge(),
        );
        let off = signing_bytes(
            Binding::AttackMode {
                token_hash: &hash,
                activate: false,
            },
            &fingerprint(),
            "marie",
            &challenge(),
        );
        assert_eq!(on[21 + 1], 0x03);
        assert_eq!(on.last(), Some(&0x01));
        assert_eq!(off.last(), Some(&0x00));
        assert_ne!(on, off, "une preuve ne sert pas à l'autre geste");
    }

    #[test]
    fn two_proofs_for_different_things_never_share_their_bytes() {
        let hash = [0x33; 32];
        let other_hash = [0x34; 32];
        let base = signing_bytes(Binding::Login, &fingerprint(), "marie", &challenge());
        let mut variants = vec![
            // usage
            signing_bytes(
                Binding::Session { token_hash: &hash },
                &fingerprint(),
                "marie",
                &challenge(),
            ),
            // identifiant
            signing_bytes(Binding::Login, &fingerprint(), "paul", &challenge()),
            // serveur
            signing_bytes(
                Binding::Login,
                &Fingerprint::from_bytes([0xab; 32]),
                "marie",
                &challenge(),
            ),
            // défi
            signing_bytes(Binding::Login, &fingerprint(), "marie", &[0; CHALLENGE_LEN]),
        ];
        // jeton
        variants.push(signing_bytes(
            Binding::Session {
                token_hash: &other_hash,
            },
            &fingerprint(),
            "marie",
            &challenge(),
        ));
        for variant in &variants {
            assert_ne!(variant, &base);
        }
        let unique: std::collections::HashSet<_> = variants.iter().collect();
        assert_eq!(unique.len(), variants.len());
    }

    #[test]
    fn the_identifier_is_normalized_the_same_way_on_both_sides() {
        let a = signing_bytes(Binding::Login, &fingerprint(), "  MARIE ", &challenge());
        let b = signing_bytes(Binding::Login, &fingerprint(), "marie", &challenge());
        assert_eq!(a, b);
        assert_eq!(normalize_identifier("  MaRie\t"), "marie");
    }

    #[test]
    fn an_absurdly_long_identifier_is_bounded_and_never_panics() {
        let long = "é".repeat(100_000);
        let normalized = normalize_identifier(&long);
        assert_eq!(normalized.chars().count(), IDENTIFIER_MAX_CHARS);
        let message = signing_bytes(Binding::Login, &fingerprint(), &long, &challenge());
        assert!(message.len() < 1024);
        let empty = signing_bytes(Binding::Login, &fingerprint(), "", &challenge());
        assert_eq!(&empty[21 + 2 + 32..21 + 2 + 32 + 2], &[0, 0]);
    }

    #[test]
    fn the_length_prefix_prevents_a_shifted_identifier_from_colliding() {
        // « ab » suivi d'un défi qui commence par 'c' ne doit pas valoir « abc » suivi du reste.
        let mut first = [0_u8; CHALLENGE_LEN];
        first[0] = b'c';
        let a = signing_bytes(Binding::Login, &fingerprint(), "ab", &first);
        let mut second = [0_u8; CHALLENGE_LEN];
        second[..CHALLENGE_LEN - 1].copy_from_slice(&first[1..]);
        let b = signing_bytes(Binding::Login, &fingerprint(), "abc", &second);
        assert_ne!(a, b);
    }

    #[test]
    fn the_key_id_is_sixteen_bytes_of_the_sha256_in_hex() {
        let id = key_id(&[0; 32]);
        assert_eq!(id.len(), 32);
        assert!(
            id.bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        );
        // SHA-256 de 32 octets nuls : 66687aadf862bd776c8fc18b8e9f8e20...
        assert_eq!(id, "66687aadf862bd776c8fc18b8e9f8e20");
        assert_ne!(key_id(&[1; 32]), id);
    }
}
