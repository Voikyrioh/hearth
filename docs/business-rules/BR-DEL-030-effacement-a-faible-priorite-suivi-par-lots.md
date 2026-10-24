---
id: BR-DEL-030
domaine: DEL
titre: L'effacement des données d'un service se fait à faible priorité, par lots, avec un suivi, sans jamais s'arrêter à la première erreur
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-030), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md §9.3, HRT-85, HRT-179
maj: 2026-10-24
---

# BR-DEL-030 : L'effacement se fait à faible priorité, par lots, avec un suivi

## Règle
`ServiceRoot::erase_tree(rel, progress)` efface un dossier géré par Hearth pour la suppression d'un service :

- **faible priorité** : le geste tourne sur un fil dédié (jamais le groupe de tâches bloquantes de l'exécuteur) dont le « nice » est abaissé à 19, ce que le noyau reprend pour les entrées-sorties des ordonnanceurs qui le lisent ; le fil appelant n'est pas touché. Ce n'est pas la classe d'entrée-sortie `idle` d'`ioprio_set` (non exposée par `rustix`, `libc` est interdit avec `unsafe`) : l'effet est à mesurer sur le serveur réel ;
- **mémoire bornée** (HRT-179) : les noms d'un dossier sont lus par lots de 1 024 au plus, retirés, puis le dossier est relu depuis le début ; un dossier de millions de fichiers coûte 1 024 noms en mémoire ;
- **aucune boucle sans fin** : un nom qui refuse de partir est retenu et ne revient plus dans les lots de ce dossier ; un dossier est abandonné au bout de 2^20 lots (un service qui créerait des fichiers à mesure) ;
- **suivi** : `progress` reçoit le nombre d'éléments retirés jusqu'ici après chaque lot et à la fin ;
- **jamais d'arrêt à la première erreur** : ce qui part part ; `Erased { removed, left, first_error }` dit combien d'éléments restent (droits refusés, système de fichiers en lecture seule, point de montage, profondeur) et la première cause, jamais un chemin.

Les garanties de BR-DEL-007 sont les mêmes (un lien est retiré comme lien, aucun montage franchi, 64 niveaux, jamais la racine).

## Application (code)
- `crates/hearth-agent/src/application/ports/safe_fs.rs::{ServiceRoot::erase_tree, Erased}`
- `crates/hearth-agent/src/infrastructure/fs/unix/erase.rs::{Eraser, next_batch, lower_priority}`
- `crates/hearth-agent/tests/support/fake_safe_fs.rs` (le faux, mêmes règles visibles)

## Vérification
- `crates/hearth-agent/src/infrastructure/fs/tests_unix.rs` : `erase_tree_counts_what_it_removes_and_reports_progress`, `a_huge_directory_is_erased_batch_by_batch`, `erasing_runs_on_a_low_priority_thread_and_leaves_the_caller_alone`, `what_refuses_to_go_is_counted_and_the_rest_is_erased`, `a_mount_point_is_left_in_place_and_counted_by_the_erasure`
- `crates/hearth-agent/tests/support/safe_fs_contract.rs` (contre le faux et le vrai)

## Cas limites
- Un dossier qui n'existe plus : `FsError::NotFound`, dit « Aucune donnée à effacer » par l'appelant, pas une erreur de suppression.
- Hors Linux, `FsError::Unsupported`.
- L'effacement n'a pas de durée maximale (BR-DEL-030) : le suivi est le seul signe de vie.

## Règles liées
- BR-DEL-007, BR-DEL-009

## Historique
- 2026-10-24 : création (HRT-85, HRT-179).
