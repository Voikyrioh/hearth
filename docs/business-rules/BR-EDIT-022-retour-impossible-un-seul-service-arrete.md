---
id: BR-EDIT-022
domaine: EDIT
titre: Si le retour échoue aussi, l'ancien n'est pas supprimé, aucun service ne repart seul : l'opération est `interr
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-022), HRT-87
maj: 2026-11-14
---

# BR-EDIT-022 — Si le retour échoue aussi, l'ancien n'est pas supprimé, aucun service ne repart seul : l'opération est `interr

## Règle
Si le retour échoue aussi, l'ancien n'est pas supprimé, aucun service ne repart seul : l'opération est `interrupted` avec la cause `step_failed`.

## Application (code)
- `crates/hearth-agent/src/application/service_update.rs::ServiceUpdates::roll_back`

## Vérification
- `crates/hearth-agent/tests/service_update_https.rs::when_the_way_back_fails_too_the_old_service_is_kept_and_nothing_restarts_alone`

## Cas limites
- Les textes affichés et les boutons sont ceux du client (HRT-88).

## Règles liées
- BR-EDIT-035, BR-EDIT-024

## Historique
- 2026-11-14 — création (HRT-87, modifier un service par recréation, côté agent).
