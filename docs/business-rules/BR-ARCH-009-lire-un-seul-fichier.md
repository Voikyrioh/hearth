---
id: BR-ARCH-009
domaine: ARCH
titre: Lire un seul fichier d'une archive rend 1 Mio au plus, après le jugement de toute l'archive
statut: active
invariant: true
source: ADR-0055 (contexts/hearth/conceptions/2026-10-13-adr-lecture-d-une-archive-brouillon.md), contexts/hearth/tickets/hrt/HRT-130.md
maj: 2026-10-13
---

# BR-ARCH-009 — Lire un seul fichier d'une archive rend 1 Mio au plus, après le jugement de toute l'archive

## Règle
`read_one` ne se fait que sur une archive déjà inspectée (donc entièrement jugée) ; il vérifie que le fichier existe (nom exact, fichier ordinaire), que sa taille annoncée est de 1 Mio au plus, que l'empreinte du fichier d'archive n'a pas changé, puis lit ce fichier seul, avec taille exacte et somme de contrôle vérifiées.

## Application (code)
- `crates/hearth-agent/src/domain/archive/mod.rs::read_one`

## Vérification
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::reading_one_file_returns_it_after_the_whole_archive_was_judged`
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::reading_one_file_gives_at_most_one_mebibyte`
- `crates/hearth-agent/src/domain/archive/tests_tar.rs::extraction_replays_exactly_what_was_inspected_and_reads_one_file`

## Cas limites
- Le JSON lu est jugé par l'appelant (profondeur et nombre d'éléments bornés).

## Règles liées
- BR-ARCH-001, BR-ARCH-002, BR-ARCH-003, BR-ARCH-004, BR-ARCH-005, BR-ARCH-006, BR-ARCH-007, BR-ARCH-008, BR-ARCH-010, BR-ARCH-011

## Historique
- 2026-10-13 — création (HRT-130, ADR-0055, lecture d'une archive hostile).
