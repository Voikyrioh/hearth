//! Flux gzip à UN SEUL membre, écrit à la main sur la décompression `deflate` (ADR-0055 point 10).
//!
//! L'en-tête est lu ici, avec des bornes (les décodeurs courants lisent le nom et le commentaire sans
//! limite) ; le pied (somme de contrôle et taille) est vérifié ; un second membre ou des octets après
//! la fin sont un refus ; le taux de compression est surveillé pendant la lecture.

use std::cell::Cell;
use std::io::{BufRead, BufReader, Read};
use std::rc::Rc;

use flate2::Crc;
use flate2::bufread::DeflateDecoder;

use super::limits::{ArchiveLimits, BUFFER};
use super::refusal::ArchiveRefusal;

const MAX_HEADER_STRING: usize = 4096;

pub(super) fn is_gzip(head: &[u8]) -> bool {
    head.starts_with(&[0x1f, 0x8b])
}

struct Counting<R> {
    inner: R,
    count: Rc<Cell<u64>>,
}

impl<R: Read> Read for Counting<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.count.set(self.count.get().saturating_add(n as u64));
        Ok(n)
    }
}

pub(super) struct GzipReader<R: Read> {
    decoder: DeflateDecoder<BufReader<Counting<R>>>,
    consumed: Rc<Cell<u64>>,
    crc: Crc,
    produced: u64,
    done: bool,
    ratio_floor: u64,
    max_ratio: u64,
}

fn skip(reader: &mut impl Read, mut n: usize) -> Result<(), ArchiveRefusal> {
    let mut scratch = [0u8; 512];
    while n > 0 {
        let step = n.min(scratch.len());
        reader
            .read_exact(&mut scratch[..step])
            .map_err(ArchiveRefusal::from_io)?;
        n -= step;
    }
    Ok(())
}

fn skip_cstring(reader: &mut impl BufRead) -> Result<(), ArchiveRefusal> {
    for _ in 0..=MAX_HEADER_STRING {
        let mut byte = [0u8; 1];
        reader
            .read_exact(&mut byte)
            .map_err(ArchiveRefusal::from_io)?;
        if byte[0] == 0 {
            return Ok(());
        }
    }
    Err(ArchiveRefusal::BadHeader)
}

impl<R: Read> GzipReader<R> {
    pub(super) fn new(source: R, limits: &ArchiveLimits) -> Result<Self, ArchiveRefusal> {
        let consumed = Rc::new(Cell::new(0));
        let mut input = BufReader::with_capacity(
            BUFFER,
            Counting {
                inner: source,
                count: Rc::clone(&consumed),
            },
        );
        let mut head = [0u8; 10];
        input
            .read_exact(&mut head)
            .map_err(ArchiveRefusal::from_io)?;
        if head[0] != 0x1f || head[1] != 0x8b {
            return Err(ArchiveRefusal::UnrecognizedFormat);
        }
        if head[2] != 8 {
            return Err(ArchiveRefusal::UnsupportedMethod);
        }
        let flags = head[3];
        if flags & 0xE0 != 0 {
            return Err(ArchiveRefusal::BadHeader);
        }
        if flags & 0x04 != 0 {
            let mut len = [0u8; 2];
            input
                .read_exact(&mut len)
                .map_err(ArchiveRefusal::from_io)?;
            skip(&mut input, usize::from(u16::from_le_bytes(len)))?;
        }
        if flags & 0x08 != 0 {
            skip_cstring(&mut input)?;
        }
        if flags & 0x10 != 0 {
            skip_cstring(&mut input)?;
        }
        if flags & 0x02 != 0 {
            skip(&mut input, 2)?;
        }
        Ok(Self {
            decoder: DeflateDecoder::new(input),
            consumed,
            crc: Crc::new(),
            produced: 0,
            done: false,
            ratio_floor: limits.ratio_floor,
            max_ratio: limits.max_archive_ratio,
        })
    }

    fn finish_member(&mut self) -> Result<(), ArchiveRefusal> {
        let input = self.decoder.get_mut();
        let mut trailer = [0u8; 8];
        input
            .read_exact(&mut trailer)
            .map_err(ArchiveRefusal::from_io)?;
        let crc = u32::from_le_bytes([trailer[0], trailer[1], trailer[2], trailer[3]]);
        let size = u32::from_le_bytes([trailer[4], trailer[5], trailer[6], trailer[7]]);
        if crc != self.crc.sum() || size != (self.produced & 0xFFFF_FFFF) as u32 {
            return Err(ArchiveRefusal::ChecksumMismatch);
        }
        let rest = input.fill_buf().map_err(ArchiveRefusal::from_io)?;
        if !rest.is_empty() {
            return Err(ArchiveRefusal::MultipleGzipMembers);
        }
        Ok(())
    }
}

impl<R: Read> Read for GzipReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.done || buf.is_empty() {
            return Ok(0);
        }
        let n = self.decoder.read(buf)?;
        if n == 0 {
            self.finish_member().map_err(ArchiveRefusal::into_io)?;
            self.done = true;
            return Ok(0);
        }
        self.crc.update(&buf[..n]);
        self.produced = self.produced.saturating_add(n as u64);
        let budget = self
            .max_ratio
            .saturating_mul(self.consumed.get().saturating_add(BUFFER as u64));
        if self.produced > self.ratio_floor && self.produced > budget {
            return Err(ArchiveRefusal::ArchiveRatioTooHigh {
                ratio: self.max_ratio,
            }
            .into_io());
        }
        Ok(n)
    }
}
