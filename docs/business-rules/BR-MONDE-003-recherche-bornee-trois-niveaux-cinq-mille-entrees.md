---
id: BR-MONDE-003
domaine: MONDE
titre: La recherche des mondes lit des noms seulement, sur trois niveaux et 5 000 entrées au plus par dossier ouvert, et dit quand elle s'arrête
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-reprendre-un-monde-existant.md (BR-MONDE-003), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (section 9.9), HRT-105
maj: 2026-11-15
---

# BR-MONDE-003 — La recherche des mondes lit des noms seulement, sur trois niveaux et 5 000 entrées au plus par dossier ouvert, et dit quand elle s'arrête

## Règle
`GET /catalog/{id}/worlds` parcourt chaque dossier ouvert à trois niveaux au plus et lit 5 000 entrées au plus par dossier (`truncated` quand la borne est atteinte). Elle lit les noms, tailles et dates, jamais un contenu. Ce qu'est un monde vient de la fiche (`BR-MONDE-039`). La liste n'est ni diffusée ni journalisée ; la route exige le droit de gérer les services.

## Application (code)
- `crates/hearth-agent/src/application/world_import/search.rs::find_worlds`
- `crates/hearth-agent/src/domain/worlds/mod.rs::{WorldRules::is_world, MAX_DEPTH, MAX_ENTRIES}`
- `crates/hearth-agent/src/application/world_import/mod.rs::WorldImport::list`
- `apps/desktop/src/fiche/world.ts::worldRows, ::worldChoice` (liste avec le dossier de chaque monde, « Aucun monde trouvé », recherche tronquée dite)

## Vérification
- `crates/hearth-agent/src/application/world_import/search.rs::tests::the_search_lists_worlds_within_three_levels_and_skips_backups_and_links`
- `crates/hearth-agent/src/application/world_import/search.rs::tests::the_search_stops_at_the_entry_bound_and_says_so`
- `crates/hearth-agent/tests/world_import_https.rs::the_list_shows_the_worlds_of_the_open_folders_and_never_a_backup_or_a_foreign_file`
- `apps/desktop/src/pages/InstallWorld.test.ts` (liste, vide, tronquée)

## Cas limites
- Un sous-dossier illisible est ignoré ; un dossier ouvert illisible est listé avec `readable = false`.
- Un nom qui n'est pas de l'UTF-8 n'est pas listé (il ne peut pas être désigné).

## Règles liées
- BR-MONDE-002, BR-MONDE-039

## Historique
- 2026-11-14 — création (HRT-105, côté agent).
- 2026-11-15 — côté client (HRT-106).
