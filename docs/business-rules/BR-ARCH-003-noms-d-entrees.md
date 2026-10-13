---
id: BR-ARCH-003
domaine: ARCH
titre: Un nom d'entrée est UTF-8, relatif, sans piège ; aucun encodage n'est deviné
statut: active
invariant: true
source: ADR-0055 (contexts/hearth/conceptions/2026-10-13-adr-lecture-d-une-archive-brouillon.md), contexts/hearth/tickets/hrt/HRT-130.md
maj: 2026-10-13
---

# BR-ARCH-003 — Un nom d'entrée est UTF-8, relatif, sans piège ; aucun encodage n'est deviné

## Règle
Refusent TOUTE l'archive : un nom vide, de plus de 1 024 octets (255 par composant), contenant un octet nul ou un caractère de contrôle, qui n'est pas de l'UTF-8 valide (phrase : « Ce fichier contient des noms que Hearth ne sait pas lire. Recrée l'archive avec un outil récent. »), contenant une barre oblique inverse, absolu, contenant « : » (lettre de lecteur, flux de données), avec un composant vide, `.` ou `..`, finissant par un point ou une espace, désignant un nom réservé de Windows (`CON`, `PRN`, `AUX`, `NUL`, `COM1` à `COM9`, `LPT1` à `LPT9`, avec ou sans extension), ou de plus de 32 niveaux. Un nom qui finit par `/` désigne un dossier. Le champ de nom Unicode en « extra » est ignoré. Posséder un `ArchivePath`, c'est avoir passé tous ces refus.

## Application (code)
- `crates/hearth-agent/src/domain/archive/path.rs::judge`
- `crates/hearth-agent/src/domain/archive/path.rs::ArchivePath`

## Vérification
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::an_absolute_path_is_refused`
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::a_parent_component_is_refused_wherever_it_stands`
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::a_backslash_is_refused_even_when_it_spells_a_parent`
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::a_drive_letter_or_a_stream_name_is_refused`
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::an_empty_component_is_refused`
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::windows_reserved_names_and_trailing_dots_are_refused`
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::a_nul_byte_or_a_control_character_is_refused`
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::a_name_that_is_not_utf8_is_refused_with_the_sentence_of_the_adr_and_never_guessed`
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::a_name_longer_than_1024_bytes_is_refused_and_1024_is_accepted`
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::a_path_deeper_than_32_levels_is_refused_and_32_is_accepted`

## Cas limites
- Les caractères réservés de Windows autres que « : » (`<>"|?*`) ne sont pas refusés.

## Règles liées
- BR-ARCH-001, BR-ARCH-002, BR-ARCH-004, BR-ARCH-005, BR-ARCH-006, BR-ARCH-007, BR-ARCH-008, BR-ARCH-009, BR-ARCH-010, BR-ARCH-011

## Historique
- 2026-10-13 — création (HRT-130, ADR-0055, lecture d'une archive hostile).
