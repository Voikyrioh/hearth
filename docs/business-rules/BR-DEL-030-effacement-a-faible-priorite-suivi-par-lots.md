---
id: BR-DEL-030
domaine: DEL
titre: L'effacement des données d'un service se fait à priorité processeur basse, par lots, avec un suivi qui peut l'arrêter, sans jamais s'arrêter à la première erreur
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-030), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md §9.3, HRT-85, HRT-179
maj: 2026-10-24
---

# BR-DEL-030 : L'effacement se fait à priorité processeur basse, par lots, avec un suivi

## Règle
`ServiceRoot::erase_tree(rel, progress)` efface un dossier géré par Hearth pour la suppression d'un service :

- **priorité PROCESSEUR basse** : le geste tourne sur un fil créé pour lui dont le « nice » est 19 ; le fil appelant n'est pas touché. L'appelant le lance par `spawn_blocking` : il occupe un fil du groupe de tâches bloquantes de l'exécuteur pendant toute la durée de l'effacement (qui attend le fil du geste). Ce n'est PAS une priorité d'entrée-sortie : `ioprio_set` exige `libc` et `unsafe` (interdit), et un « nice » ne change rien aux disques sous les ordonnanceurs `mq-deadline` ou `none`. Le critère « faible priorité d'entrée-sortie » du ticket n'est donc pas tenu ; suivi à créer ;
- **mémoire bornée** (HRT-179) : les noms d'un dossier sont lus par lots de 1 024 au plus, retirés, puis le dossier est relu depuis le début ; les noms qui refusent de partir sont retenus jusqu'à un lot (1 024), au-delà le dossier est abandonné et le reste compté (au moins un) : un disque en lecture seule ne fait pas monter la mémoire ;
- **aucune boucle sans fin** : un nom retenu ne revient plus dans les lots de ce dossier ; un dossier est abandonné au bout de 2^20 lots ;
- **suivi et arrêt** : `progress` reçoit le nombre d'éléments retirés jusqu'ici après chaque lot et à la fin, et rend `false` pour demander l'arrêt (l'agent s'arrête) : le geste s'arrête, `Erased::stopped`, et l'opération reste « à vérifier » au démarrage suivant ;
- **aucun montage franchi** : ni un autre périphérique, ni un autre montage du même système de fichiers (`STATX_MNT_ID`, Linux 5.8 ; sur un noyau plus ancien, seul le périphérique est comparé) ;
- **jamais d'arrêt à la première erreur** : ce qui part part ; `Erased { removed, left, first_error, stopped }` dit combien d'éléments restent (droits refusés, système de fichiers en lecture seule, point de montage, profondeur) et la première cause, jamais un chemin.

Les garanties de BR-DEL-007 sont les mêmes (un lien est retiré comme lien, aucun montage franchi, 64 niveaux, jamais la racine).

## Application (code)
- `crates/hearth-agent/src/application/ports/safe_fs.rs::{ServiceRoot::erase_tree, Erased}`
- `crates/hearth-agent/src/infrastructure/fs/unix/erase.rs::{Eraser, next_batch, lower_priority}`
- `crates/hearth-agent/tests/support/fake_safe_fs.rs` (le faux, mêmes règles visibles)

## Vérification
- `crates/hearth-agent/src/infrastructure/fs/tests_unix.rs` : `erase_tree_counts_what_it_removes_and_reports_progress`, `a_huge_directory_is_erased_batch_by_batch`, `erasing_runs_on_a_low_priority_thread_and_leaves_the_caller_alone`, `what_refuses_to_go_is_counted_and_the_rest_is_erased`, `a_mount_point_is_left_in_place_and_counted_by_the_erasure`, `names_that_refuse_to_go_are_kept_up_to_one_batch_then_the_rest_is_counted`, `the_progress_can_stop_an_erasure_between_two_batches`, `the_mount_id_of_an_ordinary_directory_is_readable`
- `crates/hearth-agent/tests/support/safe_fs_contract.rs` (contre le faux et le vrai)

## Cas limites
- Un dossier qui n'existe plus : `FsError::NotFound`, dit « Aucune donnée à effacer » par l'appelant, pas une erreur de suppression.
- Hors Linux, `FsError::Unsupported`.
- L'effacement n'a pas de durée maximale : le suivi est le seul signe de vie.
- Un `mount --bind` posé pendant l'effacement par un autre compte n'est vu que sur un noyau 5.8 ou plus.

## Règles liées
- BR-DEL-007, BR-DEL-009

## Historique
- 2026-10-24 : création (HRT-85, HRT-179).
