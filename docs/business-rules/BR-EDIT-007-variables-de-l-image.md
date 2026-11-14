---
id: BR-EDIT-007
domaine: EDIT
titre: Une variable est « de Hearth » si elle est dans la description, « de l'image » sinon
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-007), HRT-87
maj: 2026-11-14
---

# BR-EDIT-007 — Une variable est « de Hearth » si elle est dans la description, « de l'image » sinon

## Règle
Une variable est « de Hearth » si elle est dans la description, « de l'image » sinon ; les variables de l'image ne sont listées que si le conteneur est là.

## Application (code)
- `crates/hearth-agent/src/domain/services/update.rs::image_variables`

## Vérification
- `crates/hearth-agent/tests/service_update_https.rs::reading_the_description_never_shows_a_value_nor_the_command`

## Cas limites
- Les textes affichés et les boutons sont ceux du client (HRT-88).

## Règles liées
- BR-EDIT-035, BR-EDIT-024

## Historique
- 2026-11-14 — création (HRT-87, modifier un service par recréation, côté agent).
