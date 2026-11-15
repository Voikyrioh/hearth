---
id: BR-EDIT-019
domaine: EDIT
titre: Le nouveau service est jugé démarré quand il reste en marche 10 secondes
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-019), HRT-87
maj: 2026-11-14
---

# BR-EDIT-019 — Le nouveau service est jugé démarré quand il reste en marche 10 secondes

## Règle
Le nouveau service est jugé démarré quand il reste en marche 10 secondes ; un arrêt dans la fenêtre, code zéro compris, ou une relance par le moteur est un échec.

## Application (code)
- `crates/hearth-agent/src/application/service_update.rs::ServiceUpdates::watch`

## Vérification
- `crates/hearth-agent/tests/service_update_https.rs::a_new_service_that_stops_within_ten_seconds_is_replaced_by_the_old_one`

## Cas limites
- Les textes affichés et les boutons sont ceux du client (HRT-88).

## Règles liées
- BR-EDIT-035, BR-EDIT-024

## Historique
- 2026-11-14 — création (HRT-87, modifier un service par recréation, côté agent).
