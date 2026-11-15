---
id: BR-EDIT-025
domaine: EDIT
titre: La modification prend le verrou exclusif du service
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-025), HRT-87
maj: 2026-11-14
---

# BR-EDIT-025 — La modification prend le verrou exclusif du service

## Règle
La modification prend le verrou exclusif du service ; une seconde demande reçoit `SERVICE_BUSY`.

## Application (code)
- `crates/hearth-agent/src/application/service_update.rs::ServiceUpdates::update`

## Vérification
- `crates/hearth-agent/tests/service_update_https.rs::one_action_at_a_time_per_service`

## Cas limites
- Les textes affichés et les boutons sont ceux du client (HRT-88).

## Règles liées
- BR-EDIT-035, BR-EDIT-024

## Historique
- 2026-11-14 — création (HRT-87, modifier un service par recréation, côté agent).
