//! Outils des tests : fabrique des archives (zip, tar, gzip) octet par octet, y compris piégées, et un
//! port d'écriture en mémoire. Aucun fichier d'archive n'est versionné : chaque test construit la sienne.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::{Cursor, Write};
use std::rc::Rc;

use flate2::Crc;
use flate2::write::DeflateEncoder;

use super::{
    ArchiveLimits, ArchivePath, ArchiveRefusal, ExtractSink, Inspection, SinkError, extract,
    inspect,
};

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = Crc::new();
    crc.update(data);
    crc.sum()
}

pub fn deflate(data: &[u8]) -> Vec<u8> {
    let mut encoder = DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

pub fn gzip(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 3];
    out.extend(deflate(data));
    out.extend(crc32(data).to_le_bytes());
    out.extend((data.len() as u32).to_le_bytes());
    out
}

fn u16le(out: &mut Vec<u8>, v: u16) {
    out.extend(v.to_le_bytes());
}
fn u32le(out: &mut Vec<u8>, v: u32) {
    out.extend(v.to_le_bytes());
}
fn u64le(out: &mut Vec<u8>, v: u64) {
    out.extend(v.to_le_bytes());
}

// ---------------------------------------------------------------------------------------------
// ZIP
// ---------------------------------------------------------------------------------------------

#[derive(Clone)]
pub struct ZEntry {
    pub name: Vec<u8>,
    pub data: Vec<u8>,
    pub deflate: bool,
    pub flags: u16,
    pub made_by: u16,
    pub external: u32,
    pub local_name: Option<Vec<u8>>,
    pub central_method: Option<u16>,
    pub local_method: Option<u16>,
    pub central_usize: Option<u32>,
    /// Mensonge cohérent : taille annoncée (centrale ET locale), le flux compressé reste le vrai.
    pub lie_size: Option<u32>,
    pub lie_compressed: Option<u32>,
    pub lie_crc: Option<u32>,
    pub descriptor: bool,
    pub descriptor_crc: Option<u32>,
    pub offset_override: Option<u32>,
    pub raw_payload: Option<Vec<u8>>,
    pub central_extra: Vec<u8>,
}

impl ZEntry {
    pub fn file(name: &str, data: &[u8]) -> Self {
        Self::raw_name(name.as_bytes(), data)
    }
    pub fn raw_name(name: &[u8], data: &[u8]) -> Self {
        Self {
            name: name.to_vec(),
            data: data.to_vec(),
            deflate: false,
            flags: 0,
            made_by: 0x031E,
            external: 0o100_644 << 16,
            local_name: None,
            central_method: None,
            local_method: None,
            central_usize: None,
            lie_size: None,
            lie_compressed: None,
            lie_crc: None,
            descriptor: false,
            descriptor_crc: None,
            offset_override: None,
            raw_payload: None,
            central_extra: Vec::new(),
        }
    }
    pub fn dir(name: &str) -> Self {
        Self {
            external: 0o040_755 << 16,
            ..Self::file(name, b"")
        }
    }
    pub fn deflated(self) -> Self {
        Self {
            deflate: true,
            ..self
        }
    }
    pub fn mode(self, mode: u32) -> Self {
        Self {
            external: mode << 16,
            ..self
        }
    }
}

#[derive(Clone, Default)]
pub struct Zip {
    pub entries: Vec<ZEntry>,
    pub comment: Vec<u8>,
    pub comment_len_lie: Option<u16>,
    pub prefix: Vec<u8>,
    pub gap_before_index: Vec<u8>,
    pub count_lie: Option<u16>,
    pub index_size_lie: Option<u32>,
    pub eocd_disk: u16,
    pub second_end_record: bool,
    pub zip64: bool,
}

impl Zip {
    pub fn of(entries: Vec<ZEntry>) -> Self {
        Self {
            entries,
            ..Self::default()
        }
    }

    pub fn build(&self) -> Vec<u8> {
        let mut out = self.prefix.clone();
        let mut offsets = Vec::new();
        for e in &self.entries {
            offsets.push(out.len() as u32);
            let payload = e.raw_payload.clone().unwrap_or_else(|| {
                if e.deflate {
                    deflate(&e.data)
                } else {
                    e.data.clone()
                }
            });
            let method: u16 = if e.deflate { 8 } else { 0 };
            let crc = e.lie_crc.unwrap_or_else(|| crc32(&e.data));
            let csize = e.lie_compressed.unwrap_or(payload.len() as u32);
            let usize_ = e.lie_size.unwrap_or(e.data.len() as u32);
            let name = e.local_name.clone().unwrap_or_else(|| e.name.clone());
            u32le(&mut out, 0x0403_4b50);
            u16le(&mut out, 20);
            u16le(&mut out, e.flags | if e.descriptor { 8 } else { 0 });
            u16le(&mut out, e.local_method.unwrap_or(method));
            u16le(&mut out, 0);
            u16le(&mut out, 0x21);
            u32le(&mut out, if e.descriptor { 0 } else { crc });
            u32le(&mut out, if e.descriptor { 0 } else { csize });
            u32le(&mut out, if e.descriptor { 0 } else { usize_ });
            u16le(&mut out, name.len() as u16);
            u16le(&mut out, 0);
            out.extend(&name);
            out.extend(&payload);
            if e.descriptor {
                u32le(&mut out, 0x0807_4b50);
                u32le(&mut out, e.descriptor_crc.unwrap_or(crc));
                u32le(&mut out, csize);
                u32le(&mut out, usize_);
            }
        }
        out.extend(&self.gap_before_index);
        let index_start = out.len() as u32;
        for (e, offset) in self.entries.iter().zip(&offsets) {
            let payload_len = e.raw_payload.as_ref().map_or_else(
                || {
                    if e.deflate {
                        deflate(&e.data).len()
                    } else {
                        e.data.len()
                    }
                },
                Vec::len,
            ) as u32;
            let method: u16 = if e.deflate { 8 } else { 0 };
            let crc = e.lie_crc.unwrap_or_else(|| crc32(&e.data));
            let csize = e.lie_compressed.unwrap_or(payload_len);
            let usize_ = e
                .central_usize
                .or(e.lie_size)
                .unwrap_or(e.data.len() as u32);
            let mut extra = e.central_extra.clone();
            let (csize32, usize32) = if self.zip64 {
                u16le(&mut extra, 0x0001);
                u16le(&mut extra, 16);
                u64le(&mut extra, u64::from(usize_));
                u64le(&mut extra, u64::from(csize));
                (u32::MAX, u32::MAX)
            } else {
                (csize, usize_)
            };
            u32le(&mut out, 0x0201_4b50);
            u16le(&mut out, e.made_by);
            u16le(&mut out, 20);
            u16le(&mut out, e.flags | if e.descriptor { 8 } else { 0 });
            u16le(&mut out, e.central_method.unwrap_or(method));
            u16le(&mut out, 0);
            u16le(&mut out, 0x21);
            u32le(&mut out, crc);
            u32le(&mut out, csize32);
            u32le(&mut out, usize32);
            u16le(&mut out, e.name.len() as u16);
            u16le(&mut out, extra.len() as u16);
            u16le(&mut out, 0);
            u16le(&mut out, 0);
            u16le(&mut out, 0);
            u32le(&mut out, e.external);
            u32le(&mut out, e.offset_override.unwrap_or(*offset));
            out.extend(&e.name);
            out.extend(&extra);
        }
        let index_size = out.len() as u32 - index_start;
        let count = self.entries.len() as u16;
        if self.zip64 {
            let record_at = out.len() as u64;
            u32le(&mut out, 0x0606_4b50);
            u64le(&mut out, 44);
            u16le(&mut out, 45);
            u16le(&mut out, 45);
            u32le(&mut out, 0);
            u32le(&mut out, 0);
            u64le(&mut out, u64::from(count));
            u64le(&mut out, u64::from(count));
            u64le(&mut out, u64::from(index_size));
            u64le(&mut out, u64::from(index_start));
            u32le(&mut out, 0x0707_4b50);
            u32le(&mut out, 0);
            u64le(&mut out, record_at);
            u32le(&mut out, 1);
        }
        let end = |out: &mut Vec<u8>, with_comment: bool| {
            u32le(out, 0x0605_4b50);
            u16le(out, self.eocd_disk);
            u16le(out, 0);
            let shown = if self.zip64 {
                0xFFFF
            } else {
                self.count_lie.unwrap_or(count)
            };
            u16le(out, shown);
            u16le(out, shown);
            if self.zip64 {
                u32le(out, u32::MAX);
                u32le(out, u32::MAX);
            } else {
                u32le(out, self.index_size_lie.unwrap_or(index_size));
                u32le(out, index_start);
            }
            if with_comment {
                u16le(
                    out,
                    self.comment_len_lie.unwrap_or(self.comment.len() as u16),
                );
                out.extend(&self.comment);
            } else {
                u16le(out, 0);
            }
        };
        if self.second_end_record {
            end(&mut out, false);
        }
        end(&mut out, true);
        out
    }
}

// ---------------------------------------------------------------------------------------------
// TAR
// ---------------------------------------------------------------------------------------------

#[derive(Clone)]
pub struct TEntry {
    pub name: Vec<u8>,
    pub flag: u8,
    pub data: Vec<u8>,
    pub size_field: Option<Vec<u8>>,
    pub magic: Vec<u8>,
    pub bad_checksum: bool,
    pub prefix: Vec<u8>,
    /// Octets de données réellement écrits (pour mentir sur la taille).
    pub emit_data: bool,
}

impl TEntry {
    pub fn file(name: &str, data: &[u8]) -> Self {
        Self {
            name: name.as_bytes().to_vec(),
            flag: b'0',
            data: data.to_vec(),
            size_field: None,
            magic: b"ustar\0".to_vec(),
            bad_checksum: false,
            prefix: Vec::new(),
            emit_data: true,
        }
    }
    pub fn kind(name: &str, flag: u8) -> Self {
        Self {
            flag,
            ..Self::file(name, b"")
        }
    }
    pub fn dir(name: &str) -> Self {
        Self::kind(name, b'5')
    }

    pub fn header(&self) -> [u8; 512] {
        let mut block = [0u8; 512];
        let name = &self.name[..self.name.len().min(100)];
        block[..name.len()].copy_from_slice(name);
        block[100..107].copy_from_slice(b"0000644");
        block[108..115].copy_from_slice(b"0000000");
        block[116..123].copy_from_slice(b"0000000");
        match &self.size_field {
            Some(field) => block[124..124 + field.len()].copy_from_slice(field),
            None => {
                let text = format!("{:011o}", self.data.len());
                block[124..135].copy_from_slice(text.as_bytes());
            }
        }
        block[136..147].copy_from_slice(b"00000000000");
        block[156] = self.flag;
        block[257..257 + self.magic.len()].copy_from_slice(&self.magic);
        block[263..265].copy_from_slice(b"00");
        block[345..345 + self.prefix.len()].copy_from_slice(&self.prefix);
        block[148..156].copy_from_slice(b"        ");
        let sum: u32 = block.iter().map(|&b| u32::from(b)).sum();
        let text = if self.bad_checksum {
            format!("{:06o}\0 ", sum + 1)
        } else {
            format!("{sum:06o}\0 ")
        };
        block[148..156].copy_from_slice(text.as_bytes());
        block
    }

    pub fn bytes(&self) -> Vec<u8> {
        let mut out = self.header().to_vec();
        if self.emit_data {
            out.extend(&self.data);
            out.extend(vec![0u8; (512 - self.data.len() % 512) % 512]);
        }
        out
    }
}

pub fn tar_of(entries: &[TEntry]) -> Vec<u8> {
    let mut out = Vec::new();
    for e in entries {
        out.extend(e.bytes());
    }
    out.extend(vec![0u8; 1024]);
    out
}

pub fn pax_record(key: &str, value: &str) -> Vec<u8> {
    let body = format!(" {key}={value}\n");
    let mut length = body.len() + 1;
    while length.to_string().len() + body.len() != length {
        length = length.to_string().len() + body.len();
    }
    format!("{length}{body}").into_bytes()
}

pub fn pax_header(records: &[u8]) -> Vec<u8> {
    TEntry {
        flag: b'x',
        ..TEntry::file("PaxHeader", records)
    }
    .bytes()
}

pub fn gnu_long_name(name: &[u8]) -> Vec<u8> {
    let mut data = name.to_vec();
    data.push(0);
    TEntry {
        flag: b'L',
        ..TEntry::file("././@LongLink", &data)
    }
    .bytes()
}

// ---------------------------------------------------------------------------------------------
// Port d'écriture en mémoire et aides
// ---------------------------------------------------------------------------------------------

#[derive(Default, Clone)]
pub struct MemSink {
    pub files: Rc<RefCell<BTreeMap<String, Vec<u8>>>>,
    pub dirs: Rc<RefCell<Vec<String>>>,
    pub fail_on_file: Option<usize>,
    pub created: usize,
}

struct MemWriter {
    files: Rc<RefCell<BTreeMap<String, Vec<u8>>>>,
    key: String,
}

impl Write for MemWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.files
            .borrow_mut()
            .entry(self.key.clone())
            .or_default()
            .extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl ExtractSink for MemSink {
    fn create_dir(&mut self, path: &ArchivePath) -> Result<(), SinkError> {
        self.dirs.borrow_mut().push(path.as_str().to_string());
        Ok(())
    }
    fn create_file(&mut self, path: &ArchivePath) -> Result<Box<dyn Write + '_>, SinkError> {
        self.created += 1;
        if self.fail_on_file == Some(self.created) {
            return Err(SinkError::Io(std::io::ErrorKind::StorageFull));
        }
        self.files
            .borrow_mut()
            .insert(path.as_str().to_string(), Vec::new());
        Ok(Box::new(MemWriter {
            files: Rc::clone(&self.files),
            key: path.as_str().to_string(),
        }))
    }
}

pub fn never() -> bool {
    false
}

pub fn judge_bytes(bytes: Vec<u8>) -> Result<Inspection, ArchiveRefusal> {
    inspect(&mut Cursor::new(bytes), &ArchiveLimits::default(), &never)
}

pub fn judge_with(bytes: Vec<u8>, limits: &ArchiveLimits) -> Result<Inspection, ArchiveRefusal> {
    inspect(&mut Cursor::new(bytes), limits, &never)
}

/// Le code du refus d'une archive (échoue si elle est acceptée).
pub fn refusal_of(bytes: Vec<u8>) -> ArchiveRefusal {
    match judge_bytes(bytes) {
        Ok(_) => panic!("l'archive piégée a été acceptée"),
        Err(refusal) => refusal,
    }
}

pub fn code_of(bytes: Vec<u8>) -> &'static str {
    refusal_of(bytes).code()
}

/// Inspecte puis extrait dans un `MemSink`.
pub fn unpack(bytes: Vec<u8>) -> Result<MemSink, ArchiveRefusal> {
    let limits = ArchiveLimits::default();
    let mut source = Cursor::new(bytes);
    let inspection = inspect(&mut source, &limits, &never)?;
    let mut sink = MemSink::default();
    extract(&mut source, &inspection, &mut sink, &limits, &never)?;
    Ok(sink)
}
