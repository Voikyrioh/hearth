//! Lecture d'un `.tar` écrite à la main (ADR-0055 point 10) : formats `ustar` et `pax`, noms longs GNU.
//! Un tar n'a pas d'index : on le parcourt en flux, en-tête par en-tête, sans jamais allouer selon une
//! taille lue (les blocs d'en-têtes étendus sont bornés avant d'être lus).

use std::io::Read;

use super::flow::{Cancel, check_cancel, pump, read_some};
use super::limits::{ArchiveLimits, BUFFER};
use super::refusal::ArchiveRefusal;

const BLOCK: usize = 512;
/// Un bloc pax (attributs étendus) : au plus 64 Kio.
const MAX_PAX: u64 = 64 * 1024;

pub(super) fn looks_like_tar(head: &[u8]) -> bool {
    head.get(257..262) == Some(b"ustar".as_slice())
}

/// Une entrée annoncée par l'en-tête, avant jugement du nom.
pub(super) struct TarItem {
    pub raw_name: Vec<u8>,
    pub is_dir: bool,
    pub size: u64,
}

pub(super) struct TarWalker<'a, R: Read> {
    reader: R,
    limits: &'a ArchiveLimits,
    headers: usize,
    name_bytes: u64,
    total: u64,
    /// Octets de données et de remplissage de l'entrée rendue, pas encore consommés.
    pending: u64,
}

fn is_zero(block: &[u8]) -> bool {
    block.iter().all(|&b| b == 0)
}

/// Champ texte terminé par un octet nul ; tout ce qui suit le premier nul doit être nul (sinon deux
/// lecteurs verraient deux noms).
fn c_string(field: &[u8]) -> Result<&[u8], ArchiveRefusal> {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    if field[end..].iter().any(|&b| b != 0) {
        return Err(ArchiveRefusal::BadHeader);
    }
    Ok(&field[..end])
}

/// Nombre d'un en-tête : octal (chiffres seuls, espaces et nuls de bord) ou base 256 (bit de poids
/// fort levé). Un signe, un chiffre hors octal, un débordement : en-tête invalide.
fn parse_number(field: &[u8]) -> Result<u64, ArchiveRefusal> {
    let Some(&first) = field.first() else {
        return Err(ArchiveRefusal::BadHeader);
    };
    if first & 0x80 != 0 {
        if first & 0x40 != 0 {
            return Err(ArchiveRefusal::BadHeader);
        }
        let mut value: u64 = u64::from(first & 0x3F);
        for &byte in &field[1..] {
            value = value
                .checked_mul(256)
                .and_then(|v| v.checked_add(u64::from(byte)))
                .ok_or(ArchiveRefusal::BadHeader)?;
        }
        return Ok(value);
    }
    let mut rest = field.iter().copied().skip_while(|&b| b == b' ');
    let text: Vec<u8> = rest.by_ref().take_while(|&b| b != b' ' && b != 0).collect();
    if rest.any(|b| b != b' ' && b != 0) || text.is_empty() {
        return Err(ArchiveRefusal::BadHeader);
    }
    let mut value: u64 = 0;
    for byte in text {
        if !(b'0'..=b'7').contains(&byte) {
            return Err(ArchiveRefusal::BadHeader);
        }
        value = value
            .checked_mul(8)
            .and_then(|v| v.checked_add(u64::from(byte - b'0')))
            .ok_or(ArchiveRefusal::BadHeader)?;
    }
    Ok(value)
}

fn checksum_ok(block: &[u8; BLOCK]) -> Result<(), ArchiveRefusal> {
    let expected = parse_number(&block[148..156])?;
    let mut sum: u64 = 0;
    for (index, &byte) in block.iter().enumerate() {
        sum += if (148..156).contains(&index) {
            u64::from(b' ')
        } else {
            u64::from(byte)
        };
    }
    if sum == expected {
        Ok(())
    } else {
        Err(ArchiveRefusal::BadHeader)
    }
}

struct Pax {
    path: Option<Vec<u8>>,
    size: Option<u64>,
}

/// Enregistrements pax `LONGUEUR clé=valeur\n`. Seuls `path` et `size` sont lus ; un attribut qui
/// change la façon de lire l'archive (creux, jeu de caractères) est refusé ; le reste est ignoré.
fn parse_pax(data: &[u8], limits: &ArchiveLimits) -> Result<Pax, ArchiveRefusal> {
    let mut pax = Pax {
        path: None,
        size: None,
    };
    let mut rest = data;
    while !rest.is_empty() {
        let digits = rest.iter().take_while(|b| b.is_ascii_digit()).count();
        if digits == 0 || digits > 7 || rest.get(digits) != Some(&b' ') {
            return Err(ArchiveRefusal::BadHeader);
        }
        let mut length = 0usize;
        for &d in &rest[..digits] {
            length = length * 10 + usize::from(d - b'0');
        }
        if length <= digits + 1 || length > rest.len() || rest[length - 1] != b'\n' {
            return Err(ArchiveRefusal::BadHeader);
        }
        let body = &rest[digits + 1..length - 1];
        rest = &rest[length..];
        let Some(eq) = body.iter().position(|&b| b == b'=') else {
            return Err(ArchiveRefusal::BadHeader);
        };
        let (key, value) = (&body[..eq], &body[eq + 1..]);
        match key {
            b"path" => {
                if pax.path.is_some() {
                    return Err(ArchiveRefusal::BadHeader);
                }
                if value.len() > limits.max_name_bytes {
                    return Err(ArchiveRefusal::NameTooLong {
                        max: limits.max_name_bytes,
                    });
                }
                pax.path = Some(value.to_vec());
            }
            b"size" => {
                if pax.size.is_some() || value.is_empty() || value.len() > 20 {
                    return Err(ArchiveRefusal::BadHeader);
                }
                let mut size: u64 = 0;
                for &d in value {
                    if !d.is_ascii_digit() {
                        return Err(ArchiveRefusal::BadHeader);
                    }
                    size = size
                        .checked_mul(10)
                        .and_then(|v| v.checked_add(u64::from(d - b'0')))
                        .ok_or(ArchiveRefusal::BadHeader)?;
                }
                pax.size = Some(size);
            }
            b"hdrcharset" => return Err(ArchiveRefusal::UnsupportedFeature),
            other if other.starts_with(b"GNU.sparse") => {
                return Err(ArchiveRefusal::UnsupportedFeature);
            }
            _ => {}
        }
    }
    Ok(pax)
}

impl<'a, R: Read> TarWalker<'a, R> {
    pub(super) fn new(reader: R, limits: &'a ArchiveLimits) -> Self {
        Self {
            reader,
            limits,
            headers: 0,
            name_bytes: 0,
            total: 0,
            pending: 0,
        }
    }

    fn read_block(&mut self) -> Result<[u8; BLOCK], ArchiveRefusal> {
        let mut block = [0u8; BLOCK];
        self.reader
            .read_exact(&mut block)
            .map_err(ArchiveRefusal::from_io)?;
        Ok(block)
    }

    fn discard(&mut self, mut n: u64, cancel: Cancel<'_>) -> Result<(), ArchiveRefusal> {
        let mut scratch = vec![0u8; BUFFER];
        while n > 0 {
            check_cancel(cancel)?;
            let want = n.min(BUFFER as u64) as usize;
            let got = read_some(&mut self.reader, &mut scratch[..want])
                .map_err(ArchiveRefusal::from_io)?;
            if got == 0 {
                return Err(ArchiveRefusal::Truncated);
            }
            n -= got as u64;
        }
        Ok(())
    }

    /// Lit un bloc de données d'en-tête étendu (déjà borné) et son remplissage.
    fn read_extended(&mut self, size: u64, cancel: Cancel<'_>) -> Result<Vec<u8>, ArchiveRefusal> {
        let mut data = vec![0u8; size as usize];
        self.reader
            .read_exact(&mut data)
            .map_err(ArchiveRefusal::from_io)?;
        self.discard(padding(size), cancel)?;
        Ok(data)
    }

    /// L'entrée suivante, ou `None` après les deux blocs nuls de fin (et eux seuls).
    pub(super) fn next_item(
        &mut self,
        cancel: Cancel<'_>,
    ) -> Result<Option<TarItem>, ArchiveRefusal> {
        let pending = std::mem::take(&mut self.pending);
        self.discard(pending, cancel)?;
        let mut long_name: Option<Vec<u8>> = None;
        let mut pax: Option<Pax> = None;
        loop {
            check_cancel(cancel)?;
            self.headers += 1;
            if self.headers > self.limits.max_entries.saturating_mul(2).saturating_add(2) {
                return Err(ArchiveRefusal::TooManyEntries {
                    max: self.limits.max_entries,
                });
            }
            let block = self.read_block()?;
            if is_zero(&block) {
                if !is_zero(&self.read_block()?) || long_name.is_some() || pax.is_some() {
                    return Err(ArchiveRefusal::BadHeader);
                }
                return Ok(None);
            }
            checksum_ok(&block)?;
            if block.get(257..262) != Some(b"ustar".as_slice()) {
                return Err(ArchiveRefusal::BadHeader);
            }
            let declared = parse_number(&block[124..136])?;
            let flag = block[156];
            match flag {
                b'x' | b'L' => {
                    let cap = if flag == b'x' {
                        MAX_PAX
                    } else {
                        self.limits.max_name_bytes as u64 + 1
                    };
                    if declared > cap {
                        return Err(if flag == b'x' {
                            ArchiveRefusal::BadHeader
                        } else {
                            ArchiveRefusal::NameTooLong {
                                max: self.limits.max_name_bytes,
                            }
                        });
                    }
                    let data = self.read_extended(declared, cancel)?;
                    if flag == b'x' {
                        if pax.is_some() {
                            return Err(ArchiveRefusal::BadHeader);
                        }
                        pax = Some(parse_pax(&data, self.limits)?);
                    } else {
                        if long_name.is_some() {
                            return Err(ArchiveRefusal::BadHeader);
                        }
                        // Le nom long GNU finit par un nul ; un autre nul est un nom piégé.
                        let name = data.strip_suffix(&[0]).unwrap_or(&data);
                        long_name = Some(name.to_vec());
                    }
                    continue;
                }
                b'0' | 0 => {}
                b'5' => {}
                b'1' => return Err(ArchiveRefusal::HardLink),
                b'2' => return Err(ArchiveRefusal::SymbolicLink),
                b'3' | b'4' | b'6' => return Err(ArchiveRefusal::SpecialFile),
                _ => return Err(ArchiveRefusal::UnsupportedFeature),
            }
            let is_dir = flag == b'5';
            if long_name.is_some() && pax.as_ref().is_some_and(|p| p.path.is_some()) {
                return Err(ArchiveRefusal::BadHeader);
            }
            let raw_name = match (long_name.take(), pax.as_mut().and_then(|p| p.path.take())) {
                (Some(name), None) | (None, Some(name)) => name,
                (None, None) => {
                    let base = c_string(&block[0..100])?;
                    let prefix = c_string(&block[345..500])?;
                    if prefix.is_empty() {
                        base.to_vec()
                    } else {
                        let mut joined = prefix.to_vec();
                        joined.push(b'/');
                        joined.extend_from_slice(base);
                        joined
                    }
                }
                (Some(_), Some(_)) => return Err(ArchiveRefusal::BadHeader),
            };
            let size = pax.as_ref().and_then(|p| p.size).unwrap_or(declared);
            if is_dir && size != 0 {
                return Err(ArchiveRefusal::BadHeader);
            }
            if raw_name.len() > self.limits.max_name_bytes {
                return Err(ArchiveRefusal::NameTooLong {
                    max: self.limits.max_name_bytes,
                });
            }
            self.name_bytes = self.name_bytes.saturating_add(raw_name.len() as u64);
            if self.name_bytes > self.limits.max_index_bytes {
                return Err(ArchiveRefusal::IndexTooLarge {
                    max: self.limits.max_index_bytes,
                });
            }
            self.total = self.total.saturating_add(size);
            if self.total > self.limits.max_unpacked {
                return Err(ArchiveRefusal::TotalSizeTooLarge {
                    max: self.limits.max_unpacked,
                });
            }
            self.pending = size.saturating_add(padding(size));
            return Ok(Some(TarItem {
                raw_name,
                is_dir,
                size,
            }));
        }
    }

    /// Passe les données de l'entrée courante sans les garder.
    pub(super) fn skip_data(&mut self, cancel: Cancel<'_>) -> Result<(), ArchiveRefusal> {
        let pending = std::mem::take(&mut self.pending);
        self.discard(pending, cancel)
    }

    /// Remet exactement `size` octets de l'entrée courante à `emit`, puis passe le remplissage.
    pub(super) fn read_data(
        &mut self,
        size: u64,
        cancel: Cancel<'_>,
        emit: &mut dyn FnMut(&[u8]) -> Result<(), ArchiveRefusal>,
    ) -> Result<(), ArchiveRefusal> {
        let padded = std::mem::take(&mut self.pending);
        let mut bounded = (&mut self.reader).take(size);
        pump(&mut bounded, size, None, cancel, emit).map_err(|refusal| match refusal {
            // Moins d'octets que l'en-tête n'en annonce : le fichier est tronqué.
            ArchiveRefusal::SizeMismatch => ArchiveRefusal::Truncated,
            other => other,
        })?;
        self.discard(padded.saturating_sub(size), cancel)
    }

    /// Après la fin : tout ce qui reste doit être nul jusqu'à la fin du flux.
    pub(super) fn finish(&mut self, cancel: Cancel<'_>) -> Result<(), ArchiveRefusal> {
        let mut scratch = vec![0u8; BUFFER];
        loop {
            check_cancel(cancel)?;
            let got = read_some(&mut self.reader, &mut scratch).map_err(ArchiveRefusal::from_io)?;
            if got == 0 {
                return Ok(());
            }
            if !is_zero(&scratch[..got]) {
                return Err(ArchiveRefusal::UncoveredBytes);
            }
        }
    }
}

fn padding(size: u64) -> u64 {
    (BLOCK as u64 - size % BLOCK as u64) % BLOCK as u64
}
