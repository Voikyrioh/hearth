---
id: BR-ARCH-006
domaine: ARCH
titre: L'index de fin d'un zip est confronté à chaque en-tête local ; il ne ment pas ou l'archive est refusée
statut: active
invariant: true
source: ADR-0055 (contexts/hearth/conceptions/2026-10-13-adr-lecture-d-une-archive-brouillon.md), contexts/hearth/tickets/hrt/HRT-130.md
maj: 2026-10-14
---

# BR-ARCH-006 — L'index de fin d'un zip est confronté à chaque en-tête local ; il ne ment pas ou l'archive est refusée

## Règle
Pour chaque entrée, l'en-tête local relu doit dire la même chose que l'index : même nom octet pour octet, même méthode, mêmes drapeaux (chiffrement, descripteur, UTF-8), mêmes tailles et somme de contrôle (ou un descripteur de données qui les confirme). Les entrées doivent paver le fichier SANS trou ni chevauchement de l'octet 0 à l'index. Refus de toute l'archive : en-tête local différent, entrées qui se chevauchent ou pointent au même endroit, entrée hors du fichier, plus d'un enregistrement de fin (commentaire compris), octets non couverts (données préfixées, archive concaténée, trou), octets cachés que le pavage ne voit pas (champ « extra » local de plus de 512 octets ou qui n'est pas une suite exacte d'enregistrements ; octets après la fin du flux deflate dans la zone compressée ; corps d'une entrée de dossier autre qu'un flux deflate vide `03 00`), plusieurs volumes, entrée chiffrée, méthode autre que `stored` et `deflate`. ZIP64 est lu dans les bornes (enregistrement de 56 octets exactement) ; si l'enregistrement de fin laisse un champ non saturé, il doit dire la même chose que l'enregistrement ZIP64. Le commentaire est ignoré. Une archive stockée dans l'archive n'est pas un second enregistrement de fin.

## Application (code)
- `crates/hearth-agent/src/domain/archive/zip.rs::find_end_record`
- `crates/hearth-agent/src/domain/archive/zip.rs::confront_local_headers`
- `crates/hearth-agent/src/domain/archive/zip.rs::inspect`
- `crates/hearth-agent/src/domain/archive/zip.rs::read_entry (fin du flux deflate)`

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
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::a_local_extra_field_is_a_short_exact_run_of_records`
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::bytes_after_the_end_of_a_deflate_stream_are_refused`
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::a_folder_entry_carries_nothing_but_an_empty_deflate_stream`
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::a_partly_saturated_end_record_must_agree_with_the_zip64_record`

## Cas limites
- Le CRC-32 de chaque entrée est vérifié (inspection et extraction).
- Non couvert par un test : ZIP64 hostile au-delà du cas heureux et de l'accord partiel (localisateur menteur, champ étendu tronqué ou en double, descripteur 64 bits) ; ticket de suivi.

## Règles liées
- BR-ARCH-001, BR-ARCH-002, BR-ARCH-003, BR-ARCH-004, BR-ARCH-005, BR-ARCH-007, BR-ARCH-008, BR-ARCH-009, BR-ARCH-010, BR-ARCH-011

## Historique
- 2026-10-13 — création (HRT-130, ADR-0055, lecture d'une archive hostile).
- 2026-10-14 — octets cachés (extra local, fin de flux deflate, corps de dossier) et ZIP64 partiel refusés.
