//! Images de l'icône de la zone de notification (BR-RESIL-016, HRT-19) : une par état et par
//! taille, générées depuis `icons/source/tray/` par `npm run build:icons`. Aucun dessin à
//! l'exécution : l'âtre ne change pas, son contenu dit l'état (flamme, contour, barre, point
//! d'exclamation, croix).

use crate::presence::TrayIcon;

/// Tailles fournies (pixels), de l'écran à 100 % à celui à 200 %.
pub const SIZES: [u32; 4] = [16, 20, 24, 32];

/// La plus proche des tailles fournies pour un facteur d'échelle d'écran (1,0 donne 16 px,
/// 1,25 donne 20, 1,5 donne 24, 2,0 et plus donnent 32). Facteur absurde : 16.
pub fn pick_size(scale: f64) -> u32 {
    if !scale.is_finite() || scale <= 0.0 {
        return SIZES[0];
    }
    let wanted = 16.0 * scale;
    SIZES
        .into_iter()
        .min_by(|a, b| {
            (f64::from(*a) - wanted)
                .abs()
                .total_cmp(&(f64::from(*b) - wanted).abs())
        })
        .unwrap_or(SIZES[0])
}

macro_rules! tray_pngs {
    ($state:literal) => {
        [
            include_bytes!(concat!("../icons/tray/tray-", $state, "-16.png")).as_slice(),
            include_bytes!(concat!("../icons/tray/tray-", $state, "-20.png")).as_slice(),
            include_bytes!(concat!("../icons/tray/tray-", $state, "-24.png")).as_slice(),
            include_bytes!(concat!("../icons/tray/tray-", $state, "-32.png")).as_slice(),
        ]
    };
}

/// Le PNG de l'état pour la taille choisie (`SIZES`) ; une taille inconnue donne la plus petite.
pub fn png(icon: TrayIcon, size: u32) -> &'static [u8] {
    let set: [&'static [u8]; 4] = match icon {
        TrayIcon::NoServer => tray_pngs!("aucun-serveur"),
        TrayIcon::Connected => tray_pngs!("connecte"),
        TrayIcon::Reconnecting => tray_pngs!("reconnexion"),
        TrayIcon::Offline => tray_pngs!("hors-ligne"),
        TrayIcon::SessionExpired => tray_pngs!("session-expiree"),
        TrayIcon::AccessRevoked => tray_pngs!("acces-revoque"),
    };
    let at = SIZES.iter().position(|s| *s == size).unwrap_or(0);
    set.get(at).copied().unwrap_or(set[0])
}
