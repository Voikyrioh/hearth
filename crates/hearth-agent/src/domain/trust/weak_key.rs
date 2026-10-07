//! Clés publiques de petit ordre (HRT-22, revue de PR 25, ADR-0023).
//!
//! `ring` (0.17.14, `ec/curve25519/ed25519/verification.rs`) décode la clé publique sans contrôler son
//! ordre : avec la clé « point neutre » et la signature (R = point neutre, S = 0), la vérification est
//! vraie pour **tout** message. Une telle clé ne prouve donc rien, et le serveur la refuse **à la
//! vérification** (donc aussi à l'inscription, qui exige une preuve vérifiée).
//!
//! Source de la liste : les huit points de petit ordre de la courbe d'Ed25519 (ordre 1, 2, 4 et 8, le
//! sous-groupe de torsion, cofacteur 8) et leurs encodages non canoniques, tels que la bibliothèque
//! de référence de libsodium (`ed25519_ref10`, `has_small_order`) les liste. Un encodage de 32 octets
//! donne `y` (255 bits, petit-boutiste) et un bit de signe de `x` (bit de poids fort de l'octet 31) ;
//! les points de petit ordre ont `y` parmi : `0` (ordre 4), `1` (ordre 1), `p - 1` (ordre 2) et les
//! deux valeurs d'ordre 8 ci-dessous (`y8` et `p - y8`), plus les encodages non canoniques `p`
//! (= 0) et `p + 1` (= 1) qui tiennent sur 255 bits. Le bit de signe est ignoré : refuser l'une ou
//! l'autre de ses valeurs refuse un sur-ensemble des huit points, jamais une clé honnête (une clé
//! tirée au hasard tombe dans cette liste avec une probabilité d'environ 2^-250).
//! Les constantes sont vérifiées en test : `y8` est d'ordre 8 (calculé hors du dépôt avec l'équation
//! de la courbe, -x² + y² = 1 + d·x²·y², d = -121665/121666 modulo p = 2^255 - 19).

/// `y` des points de petit ordre, 255 bits petit-boutiste (bit de signe effacé).
const SMALL_ORDER_Y: [[u8; 32]; 7] = [
    // y = 0 (ordre 4)
    [0; 32],
    // y = 1 (ordre 1 : le point neutre)
    one(),
    // y = p - 1 (ordre 2)
    p_plus(-1),
    // y = p (encodage non canonique de 0)
    p_plus(0),
    // y = p + 1 (encodage non canonique de 1)
    p_plus(1),
    // y8 (ordre 8)
    [
        0x26, 0xe8, 0x95, 0x8f, 0xc2, 0xb2, 0x27, 0xb0, 0x45, 0xc3, 0xf4, 0x89, 0xf2, 0xef, 0x98,
        0xf0, 0xd5, 0xdf, 0xac, 0x05, 0xd3, 0xc6, 0x33, 0x39, 0xb1, 0x38, 0x02, 0x88, 0x6d, 0x53,
        0xfc, 0x05,
    ],
    // p - y8 (ordre 8)
    [
        0xc7, 0x17, 0x6a, 0x70, 0x3d, 0x4d, 0xd8, 0x4f, 0xba, 0x3c, 0x0b, 0x76, 0x0d, 0x10, 0x67,
        0x0f, 0x2a, 0x20, 0x53, 0xfa, 0x2c, 0x39, 0xcc, 0xc6, 0x4e, 0xc7, 0xfd, 0x77, 0x92, 0xac,
        0x03, 0x7a,
    ],
];

const fn one() -> [u8; 32] {
    let mut bytes = [0_u8; 32];
    bytes[0] = 1;
    bytes
}

/// `p + delta` pour `delta` dans -1..=1, avec p = 2^255 - 19 : 0xed suivi de 30 × 0xff et 0x7f.
const fn p_plus(delta: i8) -> [u8; 32] {
    let mut bytes = [0xff_u8; 32];
    bytes[31] = 0x7f;
    bytes[0] = match delta {
        -1 => 0xec,
        0 => 0xed,
        _ => 0xee,
    };
    bytes
}

/// La clé publique est-elle un point de petit ordre (ou son encodage non canonique) ? À refuser
/// avant toute vérification de signature.
pub fn has_small_order(public_key: &[u8; 32]) -> bool {
    let mut y = *public_key;
    y[31] &= 0x7f;
    SMALL_ORDER_Y.contains(&y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_neutral_point_and_the_other_small_order_encodings_are_refused_with_either_sign_bit() {
        for (n, y) in SMALL_ORDER_Y.iter().enumerate() {
            assert!(has_small_order(y), "y n° {n}");
            let mut signed = *y;
            signed[31] |= 0x80;
            assert!(has_small_order(&signed), "y n° {n}, bit de signe");
        }
        assert_eq!(SMALL_ORDER_Y.len(), 7);
    }

    #[test]
    fn the_seven_values_are_distinct_and_the_order_eight_ones_are_the_documented_ones() {
        let unique: std::collections::HashSet<_> = SMALL_ORDER_Y.iter().collect();
        assert_eq!(unique.len(), 7);
        // p - y8 = y8' : somme des deux valeurs d'ordre 8 = p (petit-boutiste, retenue propagée).
        let (a, b) = (&SMALL_ORDER_Y[5], &SMALL_ORDER_Y[6]);
        let mut carry = 0_u16;
        let mut sum = [0_u8; 32];
        for i in 0..32 {
            let total = u16::from(a[i]) + u16::from(b[i]) + carry;
            sum[i] = (total & 0xff) as u8;
            carry = total >> 8;
        }
        assert_eq!(sum, p_plus(0), "y8 + (p - y8) = p");
    }

    #[test]
    fn honest_keys_pass() {
        for key in [[2_u8; 32], [0x55; 32], [0xab; 32]] {
            assert!(!has_small_order(&key));
        }
        // Une clé qui ne diffère d'un point de petit ordre que par un octet ne l'est plus.
        let mut almost = one();
        almost[7] = 1;
        assert!(!has_small_order(&almost));
    }
}
