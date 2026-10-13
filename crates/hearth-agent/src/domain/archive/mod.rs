//! Lecture d'une archive apportée par l'utilisateur ou venue d'Internet (HRT-130, ADR-0055).
//!
//! L'archive est un contenu HOSTILE, lue par un agent qui tourne en root. Domaine pur : le module ne
//! touche aucun fichier, il reçoit une source `Read + Seek` et un port d'écriture (`ExtractSink`).
//!
//! Deux passes liées par l'empreinte SHA-256 du fichier d'archive :
//! 1. `inspect` juge TOUTE l'archive (bornes, noms, types, index confronté aux en-têtes, taux de
//!    compression, tailles et sommes de contrôle réelles) et n'écrit rien ;
//! 2. `extract` recalcule l'empreinte, refuse si elle a changé, puis relit et écrit par le port.
//!
//! Un doute refuse l'archive ENTIÈRE. Aucune fonction d'extraction toute faite d'une crate n'est
//! utilisée. Règles : `BR-ARCH-001` à `BR-ARCH-011`.

mod flow;
mod gzip;
mod limits;
mod path;
mod refusal;
mod sink;
mod tar;
mod zip;

#[cfg(test)]
mod testkit;
#[cfg(test)]
mod tests_extract;
#[cfg(test)]
mod tests_paths;
#[cfg(test)]
mod tests_tar;
#[cfg(test)]
mod tests_zip;

use std::io::{BufReader, Read, Seek, SeekFrom, Write};

pub use flow::Cancel;
pub use limits::{ArchiveLimits, GIB, KIB, MIB};
pub use path::ArchivePath;
pub use refusal::ArchiveRefusal;
pub use sink::{ExtractSink, SinkError};

use flow::{check_cancel, digest, read_some};
use limits::BUFFER;
use path::{NameRegistry, judge};
use tar::TarWalker;
use zip::ZipPlan;

/// Format reconnu par ses octets (jamais par l'extension du nom).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    Zip,
    Tar,
    TarGz,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
}

/// Une entrée jugée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryInfo {
    pub path: ArchivePath,
    pub kind: EntryKind,
    /// Taille déballée annoncée, confirmée par la lecture réelle quand `verify_data` est vrai.
    pub size: u64,
}

/// Résultat de `inspect` : tout ce qu'`extract` et `read_one` ont besoin de croire de l'archive.
#[derive(Debug, Clone)]
pub struct Inspection {
    kind: ArchiveKind,
    sha256: [u8; 32],
    file_len: u64,
    entries: Vec<EntryInfo>,
    total_unpacked: u64,
    plans: Vec<ZipPlan>,
}

impl Inspection {
    pub fn kind(&self) -> ArchiveKind {
        self.kind
    }
    pub fn sha256(&self) -> &[u8; 32] {
        &self.sha256
    }
    pub fn entries(&self) -> &[EntryInfo] {
        &self.entries
    }
    pub fn total_unpacked(&self) -> u64 {
        self.total_unpacked
    }
    pub fn file_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.kind == EntryKind::File)
            .count()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExtractReport {
    pub files: usize,
    pub dirs: usize,
    pub bytes: u64,
}

fn detect<R: Read + Seek>(source: &mut R) -> Result<ArchiveKind, ArchiveRefusal> {
    source
        .seek(SeekFrom::Start(0))
        .map_err(ArchiveRefusal::from_io)?;
    let mut head = [0u8; 512];
    let mut filled = 0;
    while filled < head.len() {
        let n = read_some(source, &mut head[filled..]).map_err(ArchiveRefusal::from_io)?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    let head = &head[..filled];
    if head.starts_with(b"PK\x03\x04") || head.starts_with(b"PK\x05\x06") {
        Ok(ArchiveKind::Zip)
    } else if gzip::is_gzip(head) {
        Ok(ArchiveKind::TarGz)
    } else if tar::looks_like_tar(head) {
        Ok(ArchiveKind::Tar)
    } else {
        // Ni zip ni tar à son début : un zip précédé d'octets étrangers est jugé comme un zip, et
        // refusé comme tel (octets non couverts) s'il en est un ; sinon le format est inconnu.
        Ok(ArchiveKind::Zip)
    }
}

type TarStream<'r> = Box<dyn Read + 'r>;
/// Destinataire des octets déballés d'une entrée.
type Emit<'e> = dyn FnMut(&[u8]) -> Result<(), ArchiveRefusal> + 'e;
/// Fournit les octets d'une entrée à un destinataire.
type EntryData<'d> = dyn FnMut(&mut Emit<'_>) -> Result<(), ArchiveRefusal> + 'd;
/// Visite d'une entrée de tar rejouée.
type TarVisit<'v> =
    dyn FnMut(&EntryInfo, &mut TarWalker<'_, TarStream<'_>>) -> Result<(), ArchiveRefusal> + 'v;

fn open_tar_stream<'r, R: Read + Seek>(
    source: &'r mut R,
    kind: ArchiveKind,
    limits: &ArchiveLimits,
) -> Result<TarStream<'r>, ArchiveRefusal> {
    source
        .seek(SeekFrom::Start(0))
        .map_err(ArchiveRefusal::from_io)?;
    Ok(match kind {
        ArchiveKind::TarGz => Box::new(gzip::GzipReader::new(source, limits)?),
        _ => Box::new(BufReader::with_capacity(BUFFER, source)),
    })
}

/// Juge un nom de tar et son type.
fn judge_tar_item(
    item: &tar::TarItem,
    limits: &ArchiveLimits,
) -> Result<(ArchivePath, EntryKind), ArchiveRefusal> {
    let judged = judge(&item.raw_name, limits)?;
    if item.is_dir {
        Ok((judged.path, EntryKind::Dir))
    } else if judged.trailing_slash {
        Err(ArchiveRefusal::BadHeader)
    } else {
        Ok((judged.path, EntryKind::File))
    }
}

/// Juge toute l'archive et n'écrit rien. La source n'est lue que par `Read + Seek`.
pub fn inspect<R: Read + Seek>(
    source: &mut R,
    limits: &ArchiveLimits,
    cancel: Cancel<'_>,
) -> Result<Inspection, ArchiveRefusal> {
    let (sha256, file_len) = digest(source, limits, cancel)?;
    let kind = detect(source)?;
    let mut plans = Vec::new();
    let entries = match kind {
        ArchiveKind::Zip => {
            let (entries, zip_plans) = zip::inspect(source, file_len, limits, cancel)?;
            if limits.verify_data {
                for (info, plan) in entries.iter().zip(&zip_plans) {
                    if info.kind == EntryKind::File {
                        check_cancel(cancel)?;
                        zip::read_entry(source, plan, cancel, &mut |_| Ok(()))?;
                    }
                }
            }
            plans = zip_plans;
            entries
        }
        ArchiveKind::Tar | ArchiveKind::TarGz => {
            let stream = open_tar_stream(source, kind, limits)?;
            let mut walker = TarWalker::new(stream, limits);
            let mut registry = NameRegistry::new(limits.max_tree_nodes);
            let mut entries: Vec<EntryInfo> = Vec::new();
            while let Some(item) = walker.next_item(cancel)? {
                if entries.len() >= limits.max_entries {
                    return Err(ArchiveRefusal::TooManyEntries {
                        max: limits.max_entries,
                    });
                }
                let (path, entry_kind) = judge_tar_item(&item, limits)?;
                registry.add(&path, entry_kind == EntryKind::Dir)?;
                walker.skip_data(cancel)?;
                entries.push(EntryInfo {
                    path,
                    kind: entry_kind,
                    size: item.size,
                });
            }
            walker.finish(cancel)?;
            entries
        }
    };
    let total_unpacked = entries.iter().map(|e| e.size).sum();
    Ok(Inspection {
        kind,
        sha256,
        file_len,
        entries,
        total_unpacked,
        plans,
    })
}

/// L'empreinte du fichier est-elle celle de l'inspection ?
fn assert_unchanged<R: Read + Seek>(
    source: &mut R,
    inspection: &Inspection,
    limits: &ArchiveLimits,
    cancel: Cancel<'_>,
) -> Result<(), ArchiveRefusal> {
    let (sha256, len) = digest(source, limits, cancel)?;
    if sha256 != inspection.sha256 || len != inspection.file_len {
        return Err(ArchiveRefusal::ArchiveChanged);
    }
    Ok(())
}

/// Rejoue un tar déjà inspecté : chaque entrée doit être celle de l'inspection (même nom, même type,
/// même taille), puis `visit` en reçoit les données.
fn replay_tar<R: Read + Seek>(
    source: &mut R,
    inspection: &Inspection,
    limits: &ArchiveLimits,
    cancel: Cancel<'_>,
    visit: &mut TarVisit<'_>,
) -> Result<(), ArchiveRefusal> {
    let stream = open_tar_stream(source, inspection.kind, limits)?;
    let mut walker = TarWalker::new(stream, limits);
    let mut index = 0usize;
    while let Some(item) = walker.next_item(cancel)? {
        let (path, kind) = judge_tar_item(&item, limits)?;
        let Some(expected) = inspection.entries.get(index) else {
            return Err(ArchiveRefusal::ArchiveChanged);
        };
        if expected.path != path || expected.kind != kind || expected.size != item.size {
            return Err(ArchiveRefusal::ArchiveChanged);
        }
        visit(expected, &mut walker)?;
        index += 1;
    }
    if index != inspection.entries.len() {
        return Err(ArchiveRefusal::ArchiveChanged);
    }
    walker.finish(cancel)
}

fn sink_failure(_: SinkError) -> ArchiveRefusal {
    ArchiveRefusal::WriteFailed
}

/// Écrit l'archive inspectée par le port. L'empreinte du fichier est recalculée d'abord ; elle doit
/// être celle de l'inspection. La destination (dossier d'attente neuf) est l'affaire de l'appelant :
/// un refus ou une annulation laisse le dossier d'attente, que l'appelant retire.
pub fn extract<R: Read + Seek>(
    source: &mut R,
    inspection: &Inspection,
    sink: &mut dyn ExtractSink,
    limits: &ArchiveLimits,
    cancel: Cancel<'_>,
) -> Result<ExtractReport, ArchiveRefusal> {
    assert_unchanged(source, inspection, limits, cancel)?;
    let mut report = ExtractReport::default();
    let mut write_entry =
        |info: &EntryInfo, data: &mut EntryData<'_>| -> Result<(), ArchiveRefusal> {
            match info.kind {
                EntryKind::Dir => {
                    sink.create_dir(&info.path).map_err(sink_failure)?;
                    report.dirs += 1;
                }
                EntryKind::File => {
                    let mut writer = sink.create_file(&info.path).map_err(sink_failure)?;
                    data(&mut |chunk| {
                        writer
                            .write_all(chunk)
                            .map_err(|_| ArchiveRefusal::WriteFailed)
                    })?;
                    writer.flush().map_err(|_| ArchiveRefusal::WriteFailed)?;
                    report.files += 1;
                    report.bytes = report.bytes.saturating_add(info.size);
                }
            }
            Ok(())
        };
    match inspection.kind {
        ArchiveKind::Zip => {
            for (info, plan) in inspection.entries.iter().zip(&inspection.plans) {
                check_cancel(cancel)?;
                write_entry(info, &mut |emit| {
                    zip::read_entry(source, plan, cancel, emit)
                })?;
            }
        }
        ArchiveKind::Tar | ArchiveKind::TarGz => {
            replay_tar(source, inspection, limits, cancel, &mut |info, walker| {
                write_entry(info, &mut |emit| walker.read_data(info.size, cancel, emit))
            })?;
        }
    }
    Ok(report)
}

/// Lit UN fichier de l'archive inspectée, 1 Mio au plus : le même jugement de toute l'archive a eu
/// lieu avant (`inspection`), l'empreinte est revérifiée.
pub fn read_one<R: Read + Seek>(
    source: &mut R,
    inspection: &Inspection,
    wanted: &str,
    limits: &ArchiveLimits,
    cancel: Cancel<'_>,
) -> Result<Vec<u8>, ArchiveRefusal> {
    let Some(position) = inspection
        .entries
        .iter()
        .position(|e| e.kind == EntryKind::File && e.path.as_str() == wanted)
    else {
        return Err(ArchiveRefusal::EntryNotFound);
    };
    let size = inspection.entries[position].size;
    if size > limits.max_single_read {
        return Err(ArchiveRefusal::EntryTooLargeToRead {
            max: limits.max_single_read,
        });
    }
    assert_unchanged(source, inspection, limits, cancel)?;
    let mut out = Vec::with_capacity(size as usize);
    match inspection.kind {
        ArchiveKind::Zip => {
            let plan = &inspection.plans[position];
            zip::read_entry(source, plan, cancel, &mut |chunk| {
                out.extend_from_slice(chunk);
                Ok(())
            })?;
        }
        ArchiveKind::Tar | ArchiveKind::TarGz => {
            let mut seen = 0usize;
            replay_tar(source, inspection, limits, cancel, &mut |info, walker| {
                let here = seen;
                seen += 1;
                if here == position {
                    walker.read_data(info.size, cancel, &mut |chunk| {
                        out.extend_from_slice(chunk);
                        Ok(())
                    })?;
                }
                Ok(())
            })?;
        }
    }
    Ok(out)
}
