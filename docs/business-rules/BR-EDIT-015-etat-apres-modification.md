---
id: BR-EDIT-015
domaine: EDIT
titre: Un service en marche est recréé puis relancé
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-015), HRT-87
maj: 2026-11-14
---

# BR-EDIT-015 — Un service en marche est recréé puis relancé

## Règle
Un service en marche est recréé puis relancé ; un service arrêté ou en erreur est recréé et reste arrêté : Hearth ne démarre jamais seul.

## Application (code)
- `crates/hearth-agent/src/domain/ops/update.rs::op_plan`

## Vérification
- `crates/hearth-agent/tests/service_update_https.rs::a_stopped_service_is_recreated_and_stays_stopped`

## Cas limites
- Les textes affichés et les boutons sont ceux du client (HRT-88).

## Règles liées
- BR-EDIT-035, BR-EDIT-024

## Historique
- 2026-11-14 — création (HRT-87, modifier un service par recréation, côté agent).
