---
id: BR-MOD-001
domaine: MOD
titre: Une fiche déclare les voies de mods de son jeu, dans une liste fermée, et rien d'autre ne s'offre
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-mods-et-packs.md (BR-MOD-001), HRT-124
maj: 2026-11-18
---

# BR-MOD-001 : Une fiche déclare les voies de mods de son jeu, dans une liste fermée, et rien d'autre ne s'offre

## Règle
Une fiche déclare `[mods] ways` parmi une liste FERMÉE (`drop_files`, `drop_pack`, `drop_jars` ; `ids_by_game` et `fetch_by_agent` arrivent avec HRT-126). Une valeur hors liste, une voie déclarée deux fois, `drop_files` sans dossier de rôle `mods` inscriptible, `drop_pack` sans `[mods.pack]` (ou l'inverse), un dossier de packs qui n'est pas de rôle `packs`, en lecture seule et hors sauvegarde, un genre de pack deux fois, `drop_jars` sans `[mods.jars]` (ou l'inverse), un dossier des mods qui n'est pas de rôle `data`, sauvegardé et inscriptible, un `dir` ou `retired_dir` qui n'est pas un seul composant `[a-z0-9-]{1,32}` ou qui sont égaux, une variante inconnue, un arrêt qui n'enregistre pas le monde par le signal (`by_image_on_signal`), toute variable `REMOVE_OLD_MODS` (BR-MOD-028), une variable invalide ou une variante inconnue : la fiche est refusée au chargement. Une fiche sans bloc `[mods]` se charge comme avant et n'offre aucune voie (BR-MOD-014). `GET /catalog/{id}` rend `mods` : ce que la fiche déclare, jamais plus.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/loading/invariants.rs::check_mods`
- `crates/hearth-agent/src/domain/catalog/loading/invariants.rs::{check_mods_jars, check_no_remove_old_mods}`
- `crates/hearth-agent/src/domain/catalog/loading/schema/fiche.rs::{Mods, ModWay, ModsJars, PackWay, PackGenreWay}`
- `crates/hearth-agent/src/application/catalog.rs::mods_view`

## Vérification
- crates/hearth-agent/src/domain/catalog/tests_mods.rs : `a_fiche_declares_its_ways_and_the_genres_of_pack_it_takes`, `a_way_a_genre_or_a_field_outside_the_closed_lists_is_refused`, `the_mods_block_must_be_coherent`
- `a_fiche_without_a_mods_block_loads_as_before`
- `a_fiche_declares_the_jars_it_takes_and_where_they_go`, `the_jars_block_is_all_or_nothing_and_points_at_a_saved_writable_data_folder`, `a_fiche_with_jars_must_save_the_world_on_the_stop_signal`, `br_mod_028_no_fiche_writes_remove_old_mods`, `no_embedded_fiche_writes_remove_old_mods_and_minecraft_declares_its_jars`

## Cas limites
- Une voie que ce client ne connaît pas est ignorée à la lecture (`Items`), jamais devinée.
- Le chargeur est fermé : un champ inconnu (une clé d'API, par exemple) est refusé par l'analyse.

## Règles liées
- BR-MOD-014, BR-MOD-017, BR-FICHE-007

## Historique
- 2026-11-15 : création (HRT-124).
- 2026-11-18 : voie `drop_jars` et bloc `[mods.jars]` (HRT-253, ADR-0062) ; interdiction de `REMOVE_OLD_MODS` (BR-MOD-028).
