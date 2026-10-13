//! Pièges du `.tar` et du `.tar.gz` : liens, fichiers spéciaux, en-têtes étendus qui redéfinissent le
//! chemin, tailles fausses, fin prématurée, gzip à plusieurs membres ou tronqué (ADR-0055 point 10).

use std::io::Cursor;

use super::testkit::*;
use super::{ArchiveKind, ArchiveLimits, ArchiveRefusal, MIB, extract, inspect, read_one};

fn honest() -> Vec<u8> {
    tar_of(&[
        TEntry::dir("mods/"),
        TEntry::file("mods/a.jar", b"jar-a"),
        TEntry::file("readme.txt", &vec![b'r'; 700]),
    ])
}

#[test]
fn plain_tar_and_tar_gz_are_accepted_and_unpacked_identically() {
    let plain = unpack(honest()).unwrap();
    let gz = unpack(gzip(&honest())).unwrap();
    assert_eq!(plain.files.borrow().len(), 2);
    assert_eq!(
        plain.files.borrow().get("mods/a.jar"),
        Some(&b"jar-a".to_vec())
    );
    assert_eq!(
        plain.files.borrow().get("readme.txt").map(Vec::len),
        Some(700)
    );
    assert_eq!(*plain.files.borrow(), *gz.files.borrow());
    assert_eq!(*plain.dirs.borrow(), vec!["mods".to_string()]);
    assert_eq!(judge_bytes(honest()).unwrap().kind(), ArchiveKind::Tar);
    assert_eq!(
        judge_bytes(gzip(&honest())).unwrap().kind(),
        ArchiveKind::TarGz
    );
}

#[test]
fn links_devices_pipes_and_exotic_entries_are_refused() {
    for (flag, code) in [
        (b'2', "SYMBOLIC_LINK"),
        (b'1', "HARD_LINK"),
        (b'3', "SPECIAL_FILE"),
        (b'4', "SPECIAL_FILE"),
        (b'6', "SPECIAL_FILE"),
        (b'g', "UNSUPPORTED_FEATURE"),
        (b'S', "UNSUPPORTED_FEATURE"),
        (b'K', "UNSUPPORTED_FEATURE"),
        (b'7', "UNSUPPORTED_FEATURE"),
    ] {
        let bytes = tar_of(&[TEntry::file("ok", b"1"), TEntry::kind("bad", flag)]);
        assert_eq!(code_of(bytes), code, "{}", flag as char);
    }
}

#[test]
fn a_pax_header_may_rename_an_entry_but_the_new_name_is_judged_like_any_other() {
    let mut bytes = pax_header(&pax_record("path", "good/renamed.txt"));
    bytes.extend(TEntry::file("decoy", b"data").bytes());
    bytes.extend(vec![0u8; 1024]);
    let inspection = judge_bytes(bytes).unwrap();
    assert_eq!(inspection.entries()[0].path.as_str(), "good/renamed.txt");

    for hostile in [
        "../../etc/cron.d/x",
        "/etc/passwd",
        "a\\b",
        "C:/x",
        "ok/\u{0001}x",
    ] {
        let mut bytes = pax_header(&pax_record("path", hostile));
        bytes.extend(TEntry::file("decoy", b"data").bytes());
        bytes.extend(vec![0u8; 1024]);
        assert!(refusal_of(bytes).is_about_the_archive(), "{hostile}");
    }
}

#[test]
fn a_gnu_long_name_is_judged_like_any_other_name() {
    let long = format!("{}/{}", "d".repeat(120), "f".repeat(120));
    let mut bytes = gnu_long_name(long.as_bytes());
    bytes.extend(TEntry::file("short", b"x").bytes());
    bytes.extend(vec![0u8; 1024]);
    assert_eq!(judge_bytes(bytes).unwrap().entries()[0].path.as_str(), long);

    let mut bytes = gnu_long_name(b"../../escape");
    bytes.extend(TEntry::file("short", b"x").bytes());
    bytes.extend(vec![0u8; 1024]);
    assert_eq!(code_of(bytes), "PARENT_COMPONENT");

    let mut bytes = gnu_long_name(&vec![b'n'; 5000]);
    bytes.extend(TEntry::file("short", b"x").bytes());
    bytes.extend(vec![0u8; 1024]);
    assert_eq!(code_of(bytes), "NAME_TOO_LONG");
}

#[test]
fn two_extended_names_for_one_entry_are_refused() {
    let mut bytes = gnu_long_name(b"first/name");
    bytes.extend(pax_header(&pax_record("path", "second/name")));
    bytes.extend(TEntry::file("short", b"x").bytes());
    bytes.extend(vec![0u8; 1024]);
    assert_eq!(code_of(bytes), "BAD_HEADER");

    let mut records = pax_record("path", "a");
    records.extend(pax_record("path", "b"));
    let mut bytes = pax_header(&records);
    bytes.extend(TEntry::file("short", b"x").bytes());
    bytes.extend(vec![0u8; 1024]);
    assert_eq!(code_of(bytes), "BAD_HEADER");
}

#[test]
fn sparse_and_charset_pax_attributes_are_refused() {
    for key in ["GNU.sparse.major", "hdrcharset"] {
        let mut bytes = pax_header(&pax_record(key, "1"));
        bytes.extend(TEntry::file("f", b"x").bytes());
        bytes.extend(vec![0u8; 1024]);
        assert_eq!(code_of(bytes), "UNSUPPORTED_FEATURE", "{key}");
    }
}

#[test]
fn a_pax_size_overrides_the_header_size_and_is_bounded_like_it() {
    let mut bytes = pax_header(&pax_record("size", "5"));
    bytes.extend(
        TEntry {
            size_field: Some(b"00000000000".to_vec()),
            emit_data: false,
            ..TEntry::file("f", b"")
        }
        .bytes(),
    );
    bytes.extend(
        b"hello"
            .iter()
            .copied()
            .chain(std::iter::repeat_n(0u8, 507)),
    );
    bytes.extend(vec![0u8; 1024]);
    assert_eq!(judge_bytes(bytes).unwrap().entries()[0].size, 5);

    let mut bytes = pax_header(&pax_record("size", "99999999999999999999"));
    bytes.extend(TEntry::file("f", b"x").bytes());
    bytes.extend(vec![0u8; 1024]);
    assert!(refusal_of(bytes).is_about_the_archive());
}

#[test]
fn a_negative_or_malformed_size_is_refused() {
    for field in [
        &b"-0000000001"[..],
        b"abcdefghijk",
        b"0000000009 ",
        b"           ",
    ] {
        let bytes = tar_of(&[TEntry {
            size_field: Some(field.to_vec()),
            emit_data: false,
            ..TEntry::file("f", b"")
        }]);
        assert_eq!(code_of(bytes), "BAD_HEADER", "{field:?}");
    }
    // Base 256 négative.
    let mut negative = vec![0xFFu8; 12];
    negative[0] = 0xFF;
    let bytes = tar_of(&[TEntry {
        size_field: Some(negative),
        emit_data: false,
        ..TEntry::file("f", b"")
    }]);
    assert_eq!(code_of(bytes), "BAD_HEADER");
}

#[test]
fn an_enormous_size_is_refused_before_anything_is_read_for_it() {
    let mut huge = vec![0u8; 12];
    huge[0] = 0x80;
    huge[4] = 0x01; // 2^56 octets
    let bytes = tar_of(&[TEntry {
        size_field: Some(huge),
        emit_data: false,
        ..TEntry::file("f", b"")
    }]);
    assert_eq!(code_of(bytes), "TOTAL_SIZE_TOO_LARGE");

    let mut too_big_for_u64 = vec![0xFFu8; 12];
    too_big_for_u64[0] = 0x80;
    let bytes = tar_of(&[TEntry {
        size_field: Some(too_big_for_u64),
        emit_data: false,
        ..TEntry::file("f", b"")
    }]);
    assert_eq!(code_of(bytes), "BAD_HEADER");
}

#[test]
fn a_size_larger_than_the_data_that_follows_is_a_truncated_archive() {
    let bytes = tar_of(&[TEntry {
        size_field: Some(format!("{:011o}", MIB).into_bytes()),
        data: b"only ten bytes".to_vec(),
        ..TEntry::file("f", b"")
    }]);
    assert_eq!(code_of(bytes), "TRUNCATED");
}

#[test]
fn a_premature_end_is_refused_whatever_the_cut() {
    let whole = honest();
    for cut in [
        100,
        512,
        520,
        1300,
        whole.len() - 1024,
        whole.len() - 512,
        whole.len() - 1,
    ] {
        let bytes = whole[..cut].to_vec();
        let refusal = refusal_of(bytes);
        assert!(
            matches!(
                refusal,
                ArchiveRefusal::Truncated
                    | ArchiveRefusal::UnrecognizedFormat
                    | ArchiveRefusal::BadHeader
            ),
            "{cut}: {refusal:?}"
        );
    }
    // Les deux blocs nuls de fin manquent entièrement.
    let mut bytes = honest();
    bytes.truncate(bytes.len() - 1024);
    assert_eq!(code_of(bytes), "TRUNCATED");
}

#[test]
fn a_corrupt_header_is_refused() {
    let bytes = tar_of(&[TEntry {
        bad_checksum: true,
        ..TEntry::file("f", b"x")
    }]);
    assert_eq!(code_of(bytes), "BAD_HEADER");
    let bytes = tar_of(&[
        TEntry::file("ok", b"x"),
        TEntry {
            magic: b"xstar\0".to_vec(),
            ..TEntry::file("f", b"x")
        },
    ]);
    assert_eq!(code_of(bytes), "BAD_HEADER");
}

#[test]
fn a_name_with_bytes_hidden_after_its_terminator_is_refused() {
    let mut entry = TEntry::file("ok", b"x");
    entry.name = b"a\0hidden".to_vec();
    assert_eq!(code_of(tar_of(&[entry])), "BAD_HEADER");
}

#[test]
fn a_folder_with_a_size_or_a_file_named_like_a_folder_is_refused() {
    let dir = TEntry {
        size_field: Some(b"00000000005".to_vec()),
        emit_data: false,
        ..TEntry::dir("d/")
    };
    assert_eq!(code_of(tar_of(&[dir])), "BAD_HEADER");
    assert_eq!(code_of(tar_of(&[TEntry::file("d/", b"x")])), "BAD_HEADER");
}

#[test]
fn bytes_after_the_end_blocks_are_refused_unless_they_are_zero() {
    let mut bytes = honest();
    bytes.extend(vec![0u8; 8192]);
    judge_bytes(bytes).unwrap();
    let mut bytes = honest();
    bytes.extend(b"hidden second archive");
    assert_eq!(code_of(bytes), "UNCOVERED_BYTES");
}

#[test]
fn duplicates_and_case_collisions_are_refused_in_a_tar_too() {
    assert_eq!(
        code_of(tar_of(&[TEntry::file("a", b"1"), TEntry::file("a", b"2")])),
        "DUPLICATE_NAME"
    );
    assert_eq!(
        code_of(tar_of(&[TEntry::file("a", b"1"), TEntry::file("A", b"2")])),
        "NAME_COLLISION"
    );
}

#[test]
fn a_ustar_prefix_is_joined_to_the_name_and_judged() {
    let ok = TEntry {
        prefix: b"some/dir".to_vec(),
        ..TEntry::file("file.txt", b"x")
    };
    assert_eq!(
        judge_bytes(tar_of(&[ok])).unwrap().entries()[0]
            .path
            .as_str(),
        "some/dir/file.txt"
    );
    let bad = TEntry {
        prefix: b"../..".to_vec(),
        ..TEntry::file("file.txt", b"x")
    };
    assert_eq!(code_of(tar_of(&[bad])), "PARENT_COMPONENT");
}

#[test]
fn the_number_of_entries_is_bounded() {
    let limits = ArchiveLimits {
        max_entries: 2,
        ..ArchiveLimits::default()
    };
    let bytes = tar_of(&[
        TEntry::file("a", b"1"),
        TEntry::file("b", b"1"),
        TEntry::file("c", b"1"),
    ]);
    assert!(matches!(
        judge_with(bytes, &limits),
        Err(ArchiveRefusal::TooManyEntries { max: 2 })
    ));
}

#[test]
fn a_gzip_with_several_members_or_trailing_bytes_is_refused() {
    let mut bytes = gzip(&honest());
    bytes.extend(gzip(&honest()));
    assert_eq!(code_of(bytes), "MULTIPLE_GZIP_MEMBERS");
    let mut bytes = gzip(&honest());
    bytes.extend(b"trailing garbage");
    assert_eq!(code_of(bytes), "MULTIPLE_GZIP_MEMBERS");
}

#[test]
fn a_truncated_gzip_is_refused() {
    let whole = gzip(&honest());
    for cut in [
        whole.len() - 1,
        whole.len() - 4,
        whole.len() - 8,
        whole.len() / 2,
        12,
    ] {
        let refusal = refusal_of(whole[..cut].to_vec());
        assert!(
            matches!(
                refusal,
                ArchiveRefusal::Truncated | ArchiveRefusal::CorruptData
            ),
            "{cut}: {refusal:?}"
        );
    }
}

#[test]
fn a_gzip_with_a_wrong_checksum_or_size_is_refused() {
    let mut bytes = gzip(&honest());
    let end = bytes.len();
    bytes[end - 8] ^= 0xFF;
    assert_eq!(code_of(bytes), "CHECKSUM_MISMATCH");
    let mut bytes = gzip(&honest());
    let end = bytes.len();
    bytes[end - 1] ^= 0x01;
    assert_eq!(code_of(bytes), "CHECKSUM_MISMATCH");
}

#[test]
fn a_gzip_header_with_reserved_flags_or_an_endless_name_is_refused() {
    let mut bytes = gzip(&honest());
    bytes[3] = 0x20;
    assert_eq!(code_of(bytes), "BAD_HEADER");
    // FNAME sans fin : le décodeur ne lit jamais plus de 4 Kio de nom.
    let mut bytes = vec![0x1f, 0x8b, 8, 0x08, 0, 0, 0, 0, 0, 3];
    bytes.extend(vec![b'n'; 20_000]);
    assert_eq!(code_of(bytes), "BAD_HEADER");
}

#[test]
fn a_gzip_bomb_is_stopped_by_the_stream_ratio_while_reading() {
    let tar = tar_of(&[TEntry::file("zeros", &vec![0u8; 16 * MIB as usize])]);
    let bytes = gzip(&tar);
    assert!(bytes.len() < 40_000);
    assert!(matches!(
        refusal_of(bytes),
        ArchiveRefusal::ArchiveRatioTooHigh { ratio: 100 }
    ));
}

#[test]
fn extraction_replays_exactly_what_was_inspected_and_reads_one_file() {
    let limits = ArchiveLimits::default();
    for bytes in [honest(), gzip(&honest())] {
        let mut source = Cursor::new(bytes);
        let inspection = inspect(&mut source, &limits, &never).unwrap();
        let one = read_one(&mut source, &inspection, "mods/a.jar", &limits, &never).unwrap();
        assert_eq!(one, b"jar-a");
        let mut sink = MemSink::default();
        let report = extract(&mut source, &inspection, &mut sink, &limits, &never).unwrap();
        assert_eq!((report.files, report.dirs, report.bytes), (2, 1, 705));
    }
}

#[test]
fn extraction_refuses_a_tar_whose_entries_changed_after_the_inspection() {
    // Même longueur, empreinte différente : refus par l'empreinte, rien n'est écrit.
    let limits = ArchiveLimits::default();
    let mut source = Cursor::new(honest());
    let inspection = inspect(&mut source, &limits, &never).unwrap();
    let mut changed = honest();
    changed[600] ^= 0x55;
    let mut other = Cursor::new(changed);
    let mut sink = MemSink::default();
    assert_eq!(
        extract(&mut other, &inspection, &mut sink, &limits, &never),
        Err(ArchiveRefusal::ArchiveChanged)
    );
    assert!(sink.files.borrow().is_empty() && sink.dirs.borrow().is_empty());
}

#[test]
fn an_extended_header_announcing_a_huge_size_is_refused_without_reading_or_reserving_it() {
    for flag in [b'L', b'x'] {
        let bytes = tar_of(&[TEntry {
            flag,
            size_field: Some(format!("{:011o}", 100 * MIB).into_bytes()),
            emit_data: false,
            ..TEntry::file("././@LongLink", b"")
        }]);
        assert!(refusal_of(bytes).is_about_the_archive(), "{}", flag as char);
    }
}
