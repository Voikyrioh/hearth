//! Pièges de noms (ADR-0055 points 6, 8, 9) : chaque test fabrique son archive et attend un refus typé.

use super::testkit::*;
use super::{ArchiveLimits, ArchiveRefusal, EntryKind};

fn one(name: &[u8]) -> Vec<u8> {
    Zip::of(vec![ZEntry::raw_name(name, b"x")]).build()
}

fn code_for(name: &[u8]) -> &'static str {
    code_of(one(name))
}

#[test]
fn an_honest_archive_is_accepted_with_its_files_and_folders() {
    let bytes = Zip::of(vec![
        ZEntry::dir("mods/"),
        ZEntry::file("mods/a.jar", b"aaaa"),
        ZEntry::file("config/b.toml", b"b").deflated(),
    ])
    .build();
    let inspection = judge_bytes(bytes).unwrap();
    let names: Vec<_> = inspection
        .entries()
        .iter()
        .map(|e| (e.path.as_str().to_string(), e.kind, e.size))
        .collect();
    assert_eq!(
        names,
        vec![
            ("mods".to_string(), EntryKind::Dir, 0),
            ("mods/a.jar".to_string(), EntryKind::File, 4),
            ("config/b.toml".to_string(), EntryKind::File, 1),
        ]
    );
    assert_eq!(inspection.file_count(), 2);
}

#[test]
fn an_absolute_path_is_refused() {
    assert_eq!(code_for(b"/etc/passwd"), "ABSOLUTE_PATH");
}

#[test]
fn a_parent_component_is_refused_wherever_it_stands() {
    for name in [&b"../x"[..], b"a/../b", b"a/..", b"./a", b"a/./b"] {
        assert_eq!(code_for(name), "PARENT_COMPONENT", "{name:?}");
    }
}

#[test]
fn a_backslash_is_refused_even_when_it_spells_a_parent() {
    assert_eq!(code_for(b"a\\b"), "BACKSLASH");
    assert_eq!(code_for(b"..\\x"), "BACKSLASH");
}

#[test]
fn a_drive_letter_or_a_stream_name_is_refused() {
    assert_eq!(code_for(b"C:/Windows/x"), "DRIVE_LETTER");
    assert_eq!(code_for(b"a/b:stream"), "DRIVE_LETTER");
}

#[test]
fn an_empty_component_is_refused() {
    assert_eq!(code_for(b"a//b"), "EMPTY_COMPONENT");
}

#[test]
fn windows_reserved_names_and_trailing_dots_are_refused() {
    for name in [
        &b"con"[..],
        b"a/NUL.txt",
        b"COM1",
        b"lpt9.log",
        b"aux .txt",
        b"a.",
        b"a ",
        b"dir./f",
    ] {
        assert_eq!(code_for(name), "RESERVED_NAME", "{name:?}");
    }
    // `com0` et `console` ne sont pas réservés.
    judge_bytes(one(b"com0")).unwrap();
    judge_bytes(one(b"console")).unwrap();
}

#[test]
fn a_nul_byte_or_a_control_character_is_refused() {
    assert_eq!(code_for(b"a\0b"), "CONTROL_CHARACTER");
    assert_eq!(code_for(b"a\nb"), "CONTROL_CHARACTER");
    assert_eq!(code_for("a\u{85}b".as_bytes()), "CONTROL_CHARACTER");
}

#[test]
fn a_name_that_is_not_utf8_is_refused_with_the_sentence_of_the_adr_and_never_guessed() {
    let refusal = refusal_of(one(&[b'a', 0xE9, b'.', b't']));
    assert_eq!(refusal, ArchiveRefusal::NonUtf8Name);
    assert_eq!(
        refusal.to_string(),
        "Ce fichier contient des noms que Hearth ne sait pas lire. Recrée l'archive avec un outil récent."
    );
}

#[test]
fn a_name_longer_than_1024_bytes_is_refused_and_1024_is_accepted() {
    let long = vec![b'a'; 1025];
    assert_eq!(code_for(&long), "NAME_TOO_LONG");
    // 1024 octets en composants de 100 : accepté.
    let ok = vec!["a".repeat(100); 10].join("/");
    assert!(ok.len() <= 1024);
    judge_bytes(one(ok.as_bytes())).unwrap();
    let component = vec![b'a'; 256];
    assert_eq!(code_for(&component), "NAME_TOO_LONG");
}

#[test]
fn a_path_deeper_than_32_levels_is_refused_and_32_is_accepted() {
    let deep33 = vec!["d"; 33].join("/");
    assert_eq!(code_for(deep33.as_bytes()), "TOO_DEEP");
    let deep32 = vec!["d"; 32].join("/");
    judge_bytes(one(deep32.as_bytes())).unwrap();
}

#[test]
fn two_entries_with_the_same_name_are_refused_and_neither_wins() {
    let bytes = Zip::of(vec![
        ZEntry::file("a.txt", b"1"),
        ZEntry::file("a.txt", b"2"),
    ])
    .build();
    assert_eq!(code_of(bytes), "DUPLICATE_NAME");
    let bytes = Zip::of(vec![ZEntry::dir("d/"), ZEntry::dir("d/")]).build();
    assert_eq!(code_of(bytes), "DUPLICATE_NAME");
}

#[test]
fn names_that_differ_only_by_case_are_refused() {
    let bytes = Zip::of(vec![
        ZEntry::file("Mods.jar", b"1"),
        ZEntry::file("mods.jar", b"2"),
    ])
    .build();
    assert_eq!(code_of(bytes), "NAME_COLLISION");
    // Le dossier parent suffit : deux dossiers qui se confondraient.
    let bytes = Zip::of(vec![
        ZEntry::file("Mods/a", b"1"),
        ZEntry::file("mods/b", b"2"),
    ])
    .build();
    assert_eq!(code_of(bytes), "NAME_COLLISION");
}

#[test]
fn names_that_differ_only_by_unicode_normalization_are_refused() {
    let composed = "caf\u{e9}.txt";
    let decomposed = "cafe\u{301}.txt";
    let bytes = Zip::of(vec![
        ZEntry::file(composed, b"1"),
        ZEntry::file(decomposed, b"2"),
    ])
    .build();
    assert_eq!(code_of(bytes), "NAME_COLLISION");
}

#[test]
fn a_file_and_a_folder_of_the_same_name_are_refused() {
    let bytes = Zip::of(vec![ZEntry::file("a", b"1"), ZEntry::dir("a/")]).build();
    assert_eq!(code_of(bytes), "FILE_DIR_CONFLICT");
    let bytes = Zip::of(vec![ZEntry::file("a", b"1"), ZEntry::file("a/b", b"2")]).build();
    assert_eq!(code_of(bytes), "FILE_DIR_CONFLICT");
    let bytes = Zip::of(vec![ZEntry::file("a/b", b"2"), ZEntry::file("a", b"1")]).build();
    assert_eq!(code_of(bytes), "FILE_DIR_CONFLICT");
}

#[test]
fn a_folder_listed_after_its_children_is_fine_once() {
    let bytes = Zip::of(vec![ZEntry::file("a/b", b"2"), ZEntry::dir("a/")]).build();
    judge_bytes(bytes).unwrap();
}

#[test]
fn the_number_of_distinct_names_is_bounded() {
    let limits = ArchiveLimits {
        max_tree_nodes: 3,
        ..ArchiveLimits::default()
    };
    let bytes = Zip::of(vec![
        ZEntry::file("a/b/c", b"1"),
        ZEntry::file("a/b/d", b"1"),
    ])
    .build();
    assert!(matches!(
        judge_with(bytes, &limits),
        Err(ArchiveRefusal::TooManyEntries { .. })
    ));
}

#[test]
fn a_refusal_never_quotes_the_entry_name_or_a_path() {
    let secret = "secret-world-name";
    for refusal in [
        refusal_of(one(format!("{secret}/../x").as_bytes())),
        refusal_of(one(format!("/{secret}").as_bytes())),
        refusal_of(one(format!("{secret}\\x").as_bytes())),
        refusal_of(
            Zip::of(vec![
                ZEntry::file(secret, b"1"),
                ZEntry::file(&secret.to_uppercase(), b"1"),
            ])
            .build(),
        ),
    ] {
        let text = format!("{refusal} {refusal:?} {}", refusal.code());
        assert!(!text.to_lowercase().contains(secret), "{text}");
        assert!(refusal.is_about_the_archive());
    }
}
