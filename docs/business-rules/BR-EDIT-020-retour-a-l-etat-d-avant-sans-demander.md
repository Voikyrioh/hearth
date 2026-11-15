---
id: BR-EDIT-020
domaine: EDIT
titre: Si le nouveau échoue, il est retiré, la révision candidate abandonnée, l'ancien remis sous son nom et relancé 
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-020), HRT-87
maj: 2026-11-14
---

# BR-EDIT-020 — Si le nouveau échoue, il est retiré, la révision candidate abandonnée, l'ancien remis sous son nom et relancé 

## Règle
Si le nouveau échoue, il est retiré, la révision candidate abandonnée, l'ancien remis sous son nom et relancé s'il tournait ; ses 20 dernières lignes sont gardées 24 h en mémoire seulement.

## Application (code)
- `crates/hearth-agent/src/application/service_update.rs::ServiceUpdates::roll_back`
- `crates/hearth-agent/src/application/service_update.rs::ServiceUpdates::restore`

## Vérification
- `crates/hearth-agent/tests/service_update_https.rs::a_new_service_that_stops_within_ten_seconds_is_replaced_by_the_old_one`
- `crates/hearth-agent/tests/service_update_https.rs::a_creation_refused_by_the_engine_puts_the_old_service_back`

## Cas limites
- Les textes affichés et les boutons sont ceux du client (HRT-88).

## Règles liées
- BR-EDIT-035, BR-EDIT-024

## Historique
- 2026-11-14 — création (HRT-87, modifier un service par recréation, côté agent).
