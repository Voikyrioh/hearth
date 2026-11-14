---
id: BR-MONDE-005
domaine: MONDE
titre: Les sauvegardes existantes ne sont jamais touchées : ni source, ni parcourues, ni lues, et seul le monde choisi est lu
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-reprendre-un-monde-existant.md (BR-MONDE-005), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (section 9.9), HRT-105
maj: 2026-11-14
---

# BR-MONDE-005 — Les sauvegardes existantes ne sont jamais touchées : ni source, ni parcourues, ni lues, et seul le monde choisi est lu

## Règle
Un élément dont le nom contient « backup » ou « sauvegarde », ou finit par « .bak » (`règle de nom des sauvegardes (HRT-97)`), n'est ni un monde ni un dossier où descendre ; un monde rangé sous un tel dossier est refusé comme source (`not_a_world`). Les voisins du monde choisi ne sont pas parcourus. Un lien ou un fichier spécial du monde n'est pas copié et est listé.

## Application (code)
- `crates/hearth-agent/src/domain/worlds/mod.rs::looks_like_backup`
- `crates/hearth-agent/src/application/world_import/search.rs::{find_worlds, plan_world}`

## Vérification
- `crates/hearth-agent/src/domain/worlds/mod.rs::tests::a_name_that_looks_like_a_backup_is_never_a_world`
- `crates/hearth-agent/src/application/world_import/search.rs::tests::a_path_that_is_not_a_world_of_the_fiche_is_refused`
- `crates/hearth-agent/tests/world_import_https.rs::a_world_is_copied_verified_placed_and_loaded_by_name_and_the_source_is_untouched`

## Cas limites
- Reprendre une sauvegarde choisie dans un dossier de sauvegardes est HRT-110 (`BR-MONDE-028` à `038`).

## Règles liées
- BR-MONDE-004, BR-MONDE-006, règle de nom des sauvegardes (HRT-97)

## Historique
- 2026-11-14 — création (HRT-105, côté agent).
