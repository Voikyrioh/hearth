---
id: BR-ARCH-007
domaine: ARCH
titre: Rien n'est écrit avant que toute l'archive soit jugée ; l'extraction refuse si le fichier a changé
statut: active
invariant: true
source: ADR-0055 (contexts/hearth/conceptions/2026-10-13-adr-lecture-d-une-archive-brouillon.md), contexts/hearth/tickets/hrt/HRT-130.md
maj: 2026-10-14
---

# BR-ARCH-007 — Rien n'est écrit avant que toute l'archive soit jugée ; l'extraction refuse si le fichier a changé

## Règle
`inspect` juge toute l'archive et n'écrit rien. `extract` recalcule le SHA-256 du fichier d'archive (et sa longueur) et refuse (`ARCHIVE_CHANGED`) s'il diffère de celui de l'inspection, avant d'écrire quoi que ce soit ; un tar est ensuite rejoué et chaque entrée doit être celle de l'inspection. Une annulation est consultée à chaque entrée et à chaque tampon de 64 Kio. Le fichier d'archive est ouvert sans suivre de lien ni bloquer sur un tube nommé (`O_NOFOLLOW | O_NONBLOCK`) et doit être un fichier ordinaire (`fstat` sur le descripteur ouvert), sinon `SOURCE_UNREADABLE` ; il en va de même pour `inspect_file`, `read_file_entry` et `extract_file`.

## Application (code)
- `crates/hearth-agent/src/domain/archive/mod.rs::inspect`
- `crates/hearth-agent/src/domain/archive/mod.rs::extract`
- `crates/hearth-agent/src/domain/archive/mod.rs::replay_tar`
- `crates/hearth-agent/src/domain/archive/flow.rs::digest`
- `crates/hearth-agent/src/infrastructure/archive_fs.rs::open_archive`
- `crates/hearth-agent/src/infrastructure/services_key.rs::open_read_no_follow`

## Vérification
- `crates/hearth-agent/src/domain/archive/tests_extract.rs::extraction_refuses_when_the_archive_file_changed_since_the_inspection`
- `crates/hearth-agent/src/domain/archive/tests_tar.rs::extraction_refuses_a_tar_whose_entries_changed_after_the_inspection`
- `crates/hearth-agent/src/domain/archive/tests_extract.rs::cancellation_is_honored_before_and_during_the_work`
- `crates/hearth-agent/tests/archive_fs.rs::unix::a_cancelled_or_failed_extraction_leaves_no_destination_and_no_staging (CI Linux)`
- `crates/hearth-agent/tests/archive_fs.rs::unix::the_archive_itself_is_never_opened_through_a_link_nor_when_it_is_not_a_regular_file (CI Linux)`

## Cas limites
- `read_one` recalcule l'empreinte de TOUT le fichier à chaque appel (et rejoue un tar entier) : lire plusieurs fichiers en une passe est un ticket de suivi.
- Un `.tar` ou `.tar.gz` ne peut traîner plus de 1 Mio de zéros après son dernier bloc (`tests_tar.rs::the_zero_padding_after_the_end_of_a_tar_is_capped`).
- Une modification du fichier PENDANT l'extraction est rattrapée par les tailles, les sommes de contrôle et le rejeu d'un tar, pas par l'empreinte ; l'appelant garde le fichier hors de portée d'écriture des tiers.

## Règles liées
- BR-ARCH-001, BR-ARCH-002, BR-ARCH-003, BR-ARCH-004, BR-ARCH-005, BR-ARCH-006, BR-ARCH-008, BR-ARCH-009, BR-ARCH-010, BR-ARCH-011

## Historique
- 2026-10-13 — création (HRT-130, ADR-0055, lecture d'une archive hostile).
- 2026-10-14 — ouverture de la source sans lien ni blocage ; plafond des zéros de fin ; coût de `read_one` noté.
