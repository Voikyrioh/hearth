---
id: BR-DEL-016
domaine: DEL
titre: Ce qui n'a pas pu être effacé est compté et dit ; la suppression reste « Réussi », rien n'est rejoué
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-016), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md §5 et §9, HRT-85
maj: 2026-10-24
---

# BR-DEL-016 : Ce qui n'a pas pu être effacé est compté et dit ; la suppression reste « Réussi », rien n'est rejoué

## Règle
Si l'effacement de certains éléments échoue (droits refusés, système de fichiers en lecture seule, point de montage), le service est déjà supprimé : l'opération réussit avec la cause `data_not_erased`, le journal d'activité garde « Réussi » avec la mention « données non effacées (n) » (un nombre, jamais un chemin) et l'agent ne réessaie pas. La racine du service reste tant qu'elle n'est pas vide.

## Application (code)
- `crates/hearth-agent/src/application/delete.rs::ServiceDeletion::erase`
- `crates/hearth-agent/src/application/ports/safe_fs.rs::Erased`

## Vérification
- `crates/hearth-agent/tests/service_delete_https.rs::data_that_refuses_to_go_is_counted_and_the_deletion_still_succeeds`
- `crates/hearth-agent/src/infrastructure/fs/tests_unix.rs::what_refuses_to_go_is_counted_and_the_rest_is_erased`

## Cas limites
- Les chemins des éléments restés (« liste ce qui reste ») viennent avec la fenêtre (HRT-86) : l'opération ne porte aujourd'hui que la cause et le décompte au journal.

## Règles liées
- BR-DEL-017
- BR-DEL-030

## Historique
- 2026-10-24 : création (HRT-85, côté agent).
