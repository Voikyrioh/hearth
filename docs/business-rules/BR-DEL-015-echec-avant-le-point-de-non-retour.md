---
id: BR-DEL-015
domaine: DEL
titre: Si l'arrêt ou la suppression du conteneur échoue, le service n'est pas supprimé et rien n'est touché
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-015), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md §5 et §9, HRT-85
maj: 2026-10-24
---

# BR-DEL-015 : Si l'arrêt ou la suppression du conteneur échoue, le service n'est pas supprimé et rien n'est touché

## Règle
Si l'arrêt échoue ou si le moteur refuse de supprimer le conteneur, l'opération échoue avec sa cause (moteur inaccessible, délai dépassé, refusé par le moteur) : le service reste dans son état, avec sa description et ses données, aucun fichier n'est touché, le verrou est rendu et le compte peut recommencer. Si le moteur ne répond plus, le résultat est inconnu : l'opération est « interrompue » et le compte relit l'état réel.

## Application (code)
- `crates/hearth-agent/src/application/delete.rs::ServiceDeletion::{stop, remove_containers}`

## Vérification
- `crates/hearth-agent/tests/service_delete_https.rs::{an_engine_that_refuses_the_removal_leaves_the_service_whole_and_a_retry_succeeds, an_engine_that_does_not_answer_makes_the_result_unknown_and_touches_nothing_else}`

## Cas limites
- Aucun.

## Règles liées
- BR-DEL-014

## Historique
- 2026-10-24 : création (HRT-85, côté agent).
