---
id: BR-ARCH-006
domaine: ARCH
titre: L'index de fin d'un zip est confronté à chaque en-tête local ; il ne ment pas ou l'archive est refusée
statut: active
invariant: true
source: ADR-0055 (contexts/hearth/conceptions/2026-10-13-adr-lecture-d-une-archive-brouillon.md), contexts/hearth/tickets/hrt/HRT-130.md
maj: 2026-10-13
---

# BR-ARCH-006 — L'index de fin d'un zip est confronté à chaque en-tête local ; il ne ment pas ou l'archive est refusée

## Règle
Pour chaque entrée, l'en-tête local relu doit dire la même chose que l'index : même nom octet pour octet, même méthode, mêmes drapeaux (chiffrement, descripteur, UTF-8), mêmes tailles et somme de contrôle (ou un descripteur de données qui les confirme). Les entrées doivent paver le fichier SANS trou ni chevauchement de l'octet 0 à l'index. Refus de toute l'archive : en-tête local différent, entrées qui se chevauchent ou pointent au même endroit, entrée hors du fichier, plus d'un enregistrement de fin (commentaire compris), octets non couverts (données préfixées, archive concaténée, trou), plusieurs volumes, entrée chiffrée, méthode autre que `stored` et `deflate`. ZIP64 est lu dans les bornes (enregistrement de 56 octets exactement). Le commentaire est ignoré. Une archive stockée dans l'archive n'est pas un second enregistrement de fin.

## Application (code)
- `crates/hearth-agent/src/domain/archive/zip.rs::find_end_record`
- `crates/hearth-agent/src/domain/archive/zip.rs::confront_local_headers`
- `crates/hearth-agent/src/domain/archive/zip.rs::inspect`

## Vérification
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::a_local_header_that_disagrees_with_the_index_refuses_the_whole_archive`
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::two_end_records_are_refused`
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::prefixed_data_and_hidden_gaps_are_refused`
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::two_concatenated_archives_are_refused`
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::an_entry_pointing_outside_the_file_is_refused`
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::entries_that_share_or_overlap_their_bytes_are_refused`
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::an_encrypted_entry_is_refused`
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::an_unsupported_compression_method_is_refused_clearly`
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::a_multi_volume_archive_is_refused`
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::stored_deflate_and_zip64_archives_are_accepted`

## Cas limites
- Le CRC-32 de chaque entrée est vérifié (inspection et extraction).

## Règles liées
- BR-ARCH-001, BR-ARCH-002, BR-ARCH-003, BR-ARCH-004, BR-ARCH-005, BR-ARCH-007, BR-ARCH-008, BR-ARCH-009, BR-ARCH-010, BR-ARCH-011

## Historique
- 2026-10-13 — création (HRT-130, ADR-0055, lecture d'une archive hostile).
