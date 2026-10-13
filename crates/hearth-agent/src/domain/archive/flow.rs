//! Lecture bornée : annulation, empreinte du fichier d'archive, et copie d'un flux dont la taille et la
//! somme de contrôle sont celles qu'annonce l'archive (donc fausses par hypothèse).

use std::io::{Read, Seek, SeekFrom};

use flate2::Crc;
use sha2::{Digest, Sha256};

use super::limits::{ArchiveLimits, BUFFER};
use super::refusal::ArchiveRefusal;

/// Demande d'annulation, consultée à chaque entrée et à chaque tampon.
pub type Cancel<'a> = &'a dyn Fn() -> bool;

pub(super) fn check_cancel(cancel: Cancel<'_>) -> Result<(), ArchiveRefusal> {
    if cancel() {
        Err(ArchiveRefusal::Cancelled)
    } else {
        Ok(())
    }
}

/// `read` qui reprend sur une interruption.
pub(super) fn read_some(reader: &mut dyn Read, buf: &mut [u8]) -> std::io::Result<usize> {
    loop {
        match reader.read(buf) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            other => return other,
        }
    }
}

/// Lit exactement `buf.len()` octets ; une fin de fichier avant est un fichier tronqué.
pub(super) fn read_exact_at<R: Read + Seek>(
    reader: &mut R,
    offset: u64,
    buf: &mut [u8],
) -> Result<(), ArchiveRefusal> {
    reader
        .seek(SeekFrom::Start(offset))
        .map_err(ArchiveRefusal::from_io)?;
    reader.read_exact(buf).map_err(ArchiveRefusal::from_io)
}

/// Taille du fichier d'archive, lue par la position de fin, bornée.
pub(super) fn file_len<R: Seek>(
    reader: &mut R,
    limits: &ArchiveLimits,
) -> Result<u64, ArchiveRefusal> {
    let len = reader
        .seek(SeekFrom::End(0))
        .map_err(ArchiveRefusal::from_io)?;
    if len > limits.max_file_size {
        return Err(ArchiveRefusal::FileTooLarge {
            max: limits.max_file_size,
        });
    }
    Ok(len)
}

/// SHA-256 du fichier d'archive et sa longueur, lus d'un bout à l'autre par tampons de 64 Kio.
pub(super) fn digest<R: Read + Seek>(
    reader: &mut R,
    limits: &ArchiveLimits,
    cancel: Cancel<'_>,
) -> Result<([u8; 32], u64), ArchiveRefusal> {
    let len = file_len(reader, limits)?;
    reader
        .seek(SeekFrom::Start(0))
        .map_err(ArchiveRefusal::from_io)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; BUFFER];
    let mut total = 0u64;
    loop {
        check_cancel(cancel)?;
        let n = read_some(reader, &mut buf).map_err(ArchiveRefusal::from_io)?;
        if n == 0 {
            break;
        }
        total = total.saturating_add(n as u64);
        if total > limits.max_file_size {
            return Err(ArchiveRefusal::ArchiveChanged);
        }
        hasher.update(&buf[..n]);
    }
    if total != len {
        return Err(ArchiveRefusal::ArchiveChanged);
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&hasher.finalize());
    Ok((out, len))
}

/// Copie `reader` vers `emit`. L'entrée doit faire EXACTEMENT `size` octets et, si `crc` est donné,
/// avoir cette somme de contrôle. Un octet de plus que `size` n'est jamais remis à `emit` : on lit au
/// plus un octet d'excès pour le constater, puis on s'arrête.
pub(super) fn pump(
    reader: &mut dyn Read,
    size: u64,
    crc: Option<u32>,
    cancel: Cancel<'_>,
    emit: &mut dyn FnMut(&[u8]) -> Result<(), ArchiveRefusal>,
) -> Result<(), ArchiveRefusal> {
    let mut buf = vec![0u8; BUFFER];
    let mut remaining = size;
    let mut sum = Crc::new();
    loop {
        check_cancel(cancel)?;
        let want = remaining.saturating_add(1).min(BUFFER as u64) as usize;
        let n = read_some(reader, &mut buf[..want]).map_err(ArchiveRefusal::from_io)?;
        if n == 0 {
            break;
        }
        let allowed = (n as u64).min(remaining) as usize;
        if allowed > 0 {
            sum.update(&buf[..allowed]);
            emit(&buf[..allowed])?;
        }
        if (n as u64) > remaining {
            return Err(ArchiveRefusal::SizeMismatch);
        }
        remaining -= n as u64;
    }
    if remaining != 0 {
        return Err(ArchiveRefusal::SizeMismatch);
    }
    if let Some(expected) = crc
        && sum.sum() != expected
    {
        return Err(ArchiveRefusal::ChecksumMismatch);
    }
    Ok(())
}
