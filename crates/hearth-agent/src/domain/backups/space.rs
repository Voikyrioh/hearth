//! L'espace d'une sauvegarde (BR-BACKUP-009, ADR-0047 point 9).
//!
//! Remplir le disque du serveur coupe tous les services : la réserve est donc **absolue**, aucune case ne
//! permet de passer outre. Avant : la taille des éléments inclus (sans compter la compression) doit tenir dans
//! l'espace libre moins la réserve. Pendant : sous la moitié de la réserve, la copie s'arrête et la partie
//! écrite est supprimée.

use std::time::Duration;

/// La réserve plancher : 5 Gio.
pub const MIN_RESERVE_BYTES: u64 = 5 * 1024 * 1024 * 1024;
/// L'espace libre est relu tous les 256 Mio écrits.
pub const CHECK_EVERY_BYTES: u64 = 256 * 1024 * 1024;
/// Une copie dure 6 heures au plus : ensuite elle s'arrête et sa partie écrite est supprimée (BR-BACKUP-010).
pub const MAX_COPY: Duration = Duration::from_secs(6 * 60 * 60);

/// La réserve d'un disque de `total` octets : le plus grand de 5 Gio et de 5 % du disque.
pub fn reserve(total: u64) -> u64 {
    MIN_RESERVE_BYTES.max(total / 20)
}

/// Ce que l'espace libre permet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceVerdict {
    Enough,
    /// « Il faut jusqu'à {needed} pour cette sauvegarde ; il reste {free} sur le disque des sauvegardes
    /// (réserve de {reserve} comprise). »
    Refused {
        needed: u64,
        free: u64,
        reserve: u64,
    },
}

/// La sauvegarde tient-elle ? `needed` : la taille des éléments inclus ; `free` et `total` : le disque des
/// sauvegardes.
pub fn judge(needed: u64, free: u64, total: u64) -> SpaceVerdict {
    let reserve = reserve(total);
    if needed > free.saturating_sub(reserve) {
        SpaceVerdict::Refused {
            needed,
            free,
            reserve,
        }
    } else {
        SpaceVerdict::Enough
    }
}

/// Pendant la copie : l'espace libre est-il tombé sous la moitié de la réserve ?
pub fn too_low_during_copy(free: u64, total: u64) -> bool {
    free < reserve(total) / 2
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn the_reserve_is_five_gibibytes_or_five_percent_whichever_is_larger() {
        assert_eq!(reserve(10 * GIB), 5 * GIB);
        assert_eq!(reserve(100 * GIB), 5 * GIB);
        assert_eq!(reserve(1000 * GIB), 50 * GIB);
    }

    #[test]
    fn a_backup_that_would_eat_the_reserve_is_refused_with_the_figures() {
        assert_eq!(judge(10 * GIB, 100 * GIB, 200 * GIB), SpaceVerdict::Enough);
        // Libre 12 Gio, réserve 5 Gio : 7 Gio possibles, 8 refusés.
        assert_eq!(judge(7 * GIB, 12 * GIB, 100 * GIB), SpaceVerdict::Enough);
        assert_eq!(
            judge(8 * GIB, 12 * GIB, 100 * GIB),
            SpaceVerdict::Refused {
                needed: 8 * GIB,
                free: 12 * GIB,
                reserve: 5 * GIB
            }
        );
        // Un disque déjà sous sa réserve n'accepte rien, pas même un petit fichier.
        assert!(matches!(
            judge(1, 4 * GIB, 100 * GIB),
            SpaceVerdict::Refused { .. }
        ));
    }

    #[test]
    fn the_copy_stops_under_half_the_reserve() {
        assert!(!too_low_during_copy(3 * GIB, 100 * GIB));
        assert!(too_low_during_copy(2 * GIB, 100 * GIB));
    }
}
