---
id: BR-EDIT-011
domaine: EDIT
titre: L'image est récupérée avant tout arrêt, service en marche
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-011), HRT-87
maj: 2026-11-14
---

# BR-EDIT-011 — L'image est récupérée avant tout arrêt, service en marche

## Règle
L'image est récupérée avant tout arrêt, service en marche ; un échec de récupération ne touche pas au service ; annulable jusqu'à l'arrêt.

## Application (code)
- `crates/hearth-agent/src/application/service_update.rs::ServiceUpdates::pull`

## Vérification
- `crates/hearth-agent/tests/service_update_https.rs::a_failed_pull_touches_nothing`

## Cas limites
- Les textes affichés et les boutons sont ceux du client (HRT-88).

## Règles liées
- BR-EDIT-035, BR-EDIT-024

## Historique
- 2026-11-14 — création (HRT-87, modifier un service par recréation, côté agent).
