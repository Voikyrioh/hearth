//! Pastille de couleur posée sur l'icône de la zone de notification (BR-RESIL-016). Du dessin de
//! pixels, pas une règle : le choix de l'état est dans `presence.rs`. Les couleurs sont celles des
//! jetons `--ok`, `--warn`, `--crit` et `--bg` de `src/styles/tokens.css` ; un test relit ce fichier
//! et casse si un jeton change sans que ces valeurs suivent.

use crate::presence::TrayStatus;

/// Jeton `--ok`.
pub const COLOR_OK: [u8; 3] = [0x7e, 0xd3, 0x9a];
/// Jeton `--warn`.
pub const COLOR_WARN: [u8; 3] = [0xff, 0xc0, 0x4d];
/// Jeton `--crit`.
pub const COLOR_CRIT: [u8; 3] = [0xff, 0x5a, 0x5a];
/// Jeton `--bg` : liseré qui détache la pastille de la flamme.
pub const COLOR_RING: [u8; 3] = [0x1c, 0x15, 0x18];

/// Couleur de la pastille d'un état ; `None` : pas de pastille.
pub fn color(status: TrayStatus) -> Option<[u8; 3]> {
    match status {
        TrayStatus::Idle => None,
        TrayStatus::Connected => Some(COLOR_OK),
        TrayStatus::Warning => Some(COLOR_WARN),
        TrayStatus::Critical => Some(COLOR_CRIT),
    }
}

/// Pose une pastille pleine de la couleur de `status` dans le coin bas droit d'une image RGBA
/// (`width * height * 4` octets). Rend l'image inchangée si `status` n'a pas de pastille ou si la
/// taille ne correspond pas. Un fin liseré sombre la détache de la flamme.
pub fn paint_badge(rgba: &[u8], width: u32, height: u32, status: TrayStatus) -> Vec<u8> {
    let mut out = rgba.to_vec();
    let Some(color) = color(status) else {
        return out;
    };
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || out.len() != w * h * 4 {
        return out;
    }
    // Rayon : un peu plus d'un quart de la largeur, centre au coin bas droit moins le rayon.
    let radius = (w.min(h) as f32) * 0.26;
    let cx = w as f32 - radius - 0.5;
    let cy = h as f32 - radius - 0.5;
    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let pixel = (y * w + x) * 4;
            if dist <= radius - 1.5 {
                out[pixel..pixel + 4].copy_from_slice(&[color[0], color[1], color[2], 0xff]);
            } else if dist <= radius {
                out[pixel..pixel + 4].copy_from_slice(&[
                    COLOR_RING[0],
                    COLOR_RING[1],
                    COLOR_RING[2],
                    0xff,
                ]);
            }
        }
    }
    out
}
