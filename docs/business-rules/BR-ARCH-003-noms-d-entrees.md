---
id: BR-ARCH-003
domaine: ARCH
titre: Un nom d'entrée est UTF-8, relatif, sans piège ; aucun encodage n'est deviné
statut: active
invariant: true
source: ADR-0055 (contexts/hearth/conceptions/2026-10-13-adr-lecture-d-une-archive-brouillon.md), contexts/hearth/tickets/hrt/HRT-130.md
maj: 2026-10-14
---

# BR-ARCH-003 — Un nom d'entrée est UTF-8, relatif, sans piège ; aucun encodage n'est deviné

## Règle
Refusent TOUTE l'archive : un nom vide, de plus de 1 024 octets (255 par composant), contenant un octet nul ou un caractère de contrôle ou de format (voir plus bas), qui n'est pas de l'UTF-8 valide (phrase : « Ce fichier contient des noms que Hearth ne sait pas lire. Recrée l'archive avec un outil récent. »), contenant une barre oblique inverse, absolu, contenant « : » (lettre de lecteur, flux de données), avec un composant vide, `.` ou `..`, finissant par un point ou une espace, désignant un nom réservé de Windows (`CON`, `PRN`, `AUX`, `NUL`, `COM1` à `COM9`, `LPT1` à `LPT9`, avec ou sans extension), ou de plus de 32 niveaux. Un nom qui finit par `/` désigne un dossier. Le champ de nom Unicode en « extra » est ignoré. Posséder un `ArchivePath`, c'est avoir passé tous ces refus.

Caractères refusés dans un nom, règle exacte : tout caractère de catégorie Unicode générale `Cc` (contrôles C0 `U+0000`-`U+001F`, `U+007F`, contrôles C1 `U+0080`-`U+009F`), `Cf` (format, Unicode 16.0 : `U+00AD`, `U+0600`-`U+0605`, `U+061C`, `U+06DD`, `U+070F`, `U+0890`-`U+0891`, `U+08E2`, `U+180E`, `U+200B`-`U+200F` dont la largeur nulle, `U+202A`-`U+202E` dont l'inversion bidirectionnelle, `U+2060`-`U+2064`, `U+2066`-`U+206F`, `U+FEFF`, `U+FFF9`-`U+FFFB`, `U+110BD`, `U+110CD`, `U+13430`-`U+1343F`, `U+1BCA0`-`U+1BCA3`, `U+1D173`-`U+1D17A`, `U+E0001`, `U+E0020`-`U+E007F`), `Zl` (`U+2028`) et `Zp` (`U+2029`), n'importe où dans le nom. La table `Cf` est écrite à la main (la bibliothèque standard de Rust n'expose pas les catégories) et se relit à chaque montée d'Unicode. Une conséquence : une séquence d'emoji liée par `U+200D` est refusée.

## Application (code)
- `crates/hearth-agent/src/domain/archive/path.rs::judge`
- `crates/hearth-agent/src/domain/archive/path.rs::ArchivePath`
- `crates/hearth-agent/src/domain/archive/path.rs::is_forbidden_char`

## Vérification
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::an_absolute_path_is_refused`
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::a_parent_component_is_refused_wherever_it_stands`
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::a_backslash_is_refused_even_when_it_spells_a_parent`
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::a_drive_letter_or_a_stream_name_is_refused`
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::an_empty_component_is_refused`
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::windows_reserved_names_and_trailing_dots_are_refused`
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::a_nul_byte_or_a_control_character_is_refused`
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::every_format_or_line_separator_character_is_refused_wherever_it_stands`
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::every_c0_and_c1_control_character_is_refused`
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::ordinary_non_ascii_names_still_pass`
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::a_name_that_is_not_utf8_is_refused_with_the_sentence_of_the_adr_and_never_guessed`
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::a_name_longer_than_1024_bytes_is_refused_and_1024_is_accepted`
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::a_path_deeper_than_32_levels_is_refused_and_32_is_accepted`

## Cas limites
- Les caractères réservés de Windows autres que « : » (`<>"|?*`) ne sont pas refusés.

## Règles liées
- BR-ARCH-001, BR-ARCH-002, BR-ARCH-004, BR-ARCH-005, BR-ARCH-006, BR-ARCH-007, BR-ARCH-008, BR-ARCH-009, BR-ARCH-010, BR-ARCH-011

## Historique
- 2026-10-13 — création (HRT-130, ADR-0055, lecture d'une archive hostile).
- 2026-10-14 — caractères de format, séparateurs de ligne et contrôles C1 refusés (revue de la PR 88).
