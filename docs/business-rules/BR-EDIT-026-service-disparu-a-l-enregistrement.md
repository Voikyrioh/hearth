---
id: BR-EDIT-026
domaine: EDIT
titre: Si le service n'existe plus à l'enregistrement : `SERVICE_NOT_FOUND`, rien n'est fait
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-026), HRT-87
maj: 2026-11-14
---

# BR-EDIT-026 — Si le service n'existe plus à l'enregistrement : `SERVICE_NOT_FOUND`, rien n'est fait

## Règle
Si le service n'existe plus à l'enregistrement : `SERVICE_NOT_FOUND`, rien n'est fait.

## Application (code)
- `crates/hearth-agent/src/application/service_update.rs::ServiceUpdates::update`

## Vérification
- `crates/hearth-agent/tests/service_update_https.rs::reading_the_description_never_shows_a_value_nor_the_command`

## Cas limites
- Les textes affichés et les boutons sont ceux du client (HRT-88).

## Règles liées
- BR-EDIT-035, BR-EDIT-024

## Historique
- 2026-11-14 — création (HRT-87, modifier un service par recréation, côté agent).
