---
id: BR-DEL-017
domaine: DEL
titre: Un dossier géré qui n'existe plus n'est pas une erreur
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-017), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md §5 et §9, HRT-85
maj: 2026-10-24
---

# BR-DEL-017 : Un dossier géré qui n'existe plus n'est pas une erreur

## Règle
Si un dossier géré n'existe plus au moment de l'effacement (`FsError::NotFound`), il n'y a rien à effacer : ni erreur, ni donnée restée. La fenêtre le montre « non mesuré ».

## Application (code)
- `crates/hearth-agent/src/application/delete.rs::ServiceDeletion::erase`

## Vérification
- `crates/hearth-agent/tests/service_delete_https.rs::a_service_whose_container_has_vanished_is_deleted_from_its_description`

## Cas limites
- Aucun.

## Règles liées
- BR-DEL-016

## Historique
- 2026-10-24 : création (HRT-85, côté agent).
