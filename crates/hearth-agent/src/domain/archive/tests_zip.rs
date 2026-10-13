//! Pièges du `.zip` : liens et fichiers spéciaux, chiffrement, méthodes, volumes, bombes, index qui
//! ment (ADR-0055 points 3 à 5 et 10). Chaque archive est fabriquée par le test.

use std::io::Cursor;

use super::testkit::*;
use super::{ArchiveLimits, ArchiveRefusal, MIB, inspect, read_one};

fn plain() -> Zip {
    Zip::of(vec![
        ZEntry::file("a.txt", b"hello a"),
        ZEntry::file("b/c.txt", b"hello c").deflated(),
    ])
}

#[test]
fn stored_deflate_and_zip64_archives_are_accepted() {
    judge_bytes(plain().build()).unwrap();
    let zip64 = Zip {
        zip64: true,
        ..plain()
    };
    let inspection = judge_bytes(zip64.build()).unwrap();
    assert_eq!(inspection.file_count(), 2);
    assert_eq!(
        judge_bytes(Zip::default().build()).unwrap().entries().len(),
        0
    );
}

#[test]
fn a_symbolic_link_is_refused() {
    let bytes = Zip::of(vec![ZEntry::file("link", b"/etc").mode(0o120_777)]).build();
    assert_eq!(code_of(bytes), "SYMBOLIC_LINK");
}

#[test]
fn a_windows_reparse_point_is_refused_as_a_link() {
    let mut entry = ZEntry::file("junction", b"");
    entry.made_by = 0x0014;
    entry.external = 0x0400;
    assert_eq!(code_of(Zip::of(vec![entry]).build()), "SYMBOLIC_LINK");
}

#[test]
fn devices_pipes_and_sockets_are_refused() {
    for mode in [0o020_666, 0o060_660, 0o010_644, 0o140_755] {
        let bytes = Zip::of(vec![ZEntry::file("dev", b"").mode(mode)]).build();
        assert_eq!(code_of(bytes), "SPECIAL_FILE", "{mode:o}");
    }
}

#[test]
fn an_encrypted_entry_is_refused() {
    let bytes = Zip::of(vec![ZEntry {
        flags: 1,
        ..ZEntry::file("secret", b"x")
    }])
    .build();
    assert_eq!(code_of(bytes), "ENCRYPTED");
    let strong = Zip::of(vec![ZEntry {
        flags: 0x40,
        ..ZEntry::file("secret", b"x")
    }])
    .build();
    assert_eq!(code_of(strong), "ENCRYPTED");
    let aes = Zip::of(vec![ZEntry {
        central_method: Some(99),
        ..ZEntry::file("secret", b"x")
    }])
    .build();
    assert_eq!(code_of(aes), "ENCRYPTED");
}

#[test]
fn an_unsupported_compression_method_is_refused_clearly() {
    for method in [1u16, 6, 9, 12, 14, 93, 95] {
        let bytes = Zip::of(vec![ZEntry {
            central_method: Some(method),
            local_method: Some(method),
            ..ZEntry::file("a", b"x")
        }])
        .build();
        assert_eq!(code_of(bytes), "UNSUPPORTED_METHOD", "{method}");
    }
}

#[test]
fn a_multi_volume_archive_is_refused() {
    let bytes = Zip {
        eocd_disk: 1,
        ..plain()
    }
    .build();
    assert_eq!(code_of(bytes), "MULTI_VOLUME");
}

#[test]
fn an_index_announcing_more_entries_than_it_can_hold_is_refused_before_any_reservation() {
    let bytes = Zip {
        count_lie: Some(60_000),
        ..plain()
    }
    .build();
    assert_eq!(code_of(bytes), "INDEX_LIES");
    let bytes = Zip {
        count_lie: Some(1),
        ..plain()
    }
    .build();
    // Un enregistrement de trop dans l'index : l'index n'est pas consommé d'un bout à l'autre.
    assert_eq!(code_of(bytes), "INDEX_LIES");
}

#[test]
fn the_entry_count_the_index_size_and_the_name_length_are_bounded() {
    let limits = ArchiveLimits {
        max_entries: 1,
        ..ArchiveLimits::default()
    };
    assert!(matches!(
        judge_with(plain().build(), &limits),
        Err(ArchiveRefusal::TooManyEntries { max: 1 })
    ));
    let bytes = Zip {
        index_size_lie: Some(100 * 1024 * 1024),
        ..plain()
    }
    .build();
    assert_eq!(code_of(bytes), "INDEX_TOO_LARGE");
    let limits = ArchiveLimits {
        max_index_bytes: 50,
        ..ArchiveLimits::default()
    };
    assert!(matches!(
        judge_with(plain().build(), &limits),
        Err(ArchiveRefusal::IndexTooLarge { max: 50 })
    ));
}

#[test]
fn a_lying_index_size_leaves_bytes_nobody_covers() {
    // Trop petit pour contenir ses enregistrements : l'index ment sur lui-même.
    let bytes = Zip {
        index_size_lie: Some(10),
        ..plain()
    }
    .build();
    assert_eq!(code_of(bytes), "INDEX_LIES");
    // Assez grand pour les contenir, mais il ne s'arrête pas là où l'enregistrement de fin commence.
    let bytes = Zip {
        index_size_lie: Some(5000),
        ..plain()
    }
    .build();
    assert_eq!(code_of(bytes), "UNCOVERED_BYTES");
}

#[test]
fn a_file_over_the_size_limit_is_refused_by_its_real_length() {
    let limits = ArchiveLimits {
        max_file_size: 64,
        ..ArchiveLimits::default()
    };
    assert!(matches!(
        judge_with(plain().build(), &limits),
        Err(ArchiveRefusal::FileTooLarge { max: 64 })
    ));
}

#[test]
fn a_local_header_that_disagrees_with_the_index_refuses_the_whole_archive() {
    let name = Zip::of(vec![ZEntry {
        local_name: Some(b"b.txt".to_vec()),
        ..ZEntry::file("a.txt", b"hello a")
    }])
    .build();
    assert_eq!(code_of(name), "LOCAL_HEADER_MISMATCH");

    let method = Zip::of(vec![ZEntry {
        local_method: Some(8),
        ..ZEntry::file("a.txt", b"hello a")
    }])
    .build();
    assert_eq!(code_of(method), "LOCAL_HEADER_MISMATCH");

    let size = Zip::of(vec![ZEntry {
        central_usize: Some(3),
        ..ZEntry::file("a.txt", b"hello a").deflated()
    }])
    .build();
    assert_eq!(code_of(size), "LOCAL_HEADER_MISMATCH");

    let descriptor = Zip::of(vec![ZEntry {
        descriptor: true,
        descriptor_crc: Some(1),
        ..ZEntry::file("a.txt", b"hello a")
    }])
    .build();
    assert_eq!(code_of(descriptor), "LOCAL_HEADER_MISMATCH");
}

#[test]
fn a_data_descriptor_that_confirms_the_index_is_accepted() {
    let bytes = Zip::of(vec![ZEntry {
        descriptor: true,
        ..ZEntry::file("a.txt", b"hello a").deflated()
    }])
    .build();
    judge_bytes(bytes).unwrap();
}

#[test]
fn two_end_records_are_refused() {
    let bytes = Zip {
        second_end_record: true,
        ..plain()
    }
    .build();
    assert_eq!(code_of(bytes), "TWO_END_RECORDS");
}

#[test]
fn prefixed_data_and_hidden_gaps_are_refused() {
    let bytes = Zip {
        prefix: b"MZ-not-a-zip-but-an-installer".to_vec(),
        ..plain()
    }
    .build();
    assert_eq!(code_of(bytes), "UNCOVERED_BYTES");
    let bytes = Zip {
        gap_before_index: b"hidden payload".to_vec(),
        ..plain()
    }
    .build();
    assert_eq!(code_of(bytes), "UNCOVERED_BYTES");
}

#[test]
fn two_concatenated_archives_are_refused() {
    let mut bytes = plain().build();
    bytes.extend(plain().build());
    assert_eq!(code_of(bytes), "TWO_END_RECORDS");
}

#[test]
fn an_entry_pointing_outside_the_file_is_refused() {
    let bytes = Zip::of(vec![
        ZEntry::file("a", b"1"),
        ZEntry {
            offset_override: Some(0x00FF_0000),
            ..ZEntry::file("b", b"2")
        },
    ])
    .build();
    assert_eq!(code_of(bytes), "ENTRY_OUTSIDE_FILE");
}

#[test]
fn entries_that_share_or_overlap_their_bytes_are_refused() {
    let same_place = Zip::of(vec![
        ZEntry::file("a", b"1"),
        ZEntry {
            offset_override: Some(0),
            ..ZEntry::file("b", b"2")
        },
    ])
    .build();
    assert_eq!(code_of(same_place), "OVERLAPPING_ENTRIES");

    let reaches_into_next = Zip::of(vec![
        ZEntry {
            lie_size: Some(40),
            lie_compressed: Some(40),
            ..ZEntry::file("a", b"1234567890")
        },
        ZEntry::file("b", b"2"),
    ])
    .build();
    assert_eq!(code_of(reaches_into_next), "OVERLAPPING_ENTRIES");
}

#[test]
fn a_one_level_bomb_is_refused_by_ratio_before_any_decompression() {
    let bytes = Zip::of(vec![
        ZEntry::file("zeros", &vec![0u8; 3 * MIB as usize]).deflated(),
    ])
    .build();
    assert!(bytes.len() < 10_000);
    assert!(matches!(
        refusal_of(bytes),
        ArchiveRefusal::EntryRatioTooHigh { ratio: 200 }
    ));
}

#[test]
fn many_moderately_compressed_entries_trip_the_archive_ratio() {
    let limits = ArchiveLimits {
        max_entry_ratio: 1_000_000,
        ..ArchiveLimits::default()
    };
    let bytes = Zip::of(vec![
        ZEntry::file("zeros", &vec![0u8; 3 * MIB as usize]).deflated(),
    ])
    .build();
    assert!(matches!(
        judge_with(bytes, &limits),
        Err(ArchiveRefusal::ArchiveRatioTooHigh { ratio: 100 })
    ));
}

#[test]
fn the_total_unpacked_size_is_bounded() {
    let limits = ArchiveLimits {
        max_unpacked: 10,
        ..ArchiveLimits::default()
    };
    assert!(matches!(
        judge_with(plain().build(), &limits),
        Err(ArchiveRefusal::TotalSizeTooLarge { max: 10 })
    ));
    let free = ArchiveLimits::for_free_space(100, 90);
    assert_eq!(free.max_unpacked, 10);
}

#[test]
fn announced_sizes_are_checked_against_what_the_data_really_inflates_to() {
    // Une entrée qui annonce 10 octets mais se déballe en 5 Mio.
    let lying = ZEntry {
        lie_size: Some(10),
        ..ZEntry::file("liar", &vec![0u8; 5 * MIB as usize]).deflated()
    };
    assert_eq!(code_of(Zip::of(vec![lying]).build()), "SIZE_MISMATCH");
    // Une entrée qui annonce plus qu'elle ne donne.
    let short = ZEntry {
        lie_size: Some(100),
        ..ZEntry::file("short", b"abc").deflated()
    };
    assert_eq!(code_of(Zip::of(vec![short]).build()), "SIZE_MISMATCH");
}

#[test]
fn a_wrong_checksum_is_refused() {
    let bytes = Zip::of(vec![ZEntry {
        lie_crc: Some(0xDEAD_BEEF),
        ..ZEntry::file("a", b"data")
    }])
    .build();
    assert_eq!(code_of(bytes), "CHECKSUM_MISMATCH");
}

#[test]
fn a_corrupt_deflate_stream_is_refused() {
    let bytes = Zip::of(vec![ZEntry {
        deflate: true,
        raw_payload: Some(vec![0xFF; 8]),
        lie_compressed: None,
        ..ZEntry::file("a", b"data")
    }])
    .build();
    let code = code_of(bytes);
    assert!(
        matches!(code, "CORRUPT_DATA" | "TRUNCATED" | "SIZE_MISMATCH"),
        "{code}"
    );
}

#[test]
fn a_giant_comment_is_ignored_but_a_lying_comment_length_is_refused() {
    let big = Zip {
        comment: vec![b'c'; 65_535],
        ..plain()
    };
    judge_bytes(big.build()).unwrap();
    let lie = Zip {
        comment: b"abc".to_vec(),
        comment_len_lie: Some(1),
        ..plain()
    };
    assert_eq!(code_of(lie.build()), "INDEX_LIES");
}

#[test]
fn a_comment_that_hides_a_second_end_record_is_refused() {
    let mut comment = b"hi".to_vec();
    comment.extend(b"PK\x05\x06");
    comment.extend(vec![0u8; 18]);
    let bytes = Zip { comment, ..plain() }.build();
    assert_eq!(code_of(bytes), "TWO_END_RECORDS");
}

#[test]
fn a_file_that_is_not_an_archive_is_not_recognized() {
    assert_eq!(
        code_of(b"just some text, not an archive".to_vec()),
        "UNRECOGNIZED_FORMAT"
    );
    assert_eq!(code_of(vec![]), "UNRECOGNIZED_FORMAT");
    assert_eq!(code_of(vec![0u8; 4096]), "UNRECOGNIZED_FORMAT");
}

#[test]
fn an_archive_inside_an_archive_is_stored_as_is_and_never_opened() {
    let inner = Zip::of(vec![ZEntry::file("evil", b"x").mode(0o120_777)]).build();
    let outer = Zip::of(vec![ZEntry::file("pack/inner.zip", &inner)]).build();
    let sink = unpack(outer).unwrap();
    assert_eq!(sink.files.borrow().get("pack/inner.zip"), Some(&inner));
    assert_eq!(sink.files.borrow().len(), 1);
}

#[test]
fn reading_one_file_returns_it_after_the_whole_archive_was_judged() {
    let manifest = br#"{"name":"pack"}"#;
    let bytes = Zip::of(vec![
        ZEntry::file("modrinth.index.json", manifest).deflated(),
        ZEntry::file("overrides/x", b"1"),
    ])
    .build();
    let limits = ArchiveLimits::default();
    let mut source = Cursor::new(bytes);
    let inspection = inspect(&mut source, &limits, &never).unwrap();
    let read = read_one(
        &mut source,
        &inspection,
        "modrinth.index.json",
        &limits,
        &never,
    )
    .unwrap();
    assert_eq!(read, manifest);
    assert_eq!(
        read_one(&mut source, &inspection, "missing", &limits, &never),
        Err(ArchiveRefusal::EntryNotFound)
    );
}

#[test]
fn reading_one_file_gives_at_most_one_mebibyte() {
    let big = vec![b'x'; MIB as usize + 1];
    let ok = vec![b'y'; MIB as usize];
    let bytes = Zip::of(vec![
        ZEntry::file("big.json", &big),
        ZEntry::file("ok.json", &ok).deflated(),
    ])
    .build();
    let limits = ArchiveLimits::default();
    let mut source = Cursor::new(bytes);
    let inspection = inspect(&mut source, &limits, &never).unwrap();
    assert_eq!(
        read_one(&mut source, &inspection, "big.json", &limits, &never),
        Err(ArchiveRefusal::EntryTooLargeToRead { max: MIB })
    );
    assert_eq!(
        read_one(&mut source, &inspection, "ok.json", &limits, &never)
            .unwrap()
            .len(),
        MIB as usize
    );
}

#[test]
fn reading_one_file_refuses_if_the_archive_was_never_acceptable() {
    // Pas d'inspection possible : un refus de toute l'archive précède toute lecture d'un fichier.
    let bytes = Zip::of(vec![
        ZEntry::file("modrinth.index.json", b"{}"),
        ZEntry::file("link", b"x").mode(0o120_777),
    ])
    .build();
    assert_eq!(code_of(bytes), "SYMBOLIC_LINK");
}

#[test]
fn an_index_claiming_sixty_mebibytes_in_a_tiny_file_is_refused_without_reserving_them() {
    let bytes = Zip {
        index_size_lie: Some(60 * 1024 * 1024),
        count_lie: Some(60_000),
        ..plain()
    }
    .build();
    assert!(bytes.len() < 1000);
    assert_eq!(code_of(bytes), "UNCOVERED_BYTES");
}
