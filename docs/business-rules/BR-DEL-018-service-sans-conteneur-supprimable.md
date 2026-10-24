---
id: BR-DEL-018
domaine: DEL
titre: Un service dont le conteneur a disparu reste supprimable par sa description
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-018), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md §5 et §9, HRT-85
maj: 2026-10-24
---

# BR-DEL-018 : Un service dont le conteneur a disparu reste supprimable par sa description

## Règle
Un service que Hearth connaît par sa description durable mais dont le conteneur n'existe plus (purge, moteur réinstallé) reste supprimable : ses dossiers sont connus par la description, la fenêtre les liste comme pour tout service. Un service que Hearth ne connaît plus (ni conteneur, ni description) n'existe plus : `404 SERVICE_NOT_FOUND`, rien n'est effacé pour son compte. Une clé mal formée est un service inconnu.

## Application (code)
- `crates/hearth-agent/src/application/delete.rs::ServiceDeletion::subject`

## Vérification
- `crates/hearth-agent/tests/service_delete_https.rs::{a_service_whose_container_has_vanished_is_deleted_from_its_description, a_service_hearth_does_not_know_is_a_service_that_is_gone}`

## Cas limites
- Aucun.

## Règles liées
- BR-EDIT-036
- BR-DEL-003

## Historique
- 2026-10-24 : création (HRT-85, côté agent).
