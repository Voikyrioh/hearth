//! Le scellement des secrets d'une description de service, par `ring` (HRT-83, ADR-0044, ADR-0009).
//!
//! **ChaCha20-Poly1305** (chiffrement authentifié, RFC 8439), `ring::aead` : la bibliothèque est celle que
//! `rustls` compile déjà (aucune pile native, ni OpenSSL ni aws-lc, aucune dépendance de plus).
//!
//! - **Nonce de 96 bits tiré du hasard du système pour CHAQUE scellement**, jamais réutilisé : 2^96
//!   valeurs, des milliers de scellés au plus (collision négligeable) ; la base refuse en plus deux nonces
//!   identiques (`service_spec_secrets_nonce`, index unique), et un tirage qui échoue est une erreur, pas
//!   un nonce par défaut.
//! - **Données associées** = étiquette de domaine, version du format, clé du service, révision, nom du
//!   champ (`domain::sealed::associated_data`) : un scellé recollé dans un autre service, une autre
//!   révision ou un autre champ ne s'ouvre pas.
//! - La **vérification de clé** est un HMAC-SHA256 d'une constante sous la même clé : elle dit plus tard si
//!   c'est la même clé, sans rien révéler d'elle.
//!
//! Pas de rotation de clé dans cette tranche : le champ `format` des scellés et les données associées
//! versionnées la rendent possible (re-sceller toutes les lignes sous une clé neuve dans une transaction),
//! elle n'est ni écrite ni exposée. Voir ADR-0044.

use std::sync::Arc;

use ring::aead::{Aad, CHACHA20_POLY1305, LessSafeKey, Nonce, UnboundKey};
use ring::hmac;
use zeroize::Zeroize;

use crate::application::ports::{SealError, SealerFactory, SecretSealer};
use crate::domain::sealed::{
    MAX_PLAIN_BYTES, NONCE_LEN, PlainBytes, SealContext, SealedBlob, ServicesKey, TAG_LEN,
    associated_data,
};

/// Ce que le HMAC de vérification signe. Une constante : sa valeur ne dépend que de la clé.
const KEY_CHECK_MESSAGE: &[u8] = b"hearth/services-key-check/v1";

pub struct RingSealer {
    key: LessSafeKey,
    check: String,
}

impl RingSealer {
    /// `None` si `ring` refuse la clé (impossible pour 32 octets : la taille est celle de l'algorithme).
    pub fn new(key: &ServicesKey) -> Option<Self> {
        let unbound = UnboundKey::new(&CHACHA20_POLY1305, key.expose()).ok()?;
        let mac = hmac::sign(
            &hmac::Key::new(hmac::HMAC_SHA256, key.expose()),
            KEY_CHECK_MESSAGE,
        );
        Some(Self {
            key: LessSafeKey::new(unbound),
            check: hex(mac.as_ref()),
        })
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

impl SecretSealer for RingSealer {
    fn seal(&self, context: &SealContext<'_>, plain: &[u8]) -> Result<SealedBlob, SealError> {
        if plain.len() > MAX_PLAIN_BYTES {
            return Err(SealError::TooLong);
        }
        let mut nonce = [0_u8; NONCE_LEN];
        getrandom::fill(&mut nonce).map_err(|_| SealError::Random)?;
        let mut in_out = Vec::with_capacity(plain.len() + TAG_LEN);
        in_out.extend_from_slice(plain);
        let sealed = self.key.seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(associated_data(context)),
            &mut in_out,
        );
        match sealed {
            // `in_out` ne contient plus que le texte chiffré et l'étiquette : `ring` l'a chiffré en place.
            Ok(()) => Ok(SealedBlob {
                nonce,
                bytes: in_out,
            }),
            Err(_) => {
                in_out.zeroize();
                Err(SealError::Open)
            }
        }
    }

    fn open(&self, context: &SealContext<'_>, blob: &SealedBlob) -> Result<PlainBytes, SealError> {
        if !blob.is_well_formed() {
            return Err(SealError::Open);
        }
        let mut buffer = blob.bytes.clone();
        let opened = self.key.open_in_place(
            Nonce::assume_unique_for_key(blob.nonce),
            Aad::from(associated_data(context)),
            &mut buffer,
        );
        let result = match opened {
            Ok(plain) => Ok(PlainBytes::new(plain.to_vec())),
            Err(_) => Err(SealError::Open),
        };
        buffer.zeroize();
        result
    }

    fn key_check(&self) -> String {
        self.check.clone()
    }
}

pub struct RingSealerFactory;

impl SealerFactory for RingSealerFactory {
    fn build(&self, key: ServicesKey) -> Result<Arc<dyn SecretSealer>, SealError> {
        RingSealer::new(&key)
            .map(|sealer| Arc::new(sealer) as Arc<dyn SecretSealer>)
            .ok_or(SealError::Open)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::sealed::{AnswerId, KEY_LEN, SecretField};

    fn sealer(byte: u8) -> RingSealer {
        RingSealer::new(&ServicesKey::from_bytes([byte; KEY_LEN])).unwrap()
    }

    fn context<'a>(service: &'a str, rev: u32, field: &'a SecretField) -> SealContext<'a> {
        SealContext {
            service,
            rev,
            field,
        }
    }

    #[test]
    fn a_sealed_value_opens_back_to_itself_and_is_not_the_clear_text() {
        let field = SecretField::Env(0);
        let ctx = context("h_A", 1, &field);
        let s = sealer(7);
        let blob = s.seal(&ctx, b"SENTINEL-VALUE").unwrap();
        assert_eq!(blob.bytes.len(), "SENTINEL-VALUE".len() + TAG_LEN);
        assert!(!blob.bytes.windows(8).any(|w| w == b"SENTINEL"));
        assert_eq!(s.open(&ctx, &blob).unwrap().expose(), b"SENTINEL-VALUE");
        // Vide : seulement l'étiquette.
        let empty = s.seal(&ctx, b"").unwrap();
        assert_eq!(empty.bytes.len(), TAG_LEN);
        assert_eq!(s.open(&ctx, &empty).unwrap().len(), 0);
    }

    #[test]
    fn every_sealing_draws_a_new_nonce_even_for_the_same_value_and_context() {
        let field = SecretField::Command;
        let ctx = context("h_A", 1, &field);
        let s = sealer(7);
        let mut nonces = std::collections::HashSet::new();
        let mut texts = std::collections::HashSet::new();
        for _ in 0..500 {
            let blob = s.seal(&ctx, b"same value").unwrap();
            assert!(nonces.insert(blob.nonce), "nonce réutilisé");
            assert!(texts.insert(blob.bytes), "deux scellés identiques");
        }
    }

    #[test]
    fn a_sealed_value_pasted_elsewhere_is_refused() {
        let s = sealer(7);
        let env0 = SecretField::Env(0);
        let env1 = SecretField::Env(1);
        let blob = s.seal(&context("h_A", 3, &env0), b"v").unwrap();
        assert!(s.open(&context("h_A", 3, &env0), &blob).is_ok());
        assert_eq!(
            s.open(&context("h_B", 3, &env0), &blob).unwrap_err(),
            SealError::Open,
            "autre service"
        );
        assert_eq!(
            s.open(&context("h_A", 4, &env0), &blob).unwrap_err(),
            SealError::Open,
            "autre révision"
        );
        assert_eq!(
            s.open(&context("h_A", 3, &env1), &blob).unwrap_err(),
            SealError::Open,
            "autre champ"
        );
        let answer = SecretField::Answer(AnswerId::parse("password").unwrap());
        assert!(
            s.open(&context("h_A", 3, &answer), &blob).is_err(),
            "autre genre de champ"
        );
    }

    #[test]
    fn another_key_or_a_flipped_bit_never_opens() {
        let field = SecretField::Env(0);
        let ctx = context("h_A", 1, &field);
        let blob = sealer(7).seal(&ctx, b"secret").unwrap();
        assert!(sealer(8).open(&ctx, &blob).is_err());
        for index in 0..blob.bytes.len() {
            let mut altered = blob.clone();
            altered.bytes[index] ^= 0x01;
            assert!(sealer(7).open(&ctx, &altered).is_err(), "octet {index}");
        }
        let mut other_nonce = blob.clone();
        other_nonce.nonce[0] ^= 0x01;
        assert!(sealer(7).open(&ctx, &other_nonce).is_err());
        // Tronqué sous la taille de l'étiquette.
        let short = SealedBlob {
            nonce: blob.nonce,
            bytes: blob.bytes[..TAG_LEN - 1].to_vec(),
        };
        assert!(sealer(7).open(&ctx, &short).is_err());
    }

    #[test]
    fn a_value_that_is_too_long_is_refused_not_cut() {
        let field = SecretField::Command;
        let ctx = context("h_A", 1, &field);
        let s = sealer(7);
        assert!(s.seal(&ctx, &vec![0_u8; MAX_PLAIN_BYTES]).is_ok());
        assert_eq!(
            s.seal(&ctx, &vec![0_u8; MAX_PLAIN_BYTES + 1]).unwrap_err(),
            SealError::TooLong
        );
    }

    #[test]
    fn the_key_check_identifies_the_key_without_showing_it() {
        let a = sealer(1).key_check();
        assert_eq!(a, sealer(1).key_check());
        assert_ne!(a, sealer(2).key_check());
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        // Le texte de vérification ne contient pas la clé.
        assert!(!a.contains(&hex(&[1; KEY_LEN])));
    }

    #[test]
    fn the_known_chacha20_poly1305_vector_of_rfc_8439_still_holds_for_the_primitive() {
        // Vérifie que l'algorithme branché est bien ChaCha20-Poly1305 (RFC 8439, section 2.8.2).
        let key = hex_to_bytes("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f");
        let nonce: [u8; 12] = hex_to_bytes("070000004041424344454647").try_into().unwrap();
        let aad = hex_to_bytes("50515253c0c1c2c3c4c5c6c7");
        let plain = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";
        let unbound = UnboundKey::new(&CHACHA20_POLY1305, &key).unwrap();
        let key = LessSafeKey::new(unbound);
        let mut buffer = plain.to_vec();
        key.seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(aad),
            &mut buffer,
        )
        .unwrap();
        assert_eq!(
            hex(&buffer[buffer.len() - TAG_LEN..]),
            "1ae10b594f09e26a7e902ecbd0600691"
        );
        assert_eq!(&hex(&buffer[..16]), "d31a8d34648e60db7b86afbc53ef7ec2");
    }

    fn hex_to_bytes(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn the_factory_builds_a_working_sealer() {
        let built = RingSealerFactory
            .build(ServicesKey::from_bytes([5; KEY_LEN]))
            .unwrap();
        assert_eq!(built.key_check(), sealer(5).key_check());
    }
}
