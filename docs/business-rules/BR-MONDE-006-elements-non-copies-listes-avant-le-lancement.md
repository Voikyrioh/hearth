---
id: BR-MONDE-006
domaine: MONDE
titre: Dans le monde choisi, les éléments qui ressemblent à une sauvegarde, les liens et les fichiers spéciaux ne sont pas copiés ; ils sont listés avant le lancement
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-reprendre-un-monde-existant.md (BR-MONDE-006), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (section 9.9), HRT-105
maj: 2026-11-15
---

# BR-MONDE-006 — Dans le monde choisi, les éléments qui ressemblent à une sauvegarde, les liens et les fichiers spéciaux ne sont pas copiés ; ils sont listés avant le lancement

## Règle
`POST /catalog/{id}/worlds/inspect` rend le nombre de fichiers et d'octets à copier, le nombre d'éléments non copiés (nom de sauvegarde) et leurs 50 premiers noms, de même pour les liens et fichiers spéciaux. L'inspection ne lit que des noms et n'écrit rien.

## Application (code)
- `crates/hearth-agent/src/application/world_import/search.rs::plan_world`
- `crates/hearth-agent/src/application/world_import/mod.rs::WorldImport::inspect`
- `apps/desktop/src/fiche/world.ts::notCopiedLines, ::inspectionSize` (ce que la copie écrit et ce qu'elle laisse où il est, « au moins » si la mesure est interrompue)

## Vérification
- `crates/hearth-agent/src/application/world_import/search.rs::tests::the_plan_copies_the_world_and_its_companions_flat_and_lists_what_it_leaves`
- `crates/hearth-agent/src/application/world_import/search.rs::tests::a_dir_world_keeps_its_tree_and_lists_backups_and_links_apart`
- `crates/hearth-agent/tests/world_import_https.rs::the_inspection_gives_the_size_to_copy_and_what_stays_where_it_is`
- `apps/desktop/src/pages/InstallWorld.test.ts` (choisir un monde)

## Cas limites
- La mesure s'arrête à 60 secondes (`partial`).

## Règles liées
- BR-MONDE-005

## Historique
- 2026-11-14 — création (HRT-105, côté agent).
- 2026-11-15 — côté client (HRT-106).
