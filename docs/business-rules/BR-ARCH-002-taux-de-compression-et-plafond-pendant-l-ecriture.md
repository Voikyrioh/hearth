---
id: BR-ARCH-002
domaine: ARCH
titre: Bombes de décompression : taux refusés à l'inspection, plafond d'octets pendant la lecture
statut: active
invariant: true
source: ADR-0055 (contexts/hearth/conceptions/2026-10-13-adr-lecture-d-une-archive-brouillon.md), contexts/hearth/tickets/hrt/HRT-130.md
maj: 2026-10-13
---

# BR-ARCH-002 — Bombes de décompression : taux refusés à l'inspection, plafond d'octets pendant la lecture

## Règle
Une entrée qui annonce plus de 200 fois sa taille compressée (au-delà de 1 Mio déballé) et une archive qui annonce plus de 100 fois sa taille sont refusées à l'inspection. Un `.tar.gz` est surveillé pendant la lecture (taux du flux, 100 fois, 64 Kio de marge). À l'inspection, chaque entrée d'un zip est relue : sa taille réelle et sa somme de contrôle doivent être celles de l'index. À l'extraction, l'écriture s'arrête au premier octet au-delà de la taille annoncée (on lit au plus un octet d'excès pour le constater et on ne le remet pas) ; le total est plafonné par la taille déballée annoncée.

## Application (code)
- `crates/hearth-agent/src/domain/archive/zip.rs::inspect (taux par entrée et total)`
- `crates/hearth-agent/src/domain/archive/flow.rs::pump (taille exacte, un octet d'excès au plus, somme de contrôle)`
- `crates/hearth-agent/src/domain/archive/gzip.rs::GzipReader (taux du flux)`

## Vérification
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::a_one_level_bomb_is_refused_by_ratio_before_any_decompression`
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::many_moderately_compressed_entries_trip_the_archive_ratio`
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::announced_sizes_are_checked_against_what_the_data_really_inflates_to`
- `crates/hearth-agent/src/domain/archive/tests_extract.rs::writing_stops_at_the_first_byte_beyond_the_announced_size`
- `crates/hearth-agent/src/domain/archive/tests_tar.rs::a_gzip_bomb_is_stopped_by_the_stream_ratio_while_reading`
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::the_total_unpacked_size_is_bounded`

## Cas limites
- Le taux ne s'applique qu'au-delà de 1 Mio déballé. Un `.tar` non compressé n'a pas de taux.

## Règles liées
- BR-ARCH-001, BR-ARCH-003, BR-ARCH-004, BR-ARCH-005, BR-ARCH-006, BR-ARCH-007, BR-ARCH-008, BR-ARCH-009, BR-ARCH-010, BR-ARCH-011

## Historique
- 2026-10-13 — création (HRT-130, ADR-0055, lecture d'une archive hostile).
