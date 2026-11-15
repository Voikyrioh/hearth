---
id: BR-MONDE-012
domaine: MONDE
titre: L'espace libre doit couvrir le monde à copier plus ce que la fiche exige pour s'installer, sans case pour passer outre
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-reprendre-un-monde-existant.md (BR-MONDE-012), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (section 9.9), HRT-105
maj: 2026-11-14
---

# BR-MONDE-012 — L'espace libre doit couvrir le monde à copier plus ce que la fiche exige pour s'installer, sans case pour passer outre

## Règle
La taille du monde (mesurée en 60 secondes au plus) s'ajoute au besoin mesuré de la fiche ; le refus est `NO_SPACE` (`needed_bytes`, `free_bytes`, `missing_bytes`) avant que rien ne soit créé.

## Application (code)
- `crates/hearth-agent/src/application/fiche_install.rs::FicheInstaller::prepare`

## Vérification
- `crates/hearth-agent/tests/world_import_https.rs::the_room_for_the_copy_is_asked_on_top_of_the_room_the_game_needs`

## Cas limites
- L'espace est lu sur le disque du moteur (images) comme pour l'installation ordinaire.

## Règles liées
- BR-MONDE-013

## Historique
- 2026-11-14 — création (HRT-105, côté agent).
