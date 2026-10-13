//! Extraction : deux passes liées par l'empreinte, bornes pendant l'écriture, annulation, pannes du
//! port d'écriture, et le test de génération aléatoire (aucune entrée ne fait paniquer le module).

use std::cell::Cell;
use std::io::Cursor;

use super::testkit::*;
use super::{ArchiveLimits, ArchiveRefusal, MIB, extract, inspect};

fn archive() -> Vec<u8> {
    Zip::of(vec![
        ZEntry::dir("mods/"),
        ZEntry::file("mods/a.jar", b"AAAA"),
        ZEntry::file("mods/b.jar", &vec![b'b'; 100_000]).deflated(),
        ZEntry::file("config.toml", b"x = 1"),
    ])
    .build()
}

#[test]
fn a_zip_is_unpacked_to_exactly_the_bytes_it_holds() {
    let sink = unpack(archive()).unwrap();
    let files = sink.files.borrow();
    assert_eq!(files.len(), 3);
    assert_eq!(files["mods/a.jar"], b"AAAA");
    assert_eq!(files["mods/b.jar"], vec![b'b'; 100_000]);
    assert_eq!(files["config.toml"], b"x = 1");
    assert_eq!(*sink.dirs.borrow(), vec!["mods".to_string()]);
}

#[test]
fn extraction_refuses_when_the_archive_file_changed_since_the_inspection() {
    let limits = ArchiveLimits::default();
    let original = archive();
    let inspection = inspect(&mut Cursor::new(original.clone()), &limits, &never).unwrap();
    // Un octet de données change, la longueur et la structure restent plausibles.
    let mut changed = original.clone();
    let at = changed.len() / 3;
    changed[at] ^= 0x01;
    let mut sink = MemSink::default();
    let result = extract(
        &mut Cursor::new(changed),
        &inspection,
        &mut sink,
        &limits,
        &never,
    );
    assert_eq!(result, Err(ArchiveRefusal::ArchiveChanged));
    assert!(sink.files.borrow().is_empty() && sink.dirs.borrow().is_empty());
    // Un octet de plus au bout : refus aussi.
    let mut longer = original;
    longer.push(0);
    let result = extract(
        &mut Cursor::new(longer),
        &inspection,
        &mut sink,
        &limits,
        &never,
    );
    assert_eq!(result, Err(ArchiveRefusal::ArchiveChanged));
    assert!(sink.files.borrow().is_empty());
}

#[test]
fn writing_stops_at_the_first_byte_beyond_the_announced_size() {
    // Sans relecture à l'inspection (cas où l'archive est jugée sur ses annonces), l'extraction
    // elle-même ne laisse jamais passer plus que la taille annoncée.
    let limits = ArchiveLimits {
        verify_data: false,
        ..ArchiveLimits::default()
    };
    let liar = ZEntry {
        lie_size: Some(10),
        ..ZEntry::file("liar", &vec![7u8; 5 * MIB as usize]).deflated()
    };
    let bytes = Zip::of(vec![liar]).build();
    let mut source = Cursor::new(bytes);
    let inspection = inspect(&mut source, &limits, &never).unwrap();
    let mut sink = MemSink::default();
    let result = extract(&mut source, &inspection, &mut sink, &limits, &never);
    assert_eq!(result, Err(ArchiveRefusal::SizeMismatch));
    assert_eq!(sink.files.borrow()["liar"].len(), 10);
}

#[test]
fn a_stored_entry_that_is_shorter_than_announced_is_refused_at_extraction() {
    let limits = ArchiveLimits {
        verify_data: false,
        ..ArchiveLimits::default()
    };
    let short = ZEntry {
        lie_size: Some(50),
        ..ZEntry::file("short", b"abc").deflated()
    };
    let mut source = Cursor::new(Zip::of(vec![short]).build());
    let inspection = inspect(&mut source, &limits, &never).unwrap();
    let mut sink = MemSink::default();
    assert_eq!(
        extract(&mut source, &inspection, &mut sink, &limits, &never),
        Err(ArchiveRefusal::SizeMismatch)
    );
}

#[test]
fn cancellation_is_honored_before_and_during_the_work() {
    let limits = ArchiveLimits::default();
    let bytes = archive();
    assert_eq!(
        inspect(&mut Cursor::new(bytes.clone()), &limits, &|| true).err(),
        Some(ArchiveRefusal::Cancelled)
    );
    let mut source = Cursor::new(bytes);
    let inspection = inspect(&mut source, &limits, &never).unwrap();
    let calls = Cell::new(0);
    let cancel = || {
        calls.set(calls.get() + 1);
        calls.get() > 6
    };
    let mut sink = MemSink::default();
    assert_eq!(
        extract(&mut source, &inspection, &mut sink, &limits, &cancel),
        Err(ArchiveRefusal::Cancelled)
    );
    assert!(sink.files.borrow().len() < 3);
}

#[test]
fn a_failing_sink_turns_into_a_write_failure_without_naming_anything() {
    let limits = ArchiveLimits::default();
    let mut source = Cursor::new(archive());
    let inspection = inspect(&mut source, &limits, &never).unwrap();
    let mut sink = MemSink {
        fail_on_file: Some(2),
        ..MemSink::default()
    };
    let refusal = extract(&mut source, &inspection, &mut sink, &limits, &never).unwrap_err();
    assert_eq!(refusal, ArchiveRefusal::WriteFailed);
    assert!(!refusal.is_about_the_archive());
    assert!(!refusal.to_string().contains("mods"));
}

#[test]
fn the_inspection_alone_writes_nothing_and_extracts_nothing() {
    // `inspect` n'a pas de port d'écriture : un refus tardif ne laisse donc rien. Le contrôle est
    // dans le type ; ce test vérifie qu'une archive refusée tard (dernière entrée) l'est avant tout.
    let bytes = Zip::of(vec![
        ZEntry::file("a", b"1"),
        ZEntry::file("b", b"2"),
        ZEntry::file("c", b"3").mode(0o120_777),
    ])
    .build();
    assert_eq!(code_of(bytes), "SYMBOLIC_LINK");
}

/// Générateur pseudo-aléatoire à graine fixe (xorshift) : le test est rejouable.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

fn try_everything(bytes: Vec<u8>) {
    let limits = ArchiveLimits {
        max_unpacked: 4 * MIB,
        ..ArchiveLimits::default()
    };
    let mut source = Cursor::new(bytes);
    if let Ok(inspection) = inspect(&mut source, &limits, &never) {
        let mut sink = MemSink::default();
        let _ = extract(&mut source, &inspection, &mut sink, &limits, &never);
        let _ = super::read_one(&mut source, &inspection, "a.txt", &limits, &never);
    }
}

#[test]
fn random_mutations_of_valid_archives_never_panic() {
    let seeds = [
        archive(),
        Zip {
            zip64: true,
            ..Zip::of(vec![ZEntry::file("a.txt", b"hello").deflated()])
        }
        .build(),
        Zip::of(vec![ZEntry {
            descriptor: true,
            ..ZEntry::file("a.txt", b"hello").deflated()
        }])
        .build(),
        tar_of(&[TEntry::dir("d/"), TEntry::file("d/a.txt", b"hello")]),
        gzip(&tar_of(&[TEntry::file("a.txt", b"hello")])),
    ];
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for seed in &seeds {
        for _ in 0..600 {
            let mut bytes = seed.clone();
            for _ in 0..1 + rng.below(4) {
                match rng.below(5) {
                    0 if !bytes.is_empty() => {
                        let at = rng.below(bytes.len());
                        bytes[at] = rng.next() as u8;
                    }
                    1 if !bytes.is_empty() => {
                        let at = rng.below(bytes.len());
                        bytes[at] ^= 1 << rng.below(8);
                    }
                    2 if !bytes.is_empty() => {
                        let to = rng.below(bytes.len());
                        bytes.truncate(to);
                    }
                    3 => {
                        let at = rng.below(bytes.len() + 1);
                        bytes.insert(at, rng.next() as u8);
                    }
                    _ if bytes.len() > 4 => {
                        // Écrase 4 octets de longueur ou de décalage par une valeur extrême.
                        let at = rng.below(bytes.len() - 3);
                        bytes[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
                    }
                    _ => {}
                }
            }
            try_everything(bytes);
        }
    }
}

#[test]
fn random_bytes_never_panic() {
    let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
    for _ in 0..400 {
        let len = rng.below(3000);
        let mut bytes: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
        // Une fois sur deux, une fin de zip plausible au bout.
        if rng.below(2) == 0 {
            bytes.extend(b"PK\x05\x06");
            bytes.extend((0..18).map(|_| rng.next() as u8));
        }
        try_everything(bytes);
    }
}
