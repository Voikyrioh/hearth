---
id: BR-EDIT-032
domaine: EDIT
titre: La récupération dure au plus 30 minutes, puis le délai d'arrêt plus une marge par étape
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-032), HRT-87
maj: 2026-11-14
---

# BR-EDIT-032 — La récupération dure au plus 30 minutes, puis le délai d'arrêt plus une marge par étape

## Règle
La récupération dure au plus 30 minutes, puis le délai d'arrêt plus une marge par étape ; au-delà, le résultat est inconnu (`interrupted`, `timeout`).

## Application (code)
- `crates/hearth-agent/src/application/service_update.rs::ServiceUpdates::execute`

## Vérification
- `crates/hearth-agent/src/application/service_update.rs::UpdateTimings`

## Cas limites
- Les textes affichés et les boutons sont ceux du client (HRT-88).

## Règles liées
- BR-EDIT-035, BR-EDIT-024

## Historique
- 2026-11-14 — création (HRT-87, modifier un service par recréation, côté agent).
