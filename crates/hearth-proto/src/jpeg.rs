//! Lecteur d'en-tête JPEG, PUR et sans panique (HRT-238, ADR-0061, BR-FICHE-062) : la signature et les dimensions d'une
//! illustration embarquée, lues dans le premier marqueur « début de trame » (SOF). Aucune décompression, aucune
//! dépendance, aucune E/S : le chargeur de fiches et `build.rs` (qui l'inclut par `#[path]`, d'où l'absence de `super::`)
//! s'en servent pour refuser à la COMPILATION une image hors règle ; la coquille du client s'en sert pour vérifier ce
//! qu'elle sert et garde. Une seule source, ici (`hearth-proto` : aucune E/S, aucune dépendance).

/// Taille maximale d'une illustration embarquée : 300 Kio.
pub const MAX_ILLUSTRATION_BYTES: usize = 300 * 1024;
/// Dimensions exigées d'une illustration embarquée : le double de l'en-tête Steam (460 sur 215), même rapport.
pub const ILLUSTRATION_WIDTH: u16 = 920;
pub const ILLUSTRATION_HEIGHT: u16 = 430;

/// Pourquoi une illustration est refusée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IllustrationRefusal {
    /// Les trois premiers octets ne sont pas `FF D8 FF`.
    NotJpeg,
    /// Plus de 300 Kio.
    TooLarge,
    /// L'en-tête n'a pas de marqueur de trame lisible, ou il est tronqué.
    Unreadable,
    /// Les dimensions ne sont pas 920 sur 430.
    Dimensions,
}

impl IllustrationRefusal {
    /// Le motif court, repris par les messages de refus.
    pub fn motif(self) -> &'static str {
        match self {
            Self::NotJpeg => "illustration_not_jpeg",
            Self::TooLarge => "illustration_size",
            Self::Unreadable => "illustration_not_jpeg",
            Self::Dimensions => "illustration_dimensions",
        }
    }
}

/// Les trois octets qui ouvrent tout JPEG.
pub fn has_jpeg_signature(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xFF, 0xD8, 0xFF])
}

/// Largeur et hauteur lues dans le premier marqueur SOF, ou `None` si le fichier n'est pas un JPEG lisible. Jamais de
/// panique : toute lecture est bornée par la longueur de la donnée, une longueur de segment fausse arrête la lecture.
pub fn jpeg_dimensions(bytes: &[u8]) -> Option<(u16, u16)> {
    if !has_jpeg_signature(bytes) {
        return None;
    }
    let mut at = 2usize;
    loop {
        // Cherche le prochain marqueur : des octets de remplissage 0xFF peuvent le précéder.
        if *bytes.get(at)? != 0xFF {
            return None;
        }
        while *bytes.get(at)? == 0xFF {
            at += 1;
        }
        let marker = *bytes.get(at)?;
        at += 1;
        match marker {
            // Marqueurs sans longueur : l'image n'a pas encore de trame, on continue.
            0x00 | 0x01 | 0xD0..=0xD7 | 0xD8 => continue,
            // Fin d'image ou début de données avant toute trame : rien à lire.
            0xD9 | 0xDA => return None,
            _ => {}
        }
        let length = usize::from(u16::from_be_bytes([*bytes.get(at)?, *bytes.get(at + 1)?]));
        if length < 2 {
            return None;
        }
        // SOF0 à SOF15 sauf DHT (C4), JPG (C8) et DAC (CC).
        if matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            let height = u16::from_be_bytes([*bytes.get(at + 3)?, *bytes.get(at + 4)?]);
            let width = u16::from_be_bytes([*bytes.get(at + 5)?, *bytes.get(at + 6)?]);
            return Some((width, height));
        }
        at = at.checked_add(length)?;
    }
}

/// Juge une illustration embarquée : signature, taille, dimensions (BR-FICHE-062).
pub fn check_illustration(bytes: &[u8]) -> Result<(), IllustrationRefusal> {
    if !has_jpeg_signature(bytes) {
        return Err(IllustrationRefusal::NotJpeg);
    }
    if bytes.len() > MAX_ILLUSTRATION_BYTES {
        return Err(IllustrationRefusal::TooLarge);
    }
    match jpeg_dimensions(bytes) {
        None => Err(IllustrationRefusal::Unreadable),
        Some((ILLUSTRATION_WIDTH, ILLUSTRATION_HEIGHT)) => Ok(()),
        Some(_) => Err(IllustrationRefusal::Dimensions),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un JPEG minimal : signature, un segment APP0, puis le marqueur de trame demandé.
    fn jpeg(sof: u8, width: u16, height: u16) -> Vec<u8> {
        let mut out = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00];
        out.extend_from_slice(&[0xFF, sof, 0x00, 0x0B, 0x08]);
        out.extend_from_slice(&height.to_be_bytes());
        out.extend_from_slice(&width.to_be_bytes());
        out.extend_from_slice(&[0x01, 0x01, 0x11, 0x00]);
        out
    }

    #[test]
    fn reads_the_dimensions_of_baseline_and_progressive_frames() {
        assert_eq!(jpeg_dimensions(&jpeg(0xC0, 920, 430)), Some((920, 430)));
        assert_eq!(jpeg_dimensions(&jpeg(0xC2, 460, 215)), Some((460, 215)));
    }

    #[test]
    fn skips_markers_before_the_frame_and_fill_bytes() {
        let mut data = vec![0xFF, 0xD8, 0xFF, 0xFF, 0xFE, 0x00, 0x05, 1, 2, 3];
        data.extend_from_slice(&[0xFF, 0xFF, 0xC4, 0x00, 0x03, 0x00]);
        data.extend_from_slice(&[
            0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x01, 0xAE, 0x03, 0x98, 0x01, 0x01, 0x11, 0x00,
        ]);
        assert_eq!(jpeg_dimensions(&data), Some((920, 430)));
    }

    #[test]
    fn refuses_what_is_not_a_readable_jpeg_without_panicking() {
        assert_eq!(jpeg_dimensions(b""), None);
        assert_eq!(jpeg_dimensions(b"\x89PNG\r\n\x1a\n"), None);
        // Tronqué dans l'en-tête de trame.
        let whole = jpeg(0xC0, 920, 430);
        for cut in 0..whole.len() {
            let _ = jpeg_dimensions(&whole[..cut]);
        }
        assert_eq!(jpeg_dimensions(&whole[..12]), None);
        // Longueur de segment fausse (0, 1) et débordante.
        assert_eq!(jpeg_dimensions(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x00]), None);
        assert_eq!(
            jpeg_dimensions(&[0xFF, 0xD8, 0xFF, 0xE0, 0xFF, 0xFF, 0, 0]),
            None
        );
        // Début de données avant toute trame.
        assert_eq!(jpeg_dimensions(&[0xFF, 0xD8, 0xFF, 0xDA, 0x00, 0x02]), None);
    }

    #[test]
    fn never_panics_on_pseudo_random_bytes() {
        // Graine fixe : suite déterministe (xorshift).
        let mut state: u64 = 0x2545_F491_4F6C_DD1D;
        for _ in 0..2000 {
            let mut data = vec![0xFF, 0xD8, 0xFF];
            for _ in 0..(state % 96) {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                data.push(state.to_le_bytes()[0]);
            }
            let _ = jpeg_dimensions(&data);
            let _ = check_illustration(&data);
        }
    }

    #[test]
    fn judges_an_illustration_by_signature_size_and_dimensions() {
        assert_eq!(check_illustration(&jpeg(0xC0, 920, 430)), Ok(()));
        assert_eq!(
            check_illustration(&jpeg(0xC0, 919, 430)),
            Err(IllustrationRefusal::Dimensions)
        );
        assert_eq!(
            check_illustration(b"\x89PNG\r\n\x1a\n...."),
            Err(IllustrationRefusal::NotJpeg)
        );
        let mut big = jpeg(0xC0, 920, 430);
        big.resize(MAX_ILLUSTRATION_BYTES + 1, 0);
        assert_eq!(check_illustration(&big), Err(IllustrationRefusal::TooLarge));
        big.truncate(MAX_ILLUSTRATION_BYTES);
        assert_eq!(check_illustration(&big), Ok(()));
        assert_eq!(
            check_illustration(&[0xFF, 0xD8, 0xFF]),
            Err(IllustrationRefusal::Unreadable)
        );
    }
}
