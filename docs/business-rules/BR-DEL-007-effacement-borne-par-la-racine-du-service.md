---
id: BR-DEL-007
domaine: DEL
titre: L'effacement d'un service est borné par sa racine : un lien est retiré comme lien, aucun montage n'est franchi
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-007), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md §9.1 règle 11, HRT-145
maj: 2026-10-17
---

# BR-DEL-007 : L'effacement d'un service est borné par sa racine : un lien est retiré comme lien, aucun montage n'est franchi

## Règle
`SafeFs::remove_service_root` vide `services/<clé>/` puis retire le dossier ; `ServiceRoot::remove_tree` retire un fichier, un lien ou un arbre sous la racine. Le parcours se fait par descripteurs (`unlinkat` relatif au dossier parent ouvert) : un lien est retiré en tant que lien et sa cible n'est pas touchée, un dossier remplacé par un lien pendant le parcours est retiré comme lien, un dossier d'un autre périphérique n'est pas descendu (`CrossesDevice`, le reste est effacé et la première erreur est rendue). La racine du service ne se retire que par `remove_service_root` (jamais par `remove_tree`, `Refused`), le dossier `services/` et les racines voisines ne sont jamais touchés. Au-delà de 64 niveaux : `Refused`, sans descendre.

## Application (code)
- `crates/hearth-agent/src/infrastructure/fs/unix.rs::{remove_service_root, empty_dir, remove_entry, UnixRoot::remove}`

## Vérification
- `crates/hearth-agent/src/infrastructure/fs/tests_unix.rs` : `remove_tree_unlinks_links_as_links_and_never_descends_into_them`, `remove_tree_of_a_link_removes_only_the_link`, `remove_tree_refuses_the_root_and_reports_a_missing_target`, `removing_the_service_root_takes_its_content_and_nothing_else`, `a_deeper_tree_than_the_bound_is_refused_not_followed_forever`, `a_mount_point_is_never_crossed`, `a_directory_swapped_for_a_link_during_removal_never_deletes_outside` (course)

## Cas limites
- Le ticket HRT-145 cite `BR-DEL-010` pour cette règle ; dans la spec `supprimer-service`, `BR-DEL-010` est « dernière sauvegarde » et l'effacement borné est `BR-DEL-007`.
- Les montages partagés avec un autre conteneur (`BR-DEL-008`) et la fenêtre de suppression sont à HRT-85.

## Règles liées
- BR-FILES-003, BR-DEL-009

## Historique
- 2026-10-17 : création (HRT-145).
