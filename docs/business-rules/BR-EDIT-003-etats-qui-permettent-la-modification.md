---
id: BR-EDIT-003
domaine: EDIT
titre: Un service est modifiable en marche, arrêté, en erreur ou en redémarrage en boucle
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-003), HRT-87
maj: 2026-11-14
---

# BR-EDIT-003 — Un service est modifiable en marche, arrêté, en erreur ou en redémarrage en boucle

## Règle
Un service est modifiable en marche, arrêté, en erreur ou en redémarrage en boucle ; tout autre état est refusé `SERVICE_NOT_DRIVABLE`.

## Application (code)
- `crates/hearth-agent/src/application/service_update.rs::updatable`

## Vérification
- `crates/hearth-agent/tests/service_update_https.rs::one_action_at_a_time_per_service`

## Cas limites
- Les textes affichés et les boutons sont ceux du client (HRT-88).

## Règles liées
- BR-EDIT-035, BR-EDIT-024

## Historique
- 2026-11-14 — création (HRT-87, modifier un service par recréation, côté agent).
