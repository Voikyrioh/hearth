//! Tirage d'un secret généré par l'agent (HRT-101, BR-FICHE-013, invariant I7) : les règles seulement.
//!
//! Le domaine ne tire aucun nombre : l'appelant fournit des octets (le port `SecretRandom`) et ce module les
//! transforme. Alphabet `alnum` (62 caractères) : un octet n'est accepté que s'il est sous 248 (4 fois 62), le
//! reste est REJETÉ, sinon les premiers caractères de l'alphabet seraient plus probables que les autres. Aucune
//! E/S ici ; la valeur rendue est un `Secret`.

use super::loading::schema::fiche::{GeneratedSecret, SecretAlphabet};
use crate::domain::secret::Secret;

const ALNUM: &[u8; 62] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
/// Le plus grand multiple de 62 sous 256 : au-dessus, l'octet est rejeté.
const ALNUM_LIMIT: u8 = 248;

/// Le caractère d'un octet, ou `None` si l'octet est rejeté (biais).
pub fn alnum_char(byte: u8) -> Option<char> {
    (byte < ALNUM_LIMIT).then(|| char::from(ALNUM[usize::from(byte) % ALNUM.len()]))
}

/// Un secret fait des octets donnés, ou `None` s'ils ne suffisent pas (trop d'octets rejetés) : l'appelant en
/// demande d'autres, il ne complète jamais lui-même.
pub fn secret_from_bytes(spec: &GeneratedSecret, bytes: &[u8]) -> Option<Secret> {
    let wanted = spec.length as usize;
    let text: String = match spec.alphabet {
        SecretAlphabet::Alnum => bytes
            .iter()
            .filter_map(|b| alnum_char(*b))
            .take(wanted)
            .collect(),
    };
    (text.chars().count() == wanted).then(|| Secret::new(text))
}

/// Nombre de demandes d'octets avant d'abandonner : la probabilité de rejeter autant d'octets est nulle en
/// pratique, mais un générateur défaillant qui rend toujours le même octet rejeté ne boucle pas.
pub const MAX_ROUNDS: usize = 8;

/// Tire un secret avec une source d'octets (le port, côté application). Un échec de la source est rendu tel quel.
pub fn draw_secret<E>(
    spec: &GeneratedSecret,
    mut fill: impl FnMut(&mut [u8]) -> Result<(), E>,
) -> Result<Option<Secret>, E> {
    // Deux fois la longueur suffit presque toujours (3 % de rejet).
    let mut buffer = vec![0_u8; (spec.length as usize).saturating_mul(2).max(16)];
    for _ in 0..MAX_ROUNDS {
        fill(&mut buffer)?;
        if let Some(secret) = secret_from_bytes(spec, &buffer) {
            buffer.fill(0);
            return Ok(Some(secret));
        }
    }
    buffer.fill(0);
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::catalog::test_support::rich;

    fn spec(length: u32) -> GeneratedSecret {
        let mut spec = rich().file.secret[0].clone();
        spec.length = length;
        spec
    }

    #[test]
    fn generated_secret_has_the_declared_length_and_alphabet_without_bias() {
        // Sans biais : les 256 valeurs d'un octet donnent chaque caractère exactement 4 fois, et les 8 dernières
        // (248 à 255) sont rejetées.
        let mut counts = std::collections::BTreeMap::new();
        for byte in 0..=255_u8 {
            match alnum_char(byte) {
                Some(c) => *counts.entry(c).or_insert(0_u32) += 1,
                None => assert!(byte >= ALNUM_LIMIT),
            }
        }
        assert_eq!(counts.len(), 62);
        assert!(counts.values().all(|n| *n == 4), "{counts:?}");
        // La longueur et l'alphabet déclarés, octets rejetés sautés.
        let mut bytes: Vec<u8> = vec![255, 254, 248];
        bytes.extend(0..=255_u8);
        for length in [1, 16, 32, 64] {
            let secret = secret_from_bytes(&spec(length), &bytes).expect("assez d'octets");
            assert_eq!(secret.expose().chars().count(), length as usize);
            assert!(secret.expose().chars().all(|c| c.is_ascii_alphanumeric()));
        }
        let first = secret_from_bytes(&spec(3), &[250, 0, 25, 26, 61]).expect("secret");
        assert_eq!(
            first.expose(),
            "AZa",
            "l'octet 250 est rejeté, 0 donne A, 25 donne Z, 26 donne a"
        );
        // Pas assez d'octets acceptés : rien, jamais un secret plus court.
        assert!(secret_from_bytes(&spec(4), &[255, 0, 1, 2, 250]).is_none());
    }

    #[test]
    fn a_source_that_only_gives_rejected_bytes_is_given_up_not_looped() {
        let mut rounds = 0;
        let drawn = draw_secret(&spec(32), |buf: &mut [u8]| -> Result<(), ()> {
            rounds += 1;
            buf.fill(255);
            Ok(())
        });
        assert_eq!(drawn.map(|s| s.is_none()), Ok(true));
        assert_eq!(rounds, MAX_ROUNDS);
        let failed = draw_secret(&spec(32), |_: &mut [u8]| -> Result<(), &str> {
            Err("panne")
        });
        assert_eq!(failed.map(|s| s.is_none()), Err("panne"));
        let ok = draw_secret(&spec(32), |buf: &mut [u8]| -> Result<(), ()> {
            for (i, b) in buf.iter_mut().enumerate() {
                *b = (i % 200) as u8;
            }
            Ok(())
        });
        assert_eq!(
            ok.ok().flatten().map(|s| s.expose().chars().count()),
            Some(32)
        );
    }
}
