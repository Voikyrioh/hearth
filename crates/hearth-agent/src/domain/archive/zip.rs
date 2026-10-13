//! Lecture d'un `.zip` écrite à la main (ADR-0055 point 5 et 10) : l'index de fin est confronté, entrée
//! par entrée, à l'en-tête local qui précède ses données, et les entrées doivent paver le fichier SANS
//! trou ni chevauchement depuis l'octet 0 jusqu'à l'index. Aucune fonction d'extraction toute faite.
//!
//! Prises en charge : `stored` et `deflate`, ZIP64 dans les bornes, descripteurs de données. Refusés :
//! chiffrement, plusieurs volumes, autres méthodes, deux enregistrements de fin, octets non couverts.

use std::io::{BufReader, Read, Seek, SeekFrom};

use flate2::read::DeflateDecoder;

use super::flow::{Cancel, check_cancel, pump, read_exact_at};
use super::limits::{ArchiveLimits, ZIP_INDEX_RECORD_MIN};
use super::path::{NameRegistry, judge};
use super::refusal::ArchiveRefusal;
use super::{EntryInfo, EntryKind};

const SIG_CENTRAL: u32 = 0x0201_4b50;
const SIG_LOCAL: u32 = 0x0403_4b50;
const SIG_END: u32 = 0x0605_4b50;
const SIG_ZIP64_END: u32 = 0x0606_4b50;
const SIG_ZIP64_LOCATOR: u32 = 0x0707_4b50;
const SIG_DESCRIPTOR: u32 = 0x0807_4b50;
const END_RECORD_LEN: u64 = 22;
const LOCATOR_LEN: u64 = 20;
const ZIP64_END_LEN: u64 = 56;
const LOCAL_HEADER_LEN: u64 = 30;
const MAX_COMMENT: u64 = 65_535;
/// Drapeaux comparés entre l'index et l'en-tête local : chiffrement (0, 6, 13), descripteur (3), UTF-8 (11).
const FLAGS_COMPARED: u16 = 0x2849;
const FLAG_ENCRYPTED: u16 = 0x0001;
const FLAG_DESCRIPTOR: u16 = 0x0008;
const FLAG_STRONG_ENCRYPTION: u16 = 0x0040;
const FLAG_MASKED_HEADERS: u16 = 0x2000;
const METHOD_STORED: u16 = 0;
const METHOD_DEFLATE: u16 = 8;
const METHOD_AES: u16 = 99;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Method {
    Stored,
    Deflate,
}

/// Ce que l'index a promis d'une entrée, une fois confronté à l'en-tête local.
#[derive(Debug, Clone)]
pub(super) struct ZipPlan {
    pub local_offset: u64,
    pub data_offset: u64,
    pub method: Method,
    pub compressed: u64,
    pub size: u64,
    pub crc: u32,
    pub has_descriptor: bool,
    pub flags: u16,
    pub is_dir: bool,
}

struct Cur<'a>(&'a [u8]);

impl<'a> Cur<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ArchiveRefusal> {
        if n > self.0.len() {
            return Err(ArchiveRefusal::IndexLies);
        }
        let (head, tail) = self.0.split_at(n);
        self.0 = tail;
        Ok(head)
    }
    fn u16(&mut self) -> Result<u16, ArchiveRefusal> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Result<u32, ArchiveRefusal> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn u64(&mut self) -> Result<u64, ArchiveRefusal> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(u64::from_le_bytes(a))
    }
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

struct EndRecord {
    entries: u64,
    cd_size: u64,
    cd_offset: u64,
    /// Début de l'index de fin (l'enregistrement ZIP64 s'il existe, sinon l'enregistrement de fin).
    index_end: u64,
}

fn find_end_record<R: Read + Seek>(r: &mut R, file_len: u64) -> Result<EndRecord, ArchiveRefusal> {
    if file_len < END_RECORD_LEN {
        return Err(ArchiveRefusal::UnrecognizedFormat);
    }
    let window = file_len.min(END_RECORD_LEN + MAX_COMMENT);
    let base = file_len - window;
    let mut tail = vec![0u8; window as usize];
    read_exact_at(r, base, &mut tail)?;
    let sig = SIG_END.to_le_bytes();
    // Toutes les occurrences de la signature dans la fenêtre de fin, de la dernière à la première.
    let found: Vec<usize> = (0..=tail.len() - END_RECORD_LEN as usize)
        .rev()
        .filter(|&pos| tail[pos..pos + 4] == sig)
        .collect();
    if found.is_empty() {
        return Err(ArchiveRefusal::UnrecognizedFormat);
    }
    // L'enregistrement de fin est le dernier dont le commentaire va exactement jusqu'au bout du fichier.
    let comment_at = |pos: usize| -> Option<u64> {
        let raw = tail.get(pos + 20..pos + 22)?;
        let len = u64::from(u16::from_le_bytes([raw[0], raw[1]]));
        (base + pos as u64 + END_RECORD_LEN + len == file_len).then_some(len)
    };
    let Some(&pos) = found.iter().find(|&&pos| comment_at(pos).is_some()) else {
        return Err(ArchiveRefusal::IndexLies);
    };
    let mut cur = Cur(&tail[pos + 4..pos + END_RECORD_LEN as usize]);
    let disk = cur.u16()?;
    let cd_disk = cur.u16()?;
    let entries_disk = cur.u16()?;
    let entries_total = cur.u16()?;
    let cd_size = cur.u32()?;
    let cd_offset = cur.u32()?;
    let _comment_len = cur.u16()?;
    let end_abs = base + pos as u64;
    if disk != 0 || cd_disk != 0 || entries_disk != entries_total {
        return Err(ArchiveRefusal::MultiVolume);
    }
    // Une autre signature de fin à partir du début de l'index (après la dernière entrée), dans un
    // commentaire ou entre l'index et la fin : deux lecteurs verraient deux archives. Une signature
    // avant l'index est dans les données d'une entrée (une archive stockée dans l'archive) : sans objet.
    let ambiguous = |index_start: u64| {
        found
            .iter()
            .any(|&other| other != pos && base + other as u64 >= index_start)
    };
    let needs_zip64 = entries_total == 0xFFFF || cd_size == u32::MAX || cd_offset == u32::MAX;
    if !needs_zip64 {
        if ambiguous(u64::from(cd_offset)) {
            return Err(ArchiveRefusal::TwoEndRecords);
        }
        return Ok(EndRecord {
            entries: u64::from(entries_total),
            cd_size: u64::from(cd_size),
            cd_offset: u64::from(cd_offset),
            index_end: end_abs,
        });
    }
    if end_abs < LOCATOR_LEN {
        return Err(ArchiveRefusal::IndexLies);
    }
    let locator_at = end_abs - LOCATOR_LEN;
    let mut raw = [0u8; LOCATOR_LEN as usize];
    read_exact_at(r, locator_at, &mut raw)?;
    let mut cur = Cur(&raw);
    if cur.u32()? != SIG_ZIP64_LOCATOR {
        return Err(ArchiveRefusal::IndexLies);
    }
    let record_disk = cur.u32()?;
    let record_at = cur.u64()?;
    let total_disks = cur.u32()?;
    if record_disk != 0 || total_disks != 1 {
        return Err(ArchiveRefusal::MultiVolume);
    }
    if record_at.checked_add(ZIP64_END_LEN) != Some(locator_at) {
        return Err(ArchiveRefusal::IndexLies);
    }
    let mut raw = [0u8; ZIP64_END_LEN as usize];
    read_exact_at(r, record_at, &mut raw)?;
    let mut cur = Cur(&raw);
    if cur.u32()? != SIG_ZIP64_END {
        return Err(ArchiveRefusal::IndexLies);
    }
    // Taille de l'enregistrement : 44 octets exactement, aucune extension.
    if cur.u64()? != ZIP64_END_LEN - 12 {
        return Err(ArchiveRefusal::IndexLies);
    }
    let _made_by = cur.u16()?;
    let _needed = cur.u16()?;
    let disk = cur.u32()?;
    let cd_disk = cur.u32()?;
    let entries_disk = cur.u64()?;
    let entries_total = cur.u64()?;
    let cd_size = cur.u64()?;
    let cd_offset = cur.u64()?;
    if disk != 0 || cd_disk != 0 || entries_disk != entries_total {
        return Err(ArchiveRefusal::MultiVolume);
    }
    if ambiguous(cd_offset) {
        return Err(ArchiveRefusal::TwoEndRecords);
    }
    Ok(EndRecord {
        entries: entries_total,
        cd_size,
        cd_offset,
        index_end: record_at,
    })
}

/// Champs 64 bits du « extra » ZIP64 (0x0001), dans l'ordre du format, pour ceux que l'enregistrement
/// à 32 bits a laissés à `0xFFFFFFFF`. Un second enregistrement 0x0001 est une ambiguïté : refus.
fn zip64_fields(
    extra: &[u8],
    want_size: bool,
    want_compressed: bool,
    want_offset: bool,
) -> Result<(u64, u64, u64), ArchiveRefusal> {
    let mut cur = Cur(extra);
    let mut data: Option<&[u8]> = None;
    while !cur.is_empty() {
        let id = cur.u16()?;
        let len = usize::from(cur.u16()?);
        let body = cur.take(len)?;
        if id == 0x0001 {
            if data.is_some() {
                return Err(ArchiveRefusal::IndexLies);
            }
            data = Some(body);
        }
    }
    let mut fields = Cur(data.unwrap_or(&[]));
    let mut next =
        |wanted: bool| -> Result<u64, ArchiveRefusal> { if wanted { fields.u64() } else { Ok(0) } };
    let size = next(want_size)?;
    let compressed = next(want_compressed)?;
    let offset = next(want_offset)?;
    Ok((size, compressed, offset))
}

fn long_enough(entries: u64, cd_size: u64) -> bool {
    entries
        .checked_mul(ZIP_INDEX_RECORD_MIN)
        .is_some_and(|needed| needed <= cd_size)
}

/// Inspecte un zip : index de fin, enregistrements, en-têtes locaux, pavage du fichier, noms.
pub(super) fn inspect<R: Read + Seek>(
    r: &mut R,
    file_len: u64,
    limits: &ArchiveLimits,
    cancel: Cancel<'_>,
) -> Result<(Vec<EntryInfo>, Vec<ZipPlan>), ArchiveRefusal> {
    let end = find_end_record(r, file_len)?;
    // Bornes AVANT toute allocation selon un nombre lu.
    if end.cd_size > limits.max_index_bytes {
        return Err(ArchiveRefusal::IndexTooLarge {
            max: limits.max_index_bytes,
        });
    }
    if end.entries > limits.max_entries as u64 {
        return Err(ArchiveRefusal::TooManyEntries {
            max: limits.max_entries,
        });
    }
    if !long_enough(end.entries, end.cd_size) {
        return Err(ArchiveRefusal::IndexLies);
    }
    if end.cd_offset.checked_add(end.cd_size) != Some(end.index_end) {
        return Err(ArchiveRefusal::UncoveredBytes);
    }
    let cd_offset = end.cd_offset;
    // L'index entre en mémoire par ce que le fichier fournit réellement, jamais par sa taille annoncée.
    r.seek(SeekFrom::Start(cd_offset))
        .map_err(ArchiveRefusal::from_io)?;
    let mut index = Vec::new();
    r.by_ref()
        .take(end.cd_size)
        .read_to_end(&mut index)
        .map_err(ArchiveRefusal::from_io)?;
    if index.len() as u64 != end.cd_size {
        return Err(ArchiveRefusal::Truncated);
    }

    let mut cur = Cur(&index);
    let mut registry = NameRegistry::new(limits.max_tree_nodes);
    let mut entries: Vec<EntryInfo> = Vec::new();
    let mut plans: Vec<ZipPlan> = Vec::new();
    let mut total: u64 = 0;
    for _ in 0..end.entries {
        check_cancel(cancel)?;
        if cur.u32()? != SIG_CENTRAL {
            return Err(ArchiveRefusal::IndexLies);
        }
        let made_by = cur.u16()?;
        let _needed = cur.u16()?;
        let flags = cur.u16()?;
        let method = cur.u16()?;
        let _time = cur.u16()?;
        let _date = cur.u16()?;
        let crc = cur.u32()?;
        let compressed32 = cur.u32()?;
        let size32 = cur.u32()?;
        let name_len = usize::from(cur.u16()?);
        let extra_len = usize::from(cur.u16()?);
        let comment_len = usize::from(cur.u16()?);
        let disk_start = cur.u16()?;
        let _internal = cur.u16()?;
        let external = cur.u32()?;
        let offset32 = cur.u32()?;
        if name_len > limits.max_name_bytes {
            return Err(ArchiveRefusal::NameTooLong {
                max: limits.max_name_bytes,
            });
        }
        let raw_name = cur.take(name_len)?;
        let extra = cur.take(extra_len)?;
        // Le commentaire est ignoré, jamais interprété.
        let _comment = cur.take(comment_len)?;

        if flags & (FLAG_ENCRYPTED | FLAG_STRONG_ENCRYPTION | FLAG_MASKED_HEADERS) != 0
            || method == METHOD_AES
        {
            return Err(ArchiveRefusal::Encrypted);
        }
        let method = match method {
            METHOD_STORED => Method::Stored,
            METHOD_DEFLATE => Method::Deflate,
            _ => return Err(ArchiveRefusal::UnsupportedMethod),
        };
        if disk_start != 0 {
            return Err(ArchiveRefusal::MultiVolume);
        }

        let judged = judge(raw_name, limits)?;
        let is_dir = judged.trailing_slash;

        // Type de l'entrée : jamais autre chose qu'un fichier ordinaire ou un dossier.
        if made_by >> 8 == 3 {
            match (external >> 16) & 0o170_000 {
                0o120_000 => return Err(ArchiveRefusal::SymbolicLink),
                0o100_000 if is_dir => return Err(ArchiveRefusal::IndexLies),
                0o040_000 if !is_dir => return Err(ArchiveRefusal::IndexLies),
                0o100_000 | 0o040_000 | 0 => {}
                _ => return Err(ArchiveRefusal::SpecialFile),
            }
        }
        if external & 0x0400 != 0 {
            // FILE_ATTRIBUTE_REPARSE_POINT : lien ou jonction Windows.
            return Err(ArchiveRefusal::SymbolicLink);
        }
        if made_by >> 8 != 3 && external & 0x10 != 0 && !is_dir {
            return Err(ArchiveRefusal::IndexLies);
        }

        let (size, compressed, offset) = {
            let wants = (
                size32 == u32::MAX,
                compressed32 == u32::MAX,
                offset32 == u32::MAX,
            );
            if wants.0 || wants.1 || wants.2 {
                let (s, c, o) = zip64_fields(extra, wants.0, wants.1, wants.2)?;
                (
                    if wants.0 { s } else { u64::from(size32) },
                    if wants.1 { c } else { u64::from(compressed32) },
                    if wants.2 { o } else { u64::from(offset32) },
                )
            } else {
                (
                    u64::from(size32),
                    u64::from(compressed32),
                    u64::from(offset32),
                )
            }
        };

        if is_dir && (size != 0 || compressed > 2) {
            return Err(ArchiveRefusal::IndexLies);
        }
        if method == Method::Stored && size != compressed {
            return Err(ArchiveRefusal::SizeMismatch);
        }
        if size > limits.ratio_floor && size > limits.max_entry_ratio.saturating_mul(compressed) {
            return Err(ArchiveRefusal::EntryRatioTooHigh {
                ratio: limits.max_entry_ratio,
            });
        }
        total = total.saturating_add(size);
        if total > limits.max_unpacked {
            return Err(ArchiveRefusal::TotalSizeTooLarge {
                max: limits.max_unpacked,
            });
        }
        if offset
            .checked_add(LOCAL_HEADER_LEN)
            .is_none_or(|end_of_header| end_of_header > cd_offset)
        {
            return Err(ArchiveRefusal::EntryOutsideFile);
        }

        registry.add(&judged.path, is_dir)?;
        entries.push(EntryInfo {
            path: judged.path,
            kind: if is_dir {
                EntryKind::Dir
            } else {
                EntryKind::File
            },
            size,
        });
        plans.push(ZipPlan {
            local_offset: offset,
            data_offset: 0,
            method,
            compressed,
            size,
            crc,
            has_descriptor: flags & FLAG_DESCRIPTOR != 0,
            flags,
            is_dir,
        });
    }
    if !cur.is_empty() {
        return Err(ArchiveRefusal::IndexLies);
    }
    if total > limits.ratio_floor && total > limits.max_archive_ratio.saturating_mul(file_len) {
        return Err(ArchiveRefusal::ArchiveRatioTooHigh {
            ratio: limits.max_archive_ratio,
        });
    }
    confront_local_headers(r, &entries, &mut plans, cd_offset, cancel)?;
    Ok((entries, plans))
}

fn method_code(method: Method) -> u16 {
    match method {
        Method::Stored => METHOD_STORED,
        Method::Deflate => METHOD_DEFLATE,
    }
}

/// Confronte chaque entrée à son en-tête local et vérifie que les entrées pavent le fichier depuis
/// l'octet 0 jusqu'à l'index, sans trou (données préfixées ou cachées) ni chevauchement.
fn confront_local_headers<R: Read + Seek>(
    r: &mut R,
    entries: &[EntryInfo],
    plans: &mut [ZipPlan],
    cd_offset: u64,
    cancel: Cancel<'_>,
) -> Result<(), ArchiveRefusal> {
    let mut order: Vec<usize> = (0..plans.len()).collect();
    order.sort_by_key(|&i| plans[i].local_offset);
    let first_offset = order.first().map_or(0, |&i| plans[i].local_offset);
    if first_offset != 0 || (order.is_empty() && cd_offset != 0) {
        return Err(ArchiveRefusal::UncoveredBytes);
    }
    for (rank, &i) in order.iter().enumerate() {
        check_cancel(cancel)?;
        let offset = plans[i].local_offset;
        let region_end = order
            .get(rank + 1)
            .map_or(cd_offset, |&j| plans[j].local_offset);
        // Le défaut d'une entrée qui déborde de sa zone : hors du fichier si c'est la dernière,
        // sinon elle mord sur la suivante.
        let overflow = if region_end == cd_offset {
            ArchiveRefusal::EntryOutsideFile
        } else {
            ArchiveRefusal::OverlappingEntries
        };
        if region_end == offset {
            return Err(ArchiveRefusal::OverlappingEntries);
        }
        if offset + LOCAL_HEADER_LEN > region_end {
            return Err(overflow);
        }
        let mut raw = [0u8; LOCAL_HEADER_LEN as usize];
        read_exact_at(r, offset, &mut raw)?;
        let mut cur = Cur(&raw);
        if cur.u32()? != SIG_LOCAL {
            return Err(ArchiveRefusal::LocalHeaderMismatch);
        }
        let _needed = cur.u16()?;
        let flags = cur.u16()?;
        let method = cur.u16()?;
        let _time = cur.u16()?;
        let _date = cur.u16()?;
        let crc32 = cur.u32()?;
        let compressed32 = cur.u32()?;
        let size32 = cur.u32()?;
        let name_len = u64::from(cur.u16()?);
        let extra_len = u64::from(cur.u16()?);
        let plan = &plans[i];
        let mut expected = entries[i].path.as_str().to_string();
        if plan.is_dir {
            expected.push('/');
        }
        if (flags ^ plan.flags) & FLAGS_COMPARED != 0
            || method != method_code(plan.method)
            || name_len != expected.len() as u64
        {
            return Err(ArchiveRefusal::LocalHeaderMismatch);
        }
        let name_at = offset + LOCAL_HEADER_LEN;
        let data_offset = name_at + name_len + extra_len;
        let data_end = data_offset.saturating_add(plan.compressed);
        if data_end > region_end {
            return Err(overflow);
        }
        let mut name = vec![0u8; expected.len()];
        read_exact_at(r, name_at, &mut name)?;
        if name != expected.as_bytes() {
            return Err(ArchiveRefusal::LocalHeaderMismatch);
        }
        let (size, compressed) = if size32 == u32::MAX || compressed32 == u32::MAX {
            let mut extra = vec![0u8; extra_len as usize];
            read_exact_at(r, name_at + name_len, &mut extra)?;
            let (s, c, _) =
                zip64_fields(&extra, size32 == u32::MAX, compressed32 == u32::MAX, false)?;
            (
                if size32 == u32::MAX {
                    s
                } else {
                    u64::from(size32)
                },
                if compressed32 == u32::MAX {
                    c
                } else {
                    u64::from(compressed32)
                },
            )
        } else {
            (u64::from(size32), u64::from(compressed32))
        };
        let descriptor = plan.has_descriptor;
        let agrees = |local: u64, central: u64| local == central || (descriptor && local == 0);
        if !agrees(u64::from(crc32), u64::from(plan.crc))
            || !agrees(compressed, plan.compressed)
            || !agrees(size, plan.size)
        {
            return Err(ArchiveRefusal::LocalHeaderMismatch);
        }
        let gap = region_end - data_end;
        if !descriptor {
            if gap != 0 {
                return Err(ArchiveRefusal::UncoveredBytes);
            }
        } else {
            if !matches!(gap, 12 | 16 | 20 | 24) {
                return Err(ArchiveRefusal::UncoveredBytes);
            }
            let mut raw = [0u8; 24];
            let raw = &mut raw[..gap as usize];
            read_exact_at(r, data_end, raw)?;
            let mut d = Cur(raw);
            if gap == 16 || gap == 24 {
                let signature = d.u32().map_err(|_| ArchiveRefusal::LocalHeaderMismatch)?;
                if signature != SIG_DESCRIPTOR {
                    return Err(ArchiveRefusal::LocalHeaderMismatch);
                }
            }
            let mismatch = |_| ArchiveRefusal::LocalHeaderMismatch;
            let dcrc = d.u32().map_err(mismatch)?;
            let (dcompressed, dsize) = if gap >= 20 {
                (d.u64().map_err(mismatch)?, d.u64().map_err(mismatch)?)
            } else {
                (
                    u64::from(d.u32().map_err(mismatch)?),
                    u64::from(d.u32().map_err(mismatch)?),
                )
            };
            if dcrc != plan.crc || dcompressed != plan.compressed || dsize != plan.size {
                return Err(ArchiveRefusal::LocalHeaderMismatch);
            }
        }
        plans[i].data_offset = data_offset;
    }
    Ok(())
}

/// Lit une entrée : octets déballés, au plus `plan.size`, avec somme de contrôle vérifiée.
pub(super) fn read_entry<R: Read + Seek>(
    r: &mut R,
    plan: &ZipPlan,
    cancel: Cancel<'_>,
    emit: &mut dyn FnMut(&[u8]) -> Result<(), ArchiveRefusal>,
) -> Result<(), ArchiveRefusal> {
    r.seek(SeekFrom::Start(plan.data_offset))
        .map_err(ArchiveRefusal::from_io)?;
    let limited = r.take(plan.compressed);
    let mut reader: Box<dyn Read + '_> = match plan.method {
        Method::Stored => Box::new(limited),
        Method::Deflate => Box::new(DeflateDecoder::new(BufReader::with_capacity(
            super::limits::BUFFER,
            limited,
        ))),
    };
    pump(&mut *reader, plan.size, Some(plan.crc), cancel, emit)
}
