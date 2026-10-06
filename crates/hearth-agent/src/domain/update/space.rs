//! BR-UPDATE-029 : la place qu'il faut avant de copier la base. Une copie qui ne tient pas, ou une
//! place qui ne se mesure pas, ne s'écrit jamais « assez de place » : l'échange n'a pas lieu et
//! l'ancien agent repart.

use thiserror::Error;

/// Marge au-delà de deux fois la taille de la base (la copie et la remise).
const MARGIN_BYTES: u64 = 1024 * 1024;

/// L'espace libre nécessaire pour copier une base de `database_bytes` : deux fois sa taille, plus
/// une marge.
pub fn required_space(database_bytes: u64) -> u64 {
    database_bytes
        .saturating_mul(2)
        .saturating_add(MARGIN_BYTES)
}

/// Pourquoi la copie de la base n'aura pas lieu. Les textes vont au journal technique.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SpaceRefusal {
    #[error(
        "espace disque insuffisant pour copier la base ({free} octets libres, {needed} nécessaires)"
    )]
    Insufficient { free: u64, needed: u64 },
    #[error("espace disque impossible à mesurer, copie de la base refusée : {0}")]
    Unknown(String),
}

/// Assez de place pour copier la base ? `free` est la mesure faite par la machine (ou son échec).
pub fn check_space(database_bytes: u64, free: Result<u64, String>) -> Result<(), SpaceRefusal> {
    let needed = required_space(database_bytes);
    match free {
        Ok(free) if free >= needed => Ok(()),
        Ok(free) => Err(SpaceRefusal::Insufficient { free, needed }),
        Err(reason) => Err(SpaceRefusal::Unknown(reason)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1024 * 1024;

    #[test]
    fn two_copies_of_the_database_and_a_margin_are_needed() {
        assert_eq!(required_space(0), MIB);
        assert_eq!(required_space(10 * MIB), 21 * MIB);
        assert_eq!(required_space(u64::MAX), u64::MAX, "pas de dépassement");
    }

    #[test]
    fn enough_room_lets_the_swap_go_on() {
        assert_eq!(check_space(10 * MIB, Ok(21 * MIB)), Ok(()));
        assert_eq!(check_space(0, Ok(u64::MAX)), Ok(()));
    }

    #[test]
    fn not_enough_room_refuses_and_says_how_much_was_missing() {
        assert_eq!(
            check_space(10 * MIB, Ok(21 * MIB - 1)),
            Err(SpaceRefusal::Insufficient {
                free: 21 * MIB - 1,
                needed: 21 * MIB
            })
        );
        assert_eq!(
            check_space(0, Ok(0)),
            Err(SpaceRefusal::Insufficient {
                free: 0,
                needed: MIB
            })
        );
    }

    #[test]
    fn a_room_that_cannot_be_measured_is_a_refusal_never_enough_room() {
        let refusal = check_space(MIB, Err("df introuvable".into())).unwrap_err();
        assert_eq!(refusal, SpaceRefusal::Unknown("df introuvable".into()));
        assert!(refusal.to_string().contains("df introuvable"));
    }
}
