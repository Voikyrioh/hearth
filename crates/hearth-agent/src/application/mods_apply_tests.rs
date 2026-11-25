//! Le journal d'un lot de mods et son retour (HRT-259) : `moves_of` lit les mouvements, `revert_blocking` les rejoue à
//! l'envers sur le faux dossier de données des tests (le même que celui des suites HTTPS).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use time::OffsetDateTime;

use super::*;
use crate::domain::ops::StepName;

#[path = "../../tests/support/fake_safe_fs.rs"]
mod fake_safe_fs;
use fake_safe_fs::FakeSafeFs;

fn mv(kind: MoveKind, from: &str, to: &str, ino: u64) -> FileMove {
    FileMove {
        kind,
        from: from.to_owned(),
        to: to.to_owned(),
        dev: 0,
        ino,
    }
}

fn row(seq: u32, state: StepState, moves: Vec<FileMove>, name: &str) -> StepRecord {
    StepRecord {
        seq,
        step: StepName::parse(name).unwrap(),
        state,
        undo: UndoInfo {
            created: Vec::new(),
            moves,
        },
        at: OffsetDateTime::UNIX_EPOCH,
    }
}

fn place(seq: u32, state: StepState, moves: Vec<FileMove>) -> StepRecord {
    row(seq, state, moves, step::PLACE)
}

const A: (&str, &str) = ("data/mods/a.jar", "data/mods-retires/a.jar");
const B: (&str, &str) = ("data/mods/b.jar", "data/mods-retires/b.jar");

fn retire(pair: (&str, &str), ino: u64) -> FileMove {
    mv(MoveKind::Retired, pair.0, pair.1, ino)
}

#[test]
fn a_done_line_replaces_the_intent_of_the_same_move() {
    let steps = [
        place(1, StepState::Intent, vec![retire(A, 0)]),
        place(2, StepState::Done, vec![retire(A, 7)]),
    ];
    assert_eq!(moves_of(&steps), vec![retire(A, 7)]);
}

#[test]
fn an_undone_line_with_a_move_removes_it_and_keeps_the_others() {
    let steps = [
        place(1, StepState::Done, vec![retire(A, 7)]),
        place(2, StepState::Done, vec![retire(B, 8)]),
        place(3, StepState::Undone, vec![retire(B, 8)]),
    ];
    assert_eq!(moves_of(&steps), vec![retire(A, 7)]);
}

#[test]
fn an_undone_line_without_a_move_closes_the_step() {
    let steps = [
        place(1, StepState::Done, vec![retire(A, 7)]),
        place(2, StepState::Done, vec![retire(B, 8)]),
        place(3, StepState::Undone, Vec::new()),
    ];
    assert!(moves_of(&steps).is_empty());
}

#[test]
fn an_orphan_intent_is_kept() {
    let steps = [
        place(1, StepState::Done, vec![retire(A, 7)]),
        place(2, StepState::Intent, vec![retire(B, 0)]),
    ];
    assert_eq!(moves_of(&steps), vec![retire(A, 7), retire(B, 0)]);
}

#[test]
fn a_failed_line_and_other_steps_are_ignored() {
    let steps = [
        place(1, StepState::Done, vec![retire(A, 7)]),
        place(2, StepState::Failed, vec![retire(B, 8)]),
        row(3, StepState::Done, vec![retire(B, 9)], step::COPY),
    ];
    assert_eq!(moves_of(&steps), vec![retire(A, 7)]);
}

fn key() -> ServiceKey {
    ServiceKey::parse("c_abc123").unwrap()
}

fn rel(text: &str) -> RelPath {
    RelPath::parse(text).unwrap()
}

fn bench() -> (FakeSafeFs, RelPath) {
    let fs = FakeSafeFs::new();
    fs.seed_dir(&key(), "data/mods");
    fs.seed_dir(&key(), "data/mods-retires");
    (fs, rel("data/mods-retires"))
}

#[test]
fn a_placed_file_with_only_its_intent_is_never_adopted() {
    let (fs, retired) = bench();
    fs.seed_file(&key(), "data/mods/x.jar", b"the admin's own file");
    let intent = mv(MoveKind::Placed, "", "data/mods/x.jar", 0);
    let outcome = revert_blocking(&fs, &key(), &retired, &[intent]);
    assert!(!outcome.all);
    assert!(outcome.moves.is_empty());
    assert_eq!(
        fs.content(&key(), "data/mods/x.jar"),
        Some(b"the admin's own file".to_vec())
    );
    assert!(fs.renames().is_empty(), "rien n'est déplacé");
}

#[test]
fn a_move_with_only_its_intent_and_its_source_still_there_is_not_adopted() {
    let (fs, retired) = bench();
    fs.seed_file(&key(), "data/mods/a.jar", b"source");
    fs.seed_file(&key(), "data/mods-retires/a.jar", b"someone else's file");
    let intent = retire(A, 0);
    let outcome = revert_blocking(&fs, &key(), &retired, &[intent]);
    assert!(!outcome.all);
    assert!(outcome.moves.is_empty());
    assert!(fs.renames().is_empty());
    assert!(fs.content(&key(), "data/mods/a.jar").is_some());
    assert!(fs.content(&key(), "data/mods-retires/a.jar").is_some());
}

#[test]
fn a_move_with_only_its_intent_is_adopted_once_and_a_replay_renames_nothing() {
    let (fs, retired) = bench();
    // Le geste a eu lieu : la source a disparu, la cible porte le fichier.
    fs.seed_file(&key(), "data/mods-retires/a.jar", b"the retired jar");
    let intent = retire(A, 0);
    let first = revert_blocking(&fs, &key(), &retired, std::slice::from_ref(&intent));
    assert!(first.all);
    assert_eq!(
        fs.content(&key(), "data/mods/a.jar"),
        Some(b"the retired jar".to_vec())
    );
    assert_eq!(fs.renames().len(), 1);
    // Rejoué : la cible n'existe plus, le mouvement compte comme revenu, aucun renommage de plus.
    let second = revert_blocking(&fs, &key(), &retired, &[intent]);
    assert!(second.all);
    assert_eq!(fs.renames().len(), 1, "aucun renommage au second passage");
    assert_eq!(
        fs.content(&key(), "data/mods/a.jar"),
        Some(b"the retired jar".to_vec())
    );
}
