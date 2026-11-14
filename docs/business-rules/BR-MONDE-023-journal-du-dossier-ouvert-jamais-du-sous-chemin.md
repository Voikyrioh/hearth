---
id: BR-MONDE-023
domaine: MONDE
titre: Chaque reprise terminée est inscrite au journal d'activité avec le dossier ouvert tel qu'écrit dans la configuration, jamais le sous-chemin ni un nom
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-reprendre-un-monde-existant.md (BR-MONDE-023), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (section 9.9), HRT-105
maj: 2026-11-14
---

# BR-MONDE-023 — Chaque reprise terminée est inscrite au journal d'activité avec le dossier ouvert tel qu'écrit dans la configuration, jamais le sous-chemin ni un nom

## Règle
L'entrée « Serveur de jeu installé » porte « monde existant repris », le dossier ouvert, le nombre de fichiers et d'octets copiés, le nombre d'éléments non copiés et l'issue (aucun fichier modifié, copie gardée ou retirée, monde repris incomplet, annulée, copie impossible, copie non commencée). Le type n'a pas de champ pour un chemin, un nom de monde ni un nom de fichier ; `WorldRef` n'affiche rien en `Debug`.

## Application (code)
- `crates/hearth-agent/src/domain/audit/event.rs::{WorldNote, WorldResult}`
- `crates/hearth-agent/src/application/deployments.rs::Deployments::world_note`
- `crates/hearth-proto/src/api/worlds.rs::WorldRef`

## Vérification
- `crates/hearth-agent/src/domain/audit/event.rs::tests::a_world_import_entry_names_the_open_folder_and_never_a_sub_path_or_a_world`
- `crates/hearth-agent/tests/world_import_https.rs::a_world_is_copied_verified_placed_and_loaded_by_name_and_the_source_is_untouched`
- `crates/hearth-proto/src/api/worlds.rs::tests::a_world_ref_never_shows_its_path_in_debug`

## Cas limites
- Les consultations (liste, inspection) ne sont pas journalisées (`BR-MONDE-003`).

## Règles liées
- BR-MONDE-003, BR-MONDE-010

## Historique
- 2026-11-14 — création (HRT-105, côté agent).
