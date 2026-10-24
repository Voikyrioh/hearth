---
id: BR-DEL-011
domaine: DEL
titre: Une case cochée exige le nom du service retapé, vérifié par l'agent
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-011), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md §5 et §9, HRT-85
maj: 2026-10-24
---

# BR-DEL-011 : Une case cochée exige le nom du service retapé, vérifié par l'agent

## Règle
Quand une case est cochée, `confirm_name` doit être égal au nom du service, casse comprise, sans espace de plus. L'agent le vérifie même si le client est modifié : `422 VALIDATION_ERROR` (champ `confirm_name`, « Le nom saisi ne correspond pas. »), rien n'est touché, aucune opération n'est créée. Sans case cochée, aucune saisie n'est demandée. Le nom retapé n'est pas signé dans l'acte : l'agent le compare à celui qu'il connaît.

## Application (code)
- `crates/hearth-agent/src/application/delete.rs::ServiceDeletion::delete`

## Vérification
- `crates/hearth-agent/tests/service_delete_https.rs::the_data_box_needs_the_exact_name_and_nothing_is_touched_before_it_is_right`

## Cas limites
- Aucun.

## Règles liées
- BR-DEL-005
- BR-DEL-012

## Historique
- 2026-10-24 : création (HRT-85, côté agent).
