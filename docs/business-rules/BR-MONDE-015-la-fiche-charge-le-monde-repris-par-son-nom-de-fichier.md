---
id: BR-MONDE-015
domaine: MONDE
titre: Pour Terraria, la fiche charge le monde repris par son nom de fichier ; la vérification de la copie garantit qu'il existe
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-reprendre-un-monde-existant.md (BR-MONDE-015), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (section 9.9), HRT-105
maj: 2026-11-14
---

# BR-MONDE-015 — Pour Terraria, la fiche charge le monde repris par son nom de fichier ; la vérification de la copie garantit qu'il existe

## Règle
Avec un monde repris, la variable `WORLD_FILENAME` de la fiche est définie (`<nom du monde>.wld`) ; sans reprise elle ne l'est pas et le serveur crée un monde.

## Application (code)
- `crates/hearth-agent/catalog/terraria/fiche.toml ([[env]] WORLD_FILENAME)`
- `crates/hearth-agent/src/application/fiche_install.rs::FicheInstaller::prepare`

## Vérification
- `crates/hearth-agent/tests/world_import_https.rs::a_world_is_copied_verified_placed_and_loaded_by_name_and_the_source_is_untouched`

## Cas limites
- Si le fichier manque malgré tout au démarrage, le service s'arrête en boucle et la phase du jeu le dit (`BR-DEPLOY-020`).

## Règles liées
- BR-MONDE-010, BR-MONDE-014

## Historique
- 2026-11-14 — création (HRT-105, côté agent).
