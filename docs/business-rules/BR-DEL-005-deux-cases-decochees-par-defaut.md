---
id: BR-DEL-005
domaine: DEL
titre: Données et sauvegardes ne partent que sur deux cases explicites, décochées par défaut
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-005), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md §5 et §9, HRT-85
maj: 2026-10-24
---

# BR-DEL-005 : Données et sauvegardes ne partent que sur deux cases explicites, décochées par défaut

## Règle
La demande de suppression porte deux cases, `erase_data` et `erase_backups`, décochées quand elles sont absentes. Sans case cochée, seuls le service, sa description et ses journaux partent ; les dossiers de données, l'image, les volumes et les sauvegardes restent. L'agent n'a pas encore de sauvegardes (HRT-97) : `erase_backups` coché est refusé (`422 VALIDATION_ERROR`, champ `erase_backups`), jamais ignoré en silence. Le plan de l'opération ne contient une étape d'effacement que si des données doivent partir.

## Application (code)
- `crates/hearth-proto/src/api/service_delete.rs::DeleteRequest`
- `crates/hearth-agent/src/domain/ops/delete.rs::op_plan`
- `crates/hearth-agent/src/application/delete.rs::ServiceDeletion::delete`

## Vérification
- `crates/hearth-agent/tests/service_delete_https.rs::{a_deletion_without_a_box_removes_the_service_and_keeps_every_byte_of_data, the_backups_box_is_refused_not_ignored_until_the_agent_makes_backups}`
- `crates/hearth-agent/src/domain/ops/delete.rs::tests::without_the_data_box_nothing_in_the_plan_can_erase`

## Cas limites
- La case « sauvegardes » du client n'est proposée que s'il y a des sauvegardes (`DeletePreview::backups`, toujours 0 aujourd'hui).

## Règles liées
- BR-DEL-004
- BR-DEL-006

## Historique
- 2026-10-24 : création (HRT-85, côté agent).
