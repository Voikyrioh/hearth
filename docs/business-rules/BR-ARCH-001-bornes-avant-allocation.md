---
id: BR-ARCH-001
domaine: ARCH
titre: Toute valeur lue dans l'archive est bornée avant de dimensionner quoi que ce soit
statut: active
invariant: true
source: ADR-0055 (contexts/hearth/conceptions/2026-10-13-adr-lecture-d-une-archive-brouillon.md), contexts/hearth/tickets/hrt/HRT-130.md
maj: 2026-10-13
---

# BR-ARCH-001 — Toute valeur lue dans l'archive est bornée avant de dimensionner quoi que ce soit

## Règle
Le fichier d'archive fait 8 Gio au plus (lu par la position de fin). Une archive annonce au plus 200 000 entrées, un index de fin de 64 Mio au plus (pour un tar : 64 Mio de noms au total), des noms de 1 024 octets au plus (255 par composant), 32 niveaux au plus, 1 000 000 de noms distincts retenus, une taille déballée totale sous l'espace libre moins la réserve et 64 Gio au plus. Un index qui annonce plus d'entrées que sa taille ne peut en contenir (46 octets au moins par entrée) est refusé avant toute réservation. L'index entre en mémoire par ce que le fichier fournit réellement, jamais par sa taille annoncée ; un en-tête étendu de tar est borné (64 Kio, 1 025 octets) avant d'être lu ; les tampons de flux sont de 64 Kio. Aucune structure n'est dimensionnée par un nombre lu sans avoir été bornée avant.

## Application (code)
- `crates/hearth-agent/src/domain/archive/limits.rs::ArchiveLimits (valeurs)`
- `crates/hearth-agent/src/domain/archive/zip.rs::inspect (index, entrées, 46 octets par entrée)`
- `crates/hearth-agent/src/domain/archive/zip.rs::find_end_record (fenêtre de fin)`
- `crates/hearth-agent/src/domain/archive/tar.rs::TarWalker::next_item (en-têtes étendus, noms, total)`
- `crates/hearth-agent/src/domain/archive/path.rs::NameRegistry::add (noeuds)`

## Vérification
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::an_index_announcing_more_entries_than_it_can_hold_is_refused_before_any_reservation`
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::the_entry_count_the_index_size_and_the_name_length_are_bounded`
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::an_index_claiming_sixty_mebibytes_in_a_tiny_file_is_refused_without_reserving_them`
- `crates/hearth-agent/src/domain/archive/tests_tar.rs::an_extended_header_announcing_a_huge_size_is_refused_without_reading_or_reserving_it`
- `crates/hearth-agent/src/domain/archive/tests_tar.rs::an_enormous_size_is_refused_before_anything_is_read_for_it`
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::the_number_of_distinct_names_is_bounded`

## Cas limites
- La mémoire de travail n'est PAS mesurée par un test (un allocateur de comptage exige `unsafe`, interdit par le lint `forbid`) : elle est bornée par construction.

## Règles liées
- BR-ARCH-002, BR-ARCH-003, BR-ARCH-004, BR-ARCH-005, BR-ARCH-006, BR-ARCH-007, BR-ARCH-008, BR-ARCH-009, BR-ARCH-010, BR-ARCH-011

## Historique
- 2026-10-13 — création (HRT-130, ADR-0055, lecture d'une archive hostile).
