use hearth_proto::api::transfers::{CHUNK_BYTES, chunk_digest, digest_of};

use super::*;

const MIB: u64 = 1 << 20;

fn digest(n: u8) -> [u8; 32] {
    digest_of(&[n])
}

#[test]
fn an_identifier_is_an_upper_case_ulid_and_names_its_files() {
    let id = TransferId::parse("01J9ZY0G3Q8M2K6W4T7V5N1B9D").expect("ulid");
    assert_eq!(id.part_name(), "01J9ZY0G3Q8M2K6W4T7V5N1B9D.part");
    assert_eq!(id.digests_name(), "01J9ZY0G3Q8M2K6W4T7V5N1B9D.dig");
    assert_eq!(id.complete_name(), "01J9ZY0G3Q8M2K6W4T7V5N1B9D.bin");
    for bad in [
        "",
        "01j9zy0g3q8m2k6w4t7v5n1b9d",
        "01J9ZY0G3Q8M2K6W4T7V5N1B9",
        "01J9ZY0G3Q8M2K6W4T7V5N1B9DD",
        "../J9ZY0G3Q8M2K6W4T7V5N1B9D",
        "01J9ZY0G3Q8M2K6W4T7V5N1B9I",
        "01J9ZY0G3Q8M2K6W4T7V5N1B/D",
    ] {
        assert!(TransferId::parse(bad).is_none(), "{bad:?}");
    }
}

#[test]
fn a_name_is_one_to_255_bytes_without_separator_nul_or_control() {
    assert!(judge_name("pack.zip"));
    assert!(judge_name("été 2026.mrpack"));
    assert!(judge_name(&"a".repeat(255)));
    for bad in ["", ".", "..", "a/b", "a\\b", "a\0b", "a\nb"] {
        assert!(!judge_name(bad), "{bad:?}");
    }
    assert!(!judge_name(&"a".repeat(256)));
}

#[test]
fn space_must_leave_the_larger_of_one_gib_and_five_percent() {
    let gib = 1u64 << 30;
    // Petit disque : la réserve est de 1 Gio.
    assert_eq!(reserve(10 * gib), gib);
    // Gros disque : 5 %.
    assert_eq!(reserve(1000 * gib), 50 * gib);
    assert_eq!(judge_space(10 * gib, 100 * gib, 5 * gib), Ok(()));
    // Il resterait 4 Gio de libre sur un disque de 100 Gio dont la réserve est de 5 Gio : il manque 1 Gio.
    assert_eq!(
        judge_space(10 * gib, 100 * gib, 6 * gib),
        Err(Shortfall {
            missing: gib,
            free_after: 4 * gib
        })
    );
    // Plus grand que le libre : le reste est nul, jamais négatif.
    let short = judge_space(gib, 100 * gib, 50 * gib).unwrap_err();
    assert_eq!(short.free_after, 0);
    assert_eq!(short.missing, 54 * gib);
    // Pas de dépassement près des bornes.
    assert!(judge_space(u64::MAX, u64::MAX, u64::MAX).is_err());
}

#[test]
fn limits_are_four_for_the_agent_two_per_account_and_eight_open() {
    assert_eq!(admit(0, 0, 0), Ok(()));
    assert_eq!(admit(3, 1, 7), Ok(()));
    assert_eq!(admit(4, 0, 0), Err(Limit::Total));
    assert_eq!(admit(3, 2, 0), Err(Limit::Account));
    assert_eq!(admit(3, 1, 8), Err(Limit::Open));
}

#[test]
fn an_empty_file_is_never_an_upload() {
    assert!(Receiver::new(0).is_none());
    // Plus d'empreintes que de morceaux : une liste altérée.
    assert!(Receiver::resume(10, vec![digest(1), digest(2)]).is_none());
}

#[test]
fn a_chunk_is_accepted_only_if_it_is_the_next_one_of_the_right_size_and_digest() {
    let size = u64::from(CHUNK_BYTES) + 10;
    let mut receiver = Receiver::new(size).unwrap();
    let full = CHUNK_BYTES as usize;
    let d0 = digest(0);
    // Mauvais rang.
    assert_eq!(
        receiver.judge(1, full, &d0, &d0),
        Err(ChunkRefusal::Sequence { expected: 0 })
    );
    // Mauvaise taille (trop courte, trop longue).
    assert_eq!(
        receiver.judge(0, full - 1, &d0, &d0),
        Err(ChunkRefusal::Sequence { expected: 0 })
    );
    assert_eq!(
        receiver.judge(0, full + 1, &d0, &d0),
        Err(ChunkRefusal::Sequence { expected: 0 })
    );
    // Mauvaise empreinte : le morceau n'est pas écrit.
    assert_eq!(
        receiver.judge(0, full, &digest(9), &d0),
        Err(ChunkRefusal::Corrupt { expected: 0 })
    );
    assert_eq!(receiver.judge(0, full, &d0, &d0), Ok(()));
    receiver.accept(d0);
    assert_eq!(receiver.received_bytes(), u64::from(CHUNK_BYTES));
    assert_eq!(receiver.remaining_bytes(), 10);
    // Le dernier morceau est court : 10 octets, pas un morceau plein.
    assert_eq!(
        receiver.judge(1, full, &digest(1), &digest(1)),
        Err(ChunkRefusal::Sequence { expected: 1 })
    );
    assert_eq!(receiver.judge(1, 10, &digest(1), &digest(1)), Ok(()));
    receiver.accept(digest(1));
    assert!(receiver.is_complete());
    assert_eq!(receiver.received_bytes(), size);
    // Un morceau de plus après la fin.
    assert_eq!(
        receiver.judge(2, 10, &digest(2), &digest(2)),
        Err(ChunkRefusal::Sequence { expected: 2 })
    );
}

#[test]
fn the_end_needs_every_chunk_and_the_same_tree_digest() {
    let size = 2 * u64::from(CHUNK_BYTES);
    let mut receiver = Receiver::new(size).unwrap();
    let claimed = chunk_digest(&[digest(0), digest(1)]);
    assert_eq!(receiver.finish(&claimed), Err(FinishRefusal::Incomplete));
    receiver.accept(digest(0));
    assert_eq!(receiver.finish(&claimed), Err(FinishRefusal::Incomplete));
    receiver.accept(digest(1));
    assert_eq!(receiver.finish(&claimed), Ok(claimed));
    assert_eq!(receiver.tree_digest(), Some(claimed));
    assert_eq!(
        receiver.finish(&chunk_digest(&[digest(1), digest(0)])),
        Err(FinishRefusal::Mismatch)
    );
}

#[test]
fn resuming_from_the_kept_list_gives_the_same_answer_as_never_stopping() {
    let size = 3 * u64::from(CHUNK_BYTES) - MIB;
    let digests = [digest(0), digest(1), digest(2)];
    let mut straight = Receiver::new(size).unwrap();
    for d in digests {
        straight.accept(d);
    }
    let mut resumed = Receiver::resume(size, digests[..2].to_vec()).unwrap();
    assert_eq!(resumed.next_index(), 2);
    assert_eq!(resumed.received_bytes(), 2 * u64::from(CHUNK_BYTES));
    resumed.accept(digests[2]);
    assert_eq!(resumed.tree_digest(), straight.tree_digest());
}
