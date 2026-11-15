---
id: BR-MOD-001
domaine: MOD
titre: Une fiche déclare les voies de mods de son jeu, dans une liste fermée, et rien d'autre ne s'offre
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-mods-et-packs.md (BR-MOD-001), HRT-124
maj: 2026-11-15
---

# BR-MOD-001 : Une fiche déclare les voies de mods de son jeu, dans une liste fermée, et rien d'autre ne s'offre

## Règle
Une fiche déclare `[mods] ways` parmi une liste FERMÉE (`drop_files`, `drop_pack` ; `ids_by_game` et `fetch_by_agent` arrivent avec HRT-126). Une valeur hors liste, une voie déclarée deux fois, `drop_files` sans dossier de rôle `mods` inscriptible, `drop_pack` sans `[mods.pack]` (ou l'inverse), un dossier de packs qui n'est pas de rôle `packs`, en lecture seule et hors sauvegarde, un genre de pack deux fois, une variable invalide ou une variante inconnue : la fiche est refusée au chargement. Une fiche sans bloc `[mods]` se charge comme avant et n'offre aucune voie (BR-MOD-014). `GET /catalog/{id}` rend `mods` : ce que la fiche déclare, jamais plus.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/loading/invariants.rs::check_mods`
- `crates/hearth-agent/src/domain/catalog/loading/schema/fiche.rs::{Mods, ModWay, PackWay, PackGenreWay}`
- `crates/hearth-agent/src/application/catalog.rs::mods_view`

## Vérification
- crates/hearth-agent/src/domain/catalog/tests_mods.rs : `a_fiche_declares_its_ways_and_the_genres_of_pack_it_takes`, `a_way_a_genre_or_a_field_outside_the_closed_lists_is_refused`, `the_mods_block_must_be_coherent`
- `a_fiche_without_a_mods_block_loads_as_before`

## Cas limites
- Une voie que ce client ne connaît pas est ignorée à la lecture (`Items`), jamais devinée.
- Le chargeur est fermé : un champ inconnu (une clé d'API, par exemple) est refusé par l'analyse.

## Règles liées
- BR-MOD-014, BR-MOD-017, BR-FICHE-007

## Historique
- 2026-11-15 : création (HRT-124).
