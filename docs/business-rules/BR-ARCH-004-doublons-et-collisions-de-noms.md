---
id: BR-ARCH-004
domaine: ARCH
titre: Deux entrées qui désignent le même chemin refusent l'archive ; aucune ne « gagne »
statut: active
invariant: true
source: ADR-0055 (contexts/hearth/conceptions/2026-10-13-adr-lecture-d-une-archive-brouillon.md), contexts/hearth/tickets/hrt/HRT-130.md
maj: 2026-10-13
---

# BR-ARCH-004 — Deux entrées qui désignent le même chemin refusent l'archive ; aucune ne « gagne »

## Règle
Refusent TOUTE l'archive : deux entrées au même nom exact ; deux noms qui ne diffèrent que par la casse ou la forme Unicode (comparaison après normalisation NFC et minuscules, sur le chemin entier ET sur chaque dossier parent) ; un fichier et un dossier du même nom ; un fichier dont un préfixe est un fichier. Un dossier listé après ses enfants est accepté une fois. Le registre garde des empreintes de 16 octets, pas les noms.

## Application (code)
- `crates/hearth-agent/src/domain/archive/path.rs::NameRegistry::add`

## Vérification
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::two_entries_with_the_same_name_are_refused_and_neither_wins`
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::names_that_differ_only_by_case_are_refused`
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::names_that_differ_only_by_unicode_normalization_are_refused`
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::a_file_and_a_folder_of_the_same_name_are_refused`
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::a_folder_listed_after_its_children_is_fine_once`
- `crates/hearth-agent/src/domain/archive/tests_tar.rs::duplicates_and_case_collisions_are_refused_in_a_tar_too`

## Cas limites
- Les repliements de casse spéciaux (`ß`/`ss`) ne sont pas traités.

## Règles liées
- BR-ARCH-001, BR-ARCH-002, BR-ARCH-003, BR-ARCH-005, BR-ARCH-006, BR-ARCH-007, BR-ARCH-008, BR-ARCH-009, BR-ARCH-010, BR-ARCH-011

## Historique
- 2026-10-13 — création (HRT-130, ADR-0055, lecture d'une archive hostile).
